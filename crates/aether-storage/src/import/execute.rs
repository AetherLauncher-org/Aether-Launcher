//! Executing an import: copy version metadata, then create managed instances.
//!
//! [`super`] only *plans* an import. This module performs the part of that plan
//! that is safe to perform automatically, and nothing more:
//!
//! 1. the selected version's `versions/<id>/<id>.json` is read from the source
//!    directory, together with every `inheritsFrom` ancestor it names;
//! 2. each of those files is published into the launcher-owned game tree at
//!    `<game_root>/versions/<id>/<id>.json`, **never overwriting** a file that is
//!    already there;
//! 3. one managed instance record is created per selected version.
//!
//! # What is deliberately not copied
//!
//! Client JARs, libraries, assets and native archives stay where they are. They
//! are hundreds of megabytes, they are shared between versions, and the launcher
//! already knows how to fetch and verify them from the manifest pipeline (with
//! the BMCLAPI mirror as a fallback source). Copying them would duplicate data
//! and would let a foreign launcher's directory decide what this launcher
//! trusts. The version JSON is different: it is small, it is the only thing that
//! makes a version resolvable at all, and it carries the SHA-1 values that the
//! download pipeline verifies against.
//!
//! # Trust boundary
//!
//! The source directory is untrusted input. Every read is bounded
//! ([`MAX_VERSION_JSON_BYTES`], [`MAX_INHERITANCE_DEPTH`]), symbolic links in
//! the source are refused instead of followed, and version identifiers are
//! validated as single path segments before they are joined onto any path. The
//! destination is always the caller-provided game tree and instance store; no
//! path from the source is ever used as a write destination.

use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use aether_core::GameInstance;
use thiserror::Error;

use super::prism;
use crate::create::{CreateError, create_instance};

/// Maximum size of one version JSON copied out of a foreign launcher.
pub const MAX_VERSION_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// Maximum number of `inheritsFrom` levels followed while copying metadata.
pub const MAX_INHERITANCE_DEPTH: usize = 32;

/// Maximum number of versions one call may import.
pub const MAX_IMPORT_VERSIONS: usize = 256;

/// Failures that abort the whole import instead of one selected version.
#[derive(Debug, Error)]
pub enum ImportExecuteError {
    /// The source is missing, is not a directory, or is a symbolic link.
    #[error("import source is not a readable directory")]
    InvalidSource,
    /// The caller passed an empty selection.
    #[error("no version was selected for import")]
    NothingSelected,
    /// The caller passed more than [`MAX_IMPORT_VERSIONS`] identifiers.
    #[error("too many versions were selected for one import")]
    TooManyVersions,
    /// The game directory (the user's `.minecraft`) is missing or not a directory.
    #[error("the game directory is not usable")]
    InvalidGameRoot,
    /// A directory operation on the managed game tree failed.
    #[error("cannot prepare the managed game directory")]
    Io(#[source] std::io::Error),
}

/// Why one selected version was not imported.
///
/// The labels are stable and cross the IPC boundary as-is; the frontend owns the
/// translation. They never contain a file system path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSkipReason {
    /// The identifier is not a single, safe path segment.
    UnsafeVersionId,
    /// `versions/<id>/<id>.json` is absent from the source.
    MissingVersionJson,
    /// A Prism/MultiMC instance has no readable `mmc-pack.json`.
    MissingPackMetadata,
    /// The source entry is a symbolic link or not a regular file.
    UnsafeSource,
    /// The version JSON exceeds [`MAX_VERSION_JSON_BYTES`].
    VersionJsonTooLarge,
    /// The version JSON is not a JSON object.
    InvalidVersionJson,
    /// The `inheritsFrom` chain is longer than [`MAX_INHERITANCE_DEPTH`].
    InheritanceTooDeep,
    /// The `inheritsFrom` chain points back at a version it already visited.
    InheritanceCycle,
    /// Another instance in the store already carries this display name.
    DuplicateName,
    /// The version identifier is not an acceptable instance name.
    InvalidName,
    /// A file system operation failed for this version only.
    Io,
}

impl ImportSkipReason {
    /// Stable, lowercase identifier used in IPC payloads and logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsafeVersionId => "unsafe-version-id",
            Self::MissingVersionJson => "missing-version-json",
            Self::MissingPackMetadata => "missing-pack-metadata",
            Self::UnsafeSource => "unsafe-source",
            Self::VersionJsonTooLarge => "version-json-too-large",
            Self::InvalidVersionJson => "invalid-version-json",
            Self::InheritanceTooDeep => "inheritance-too-deep",
            Self::InheritanceCycle => "inheritance-cycle",
            Self::DuplicateName => "duplicate-name",
            Self::InvalidName => "invalid-name",
            Self::Io => "io",
        }
    }
}

impl std::fmt::Display for ImportSkipReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Result of one import call.
#[derive(Debug, Default)]
pub struct ImportOutcome {
    /// Instances that were created and persisted.
    pub imported: Vec<GameInstance>,
    /// Versions that were skipped, with the reason, in selection order.
    pub skipped: Vec<(String, ImportSkipReason)>,
}

/// Copies the selected versions out of `source_root` and creates one instance
/// per version under `base`.
///
/// `game_root` is the user's own `.minecraft`: the version metadata is written into
/// its `versions/` directory, in place, so every launcher on the machine sees the
/// same versions. `base` is the instance store root, the same one
/// [`create_instance`] expects.
///
/// A version that cannot be imported is reported in [`ImportOutcome::skipped`]
/// and does not stop the others. Only a source that is not a usable directory,
/// an empty or oversized selection, or an unusable game tree aborts the call.
pub fn import_versions(
    source_root: &Path,
    game_root: &Path,
    base: &Path,
    version_ids: &[String],
) -> Result<ImportOutcome, ImportExecuteError> {
    let source_root = canonical_directory(source_root).ok_or(ImportExecuteError::InvalidSource)?;
    let game_root = canonical_directory(game_root).ok_or(ImportExecuteError::InvalidGameRoot)?;

    if version_ids.is_empty() {
        return Err(ImportExecuteError::NothingSelected);
    }
    if version_ids.len() > MAX_IMPORT_VERSIONS {
        return Err(ImportExecuteError::TooManyVersions);
    }

    let versions_root = prepare_versions_root(&game_root)?;

    let mut outcome = ImportOutcome::default();
    let mut seen: Vec<&str> = Vec::new();

    for version_id in version_ids {
        // A repeated identifier would otherwise be reported as a duplicate name.
        if seen.contains(&version_id.as_str()) {
            continue;
        }
        seen.push(version_id.as_str());

        match import_one(&source_root, &versions_root, &game_root, base, version_id) {
            Ok(instance) => outcome.imported.push(instance),
            Err(reason) => outcome.skipped.push((version_id.clone(), reason)),
        }
    }

    Ok(outcome)
}

/// Creates one managed instance per selected Prism/MultiMC instance.
///
/// `instance_ids` are folder names under the launcher's instance directory, the
/// same identifiers [`super::prism::PrismInstance::id`] reports. The display
/// name is read from each `instance.cfg` rather than taken from the caller, so a
/// selection can only ever name an instance that actually exists on disk.
///
/// Nothing is copied: Prism keeps no `versions/<id>/<id>.json`, and the version
/// metadata it caches under `meta/` is its own component format rather than a
/// Minecraft version JSON. Each instance is therefore created against the base
/// version named by `mmc-pack.json`, and the caller fetches that version's
/// metadata from the official catalog afterwards — the same path a version
/// imported from a folder takes when its files are still missing.
///
/// Loader components are read but not installed. An instance whose pack names
/// Fabric, Quilt, Forge or NeoForge imports as its base version, and the caller
/// is expected to say so.
pub fn import_prism_instances(
    launcher_root: &Path,
    game_root: &Path,
    base: &Path,
    instance_ids: &[String],
) -> Result<ImportOutcome, ImportExecuteError> {
    let launcher_root =
        canonical_directory(launcher_root).ok_or(ImportExecuteError::InvalidSource)?;
    let game_root = canonical_directory(game_root).ok_or(ImportExecuteError::InvalidGameRoot)?;

    if instance_ids.is_empty() {
        return Err(ImportExecuteError::NothingSelected);
    }
    if instance_ids.len() > MAX_IMPORT_VERSIONS {
        return Err(ImportExecuteError::TooManyVersions);
    }

    let mut outcome = ImportOutcome::default();
    let mut seen: Vec<&str> = Vec::new();

    for instance_id in instance_ids {
        if seen.contains(&instance_id.as_str()) {
            continue;
        }
        seen.push(instance_id.as_str());

        match import_one_prism(&launcher_root, &game_root, base, instance_id) {
            Ok(instance) => outcome.imported.push(instance),
            Err(reason) => outcome.skipped.push((instance_id.clone(), reason)),
        }
    }

    Ok(outcome)
}

/// Imports exactly one Prism instance, returning the reason instead of aborting.
fn import_one_prism(
    launcher_root: &Path,
    game_root: &Path,
    base: &Path,
    instance_id: &str,
) -> Result<GameInstance, ImportSkipReason> {
    if !is_safe_version_id(instance_id) {
        return Err(ImportSkipReason::UnsafeVersionId);
    }

    let instances_root =
        prism::locate_instances_root(launcher_root).ok_or(ImportSkipReason::MissingPackMetadata)?;
    let instance = prism::read_instance(&instances_root.join(instance_id))
        .ok_or(ImportSkipReason::MissingPackMetadata)?;

    if !is_safe_version_id(&instance.minecraft_version) {
        return Err(ImportSkipReason::UnsafeVersionId);
    }

    // 每个 Prism/MultiMC 实例的游戏目录在它自己的文件夹里
    // （`instances/<id>/.minecraft`）；目录是 `instances/` 时按实例 ID 取。
    let instance_game_directory = game_root.join(instance_id).join(".minecraft");
    let game_directory = if instance_game_directory.is_dir() {
        instance_game_directory
    } else {
        game_root.join(instance_id)
    };

    create_instance(
        base,
        &instance.name,
        &instance.minecraft_version,
        &game_directory,
    )
    .map_err(map_create_error)
}

fn import_one(
    source_root: &Path,
    versions_root: &Path,
    game_root: &Path,
    base: &Path,
    version_id: &str,
) -> Result<GameInstance, ImportSkipReason> {
    if !is_safe_version_id(version_id) {
        return Err(ImportSkipReason::UnsafeVersionId);
    }

    // Read the whole chain before writing anything, so a broken parent cannot
    // leave a half-imported child behind in the game tree.
    let chain = read_inheritance_chain(source_root, version_id)?;
    // 导入**不判断重名**：同一文件夹里的版本目录名本来就唯一，而且从文件夹
    // 导入时源和目标常常是同一个 `.minecraft` —— 文件已经在那儿，直接登记成
    // 实例即可。真正需要「重名提示」的是下载新版本（用户自己起名字才会撞），
    // 那一侧在 `install.rs` 单独检查。
    for (id, bytes) in &chain {
        write_version_json(versions_root, id, bytes)?;
    }

    create_instance(base, version_id, version_id, game_root).map_err(map_create_error)
}

/// Reads `version_id` and every ancestor it inherits from, child first.
fn read_inheritance_chain(
    source_root: &Path,
    version_id: &str,
) -> Result<Vec<(String, Vec<u8>)>, ImportSkipReason> {
    let mut chain: Vec<(String, Vec<u8>)> = Vec::new();
    let mut current = version_id.to_owned();

    loop {
        if chain.iter().any(|(id, _)| id == &current) {
            return Err(ImportSkipReason::InheritanceCycle);
        }
        if chain.len() >= MAX_INHERITANCE_DEPTH {
            return Err(ImportSkipReason::InheritanceTooDeep);
        }

        let bytes = read_version_json(source_root, &current)?;
        let parent = parse_inherits_from(&bytes)?;
        chain.push((current, bytes));

        match parent {
            Some(parent) => {
                if !is_safe_version_id(&parent) {
                    return Err(ImportSkipReason::UnsafeVersionId);
                }
                current = parent;
            }
            None => return Ok(chain),
        }
    }
}

/// Reads one `<source>/versions/<id>/<id>.json` with a size cap.
fn read_version_json(source_root: &Path, version_id: &str) -> Result<Vec<u8>, ImportSkipReason> {
    let path = source_root
        .join("versions")
        .join(version_id)
        .join(format!("{version_id}.json"));

    let metadata = fs::symlink_metadata(&path).map_err(|_| ImportSkipReason::MissingVersionJson)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ImportSkipReason::UnsafeSource);
    }
    if metadata.len() > MAX_VERSION_JSON_BYTES {
        return Err(ImportSkipReason::VersionJsonTooLarge);
    }

    let file = fs::File::open(&path).map_err(|_| ImportSkipReason::MissingVersionJson)?;
    let mut bytes = Vec::new();
    file.take(MAX_VERSION_JSON_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ImportSkipReason::MissingVersionJson)?;
    if bytes.len() as u64 > MAX_VERSION_JSON_BYTES {
        return Err(ImportSkipReason::VersionJsonTooLarge);
    }

    Ok(bytes)
}

/// Minimal view of a version JSON: only the parent link matters here.
#[derive(serde::Deserialize)]
struct VersionHeader {
    #[serde(rename = "inheritsFrom")]
    inherits_from: Option<String>,
}

/// Extracts `inheritsFrom`, rejecting anything that is not a JSON object.
///
/// A UTF-8 byte order mark is tolerated because some launchers write one; the
/// copied file keeps it, which the manifest loader also accepts.
fn parse_inherits_from(bytes: &[u8]) -> Result<Option<String>, ImportSkipReason> {
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let header: VersionHeader =
        serde_json::from_slice(body).map_err(|_| ImportSkipReason::InvalidVersionJson)?;
    Ok(header.inherits_from)
}

/// 写一份版本 JSON 到 `versions/<id>/<id>.json`，**绝不覆盖**已有文件。
///
/// 文件已存在时直接返回 `Ok(())`：同一文件夹里的版本目录名本来就唯一，而且从
/// 文件夹导入时源和目标常常是同一个 `.minecraft`。那里已存在的 JSON 可能是官方
/// 校验过的、也可能是别的启动器写的，都不该被替换，也不该报「重名」——那不是错误。
///
/// 需要提示「这个名字已经有了」的是**下载新版本**（名字由用户输入才会撞），
/// 那一侧在 `src-tauri/src/commands/install.rs` 里单独检查。
fn write_version_json(
    versions_root: &Path,
    version_id: &str,
    bytes: &[u8],
) -> Result<(), ImportSkipReason> {
    let directory = versions_root.join(version_id);
    let destination = directory.join(format!("{version_id}.json"));

    match fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(ImportSkipReason::UnsafeSource);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(ImportSkipReason::Io),
    }

    match fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(ImportSkipReason::UnsafeSource);
        }
        // 已存在：原样保留，继续给这个版本建实例。
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(ImportSkipReason::Io),
    }

    fs::create_dir_all(&directory).map_err(|_| ImportSkipReason::Io)?;

    // Stage and rename so a crash cannot leave a truncated JSON behind.
    let staging = directory.join(format!(
        ".import-{}-{version_id}.json.tmp",
        std::process::id()
    ));
    let published = fs::write(&staging, bytes).and_then(|()| fs::rename(&staging, &destination));
    if published.is_err() {
        let _ = fs::remove_file(&staging);
        return Err(ImportSkipReason::Io);
    }

    Ok(())
}

/// Creates `<game_root>/versions` when missing and returns its canonical path.
fn prepare_versions_root(game_root: &Path) -> Result<PathBuf, ImportExecuteError> {
    fs::create_dir_all(game_root).map_err(ImportExecuteError::Io)?;

    let versions = game_root.join("versions");
    match fs::symlink_metadata(&versions) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(ImportExecuteError::InvalidGameRoot);
        }
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(ImportExecuteError::InvalidGameRoot),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&versions).map_err(ImportExecuteError::Io)?;
        }
        Err(error) => return Err(ImportExecuteError::Io(error)),
    }

    fs::canonicalize(&versions).map_err(ImportExecuteError::Io)
}

/// Canonicalizes `path` only when it is a plain directory.
fn canonical_directory(path: &Path) -> Option<PathBuf> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return None;
    }
    fs::canonicalize(path).ok()
}

/// Whether `id` can be used as one `versions/<id>/` directory name.
///
/// Mirrors the rules `aether-manifest` applies when it loads a version, so a
/// version this module accepts is one the loader can also open. The rules are
/// duplicated rather than imported because `aether-storage` does not depend on
/// `aether-manifest`; see `docs/architecture.md` for the dependency direction.
fn is_safe_version_id(id: &str) -> bool {
    if id.trim().is_empty() || matches!(id, "." | "..") || id.ends_with([' ', '.']) {
        return false;
    }
    if id.chars().any(|character| {
        character.is_control()
            || matches!(
                character,
                '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'
            )
    }) {
        return false;
    }

    // Windows device names are reserved even when followed by an extension.
    let stem = id.split('.').next().unwrap_or(id).to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    if reserved {
        return false;
    }

    let mut components = Path::new(id).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

/// Maps an instance-creation failure onto a per-version skip reason.
fn map_create_error(error: CreateError) -> ImportSkipReason {
    match error {
        CreateError::DuplicateName(_) => ImportSkipReason::DuplicateName,
        CreateError::InvalidName(_) => ImportSkipReason::InvalidName,
        CreateError::InvalidVersion(_) => ImportSkipReason::UnsafeVersionId,
        CreateError::UnsafeBase
        | CreateError::UnsafeGameDirectory
        | CreateError::AlreadyExists
        | CreateError::Io(_) => ImportSkipReason::Io,
    }
}

#[cfg(test)]
#[path = "execute/execute_tests.rs"]
mod tests;
