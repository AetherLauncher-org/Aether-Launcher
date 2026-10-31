//! Reading and writing files in the cache folder, which other programs on the machine can
//! also write to. A cache file may have been replaced by a link or swapped for something huge;
//! neither may make the launcher overwrite another file or read without a limit.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

/// Writes `bytes` to `path` through a temporary file next to it, then renames it into place.
/// A link at `path` or at the temporary name is replaced, never followed, so the file it
/// points to is left alone.
pub(crate) fn write_replacing(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temporary = path.with_file_name(name);
    // `remove_file` removes a link itself and not its target
    let _ = fs::remove_file(&temporary);
    let written = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .and_then(|mut file| file.write_all(bytes));
    let renamed = written.and_then(|()| fs::rename(&temporary, path));
    if renamed.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    renamed
}

/// The contents of `path` if it holds at most `limit` bytes. The file is opened once and read
/// through a limit, so a file that grows after a size check cannot be read whole.
pub(crate) fn read_capped(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

#[cfg(test)]
#[path = "cachefile_tests.rs"]
mod tests;

/// Reject links in all directory components, including Windows junctions.
/// Callers must prevent concurrent replacement of directory components.
pub(crate) fn check_directory(path: &Path) -> io::Result<()> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse(&metadata) {
            return Err(io::Error::other("not a plain directory"));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_reparse(_: &fs::Metadata) -> bool {
    false
}
