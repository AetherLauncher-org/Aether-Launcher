//! Planning an import: which official versions to install, under which names.
//!
//! [`super`] only *detects* what a foreign launcher has. This module turns the
//! user's selection into a list of [`ImportRequest`]s the desktop layer can run
//! through the normal installer, and nothing more. It writes nothing and creates
//! no instance.
//!
//! # Why an import is an install
//!
//! Another launcher's version JSON is untrusted input. It carries the download
//! URLs and SHA-1 values that would drive an install, and a hash is only worth
//! anything when it comes from Mojang's official manifest. So an import never
//! installs *from* the foreign JSON. It reads just enough of it to learn which
//! official version the entry is, and the installer then fetches that version's
//! JSON from the official catalog and verifies every file against it.
//!
//! What the foreign directory *does* offer is files that are already on disk. The
//! installer may copy such a file into the launcher-owned game tree, but only
//! after its SHA-1 matches the official manifest. [`ImportPlan::seed`] names the
//! directory to look in; it is read-only and never a write destination.
//!
//! # What is not supported
//!
//! A version that inherits from another one (Fabric, Forge, Quilt, NeoForge
//! profiles and most modpacks) is skipped with [`ImportSkipReason::NeedsLoader`]:
//! a loader's libraries come from Maven repositories and have no official hash.
//! Prism and MultiMC instances that name a loader are skipped the same way.
//!
//! # Trust boundary
//!
//! The source directory is untrusted. Every read is bounded
//! ([`MAX_VERSION_JSON_BYTES`]), symbolic links are refused instead of followed,
//! and identifiers are validated as single path segments before they are joined
//! onto any path.

use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use super::prism;

/// Maximum size of one version JSON read out of a foreign launcher.
pub const MAX_VERSION_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// Maximum number of entries one call may import.
pub const MAX_IMPORT_VERSIONS: usize = 256;

/// Failures that abort the whole plan instead of one selected entry.
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
}

/// Why one selected entry was not imported.
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
    /// The entry inherits from another version or names a loader.
    NeedsLoader,
    /// The version is not in Mojang's official catalog (renamed or custom builds).
    NotOfficial,
    /// An instance with this name already exists.
    DuplicateName,
    /// The name is not an acceptable instance name.
    InvalidName,
    /// The installer failed for this entry only; the files already fetched stay.
    InstallFailed,
    /// The import was cancelled before this entry was reached.
    Cancelled,
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
            Self::NeedsLoader => "needs-loader",
            Self::NotOfficial => "not-official",
            Self::DuplicateName => "duplicate-name",
            Self::InvalidName => "invalid-name",
            Self::InstallFailed => "install-failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl std::fmt::Display for ImportSkipReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One entry the installer should handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRequest {
    /// The identifier the user ticked (a version id or a Prism folder name),
    /// echoed back in results so the UI can match them up.
    pub id: String,
    /// The official Minecraft version to install. Whether it really is one is
    /// decided against the official catalog by the caller.
    pub version: String,
    /// Suggested instance name.
    pub name: String,
}

/// The outcome of planning: entries to install, entries that were refused, and
/// where already-downloaded files may be borrowed from.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportPlan {
    pub requests: Vec<ImportRequest>,
    /// Entries refused at planning time, in selection order.
    pub skipped: Vec<(String, ImportSkipReason)>,
    /// A read-only directory laid out like a `.minecraft` (`libraries/`,
    /// `assets/`, `versions/`). Files are copied from it only after their SHA-1
    /// matches the official manifest.
    pub seed: PathBuf,
}

/// Plans the import of the selected versions of a `.minecraft`-shaped folder.
///
/// `version_ids` are `versions/<id>` folder names. An entry that inherits from
/// another version is skipped; so is an entry whose JSON cannot be read safely.
pub fn plan_folder_import(
    source_root: &Path,
    version_ids: &[String],
) -> Result<ImportPlan, ImportExecuteError> {
    let source_root = canonical_directory(source_root).ok_or(ImportExecuteError::InvalidSource)?;
    check_selection(version_ids)?;

    let mut plan = ImportPlan {
        seed: source_root.clone(),
        ..ImportPlan::default()
    };
    for version_id in unique(version_ids) {
        match classify_folder_version(&source_root, version_id) {
            Ok(()) => plan.requests.push(ImportRequest {
                id: version_id.to_owned(),
                version: version_id.to_owned(),
                name: version_id.to_owned(),
            }),
            Err(reason) => plan.skipped.push((version_id.to_owned(), reason)),
        }
    }
    Ok(plan)
}

/// Plans the import of the selected Prism/MultiMC instances.
///
/// `instance_ids` are folder names under the launcher's instance directory. The
/// instance name and base version are read from the instance itself, so a
/// selection can only ever name an instance that exists on disk. Instances that
/// name a loader are skipped.
pub fn plan_prism_import(
    launcher_root: &Path,
    instance_ids: &[String],
) -> Result<ImportPlan, ImportExecuteError> {
    let launcher_root =
        canonical_directory(launcher_root).ok_or(ImportExecuteError::InvalidSource)?;
    check_selection(instance_ids)?;

    let mut plan = ImportPlan {
        seed: launcher_root.clone(),
        ..ImportPlan::default()
    };
    for instance_id in unique(instance_ids) {
        match classify_prism_instance(&launcher_root, instance_id) {
            Ok((version, name)) => plan.requests.push(ImportRequest {
                id: instance_id.to_owned(),
                version,
                name,
            }),
            Err(reason) => plan.skipped.push((instance_id.to_owned(), reason)),
        }
    }
    Ok(plan)
}

fn check_selection(ids: &[String]) -> Result<(), ImportExecuteError> {
    if ids.is_empty() {
        return Err(ImportExecuteError::NothingSelected);
    }
    if ids.len() > MAX_IMPORT_VERSIONS {
        return Err(ImportExecuteError::TooManyVersions);
    }
    Ok(())
}

/// The identifiers in order, without repeats (a repeated one would otherwise be
/// reported as a duplicate name).
fn unique(ids: &[String]) -> Vec<&str> {
    let mut seen: Vec<&str> = Vec::new();
    for id in ids {
        if !seen.contains(&id.as_str()) {
            seen.push(id.as_str());
        }
    }
    seen
}

/// Decides whether one folder version can be handed to the installer.
fn classify_folder_version(source_root: &Path, version_id: &str) -> Result<(), ImportSkipReason> {
    if !is_safe_version_id(version_id) {
        return Err(ImportSkipReason::UnsafeVersionId);
    }
    let bytes = read_version_json(source_root, version_id)?;
    if parse_inherits_from(&bytes)?.is_some() {
        return Err(ImportSkipReason::NeedsLoader);
    }
    Ok(())
}

/// Reads one Prism instance and returns `(base version, instance name)`.
fn classify_prism_instance(
    launcher_root: &Path,
    instance_id: &str,
) -> Result<(String, String), ImportSkipReason> {
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
    if instance.loader.is_some() {
        return Err(ImportSkipReason::NeedsLoader);
    }
    Ok((instance.minecraft_version, instance.name))
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
/// A UTF-8 byte order mark is tolerated because some launchers write one.
fn parse_inherits_from(bytes: &[u8]) -> Result<Option<String>, ImportSkipReason> {
    let body = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let header: VersionHeader =
        serde_json::from_slice(body).map_err(|_| ImportSkipReason::InvalidVersionJson)?;
    Ok(header.inherits_from)
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

#[cfg(test)]
#[path = "execute/execute_tests.rs"]
mod tests;
