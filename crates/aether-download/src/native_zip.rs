//! Preflight metadata for native ZIP archives before any extraction occurs.
//! ZIP parsing and filesystem writes are intentionally handled by a later layer.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::is_safe_relative_path;

pub const MAX_NATIVE_ZIP_ENTRIES: usize = 16_384;
pub const MAX_NATIVE_FILE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_NATIVE_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeZipEntry {
    /// Raw path as listed inside the archive.
    pub name: String,
    pub uncompressed_size: u64,
    pub is_directory: bool,
    /// Unix mode if available; symlinks and special file types are forbidden.
    pub unix_mode: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedNativeEntry {
    pub relative_path: PathBuf,
    pub expected_size: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativeZipError {
    #[error("native archive contains too many entries")]
    TooManyEntries,
    #[error("native archive entry is not a safe relative path: {0}")]
    UnsafePath(String),
    #[error("native archive entry is a symlink or unsupported file type: {0}")]
    UnsupportedEntryType(String),
    #[error("native archive contains a duplicate output name: {0}")]
    DuplicateEntry(String),
    #[error("native archive entry is too large: {0}")]
    FileTooLarge(String),
    #[error("native archive exceeds the uncompressed size limit")]
    ArchiveTooLarge,
    #[error("invalid native extraction exclusion: {0}")]
    InvalidExclusion(String),
}

/// Validate the complete ZIP directory listing prior to extraction.
/// Filesystem writes must still use a private staging directory, refuse
/// preexisting symlinks and re-check sizes during streaming extraction.
pub fn preflight_native_zip(
    entries: &[NativeZipEntry],
    exclusions: &[String],
) -> Result<Vec<PreparedNativeEntry>, NativeZipError> {
    if entries.len() > MAX_NATIVE_ZIP_ENTRIES {
        return Err(NativeZipError::TooManyEntries);
    }
    let mut prefixes = Vec::new();
    for exclusion in exclusions {
        let text = exclusion.replace('\\', "/");
        let prefix = text.trim_end_matches('/');
        if prefix.is_empty() || !is_safe_relative_path(Path::new(prefix)) {
            return Err(NativeZipError::InvalidExclusion(exclusion.clone()));
        }
        prefixes.push(prefix.to_owned());
    }

    let mut outputs = Vec::new();
    let mut names = HashSet::new();
    let mut total_bytes = 0_u64;
    for entry in entries {
        let name = entry.name.replace('\\', "/");
        if name.ends_with("//") || (!entry.is_directory && name.ends_with('/')) {
            return Err(NativeZipError::UnsafePath(entry.name.clone()));
        }
        let relative = if entry.is_directory {
            name.strip_suffix('/').unwrap_or(&name)
        } else {
            &name
        };
        if !is_safe_relative_path(Path::new(relative)) {
            return Err(NativeZipError::UnsafePath(entry.name.clone()));
        }
        if let Some(mode) = entry.unix_mode {
            let file_type = mode & 0o170000;
            if !matches!(file_type, 0 | 0o100000 | 0o040000)
                || (file_type == 0o040000 && !entry.is_directory)
                || (file_type == 0o100000 && entry.is_directory)
            {
                return Err(NativeZipError::UnsupportedEntryType(entry.name.clone()));
            }
        }

        // Exclusion is applied only after security checks, even for directories.
        if prefixes.iter().any(|prefix| {
            relative == prefix || relative.starts_with(&format!("{prefix}/"))
        }) {
            continue;
        }
        let normalized = relative.to_ascii_lowercase();
        if !names.insert(normalized) {
            return Err(NativeZipError::DuplicateEntry(entry.name.clone()));
        }
        if entry.is_directory {
            continue;
        }
        if entry.uncompressed_size > MAX_NATIVE_FILE_BYTES {
            return Err(NativeZipError::FileTooLarge(entry.name.clone()));
        }
        total_bytes = total_bytes
            .checked_add(entry.uncompressed_size)
            .ok_or(NativeZipError::ArchiveTooLarge)?;
        if total_bytes > MAX_NATIVE_UNCOMPRESSED_BYTES {
            return Err(NativeZipError::ArchiveTooLarge);
        }
        outputs.push(PreparedNativeEntry {
            relative_path: PathBuf::from(relative),
            expected_size: entry.uncompressed_size,
        });
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, bytes: u64) -> NativeZipEntry {
        NativeZipEntry {
            name: name.to_owned(),
            uncompressed_size: bytes,
            is_directory: false,
            unix_mode: Some(0o100644),
        }
    }

    #[test]
    fn excludes_metadata_and_keeps_native_binaries() {
        let entries = [
            file("META-INF/MANIFEST.MF", 20),
            file("windows/x64/lwjgl.dll", 50),
            file("libnative.so", 30),
        ];
        let prepared = preflight_native_zip(&entries, &["META-INF/".to_owned()]).unwrap();
        assert_eq!(prepared.len(), 2);
        assert_eq!(prepared[0].relative_path, PathBuf::from("windows/x64/lwjgl.dll"));
    }

    #[test]
    fn rejects_zip_slip_and_windows_paths() {
        for name in ["../outside.dll", "/absolute.dll", "C:/x.dll", "a/../../x.dll",
            "a//b.dll", "CON.dll", "dir\\..\\escape.dll", "file.dll:stream"]
        {
            assert_eq!(
                preflight_native_zip(&[file(name, 1)], &[]),
                Err(NativeZipError::UnsafePath(name.to_owned()))
            );
        }
    }

    #[test]
    fn rejects_links_duplicates_and_excessive_sizes() {
        let mut link = file("link.dll", 0);
        link.unix_mode = Some(0o120777);
        assert_eq!(
            preflight_native_zip(&[link], &[]),
            Err(NativeZipError::UnsupportedEntryType("link.dll".to_owned()))
        );
        assert!(matches!(
            preflight_native_zip(&[file("Native.dll", 1), file("native.dll", 2)], &[]),
            Err(NativeZipError::DuplicateEntry(_))
        ));
        assert!(matches!(
            preflight_native_zip(&[file("large.dll", MAX_NATIVE_FILE_BYTES + 1)], &[]),
            Err(NativeZipError::FileTooLarge(_))
        ));
        assert!(matches!(
            preflight_native_zip(&[
                file("a.dll", MAX_NATIVE_FILE_BYTES),
                file("b.dll", MAX_NATIVE_FILE_BYTES),
                file("c.dll", 1)
            ], &[]),
            Err(NativeZipError::ArchiveTooLarge)
        ));
    }

    #[test]
    fn excluded_entries_are_still_validated_and_bad_exclusions_are_rejected() {
        assert_eq!(
            preflight_native_zip(&[file("../outside", 1)], &["../".to_owned()]),
            Err(NativeZipError::InvalidExclusion("../".to_owned()))
        );
        assert!(matches!(
            preflight_native_zip(&[file("META-INF/../../outside", 1)], &["META-INF/".to_owned()]),
            Err(NativeZipError::UnsafePath(_))
        ));
    }
}
