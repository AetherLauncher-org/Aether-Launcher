//! Detection and planning for importing instances from other launchers.
//!
//! This module **never copies or moves a file**. It only looks at a directory
//! listing, decides which launcher produced it, and returns an
//! [`ImportCandidate`] per version it recognised. The actual transfer is left to
//! a later step that consumes the candidates.
//!
//! Detection is split in two layers on purpose:
//!
//! * [`detect`] and [`plan_candidates`] are pure functions over an
//!   [`ExistingFiles`] snapshot. They never touch the file system, so every
//!   layout below is covered by ordinary unit tests.
//! * [`scan_directory`] is the thin wrapper that builds that snapshot from a real
//!   directory. It is bounded in depth and entry count so a huge or hostile tree
//!   cannot stall the UI thread.
//!
//! Supported layouts:
//!
//! | Launcher | Strong marker | Versions |
//! | --- | --- | --- |
//! | PCL2 | `PCL/Setup.ini`, `PCL.ini` | `versions/<id>/<id>.json` (also below `.minecraft/`) |
//! | HMCL | `.hmcl.json`, `hmcl.json`, `hmclversion.cfg` | `versions/<id>/<id>.json` |
//!
//! Importing is additive only: a candidate describes what would be created, and
//! the caller must skip every target that already exists instead of overwriting
//! it. See [`ImportCandidate::existing_targets`] and
//! [`ImportCandidate::missing_targets`] for the planned-file split.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

pub mod hmcl;
pub mod pcl2;

/// Maximum directory depth [`scan_directory`] walks below its root.
pub const MAX_SCAN_DEPTH: usize = 3;

/// Maximum number of entries [`scan_directory`] collects.
pub const MAX_SCAN_ENTRIES: usize = 4096;

/// Which launcher a candidate was recognised from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportSourceKind {
    /// Plain Craft Launcher 2.
    Pcl2,
    /// Hello Minecraft! Launcher.
    Hmcl,
}

impl ImportSourceKind {
    /// Stable, lowercase identifier used in IPC payloads and logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pcl2 => "pcl2",
            Self::Hmcl => "hmcl",
        }
    }
}

impl std::fmt::Display for ImportSourceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A directory listing normalised for detection.
///
/// Every entry is a path relative to the scanned root, using `/` as the
/// separator on every platform so the parsers are platform independent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExistingFiles {
    /// Relative paths, `/` separated, without a leading separator.
    pub entries: Vec<String>,
}

impl ExistingFiles {
    /// Builds a snapshot from relative paths, normalising separators.
    ///
    /// Absolute entries are dropped: a snapshot describes paths inside the
    /// scanned root, and following a hostile absolute entry would let a listing
    /// escape it. See `is_absolute_entry`.
    #[must_use]
    pub fn new(entries: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut normalised: Vec<String> = entries
            .into_iter()
            .map(|entry| entry.into())
            .filter(|entry| !is_absolute_entry(entry))
            .map(|entry| normalise_relative(&entry))
            .filter(|entry| !entry.is_empty())
            .collect();
        normalised.sort();
        normalised.dedup();

        Self {
            entries: normalised,
        }
    }

    /// Whether the snapshot contains `path` exactly.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        let needle = normalise_relative(path);
        self.entries.contains(&needle)
    }

    /// Whether the snapshot contains any entry whose last segment is `name`.
    ///
    /// Used for marker files that laguers keep in a user-configurable place.
    #[must_use]
    pub fn contains_file_named(&self, name: &str) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.rsplit('/').next() == Some(name))
    }
}

/// One importable Minecraft version.
///
/// The struct is a plan, not a copy: `existing_targets` lists the files the
/// import would have to create but that are already present, so the caller knows
/// to skip them rather than overwrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidate {
    /// Launcher the candidate came from.
    pub source_kind: ImportSourceKind,
    /// Whether the source keeps each version in its own `versions/<id>/` folder.
    pub version_isolation: bool,
    /// Game directory the version would be attached to, relative to the source.
    pub game_directory: PathBuf,
    /// Minecraft version identifier, usually the `versions/<id>` folder name.
    pub version_id: String,
    /// Human readable name suggested for the new instance.
    pub display_name: String,
    /// Planned files that already exist at the target and must be skipped.
    pub existing_targets: Vec<String>,
    /// Planned files that still have to be created.
    pub missing_targets: Vec<String>,
}

impl ImportCandidate {
    /// Creates a candidate whose plan is fully "missing", i.e. a fresh import.
    #[must_use]
    pub fn planned(
        source_kind: ImportSourceKind,
        version_isolation: bool,
        game_directory: PathBuf,
        version_id: String,
    ) -> Self {
        let display_name = display_name_for(&version_id);
        let mut candidate = Self {
            source_kind,
            version_isolation,
            game_directory,
            version_id,
            display_name,
            existing_targets: Vec::new(),
            missing_targets: Vec::new(),
        };
        let planned = candidate.planned_relative_paths();
        candidate.missing_targets = planned;
        candidate
    }

    /// Marks the planned paths that already exist at the destination.
    ///
    /// Matching is case-insensitive because the launchers run mostly on Windows,
    /// where `abc` and `ABC` are the same file. Existing paths are never
    /// overwritten by an import.
    #[must_use]
    pub fn with_existing(mut self, existing: &ExistingFiles) -> Self {
        let planned = self.planned_relative_paths();
        let mut missing = Vec::new();
        let mut present = Vec::new();

        for path in planned {
            let needle = path.to_ascii_lowercase();
            let exists = existing
                .entries
                .iter()
                .any(|entry| entry.to_ascii_lowercase() == needle);
            if exists {
                present.push(path);
            } else {
                missing.push(path);
            }
        }

        self.existing_targets = present;
        self.missing_targets = missing;
        self
    }

    /// Paths this candidate would contribute, relative to the new instance root.
    ///
    /// Only the version metadata file is planned: the game jar and libraries are
    /// discovered and downloaded later by the manifest pipeline, never copied out
    /// of another launcher's directory.
    #[must_use]
    pub fn planned_relative_paths(&self) -> Vec<String> {
        vec![format!(
            "versions/{version}/{version}.json",
            version = self.version_id
        )]
    }

    /// Whether the import would actually create anything.
    #[must_use]
    pub fn is_fully_present(&self) -> bool {
        self.missing_targets.is_empty()
    }
}

/// Failures while turning a directory into [`ExistingFiles`].
#[derive(Debug, Error)]
pub enum ImportError {
    /// The root does not exist, is not a directory, or is a symbolic link.
    #[error("import source root is not a readable directory")]
    InvalidRoot,
    /// The tree exceeded [`MAX_SCAN_ENTRIES`].
    #[error("import source has too many entries")]
    TooManyEntries,
    /// A directory operation failed.
    #[error("cannot scan the import source")]
    Io(#[source] std::io::Error),
}

/// Recognises which launcher produced a listing.
///
/// PCL2 wins when both launchers left markers behind, because PCL2 keeps its
/// configuration in a subdirectory that a mixed tree would still contain.
#[must_use]
pub fn detect(files: &ExistingFiles) -> Option<ImportSourceKind> {
    if pcl2::matches(files) {
        return Some(ImportSourceKind::Pcl2);
    }
    if hmcl::matches(files) {
        return Some(ImportSourceKind::Hmcl);
    }
    None
}

/// Builds one candidate per version found in a listing.
///
/// Returns an empty vector when the layout has no `versions/` directory or no
/// version metadata file; the caller can then report "nothing to import" without
/// treating it as a failure.
#[must_use]
pub fn plan_candidates(kind: ImportSourceKind, files: &ExistingFiles) -> Vec<ImportCandidate> {
    let versions = versions_in(files);
    let isolation = versions.len() > 1;

    versions
        .into_iter()
        .map(|(version_id, game_directory)| {
            ImportCandidate::planned(kind, isolation, game_directory, version_id)
        })
        .collect()
}

/// Detect and plan in one call.
#[must_use]
pub fn plan_import(files: &ExistingFiles) -> (Option<ImportSourceKind>, Vec<ImportCandidate>) {
    match detect(files) {
        Some(kind) => (Some(kind), plan_candidates(kind, files)),
        None => (None, Vec::new()),
    }
}

/// Scans `root` and returns a relative, `/`-separated snapshot of it.
///
/// Symbolic links are skipped, the walk stops at [`MAX_SCAN_DEPTH`] levels and
/// fails with [`ImportError::TooManyEntries`] past [`MAX_SCAN_ENTRIES`] so a
/// pathological tree cannot hang the caller.
pub fn scan_directory(root: &Path) -> Result<ExistingFiles, ImportError> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => return Err(ImportError::InvalidRoot),
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(ImportError::InvalidRoot),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ImportError::InvalidRoot);
        }
        Err(error) => return Err(ImportError::Io(error)),
    }

    let mut entries = Vec::new();
    let mut budget = MAX_SCAN_ENTRIES;
    walk(root, "", 0, &mut entries, &mut budget)?;

    Ok(ExistingFiles::new(entries))
}

/// Depth-first walk that appends `/`-separated relative paths.
fn walk(
    directory: &Path,
    prefix: &str,
    depth: usize,
    entries: &mut Vec<String>,
    budget: &mut usize,
) -> Result<(), ImportError> {
    if depth > MAX_SCAN_DEPTH {
        return Ok(());
    }

    for entry in fs::read_dir(directory).map_err(ImportError::Io)? {
        let entry = entry.map_err(ImportError::Io)?;
        if *budget == 0 {
            return Err(ImportError::TooManyEntries);
        }
        *budget -= 1;

        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }

        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };

        if metadata.is_dir() {
            entries.push(relative.clone());
            walk(&entry.path(), &relative, depth + 1, entries, budget)?;
        } else if metadata.is_file() {
            entries.push(relative);
        }
    }

    Ok(())
}

/// Finds every `versions/<id>/<id>.json` pair and the game directory it implies.
///
/// The returned game directory is always relative (`""` for `<root>` and
/// `".minecraft"` for `<root>/.minecraft`), which keeps the parser platform
/// independent and avoids leaking absolute paths into logs.
fn versions_in(files: &ExistingFiles) -> Vec<(String, PathBuf)> {
    let mut found: BTreeSet<(String, String)> = BTreeSet::new();

    for entry in &files.entries {
        let segments: Vec<&str> = entry.split('/').collect();
        if segments.len() < 3 {
            continue;
        }

        let file_name = segments[segments.len() - 1];
        let version_dir = segments[segments.len() - 2];
        let versions_dir = segments[segments.len() - 3];
        if versions_dir != "versions" {
            continue;
        }

        let expected = format!("{version_dir}.json");
        if file_name != expected {
            continue;
        }

        let game_directory = segments[..segments.len() - 3].join("/");
        found.insert((game_directory, version_dir.to_owned()));
    }

    found
        .into_iter()
        .map(|(game_directory, version_id)| (version_id, PathBuf::from(game_directory)))
        .collect()
}

/// Suggests a display name for an instance created from `version_id`.
fn display_name_for(version_id: &str) -> String {
    let trimmed = version_id.trim();
    if trimmed.is_empty() {
        "Imported instance".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Normalises a relative path to `/` separators without `.` segments.
fn normalise_relative(input: &str) -> String {
    let replaced = input.replace('\\', "/");
    let mut segments = Vec::new();

    for segment in replaced.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }

    segments.join("/")
}

/// Rejects an absolute path in a snapshot entry.
///
/// Exposed for the source modules so they never treat a hostile listing entry as
/// a path they may follow.
pub(crate) fn is_absolute_entry(entry: &str) -> bool {
    let path = Path::new(entry);
    path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
