//! Extract verified native JARs into a fresh, private staging directory.
//! We deliberately never call ZipArchive::extract: entry names and output
//! locations are validated before using streaming reads and create_new writes.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;
use zip::ZipArchive;

use crate::DownloadArtifact;
use crate::native_zip::{
    MAX_NATIVE_UNCOMPRESSED_BYTES, MAX_NATIVE_ZIP_ENTRIES, NativeZipEntry, NativeZipError,
    preflight_native_zip,
};
use crate::verify::{ArtifactVerificationError, verify_local_artifact};

static INSTALL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub struct NativeInstallArchive<'a> {
    pub artifact: &'a DownloadArtifact,
    pub exclusions: &'a [String],
}

#[derive(Debug, Error)]
pub enum NativeInstallError {
    #[error(transparent)]
    Verification(#[from] ArtifactVerificationError),
    #[error(transparent)]
    Preflight(#[from] NativeZipError),
    #[error("failed to read ZIP archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid non-UTF-8 ZIP entry name")]
    InvalidEntryName,
    #[error("native destination must be a new directory inside the game directory")]
    UnsafeDestination,
    #[error("native destination already exists: {0:?}")]
    DestinationExists(PathBuf),
    #[error("native filename occurs in multiple archives: {0:?}")]
    DuplicateOutput(PathBuf),
    #[error("native extraction exceeds the total uncompressed-byte budget")]
    TooLarge,
    #[error("native ZIP file {0:?} returned a different uncompressed size")]
    ChangedFileSize(PathBuf),
    #[error("installed native file is missing, redirected or differs from its verified source: {0:?}")]
    InvalidInstalledFile(PathBuf),
    #[error("installed natives contain an unexpected file or symlink: {0:?}")]
    UnexpectedInstalledFile(PathBuf),
}

struct StageGuard(PathBuf);

impl Drop for StageGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Verify the JAR hashes, validate every ZIP entry, then stream-extract native
/// files to a new directory in the same parent filesystem as the final path.
///
/// An existing final directory is never intentionally reused or merged.
/// This is an internal API: callers must ensure no untrusted process can
/// concurrently rename entries in the trusted game directory.
pub fn install_native_archives(
    game_directory: &Path,
    natives_directory: &Path,
    archives: &[NativeInstallArchive<'_>],
) -> Result<PathBuf, NativeInstallError> {
    let game_root = fs::canonicalize(game_directory)?;
    let parent = natives_directory
        .parent()
        .ok_or(NativeInstallError::UnsafeDestination)?;
    let parent = fs::canonicalize(parent)?;
    if !parent.starts_with(&game_root) {
        return Err(NativeInstallError::UnsafeDestination);
    }
    let name = natives_directory
        .file_name()
        .ok_or(NativeInstallError::UnsafeDestination)?;
    if !crate::is_safe_relative_path(Path::new(name)) {
        return Err(NativeInstallError::UnsafeDestination);
    }
    let destination = parent.join(name);
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(NativeInstallError::DestinationExists(destination));
    }

    // Preflight *all* archives first, including those whose names would be excluded.
    // This rejects corrupt paths/symlinks without leaving partial output behind.
    let mut inputs = Vec::new();
    let mut names = HashSet::new();
    let mut budget = 0_u64;
    let mut entry_count = 0_usize;
    for archive in archives {
        let path = verify_local_artifact(&game_root, archive.artifact)?;
        let mut zip = ZipArchive::new(File::open(&path)?)?;
        entry_count = entry_count.saturating_add(zip.len());
        if entry_count > MAX_NATIVE_ZIP_ENTRIES {
            return Err(NativeInstallError::Preflight(
                NativeZipError::TooManyEntries,
            ));
        }
        let mut entries = Vec::with_capacity(zip.len());
        for index in 0..zip.len() {
            let file = zip.by_index(index)?;
            let name = std::str::from_utf8(file.name_raw())
                .map_err(|_| NativeInstallError::InvalidEntryName)?;
            entries.push(NativeZipEntry {
                name: name.to_owned(),
                uncompressed_size: file.size(),
                is_directory: file.is_dir(),
                unix_mode: file.unix_mode(),
            });
        }
        let selected: HashSet<_> = preflight_native_zip(&entries, archive.exclusions)?
            .into_iter()
            .map(|entry| entry.relative_path)
            .collect();
        let mut indices = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let path = PathBuf::from(entry.name.replace('\\', "/"));
            if entry.is_directory || !selected.contains(&path) {
                continue;
            }
            let size = entry.uncompressed_size;
            budget = budget
                .checked_add(size)
                .ok_or(NativeInstallError::TooLarge)?;
            if budget > MAX_NATIVE_UNCOMPRESSED_BYTES {
                return Err(NativeInstallError::TooLarge);
            }
            let key = path.to_string_lossy().to_ascii_lowercase();
            if !names.insert(key) {
                return Err(NativeInstallError::DuplicateOutput(path));
            }
            indices.push((index, path, size));
        }
        inputs.push((path, indices));
    }

    // A private staging directory avoids writes to preexisting user-controlled
    // files and allows a single rename to publish a fully extracted tree.
    let stage = loop {
        let sequence = INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".aether-native-stage-{}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(NativeInstallError::Io(error)),
        }
    };
    let guard = StageGuard(stage.clone());

    for (archive_path, indices) in inputs {
        let mut zip = ZipArchive::new(File::open(archive_path)?)?;
        for (index, relative, expected) in indices {
            let output = stage.join(&relative);
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = zip.by_index(index)?;
            let mut dest = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&output)?;
            // Enforce the declared length during streaming as well as preflight.
            let copied = io::copy(&mut file.by_ref().take(expected + 1), &mut dest)?;
            dest.flush()?;
            if copied != expected {
                return Err(NativeInstallError::ChangedFileSize(relative));
            }
        }
    }
    if fs::symlink_metadata(&destination).is_ok() {
        return Err(NativeInstallError::DestinationExists(destination));
    }
    fs::rename(&stage, &destination)?;
    // The stage no longer exists after a successful same-filesystem rename.
    drop(guard);
    Ok(destination)
}

/// Compare an existing native directory against the validated contents of
/// its source JARs. Every file is streamed and byte-compared; a stale or
/// tampered directory is never trusted merely because it exists.
///
/// The application-owned game tree must not be concurrently altered by
/// untrusted processes during this check or the eventual launch.
pub fn verify_installed_native_archives(
    game_directory: &Path,
    natives_directory: &Path,
    archives: &[NativeInstallArchive<'_>],
) -> Result<(), NativeInstallError> {
    let game_root = fs::canonicalize(game_directory)?;
    let metadata = fs::symlink_metadata(natives_directory)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(NativeInstallError::UnsafeDestination);
    }
    let native_root = fs::canonicalize(natives_directory)?;
    if !native_root.starts_with(&game_root) {
        return Err(NativeInstallError::UnsafeDestination);
    }
    let mut expected = HashSet::<String>::new();
    let mut byte_budget = 0_u64;
    let mut total_entries = 0_usize;

    for archive in archives {
        let path = verify_local_artifact(&game_root, archive.artifact)?;
        let mut zip = ZipArchive::new(File::open(path)?)?;
        total_entries = total_entries.saturating_add(zip.len());
        if total_entries > MAX_NATIVE_ZIP_ENTRIES {
            return Err(NativeInstallError::Preflight(NativeZipError::TooManyEntries));
        }
        let mut entries = Vec::with_capacity(zip.len());
        for index in 0..zip.len() {
            let file = zip.by_index(index)?;
            let name = std::str::from_utf8(file.name_raw())
                .map_err(|_| NativeInstallError::InvalidEntryName)?;
            entries.push(NativeZipEntry {
                name: name.to_owned(),
                uncompressed_size: file.size(),
                is_directory: file.is_dir(),
                unix_mode: file.unix_mode(),
            });
        }
        let selected = preflight_native_zip(&entries, archive.exclusions)?;
        for entry in selected {
            byte_budget = byte_budget.checked_add(entry.expected_size)
                .ok_or(NativeInstallError::TooLarge)?;
            if byte_budget > MAX_NATIVE_UNCOMPRESSED_BYTES {
                return Err(NativeInstallError::TooLarge);
            }
            let key = entry.relative_path.to_string_lossy().to_ascii_lowercase();
            if !expected.insert(key) {
                return Err(NativeInstallError::DuplicateOutput(entry.relative_path));
            }
            let relative = entry.relative_path;
            let output = native_root.join(&relative);
            let metadata = fs::symlink_metadata(&output)
                .map_err(|_| NativeInstallError::InvalidInstalledFile(output.clone()))?;
            if !metadata.is_file() || metadata.file_type().is_symlink()
                || metadata.len() != entry.expected_size
            {
                return Err(NativeInstallError::InvalidInstalledFile(output));
            }
            let canonical = fs::canonicalize(&output)?;
            if !canonical.starts_with(&native_root) {
                return Err(NativeInstallError::InvalidInstalledFile(output));
            }
            // Find the already preflighted ZIP member, not a guessed output.
            let index = entries.iter().position(|candidate| {
                !candidate.is_directory
                    && PathBuf::from(candidate.name.replace('\\', "/")) == relative
            }).ok_or_else(|| NativeInstallError::InvalidInstalledFile(output.clone()))?;
            let mut zipped = zip.by_index(index)?;
            let mut installed = File::open(&output)?;
            let mut left = [0_u8; 64 * 1024];
            let mut right = [0_u8; 64 * 1024];
            loop {
                let n = zipped.read(&mut left)?;
                if n == 0 {
                    break;
                }
                installed.read_exact(&mut right[..n])?;
                if left[..n] != right[..n] {
                    return Err(NativeInstallError::InvalidInstalledFile(output));
                }
            }
        }
    }

    // Also reject unexpected files/symlinks in the native search path. This
    // prevents an unrelated binary from being loaded ahead of trusted natives.
    let mut pending = vec![native_root.clone()];
    let mut walked = 0_usize;
    while let Some(dir) = pending.pop() {
        for item in fs::read_dir(&dir)? {
            let item = item?;
            walked += 1;
            if walked > MAX_NATIVE_ZIP_ENTRIES {
                return Err(NativeInstallError::Preflight(NativeZipError::TooManyEntries));
            }
            let meta = fs::symlink_metadata(item.path())?;
            let path = item.path();
            if meta.file_type().is_symlink() {
                return Err(NativeInstallError::UnexpectedInstalledFile(path));
            }
            if meta.is_dir() {
                pending.push(path);
            } else if meta.is_file() {
                let relative = path.strip_prefix(&native_root)
                    .map_err(|_| NativeInstallError::UnsafeDestination)?;
                if !expected.contains(&relative.to_string_lossy().to_ascii_lowercase()) {
                    return Err(NativeInstallError::UnexpectedInstalledFile(path));
                }
            } else {
                return Err(NativeInstallError::UnexpectedInstalledFile(path));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "native_install/native_install_tests.rs"]
mod tests;
