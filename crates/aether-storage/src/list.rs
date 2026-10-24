//! Scanning the managed instance store.
//!
//! `list_instances` walks `<base>/instances/` and turns every trustworthy
//! `instance.json` into a [`GameInstance`]. One broken directory must never take
//! the whole listing down, so a record that cannot be trusted is dropped from
//! `instances` and reported in `skipped` with a stable, path-free reason category
//! instead of a raw file system error.
//!
//! Trust rules applied per entry:
//!
//! * only directories are considered, and symbolic links are never followed;
//! * the record must exist, be a plain file (not a link or a directory), and stay
//!   under [`MAX_INSTANCE_RECORD_BYTES`];
//! * the record must decode as a [`GameInstance`];
//! * the record must be self-consistent with its location: `id` must match the
//!   directory name (compared case-insensitively) and `root` must be that same
//!   directory once both sides are canonicalized.
//!
//! # Why only UUID-shaped directories are reported
//!
//! Because the reader is a plain scan of the store root, `skipped` only reports
//! entries whose name has the shape of an instance identifier (36 bytes: a UUID
//! `8-4-4-4-12` hexadecimal layout). Everything else that does not look like a
//! managed instance directory (`versions/`, editor backup folders, temporary
//! directories, stray files, the `instance.json.tmp` left behind by an
//! interrupted write) is ignored silently.
//!
//! This is a deliberate trade-off. A UUID-shaped directory is, by construction,
//! an instance this store claims to own: it was created by
//! [`crate::create_instance`] and it failed to load, which is something the user
//! can act on ("this instance is corrupt, repair or remove it"). Any other name is
//! a leftover the store never created, so listing it would only fill the UI with
//! entries the user cannot do anything about, and it would also make the store
//! root unusable for unrelated folders. Noise stays invisible; real damage is
//! always reported.
//!
//! The identifier is only a hint about the directory: a skipped entry is
//! identified by its directory name and may not be a valid
//! [`aether_core::InstanceId`] at all.

use std::fs;
use std::path::{Path, PathBuf};

use aether_core::GameInstance;
use thiserror::Error;

/// Directory under the store root that holds one subdirectory per instance.
///
/// Kept identical to `create::INSTANCES_DIR` (and to the name used by
/// `src-tauri/src/commands/instances.rs`); it is redefined here so that `list`
/// can be read on its own.
pub const INSTANCES_DIR: &str = "instances";

/// Record file name inside an instance directory.
pub const INSTANCE_RECORD_FILE: &str = "instance.json";

/// Hard cap for a single `instance.json` payload.
///
/// Kept identical to the `/instance.json` limit enforced by the trusted reader in
/// `src-tauri/src/commands/instances.rs`, and to
/// `update::MAX_INSTANCE_RECORD_BYTES`; a record the writer accepts must never be
/// rejected by the reader, and vice versa.
pub const MAX_INSTANCE_RECORD_BYTES: u64 = 128 * 1024;

/// Why a store entry was not turned into an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The entry is not a directory (for example a file or a junction-like
    /// object we refuse to inspect).
    NotADirectory,
    /// The entry is a symbolic link and was not followed.
    SymbolicLink,
    /// The directory has no `instance.json`.
    MissingRecord,
    /// The record is larger than [`MAX_INSTANCE_RECORD_BYTES`].
    RecordTooLarge,
    /// The record is a directory or a symbolic link instead of a plain file.
    UnsafeRecord,
    /// The record is not valid UTF-8 JSON for a [`GameInstance`].
    DecodeFailed,
    /// The record id does not match the directory name.
    IdMismatch,
    /// The record root does not point at the directory it was found in.
    RootMismatch,
}

impl SkipReason {
    /// Stable, path-free label for the UI and for logs.
    ///
    /// The strings are part of the IPC contract: the desktop shell forwards them
    /// verbatim into a `&'static str` field, so they must never change without
    /// changing the frontend at the same time.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotADirectory => "not-a-directory",
            Self::SymbolicLink => "symbolic-link",
            Self::MissingRecord => "missing-record",
            Self::RecordTooLarge => "record-too-large",
            Self::UnsafeRecord => "unsafe-record",
            Self::DecodeFailed => "decode-failed",
            Self::IdMismatch => "id-mismatch",
            Self::RootMismatch => "root-mismatch",
        }
    }
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One store entry that was skipped, identified by directory name only.
///
/// `reason` stores the stable label from [`SkipReason::as_str`] directly, so the
/// desktop shell can copy it into its own `&'static str` field without converting
/// it. It never contains a path or an `io::Error` message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedInstance {
    /// Directory name as found on disk. It is only a hint: a skipped record may
    /// not carry a valid [`aether_core::InstanceId`].
    pub id: String,
    /// Stable reason label, see [`SkipReason::as_str`].
    pub reason: &'static str,
}

/// Result of a listing pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstanceListing {
    /// Every record that passed all trust checks, sorted by name
    /// (case-insensitively).
    pub instances: Vec<GameInstance>,
    /// Entries that looked like instances but could not be loaded, sorted by id.
    pub skipped: Vec<SkippedInstance>,
}

/// Failures that abort the whole listing.
#[derive(Debug, Error)]
pub enum ListError {
    /// `<base>/instances` is missing, is not a directory, or is a symbolic link.
    #[error("instance store root is not a readable directory")]
    InvalidStore,
    /// A directory operation on the store root failed.
    #[error("cannot scan the instance store")]
    Io(#[source] std::io::Error),
}

/// Lists every instance stored under `base`.
///
/// `base` is the store root, the parent of `instances/`. Entries that fail any
/// trust check end up in [`InstanceListing::skipped`]; loaded instances are sorted
/// by name and skipped entries by directory name, so the output is deterministic.
///
/// # Errors
///
/// Returns [`ListError::InvalidStore`] when `<base>/instances` is missing, is not
/// a plain directory, or is a symbolic link, and [`ListError::Io`] when the store
/// root itself cannot be read. Individual broken entries never fail the call.
pub fn list_instances(base: &Path) -> Result<InstanceListing, ListError> {
    // The store is always addressed by joining components onto the caller's base,
    // never by building a path string.
    let instances_dir = base.join(INSTANCES_DIR);

    match fs::symlink_metadata(&instances_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => return Err(ListError::InvalidStore),
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err(ListError::InvalidStore),
        Err(error) if is_missing(&error) => return Err(ListError::InvalidStore),
        Err(error) => return Err(ListError::Io(error)),
    }

    let mut listing = InstanceListing::default();

    for entry in fs::read_dir(&instances_dir).map_err(ListError::Io)? {
        let entry = entry.map_err(ListError::Io)?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };

        match classify_entry(&instances_dir, &name, &entry) {
            EntryOutcome::Loaded(instance) => listing.instances.push(instance),
            EntryOutcome::Skipped(reason) => listing.skipped.push(SkippedInstance {
                id: name,
                reason: reason.as_str(),
            }),
            EntryOutcome::Ignored => {}
        }
    }

    listing.instances.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    listing
        .skipped
        .sort_by(|left, right| left.id.cmp(&right.id));

    Ok(listing)
}

/// Whether an error means "this path does not exist".
fn is_missing(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// Outcome of inspecting a single store entry.
enum EntryOutcome {
    Loaded(GameInstance),
    Skipped(SkipReason),
    Ignored,
}

/// Inspects one entry of `instances/`.
///
/// Entries that do not carry a UUID-shaped name are only ever `Ignored`; see the
/// module documentation for why.
fn classify_entry(instances_dir: &Path, name: &str, entry: &fs::DirEntry) -> EntryOutcome {
    let report = looks_like_instance_dir(name);

    // `symlink_metadata` never follows the entry, so a link to a good instance
    // outside the store is rejected instead of silently adopted.
    let metadata = match fs::symlink_metadata(entry.path()) {
        Ok(metadata) => metadata,
        Err(_) if report => return EntryOutcome::Skipped(SkipReason::MissingRecord),
        Err(_) => return EntryOutcome::Ignored,
    };

    if metadata.file_type().is_symlink() {
        return if report {
            EntryOutcome::Skipped(SkipReason::SymbolicLink)
        } else {
            EntryOutcome::Ignored
        };
    }

    if !metadata.is_dir() {
        return if report {
            EntryOutcome::Skipped(SkipReason::NotADirectory)
        } else {
            EntryOutcome::Ignored
        };
    }

    // The record is inspected only for instance-shaped directories: any other
    // directory is a leftover and must not even be opened.
    if !report {
        return EntryOutcome::Ignored;
    }

    match load_record(instances_dir, name) {
        Ok(instance) => EntryOutcome::Loaded(instance),
        Err(reason) => EntryOutcome::Skipped(reason),
    }
}

/// Loads and validates `<instances_dir>/<name>/instance.json`.
fn load_record(instances_dir: &Path, name: &str) -> Result<GameInstance, SkipReason> {
    let directory = instances_dir.join(name);
    let record_path = directory.join(INSTANCE_RECORD_FILE);

    // The record is inspected without following links: a symlinked or otherwise
    // non-plain record cannot be trusted, whatever it points at.
    let metadata = fs::symlink_metadata(&record_path).map_err(|error| {
        if is_missing(&error) {
            SkipReason::MissingRecord
        } else {
            SkipReason::UnsafeRecord
        }
    })?;

    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SkipReason::UnsafeRecord);
    }
    if metadata.len() > MAX_INSTANCE_RECORD_BYTES {
        return Err(SkipReason::RecordTooLarge);
    }

    let raw = fs::read_to_string(&record_path).map_err(|_| SkipReason::DecodeFailed)?;
    let instance = crate::decode_instance(&raw).map_err(|_| SkipReason::DecodeFailed)?;

    // `InstanceId` is a transparent UUID newtype with no accessor for its text,
    // so the record is compared through its serialized form: the same canonical
    // spelling that `create`/`update` write into the directory name.
    let recorded_id = serde_json::to_value(instance.id)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or(SkipReason::IdMismatch)?;
    if !recorded_id.eq_ignore_ascii_case(name) {
        return Err(SkipReason::IdMismatch);
    }

    if !same_directory(&instance.root, &directory) {
        return Err(SkipReason::RootMismatch);
    }

    Ok(instance)
}

/// Compares a stored root with the directory it was found in.
///
/// The record stores a canonical path, so the comparison canonicalizes the
/// directory first. When canonicalization fails (the directory vanished, or a
/// permission problem) the record cannot be proven to belong here and the
/// comparison reports a mismatch.
fn same_directory(stored: &Path, directory: &Path) -> bool {
    match fs::canonicalize(directory) {
        Ok(canonical) => paths_equal(stored, &canonical),
        Err(_) => false,
    }
}

/// Compares two paths after normalizing the Windows verbatim prefix.
///
/// Both sides are normalized. `Path::eq` compares components, so a stored
/// `C:\store\instances\<uuid>` and a canonicalized `\\?\C:\store\instances\<uuid>`
/// would be different paths on Windows even though they name the same directory;
/// stripping the prefix on both sides makes the comparison work on every
/// platform.
fn paths_equal(left: &Path, right: &Path) -> bool {
    strip_verbatim(left) == strip_verbatim(right)
}

/// Removes a leading `\\?\` verbatim prefix from a path, if present.
///
/// `\\?\UNC\server\share\...` is folded back to the equivalent
/// `\\server\share\...` form. A path without a prefix is returned unchanged.
fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let rest = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        return path.to_path_buf();
    };

    // Prefer a plain string so nothing is normalized away on platforms where the
    // verbatim spelling is not a real path (for example `\\?\C:\...` on Unix).
    match PathBuf::from(&rest) {
        plain if plain.as_os_str().len() == rest.len() => plain,
        _ => PathBuf::from(rest.as_str()),
    }
}

/// Whether a directory name could plausibly be an instance directory.
///
/// Only UUID-shaped names are reported when they fail to load; see the module
/// documentation for the rationale. The check works on bytes, so it is
/// independent of the case the identifier was written in.
fn looks_like_instance_dir(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() != 36 {
        return false;
    }

    bytes.iter().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => *byte == b'-',
        _ => byte.is_ascii_hexdigit(),
    })
}

#[cfg(test)]
#[path = "list/list_tests.rs"]
mod tests;
