//! Creating a new managed instance.
//!
//! An instance lives at `<base>/instances/<uuid>/`. The record stored in that
//! directory is the only source of truth, and `GameInstance::root` is set to the
//! canonicalized path of the UUID directory so that later reads can prove the
//! record belongs where it was found.
//!
//! Layout produced by one successful call:
//!
//! ```text
//! <base>/instances/<uuid>/
//! ├── instance.json     # the record, written atomically
//! └── minecraft/        # game directory skeleton, currently empty
//! ```
//!
//! The skeleton is deliberately minimal: `minecraft/` is the working directory
//! that the launch pipeline will eventually hand to the game, so it has to exist
//! before the first launch, but nothing else is created until a version is
//! installed. Completing a partially created instance (a UUID directory with no
//! record) is not attempted here; it surfaces as a corrupt entry when listing.
//!
//! # Order of operations
//!
//! Everything that can be rejected is rejected before the first byte is written:
//!
//! 1. `base` must already exist, must be a plain directory, and must not be a
//!    symbolic link; it is then canonicalized so every derived path is absolute
//!    and free of `.`/`..` components;
//! 2. the display name is validated by [`crate::update::validate_instance_name`],
//!    the same rules the updater enforces, so a name that can be created can always
//!    be renamed back to itself;
//! 3. the version identifier is validated by [`crate::update::validate_version_id`];
//! 4. `<base>/instances` is created when missing and inspected again with
//!    [`fs::symlink_metadata`], so a link planted at that name is rejected instead
//!    of being written through;
//! 5. no existing `instance.json` may already carry the requested display name;
//! 6. the directory is allocated under a fresh [`InstanceId`], which is a UUID from
//!    the OS random source and can be created without the `uuid` crate;
//! 7. `minecraft/` is created inside it and the record is written atomically.
//!
//! # Rollback
//!
//! The UUID directory is the only thing this function allocates for itself, and
//! every failure after step 6 removes it again. `<base>/instances` is deliberately
//! *not* removed on failure: it is a lasting part of the store rather than a
//! temporary allocation, and by the time an error surfaces it may already hold the
//! user's other instances.

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use aether_core::{GameInstance, InstanceId, InstanceState, VersionId};
use thiserror::Error;

use crate::update::{self, UpdateError, validate_instance_name, validate_version_id};

/// Directory under the store root that holds one subdirectory per instance.
///
/// Kept identical to the directory name used by PR #12 in `instances.rs`;
/// redefined here because that file is owned by PR #12.
pub const INSTANCES_DIR: &str = "instances";

/// Game directory skeleton created for every new instance.
pub const MINECRAFT_DIR: &str = "minecraft";

/// Record file name inside an instance directory.
///
/// Taken from [`update`] so the writer and the duplicate check can never drift
/// apart; kept as a named constant because it is also the natural hook for tests.
const INSTANCE_RECORD_FILE: &str = update::INSTANCE_RECORD_FILE;

/// How often a fresh identifier is retried when a UUID directory already exists.
const MAX_ID_ATTEMPTS: u32 = 8;

/// Failures when creating an instance.
#[derive(Debug, Error)]
pub enum CreateError {
    /// The store root is missing, is not a directory, or is a symbolic link.
    #[error("instance store base is not a safe existing directory")]
    UnsafeBase,
    /// The display name is not acceptable.
    #[error("invalid instance name: {0}")]
    InvalidName(String),
    /// The version identifier is not acceptable.
    #[error("invalid version identifier: {0}")]
    InvalidVersion(String),
    /// Another instance in the store already uses the display name.
    #[error("an instance named {0:?} already exists")]
    DuplicateName(String),
    /// Every identifier tried for the new directory was already taken.
    ///
    /// This is the only way this variant can surface: the records are UUIDs drawn
    /// from the OS random source, so it takes `MAX_ID_ATTEMPTS` collisions in a
    /// row to reach it. It never reports a user error and never reports a name that
    /// merely already exists — that is [`CreateError::DuplicateName`].
    #[error("the instance directory already exists")]
    AlreadyExists,
    /// A file system operation failed.
    #[error("cannot create the instance")]
    Io(#[source] std::io::Error),
}

impl From<UpdateError> for CreateError {
    fn from(error: UpdateError) -> Self {
        match error {
            // The updater refuses a directory it does not trust; the only directory
            // in play here is the store root.
            UpdateError::UnsafeDirectory => Self::UnsafeBase,
            UpdateError::InvalidName(message) => Self::InvalidName(message),
            // Unreachable for a validated name and version (see
            // `update::write_instance_atomic`), so this is not a field problem:
            // report it as an I/O-level failure rather than blaming the version.
            UpdateError::TooLarge => Self::Io(std::io::Error::other(
                "instance record exceeds the size limit",
            )),
            // The original error is preserved when it is already an `io::Error`.
            UpdateError::Io(error) => Self::Io(error),
            UpdateError::Serialize(error) => Self::Io(std::io::Error::other(error.to_string())),
        }
    }
}

/// Maps a name validation failure onto [`CreateError::InvalidName`].
fn map_name_error(error: UpdateError) -> CreateError {
    match error {
        UpdateError::InvalidName(message) => CreateError::InvalidName(message),
        // `validate_instance_name` only ever returns `InvalidName`; anything else
        // would be a bug in this crate, so it is not disguised as a user error.
        other => CreateError::from(other),
    }
}

/// Maps a version validation failure onto [`CreateError::InvalidVersion`].
fn map_version_error(error: UpdateError) -> CreateError {
    match error {
        UpdateError::InvalidName(message) => CreateError::InvalidVersion(message),
        // See `map_name_error`: unreachable for `validate_version_id`.
        other => CreateError::from(other),
    }
}

/// Creates a new managed instance under `base`.
///
/// `base` is the store root (the parent of `instances/`). The directory
/// `instances/` is created when missing. The returned instance is already
/// persisted; `root` is the canonicalized `<base>/instances/<uuid>` path.
///
/// # Errors
///
/// * [`CreateError::UnsafeBase`] when `base` is missing, is not a plain directory,
///   is a symbolic link, or when `<base>/instances` is not a plain directory;
/// * [`CreateError::InvalidName`] when `name` fails
///   [`crate::update::validate_instance_name`];
/// * [`CreateError::InvalidVersion`] when `version` fails
///   [`crate::update::validate_version_id`] or [`VersionId::parse`];
/// * [`CreateError::DuplicateName`] when another record already carries the name
///   (compared case-insensitively for ASCII);
/// * [`CreateError::Io`] when a directory or the record cannot be created, and
///   [`CreateError::AlreadyExists`] when no free identifier was found.
///
/// Nothing is left behind on failure: a UUID directory that was already allocated
/// is removed again.
pub fn create_instance(
    base: &Path,
    name: &str,
    version: &str,
) -> Result<GameInstance, CreateError> {
    // 1. The store root must be a real directory before anything is derived from it.
    let base = canonical_base(base)?;

    // 2. The display name follows the same rules the updater enforces.
    validate_instance_name(name).map_err(map_name_error)?;

    // 3. The version is stored verbatim, so it must not contain a separator.
    validate_version_id(version).map_err(map_version_error)?;
    let parsed_version = VersionId::parse(version)
        .map_err(|error| CreateError::InvalidVersion(error.to_string()))?;

    // 4. `<base>/instances` is created on demand and re-checked through metadata.
    let instances = instances_dir(&base)?;

    // 5. Two instances must not share a display name.
    reject_duplicate_name(&instances, name)?;

    // 6. + 7. A fresh UUID directory, allocated exclusively for this call.
    let (id, directory) = allocate_directory(&instances)?;

    // 8. Everything from here on has to roll the directory back on failure.
    if let Err(error) = create_skeleton(&directory) {
        rollback(&directory);
        return Err(error);
    }

    let instance = GameInstance {
        id,
        name: name.to_owned(),
        version: parsed_version,
        root: directory.clone(),
        state: InstanceState::Incomplete,
    };

    // 9. The record is written through the same atomic path as every update.
    //    No record can exist yet: `allocate_directory` created this directory
    //    itself, so there is nothing to check for and `AlreadyExists` cannot
    //    originate here — see its documentation for where it does come from.
    if let Err(error) = update::write_instance_atomic(&directory, &instance) {
        // 10. Roll back the half-built directory so a retry starts from a clean slate.
        rollback(&directory);
        return Err(error.into());
    }

    Ok(instance)
}

/// Verifies that `base` is a plain, existing directory and canonicalizes it.
///
/// Canonicalizing makes every path derived from `base` absolute, which is what lets
/// the record store a root that can be compared with a canonicalized directory
/// later. A missing `base` is reported as unsafe rather than as an I/O error: the
/// store root is not created on demand, because doing so would silently adopt a
/// surviving reparse point or a wrong drive.
fn canonical_base(base: &Path) -> Result<PathBuf, CreateError> {
    match fs::symlink_metadata(base) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(CreateError::UnsafeBase),
        Ok(metadata) if metadata.is_dir() => {
            fs::canonicalize(base).map_err(|_| CreateError::UnsafeBase)
        }
        Ok(_) => Err(CreateError::UnsafeBase),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(CreateError::UnsafeBase),
        Err(error) => Err(CreateError::Io(error)),
    }
}

/// Returns `<base>/instances`, creating it when missing and rejecting links.
///
/// `create_dir` rather than `create_dir_all` is used on purpose: it fails instead
/// of silently rebuilding a `base` that vanished, and it is atomic about "only if
/// absent", which a metadata check followed by a create is not.
///
/// A failure is propagated without deleting anything afterwards. Unlike the UUID
/// directory, `<base>/instances` is a lasting part of the store rather than a
/// temporary allocation, and by the time an error surfaces it may already hold the
/// user's other instances.
fn instances_dir(base: &Path) -> Result<PathBuf, CreateError> {
    let instances = base.join(INSTANCES_DIR);

    match fs::symlink_metadata(&instances) {
        Ok(metadata) if metadata.file_type().is_symlink() => return Err(CreateError::UnsafeBase),
        Ok(metadata) if metadata.is_dir() => return Ok(instances),
        Ok(_) => return Err(CreateError::UnsafeBase),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(CreateError::Io(error)),
    }

    if let Err(error) = fs::create_dir(&instances) {
        // Another process may have won the race; the re-check below decides.
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(CreateError::Io(error));
        }
    }

    // Inspect the entry itself, never what it points at: a link planted at this
    // name between the two calls is rejected instead of being written through.
    match fs::symlink_metadata(&instances) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(instances),
        Ok(_) => Err(CreateError::UnsafeBase),
        Err(error) => Err(CreateError::Io(error)),
    }
}

/// Scans existing records for a display name collision.
///
/// Unreadable, oversized or corrupt records are ignored on purpose: they cannot be
/// renamed from the UI anyway, and refusing every new instance because of one
/// broken directory would be worse than allowing two instances to share a name.
///
/// # Case sensitivity
///
/// The comparison folds ASCII case only (`eq_ignore_ascii_case`) plus an exact
/// match, rather than applying full Unicode case folding. That matches how NTFS
/// compares names for the characters people actually type into an instance name,
/// it is cheap and it is predictable — but `Straße` and `STRASSE` are *not* treated
/// as the same name, and composed versus decomposed forms are not normalized.
/// Full folding would need a Unicode table, would still not agree with the file
/// system in every case, and would make a duplicate check depend on Unicode data
/// version, so the trade-off is documented instead of hidden.
fn reject_duplicate_name(instances: &Path, name: &str) -> Result<(), CreateError> {
    for entry in fs::read_dir(instances).map_err(CreateError::Io)? {
        let entry = entry.map_err(CreateError::Io)?;

        // Only plain directories can be candidates: a link or a stray file is not
        // an instance of this store and must not be followed.
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }

        let Some(existing) = read_record_name(&entry.path()) else {
            continue;
        };

        if existing.eq_ignore_ascii_case(name) || existing == name {
            return Err(CreateError::DuplicateName(name.to_owned()));
        }
    }

    Ok(())
}

/// Reads the display name of the record in `directory`, when it is trustworthy.
///
/// The read is bounded by [`update::MAX_INSTANCE_RECORD_BYTES`] — both on the
/// directory entry and on the bytes actually read — so a planted multi-gigabyte
/// `instance.json` cannot be pulled into memory just to compare a name. Records that
/// are links, are not regular files, or do not decode are reported as `None`, which
/// the caller treats as "no name here".
fn read_record_name(directory: &Path) -> Option<String> {
    let record = directory.join(INSTANCE_RECORD_FILE);

    let metadata = fs::symlink_metadata(&record).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return None;
    }
    if metadata.len() > update::MAX_INSTANCE_RECORD_BYTES {
        return None;
    }

    let file = fs::File::open(&record).ok()?;
    let mut raw = String::new();
    file.take(update::MAX_INSTANCE_RECORD_BYTES.saturating_add(1))
        .read_to_string(&mut raw)
        .ok()?;
    if raw.len() as u64 > update::MAX_INSTANCE_RECORD_BYTES {
        return None;
    }

    crate::decode_instance(&raw)
        .ok()
        .map(|existing| existing.name)
}

/// Creates the `minecraft/` working directory inside a freshly allocated directory.
fn create_skeleton(directory: &Path) -> Result<(), CreateError> {
    fs::create_dir(directory.join(MINECRAFT_DIR)).map_err(CreateError::Io)
}

/// Picks a fresh identifier, creates its directory and returns both.
///
/// `fs::create_dir` is the existence check: it is atomic, it reports
/// `AlreadyExists` for a taken name, and it also fails for a symbolic link planted
/// at that name, so a hostile entry can neither be adopted nor written through.
/// A canonicalization failure removes the directory again, because a directory that
/// cannot be canonicalized can never be a valid `GameInstance::root`.
///
/// Returns [`CreateError::AlreadyExists`] when `MAX_ID_ATTEMPTS` consecutive
/// identifiers were all taken; that is the only place the variant originates.
fn allocate_directory(instances: &Path) -> Result<(InstanceId, PathBuf), CreateError> {
    for _ in 0..MAX_ID_ATTEMPTS {
        let id = InstanceId::new();
        let directory = instances.join(id.to_string());

        match fs::create_dir(&directory) {
            Ok(()) => {
                return match fs::canonicalize(&directory) {
                    Ok(canonical) => Ok((id, canonical)),
                    Err(error) => {
                        rollback(&directory);
                        Err(CreateError::Io(error))
                    }
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(CreateError::Io(error)),
        }
    }

    Err(CreateError::AlreadyExists)
}

/// Removes a half-created instance directory, ignoring errors.
///
/// The removal is best effort on purpose: the original error is what the caller
/// needs to see, and a leftover UUID directory is already reported as a skipped
/// entry by `list_instances`, so a failed cleanup degrades into a visible warning
/// rather than into a misleading secondary error.
fn rollback(directory: &Path) {
    let _ = fs::remove_dir_all(directory);
}

#[cfg(test)]
#[path = "create/create_tests.rs"]
mod tests;
