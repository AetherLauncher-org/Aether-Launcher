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
    NativeZipEntry, NativeZipError, MAX_NATIVE_UNCOMPRESSED_BYTES, preflight_native_zip,
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
    let parent = natives_directory.parent().ok_or(NativeInstallError::UnsafeDestination)?;
    let parent = fs::canonicalize(parent)?;
    if !parent.starts_with(&game_root) {
        return Err(NativeInstallError::UnsafeDestination);
    }
    let name = natives_directory.file_name().ok_or(NativeInstallError::UnsafeDestination)?;
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
    for archive in archives {
        let path = verify_local_artifact(&game_root, archive.artifact)?;
        let mut zip = ZipArchive::new(File::open(&path)?)?;
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
        let mut indices = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let path = PathBuf::from(entry.name.replace('\\', "/"));
            let candidate = selected.iter().find(|file| file.relative_path == path);
            if entry.is_directory || candidate.is_none() {
                continue;
            }
            let size = entry.uncompressed_size;
            budget = budget.checked_add(size).ok_or(NativeInstallError::TooLarge)?;
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
            let mut dest = OpenOptions::new().write(true).create_new(true).open(&output)?;
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

#[cfg(test)]
#[path = "native_install/native_install_tests.rs"]
mod tests;
