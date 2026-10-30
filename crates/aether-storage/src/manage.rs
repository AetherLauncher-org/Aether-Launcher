//! Managing the files of one instance: what is installed in it, switching and removing
//! individual items, reading its logs, and deleting the instance.
//!
//! Everything here works from an [`InstanceStore`]-verified instance and a closed set of
//! folders inside its `minecraft/` directory. Callers name a [`ContentKind`] or a
//! [`LogSource`] and, for a single item, a plain file or folder name; they never supply a
//! path. A name must be exactly one ordinary path component ([`is_plain_name`]), the item
//! must sit directly in the folder, and links are neither listed nor followed, so nothing
//! outside the instance can be reached through this module.
//!
//! The contents of mods, packs and worlds are untrusted data and are never executed or
//! opened here: a listing only reads file names, sizes and (for worlds) whether `level.dat`
//! exists. Every walk is bounded in depth and entries.

use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use aether_core::{GameInstance, InstanceId};
use thiserror::Error;

use crate::create::{INSTANCES_DIR, MINECRAFT_DIR};
use crate::instances::{InstanceLoadError, InstanceStore};

/// The most entries one listing returns. A folder with more than this is cut off.
pub const MAX_LISTED_ENTRIES: usize = 2000;
/// The most files and folders counted when measuring a directory's size.
const MAX_SIZE_WALK_ENTRIES: usize = 200_000;
const MAX_SIZE_WALK_DEPTH: usize = 40;
/// The most bytes of a log that are returned (the end of the file).
pub const MAX_LOG_BYTES: u64 = 256 * 1024;
/// The most log files listed.
const MAX_LOG_FILES: usize = 200;

/// The suffix that switches a mod off without deleting it.
const DISABLED_SUFFIX: &str = ".disabled";

/// Failures when managing an instance's files.
#[derive(Debug, Error)]
pub enum ManageError {
    #[error("that is not a plain file or folder name")]
    InvalidName,
    #[error("that item does not exist")]
    NotFound,
    #[error("that item cannot be changed that way")]
    NotAllowed,
    #[error("a file with the new name already exists")]
    AlreadyExists,
    #[error("the instance is not where the store expects it")]
    UnsafeInstance,
    #[error(transparent)]
    Load(#[from] InstanceLoadError),
    #[error("cannot access the instance files: {0}")]
    Io(#[from] io::Error),
}

// --------------------------------------------------------------------------------------
// Where things are
// --------------------------------------------------------------------------------------

/// The instance's own game directory (`<instance>/minecraft`).
#[must_use]
pub fn game_directory(instance: &GameInstance) -> PathBuf {
    instance.root.join(MINECRAFT_DIR)
}

/// The kinds of content a game directory holds that the launcher can list and manage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentKind {
    Mods,
    ResourcePacks,
    ShaderPacks,
    Saves,
    Schematics,
}

impl ContentKind {
    pub const ALL: [Self; 5] = [
        Self::Mods,
        Self::ResourcePacks,
        Self::ShaderPacks,
        Self::Saves,
        Self::Schematics,
    ];

    /// The folder inside the game directory.
    #[must_use]
    pub fn folder(self) -> &'static str {
        match self {
            Self::Mods => "mods",
            Self::ResourcePacks => "resourcepacks",
            Self::ShaderPacks => "shaderpacks",
            Self::Saves => "saves",
            Self::Schematics => "schematics",
        }
    }

    /// The stable label used across IPC.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Mods => "mods",
            Self::ResourcePacks => "resourcepacks",
            Self::ShaderPacks => "shaderpacks",
            Self::Saves => "saves",
            Self::Schematics => "schematics",
        }
    }

    /// Parses an IPC label; anything else is `None`.
    #[must_use]
    pub fn parse(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.label() == label)
    }
}

/// `true` for one ordinary file or folder name: not empty, not a path, not `.`/`..`, no
/// control characters, no `:` (which names an alternate data stream on Windows), no trailing
/// dot or space, and short enough for any file system.
#[must_use]
pub fn is_plain_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 255
        || name.ends_with('.')
        || name.ends_with(' ')
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return false;
    }
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

// --------------------------------------------------------------------------------------
// Listing
// --------------------------------------------------------------------------------------

/// One item in a content folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentEntry {
    /// The file or folder name, exactly as on disk (a switched-off mod keeps `.disabled`).
    pub name: String,
    /// Bytes on disk; for a folder the sum of what a bounded walk found.
    pub size: u64,
    /// `false` only for a mod that is switched off.
    pub enabled: bool,
    pub is_folder: bool,
}

/// Lists one kind of content. A missing folder is an empty list. Links, odd names and
/// anything that is not the right kind of item for the folder are left out.
///
/// # Errors
///
/// [`ManageError::Io`] when the folder exists but cannot be read.
pub fn list_content(game_dir: &Path, kind: ContentKind) -> Result<Vec<ContentEntry>, ManageError> {
    let folder = game_dir.join(kind.folder());
    let reader = match fs::read_dir(&folder) {
        Ok(reader) => reader,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut entries = Vec::new();
    for item in reader.flatten() {
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_plain_name(&name) {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(item.path()) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        let is_folder = metadata.is_dir();
        if !is_listed(kind, &name, is_folder, &item.path()) {
            continue;
        }
        let size = if is_folder {
            directory_size(&item.path())
        } else {
            metadata.len()
        };
        entries.push(ContentEntry {
            enabled: !(kind == ContentKind::Mods && name.ends_with(DISABLED_SUFFIX)),
            name,
            size,
            is_folder,
        });
        if entries.len() >= MAX_LISTED_ENTRIES {
            break;
        }
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    Ok(entries)
}

/// How many items of a kind there are, without measuring any of them.
#[must_use]
pub fn count_content(game_dir: &Path, kind: ContentKind) -> usize {
    let Ok(reader) = fs::read_dir(game_dir.join(kind.folder())) else {
        return 0;
    };
    reader
        .flatten()
        .filter(|item| {
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                return false;
            };
            let Ok(metadata) = fs::symlink_metadata(item.path()) else {
                return false;
            };
            is_plain_name(&name)
                && !metadata.file_type().is_symlink()
                && is_listed(kind, &name, metadata.is_dir(), &item.path())
        })
        .take(MAX_LISTED_ENTRIES)
        .count()
}

fn extension_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Whether an item of this name and shape belongs in a folder of this kind.
fn is_listed(kind: ContentKind, name: &str, is_folder: bool, path: &Path) -> bool {
    match kind {
        ContentKind::Mods => {
            !is_folder && {
                let active = name.strip_suffix(DISABLED_SUFFIX).unwrap_or(name);
                extension_of(active) == "jar"
            }
        }
        // A pack is a zip or an unpacked folder; shaders are the same.
        ContentKind::ResourcePacks | ContentKind::ShaderPacks => {
            is_folder || extension_of(name) == "zip"
        }
        // A world is a folder with a level.dat in it.
        ContentKind::Saves => is_folder && path.join("level.dat").is_file(),
        ContentKind::Schematics => {
            !is_folder
                && matches!(
                    extension_of(name).as_str(),
                    "litematic" | "schem" | "schematic" | "nbt"
                )
        }
    }
}

/// The size of a directory: files only, links skipped, bounded in depth and entries. A
/// directory larger than the bounds is reported as what was counted so far.
#[must_use]
pub fn directory_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut seen = 0usize;
    let mut stack = vec![(path.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = stack.pop() {
        let Ok(reader) = fs::read_dir(&directory) else {
            continue;
        };
        for item in reader.flatten() {
            seen += 1;
            if seen > MAX_SIZE_WALK_ENTRIES {
                return total;
            }
            let Ok(metadata) = fs::symlink_metadata(item.path()) else {
                continue;
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                if depth < MAX_SIZE_WALK_DEPTH {
                    stack.push((item.path(), depth + 1));
                }
            } else {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

// --------------------------------------------------------------------------------------
// Changing
// --------------------------------------------------------------------------------------

/// Resolves a plain name inside a kind's folder to an existing, non-link item.
fn existing_item(game_dir: &Path, kind: ContentKind, name: &str) -> Result<PathBuf, ManageError> {
    if !is_plain_name(name) {
        return Err(ManageError::InvalidName);
    }
    let path = game_dir.join(kind.folder()).join(name);
    let metadata = fs::symlink_metadata(&path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => ManageError::NotFound,
        _ => ManageError::Io(error),
    })?;
    if metadata.file_type().is_symlink() {
        return Err(ManageError::NotAllowed);
    }
    Ok(path)
}

/// Switches a mod on or off by renaming `x.jar` to `x.jar.disabled` and back.
///
/// # Errors
///
/// [`ManageError::NotAllowed`] for anything that is not a `.jar` / `.jar.disabled` file;
/// [`ManageError::AlreadyExists`] when the other name is taken.
pub fn set_mod_enabled(game_dir: &Path, name: &str, enabled: bool) -> Result<(), ManageError> {
    let path = existing_item(game_dir, ContentKind::Mods, name)?;
    if !path.is_file() {
        return Err(ManageError::NotAllowed);
    }
    let currently_enabled = !name.ends_with(DISABLED_SUFFIX);
    if currently_enabled == enabled {
        return Ok(());
    }
    let target_name = if enabled {
        name.strip_suffix(DISABLED_SUFFIX)
            .unwrap_or(name)
            .to_owned()
    } else {
        format!("{name}{DISABLED_SUFFIX}")
    };
    if extension_of(
        target_name
            .strip_suffix(DISABLED_SUFFIX)
            .unwrap_or(&target_name),
    ) != "jar"
    {
        return Err(ManageError::NotAllowed);
    }
    let target = path.with_file_name(&target_name);
    if fs::symlink_metadata(&target).is_ok() {
        return Err(ManageError::AlreadyExists);
    }
    fs::rename(&path, &target)?;
    Ok(())
}

/// Deletes one item (a file, or a folder with everything in it) from a content folder.
///
/// # Errors
///
/// [`ManageError::InvalidName`], [`ManageError::NotFound`] and [`ManageError::NotAllowed`]
/// (a link) before anything is touched.
pub fn remove_content(game_dir: &Path, kind: ContentKind, name: &str) -> Result<(), ManageError> {
    let path = existing_item(game_dir, kind, name)?;
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.is_dir() {
        fs::remove_dir_all(&path)?;
    } else {
        fs::remove_file(&path)?;
    }
    Ok(())
}

/// Deletes an instance: its whole directory under `<base>/instances/<uuid>`.
///
/// The instance is loaded through the [`InstanceStore`] first, which checks that the record
/// and its location agree, and the directory must be a real directory (not a link) that is
/// exactly `<base>/instances/<uuid>`. The shared game files (`versions`, `libraries`,
/// `assets`) are not in that directory and are left alone. The caller makes sure the
/// instance is not running.
///
/// # Errors
///
/// [`ManageError::Load`] when the instance cannot be verified, [`ManageError::UnsafeInstance`]
/// when its directory is not where the store puts it.
pub fn delete_instance(base: &Path, id: InstanceId) -> Result<(), ManageError> {
    let instance = InstanceStore::open(base)?.load(id)?;
    let expected = fs::canonicalize(base)?
        .join(INSTANCES_DIR)
        .join(id.to_string());
    let actual = fs::canonicalize(&instance.root)?;
    let is_link = fs::symlink_metadata(&actual).is_ok_and(|m| m.file_type().is_symlink());
    if is_link || !same_path(&actual, &expected) {
        return Err(ManageError::UnsafeInstance);
    }
    fs::remove_dir_all(&actual)?;
    Ok(())
}

/// Compares two canonical paths, ignoring ASCII case (a UUID directory may be written in
/// either case) and the `\\?\` prefix Windows adds.
fn same_path(a: &Path, b: &Path) -> bool {
    let normal = |path: &Path| {
        path.to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_ascii_lowercase()
    };
    normal(a) == normal(b)
}

// --------------------------------------------------------------------------------------
// Logs
// --------------------------------------------------------------------------------------

/// Which log to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogSource {
    /// `logs/latest.log`, the current or last session.
    Latest,
    /// A file in `crash-reports/`, by plain name.
    CrashReport(String),
}

/// A log file the launcher can show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFile {
    pub source: LogSource,
    pub name: String,
    pub size: u64,
    /// Seconds since the Unix epoch, when the file system reports it.
    pub modified: Option<u64>,
}

fn modified_seconds(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs())
}

/// The latest log and the crash reports, newest first (crash reports) after the latest log.
///
/// # Errors
///
/// [`ManageError::Io`] when a folder exists but cannot be read.
pub fn list_logs(game_dir: &Path) -> Result<Vec<LogFile>, ManageError> {
    let mut files = Vec::new();
    if let Ok(metadata) = fs::symlink_metadata(game_dir.join("logs").join("latest.log"))
        && metadata.is_file()
    {
        files.push(LogFile {
            source: LogSource::Latest,
            name: "latest.log".to_owned(),
            size: metadata.len(),
            modified: modified_seconds(&metadata),
        });
    }

    let mut crashes = Vec::new();
    match fs::read_dir(game_dir.join("crash-reports")) {
        Ok(reader) => {
            for item in reader.flatten() {
                let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                let Ok(metadata) = fs::symlink_metadata(item.path()) else {
                    continue;
                };
                if is_plain_name(&name) && metadata.is_file() && extension_of(&name) == "txt" {
                    crashes.push(LogFile {
                        source: LogSource::CrashReport(name.clone()),
                        name,
                        size: metadata.len(),
                        modified: modified_seconds(&metadata),
                    });
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    crashes.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| b.name.cmp(&a.name))
    });
    crashes.truncate(MAX_LOG_FILES);
    files.extend(crashes);
    Ok(files)
}

/// A piece of a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogText {
    pub text: String,
    /// `true` when the file is longer than what is returned (the start was cut off).
    pub truncated: bool,
}

/// Reads the end of a log, at most [`MAX_LOG_BYTES`], as text. Bytes that are not UTF-8 are
/// replaced, and when the start was cut off the first, partial line is dropped.
///
/// # Errors
///
/// [`ManageError::NotFound`] when the log does not exist; [`ManageError::InvalidName`] for
/// a crash report name that is not plain.
pub fn read_log(game_dir: &Path, source: &LogSource) -> Result<LogText, ManageError> {
    let path = match source {
        LogSource::Latest => game_dir.join("logs").join("latest.log"),
        LogSource::CrashReport(name) => {
            if !is_plain_name(name) || extension_of(name) != "txt" {
                return Err(ManageError::InvalidName);
            }
            game_dir.join("crash-reports").join(name)
        }
    };
    let metadata = fs::symlink_metadata(&path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => ManageError::NotFound,
        _ => ManageError::Io(error),
    })?;
    if !metadata.is_file() {
        return Err(ManageError::NotAllowed);
    }
    let length = metadata.len();
    let truncated = length > MAX_LOG_BYTES;
    let mut file = fs::File::open(&path)?;
    if truncated {
        file.seek(SeekFrom::Start(length - MAX_LOG_BYTES))?;
    }
    let mut bytes = Vec::new();
    file.take(MAX_LOG_BYTES).read_to_end(&mut bytes)?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if truncated && let Some(newline) = text.find('\n') {
        text.drain(..=newline);
    }
    Ok(LogText { text, truncated })
}

#[cfg(test)]
#[path = "manage_tests.rs"]
mod tests;
