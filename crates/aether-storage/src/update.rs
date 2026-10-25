//! Atomic updates of a single instance record.
//!
//! The on-disk layout, fixed by PR #12, is `<store>/instances/<uuid>/instance.json`.
//! Every record write in this crate goes through [`write_instance_atomic`], so a
//! crash in the middle of an update can only ever leave the previous record or
//! the new one behind, never a truncated file:
//!
//! 1. the new bytes are written to `instance.json.tmp` in the same directory,
//! 2. the descriptor is flushed,
//! 3. the temporary file is renamed over `instance.json`.
//!
//! `fs::rename` inside one directory is atomic on NTFS and on the common Unix
//! file systems, and the temporary name is stable, so leftovers from an
//! interrupted run are simply reused by the next attempt.
//!
//! # Symbolic links
//!
//! This module never follows a link it could be redirected through. The instance
//! directory and the temporary file are inspected with [`fs::symlink_metadata`],
//! and a link is reported as [`UpdateError::UnsafeDirectory`] instead of being
//! written through. The temporary name needs that check because `File::create`
//! follows links and truncates its target.
//!
//! A link at `instance.json` itself is a different case and is *not* dangerous
//! for the write: `fs::rename` replaces the directory entry, so a link there is
//! unlinked rather than followed and its target is never touched. The read pass
//! of `update_record` still refuses such a record, because it would be reading
//! content from wherever the link points, which may describe another instance
//! entirely.
//!
//! # Identity of the directory
//!
//! [`rename_instance`], [`set_instance_state`] and [`set_instance_version`] treat
//! the stored record as the source of truth: it is read, checked against both the
//! directory it was found in and the caller's [`GameInstance`] handle, and only
//! then written back. A stale handle can therefore never rewrite the identity of
//! a record that now describes something else.

use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use aether_core::{GameInstance, InstanceState, VersionId};
use thiserror::Error;

use crate::decode_instance;

/// File name of the persisted instance record inside the instance directory.
pub const INSTANCE_RECORD_FILE: &str = "instance.json";

/// Temporary file name used for the atomic replace.
pub const INSTANCE_RECORD_TEMP_FILE: &str = "instance.json.tmp";

/// Hard cap for a single `instance.json` payload.
///
/// Kept identical to `list::MAX_INSTANCE_RECORD_BYTES` and to the
/// `/instance.json` limit enforced by the trusted reader in PR #12
/// (`instances.rs`); it is redefined here because that file is owned by PR #12.
///
/// The cap is enforced in both directions: [`write_instance_atomic`] refuses to
/// encode a larger record, and the read pass of `update_record` refuses to load
/// one, so an oversized file on disk is never read into memory in full.
pub const MAX_INSTANCE_RECORD_BYTES: u64 = 128 * 1024;

/// Upper bound for an instance display name, counted in characters.
pub const MAX_INSTANCE_NAME_CHARS: usize = 64;

/// Upper bound for a Minecraft version identifier, counted in characters.
pub const MAX_VERSION_ID_CHARS: usize = 64;

/// Characters that are illegal in a Windows file name and that would also allow
/// escaping the instance directory on any platform.
const FORBIDDEN_NAME_CHARS: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Device names that Windows reserves in every directory.
///
/// The list covers the classic MS-DOS device names plus the two console handles
/// Windows still resolves. It deliberately stops there: `COM10` and above are
/// ordinary file names on current Windows, and the superscript variants
/// (`COM¹`) are not folded by the ASCII lowercase that [`validate_instance_name`]
/// applies, so they are not claimed here.
const RESERVED_DEVICE_NAMES: [&str; 26] = [
    "con", "prn", "aux", "nul", "conin$", "conout$", "com0", "com1", "com2", "com3", "com4",
    "com5", "com6", "com7", "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6",
    "lpt7", "lpt8", "lpt9",
];

/// Failures when rewriting an instance record.
#[derive(Debug, Error)]
pub enum UpdateError {
    /// The target is not an existing plain directory: missing, a file, or a
    /// symbolic link that would redirect the write somewhere else.
    ///
    /// Also reported when the directory holds a record that does not belong to
    /// the instance the caller asked to update; see `update_record`.
    #[error("instance directory is not a safe existing directory")]
    UnsafeDirectory,
    /// The requested display name or version identifier is not acceptable.
    #[error("invalid instance field: {0}")]
    InvalidName(String),
    /// The encoded record or the stored record exceeds the size limit.
    #[error("instance record exceeds the size limit")]
    TooLarge,
    /// The record could not be encoded or decoded.
    #[error("failed to handle the instance record")]
    Serialize(#[source] crate::StorageError),
    /// Reading, writing or renaming the record failed.
    #[error("instance record update failed")]
    Io(#[source] std::io::Error),
}

/// Validates a user-supplied instance display name.
///
/// Rules: not empty after trimming, at most [`MAX_INSTANCE_NAME_CHARS`]
/// characters, no `\ / : * ? " < > |`, no control characters, not `.` or `..`,
/// must not end with a dot or a space (Windows silently strips those), and must
/// not be a Windows device name such as `CON`, `NUL` or `COM1` — including the
/// `CON.txt` form.
///
/// The length limit counts characters rather than bytes, so a name written in any
/// script gets the same allowance as an ASCII one. Control characters are detected
/// with [`char::is_control`], which already covers the C1 range U+0080–U+009F and
/// not just the ASCII control characters.
pub fn validate_instance_name(name: &str) -> Result<(), UpdateError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(UpdateError::InvalidName(
            "name must not be empty".to_owned(),
        ));
    }

    if name.chars().count() > MAX_INSTANCE_NAME_CHARS {
        return Err(UpdateError::InvalidName(format!(
            "name must be at most {MAX_INSTANCE_NAME_CHARS} characters"
        )));
    }

    if let Some(bad) = name.chars().find(|c| FORBIDDEN_NAME_CHARS.contains(c)) {
        return Err(UpdateError::InvalidName(format!(
            "name must not contain {bad:?}"
        )));
    }

    if name.chars().any(char::is_control) {
        return Err(UpdateError::InvalidName(
            "name must not contain control characters".to_owned(),
        ));
    }

    if trimmed == "." || trimmed == ".." {
        return Err(UpdateError::InvalidName(
            "name must not be a relative path segment".to_owned(),
        ));
    }

    if name.ends_with('.') || name.ends_with(' ') {
        return Err(UpdateError::InvalidName(
            "name must not end with a dot or a space".to_owned(),
        ));
    }

    // Windows resolves `CON`, `CON.txt` and `con .txt` to the device itself.
    let stem = trimmed
        .split_once('.')
        .map_or(trimmed, |(stem, _)| stem)
        .trim_end()
        .to_ascii_lowercase();
    if RESERVED_DEVICE_NAMES.contains(&stem.as_str()) {
        return Err(UpdateError::InvalidName(format!(
            "name must not be the reserved device name {stem}"
        )));
    }

    Ok(())
}

/// Validates a Minecraft version identifier used by an instance.
///
/// The identifier is stored verbatim, so it must not be able to reach outside the
/// instance directory: no separator of either kind, bounded length, no control
/// characters. Whether the version actually exists is a question for the manifest
/// layer, not for this one.
pub fn validate_version_id(version: &str) -> Result<(), UpdateError> {
    if version.trim().is_empty() {
        return Err(UpdateError::InvalidName(
            "version must not be empty".to_owned(),
        ));
    }

    if version.chars().count() > MAX_VERSION_ID_CHARS {
        return Err(UpdateError::InvalidName(format!(
            "version must be at most {MAX_VERSION_ID_CHARS} characters"
        )));
    }

    if let Some(bad) = version.chars().find(|c| matches!(c, '\\' | '/')) {
        return Err(UpdateError::InvalidName(format!(
            "version must not contain the path separator {bad:?}"
        )));
    }

    if version.chars().any(char::is_control) {
        return Err(UpdateError::InvalidName(
            "version must not contain control characters".to_owned(),
        ));
    }

    Ok(())
}

/// Verifies that `directory` is an existing plain directory, not a link.
fn ensure_safe_directory(directory: &Path) -> Result<(), UpdateError> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(UpdateError::UnsafeDirectory),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(UpdateError::UnsafeDirectory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(UpdateError::UnsafeDirectory)
        }
        Err(error) => Err(UpdateError::Io(error)),
    }
}

/// Verifies that the stable temporary name is ours to use.
///
/// `File::create` truncates whatever it can open, and opening follows links on
/// both Windows and Unix, so a pre-existing `instance.json.tmp` that is a symbolic
/// link — or a directory, or any other object that is not a regular file — would
/// make the write land somewhere it does not belong. Such an object is refused and
/// deliberately left in place: this module removes only files it wrote itself, and
/// a foreign object at the temporary name is something the user has to resolve.
///
/// A plain file is fine: it is the leftover of an interrupted run and is reused on
/// purpose, which is what makes the recovery described in the module docs work.
fn ensure_safe_temp(temp: &Path) -> Result<(), UpdateError> {
    match fs::symlink_metadata(temp) {
        // `symlink_metadata` does not follow, so `is_file` is already false for a
        // link, a directory or a device.
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(UpdateError::UnsafeDirectory),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::Io(error)),
    }
}

/// Serializes `instance` and replaces `<directory>/instance.json` atomically.
///
/// The directory must already exist and must not be a symbolic link. On failure
/// the previous record stays untouched and the temporary file is removed.
///
/// # Errors
///
/// * [`UpdateError::UnsafeDirectory`] when `directory` is missing, is not a plain
///   directory, or when `instance.json.tmp` is occupied by a link or another
///   non-file object;
/// * [`UpdateError::Serialize`] when the record cannot be encoded;
/// * [`UpdateError::TooLarge`] when the encoded record exceeds
///   [`MAX_INSTANCE_RECORD_BYTES`];
/// * [`UpdateError::Io`] when writing or renaming fails.
///
/// `TooLarge` is defence in depth rather than a routine outcome: a name that
/// passed [`validate_instance_name`] is at most [`MAX_INSTANCE_NAME_CHARS`]
/// characters and a version at most [`MAX_VERSION_ID_CHARS`], so a record built
/// from validated fields is a few hundred bytes and can never approach the cap.
/// The check only fires for a [`GameInstance`] that was never validated — and then
/// it fires before anything is written, so the previous record and any temporary
/// file are left untouched.
pub fn write_instance_atomic(directory: &Path, instance: &GameInstance) -> Result<(), UpdateError> {
    ensure_safe_directory(directory)?;

    let encoded = crate::encode_instance(instance).map_err(UpdateError::Serialize)?;
    if encoded.len() as u64 > MAX_INSTANCE_RECORD_BYTES {
        return Err(UpdateError::TooLarge);
    }

    let record = directory.join(INSTANCE_RECORD_FILE);
    let temp = directory.join(INSTANCE_RECORD_TEMP_FILE);

    // Refuse an object we did not create at the temporary name before opening it:
    // a leftover plain file is truncated and rewritten, anything else is somebody
    // else's problem.
    ensure_safe_temp(&temp)?;

    write_flushing(&temp, encoded.as_bytes()).inspect_err(|_| {
        let _ = fs::remove_file(&temp);
    })?;

    // The rename cannot cross a device boundary: both names were derived from
    // `directory`, so a failure here is a real file system error (destination
    // vanished, permissions changed) and not the cross-device case that would
    // need a copy fallback.
    if let Err(error) = fs::rename(&temp, &record) {
        let _ = fs::remove_file(&temp);
        return Err(UpdateError::Io(error));
    }

    Ok(())
}

/// Writes `bytes` to `path` and flushes them to the storage device before the
/// rename; otherwise a power loss could publish an empty file.
///
/// The caller has already rejected a link at `path` with [`ensure_safe_temp`], and
/// the file is opened with `create_new(false)` plus `truncate`, so the remaining
/// exposure is a narrow window that Windows cannot close portably (there is no
/// `O_NOFOLLOW` to pass to the open call). An object planted inside that window is
/// caught by the next attempt, which repeats the same check; it can never be
/// followed, because the check rejects every non-regular file first.
fn write_flushing(path: &Path, bytes: &[u8]) -> Result<(), UpdateError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(UpdateError::Io)?;
    file.write_all(bytes).map_err(UpdateError::Io)?;
    file.sync_all().map_err(UpdateError::Io)
}

/// Reads and decodes the record stored in `directory`.
///
/// The size cap is applied twice: once to the directory entry and once to the
/// bytes that are actually read. The metadata check alone is advisory — the file
/// can grow between the two calls, and a network share can report a size it does
/// not honour — so at most [`MAX_INSTANCE_RECORD_BYTES`] plus one byte are ever
/// pulled into memory.
///
/// A missing record is reported as [`UpdateError::Io`]: the caller asked to update
/// an instance whose record is not there, which is different from a record that
/// exists but cannot be trusted.
fn read_record(directory: &Path) -> Result<GameInstance, UpdateError> {
    let record = directory.join(INSTANCE_RECORD_FILE);

    let metadata = fs::symlink_metadata(&record).map_err(UpdateError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(UpdateError::UnsafeDirectory);
    }
    if metadata.len() > MAX_INSTANCE_RECORD_BYTES {
        return Err(UpdateError::TooLarge);
    }

    let file = fs::File::open(&record).map_err(UpdateError::Io)?;
    let mut raw = String::new();
    file.take(MAX_INSTANCE_RECORD_BYTES.saturating_add(1))
        .read_to_string(&mut raw)
        .map_err(UpdateError::Io)?;

    // Reached when the record grew after the metadata check, or when it is exactly
    // one byte over the limit; either way it must not be decoded.
    if raw.len() as u64 > MAX_INSTANCE_RECORD_BYTES {
        return Err(UpdateError::TooLarge);
    }

    decode_instance(&raw).map_err(UpdateError::Serialize)
}

/// Applies `edit` to the stored record and writes the result back atomically.
///
/// The record on disk is the source of truth, so a stale in-memory copy can never
/// resurrect an old name, state or version. Two checks run before `edit`:
///
/// * the stored record must carry the same identifier as the caller's `instance`;
/// * its `root` must resolve to `directory`, compared canonically and tolerant of
///   the Windows `\\?\` verbatim prefix.
///
/// Together they prove that this directory really holds the instance the caller
/// named, so a handle pointing at another instance — or at a record copied in from
/// somewhere else, which keeps a consistent `id` but a foreign root — is rejected
/// instead of silently rewriting an identity.
///
/// # Why `UnsafeDirectory` and not a sharper variant
///
/// A mismatch means "this directory is not a safe target for the write you asked
/// for", which is exactly what [`UpdateError::UnsafeDirectory`] promises, and the
/// error set is part of the public contract the IPC layer matches on. A dedicated
/// variant would read better but would change that contract for a case the UI
/// cannot act on differently anyway.
fn update_record(
    directory: &Path,
    instance: &GameInstance,
    edit: impl FnOnce(&mut GameInstance) -> Result<(), UpdateError>,
) -> Result<GameInstance, UpdateError> {
    ensure_safe_directory(directory)?;

    let mut stored = read_record(directory)?;
    ensure_record_identity(directory, &stored, instance)?;
    edit(&mut stored)?;
    write_instance_atomic(directory, &stored)?;

    Ok(stored)
}

/// Verifies that the record read from `directory` belongs here and to `instance`.
///
/// The identifier is an exact, cheap comparison: [`GameInstance::id`] is a UUID
/// newtype without a text form to normalize. The root comparison catches what the
/// identifier cannot — a record copied verbatim into another directory keeps a
/// perfectly consistent `id` while pointing at the original instance.
fn ensure_record_identity(
    directory: &Path,
    stored: &GameInstance,
    instance: &GameInstance,
) -> Result<(), UpdateError> {
    if stored.id != instance.id {
        return Err(UpdateError::UnsafeDirectory);
    }

    if !same_directory(&stored.root, directory) {
        return Err(UpdateError::UnsafeDirectory);
    }

    Ok(())
}

/// Compares a stored root with the directory the record was read from.
///
/// The record stores a canonical path, so the directory is canonicalized first.
/// When canonicalization fails — the directory vanished, or there is a permission
/// problem — the record cannot be proven to belong here and the comparison reports
/// a mismatch, which the caller turns into [`UpdateError::UnsafeDirectory`].
fn same_directory(stored: &Path, directory: &Path) -> bool {
    match fs::canonicalize(directory) {
        Ok(canonical) => paths_equal(stored, &canonical),
        Err(_) => false,
    }
}

/// Compares two paths after normalizing the Windows verbatim prefix.
///
/// `Path::eq` compares components, so a stored `C:\store\instances\<uuid>` and a
/// canonicalized `\\?\C:\store\instances\<uuid>` would be different paths on
/// Windows even though they name the same directory; stripping the prefix on both
/// sides makes the comparison work on every platform.
fn paths_equal(left: &Path, right: &Path) -> bool {
    strip_verbatim(left) == strip_verbatim(right)
}

/// Removes a leading `\\?\` verbatim prefix from a path, if present.
///
/// `\\?\UNC\server\share\...` is folded back to the equivalent
/// `\\server\share\...` form. A path without a prefix is returned unchanged.
///
/// This mirrors the helper in `list` instead of importing it: the two modules check
/// the same invariant from opposite ends, and neither should depend on the private
/// items of the other.
fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

/// Renames the instance stored in `directory`.
///
/// `instance` identifies the record to rewrite: the stored record must carry the
/// same identifier and must point at `directory` (see `update_record`), so a
/// stale handle is rejected rather than used to relabel a different instance.
///
/// Only the `name` is replaced; `id`, `root`, `version` and `state` are taken from
/// the record on disk, so fields written by another run are preserved.
///
/// # Errors
///
/// [`UpdateError::InvalidName`] when `new_name` fails
/// [`validate_instance_name`], plus every error of `update_record`.
pub fn rename_instance(
    directory: &Path,
    instance: &GameInstance,
    new_name: &str,
) -> Result<GameInstance, UpdateError> {
    validate_instance_name(new_name)?;

    update_record(directory, instance, |stored| {
        stored.name = new_name.to_owned();
        Ok(())
    })
}

/// Replaces the lifecycle state of the instance stored in `directory`.
///
/// `instance` identifies the record to rewrite; see `update_record`. The state
/// belongs to the caller (it is not `Copy`) and is moved into the stored record.
///
/// # Errors
///
/// Every error of `update_record`.
pub fn set_instance_state(
    directory: &Path,
    instance: &GameInstance,
    state: InstanceState,
) -> Result<GameInstance, UpdateError> {
    update_record(directory, instance, |stored| {
        stored.state = state;
        Ok(())
    })
}

/// Replaces the Minecraft version of the instance stored in `directory`.
///
/// `instance` identifies the record to rewrite; see `update_record`.
///
/// # Errors
///
/// [`UpdateError::InvalidName`] when `version` fails [`validate_version_id`] or is
/// rejected by [`VersionId::parse`], plus every error of `update_record`.
pub fn set_instance_version(
    directory: &Path,
    instance: &GameInstance,
    version: &str,
) -> Result<GameInstance, UpdateError> {
    validate_version_id(version)?;
    let parsed = VersionId::parse(version).map_err(|error| {
        UpdateError::InvalidName(format!("version identifier rejected: {error}"))
    })?;

    update_record(directory, instance, |stored| {
        stored.version = parsed;
        Ok(())
    })
}

#[cfg(test)]
#[path = "update/update_tests.rs"]
mod tests;
