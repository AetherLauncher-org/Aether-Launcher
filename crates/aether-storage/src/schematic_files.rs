//! Reading a schematic out of an instance and saving a converted one next to it.
//!
//! Like the rest of [`crate::manage`], callers give a plain file name, never a path: the file
//! must sit directly in the instance's `schematics/` folder, be an ordinary file (not a link)
//! and carry one of the schematic extensions. Reading is capped in size. Writing never
//! overwrites: it picks a free name and creates the file exclusively.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::manage::{ContentKind, ManageError, is_plain_name};

/// The largest schematic file that will be read, compressed. Larger files are refused before
/// any of it is read into memory.
pub const MAX_SCHEMATIC_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Extensions this launcher reads and writes.
pub const SCHEMATIC_EXTENSIONS: [&str; 3] = ["litematic", "schem", "nbt"];

fn schematics_folder(game_dir: &Path) -> PathBuf {
    game_dir.join(ContentKind::Schematics.folder())
}

fn extension_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Reads one schematic file of an instance.
///
/// # Errors
///
/// [`ManageError::InvalidName`] for a name that is not a plain schematic file name,
/// [`ManageError::NotFound`], [`ManageError::NotAllowed`] for a link, a folder or a file over
/// [`MAX_SCHEMATIC_FILE_BYTES`].
pub fn read_schematic_file(game_dir: &Path, name: &str) -> Result<Vec<u8>, ManageError> {
    if !is_plain_name(name) || !SCHEMATIC_EXTENSIONS.contains(&extension_of(name).as_str()) {
        return Err(ManageError::InvalidName);
    }
    let path = schematics_folder(game_dir).join(name);
    let metadata = fs::symlink_metadata(&path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => ManageError::NotFound,
        _ => ManageError::Io(error),
    })?;
    if !metadata.is_file() || metadata.len() > MAX_SCHEMATIC_FILE_BYTES {
        return Err(ManageError::NotAllowed);
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    // Read at most the cap plus one byte, in case the file grew after the check.
    fs::File::open(&path)?
        .take(MAX_SCHEMATIC_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_SCHEMATIC_FILE_BYTES {
        return Err(ManageError::NotAllowed);
    }
    Ok(bytes)
}

/// A file-name-safe version of a schematic's name: letters, digits, spaces, `-`, `_`, `.`
/// and `(`/`)` stay, everything else becomes `_`, and it never ends in a dot or space.
#[must_use]
pub fn safe_stem(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let trimmed = cleaned.trim_matches(|c| c == ' ' || c == '.');
    if trimmed.is_empty() {
        "schematic".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Saves new schematic bytes in the instance's `schematics/` folder under a free name
/// (`stem.ext`, then `stem 2.ext`, `stem 3.ext`, …) and returns that name.
///
/// The file is created exclusively, so a name that appears between the check and the write
/// is never overwritten; a failed write leaves nothing behind.
///
/// # Errors
///
/// [`ManageError::InvalidName`] for an extension that is not a schematic extension,
/// [`ManageError::AlreadyExists`] when no free name was found, [`ManageError::Io`].
pub fn write_new_schematic(
    game_dir: &Path,
    stem: &str,
    extension: &str,
    bytes: &[u8],
) -> Result<String, ManageError> {
    if !SCHEMATIC_EXTENSIONS.contains(&extension) {
        return Err(ManageError::InvalidName);
    }
    let folder = schematics_folder(game_dir);
    fs::create_dir_all(&folder)?;
    let stem = safe_stem(stem);
    for attempt in 1..=1000u32 {
        let name = if attempt == 1 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem} {attempt}.{extension}")
        };
        if !is_plain_name(&name) {
            return Err(ManageError::InvalidName);
        }
        let path = folder.join(&name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                let written = file.write_all(bytes).and_then(|()| file.flush());
                if let Err(error) = written {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(error.into());
                }
                return Ok(name);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(ManageError::AlreadyExists)
}

#[cfg(test)]
#[path = "schematic_files_tests.rs"]
mod tests;
