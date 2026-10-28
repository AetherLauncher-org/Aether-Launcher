//! BakaXL detection.
//!
//! BakaXL 3 stores the game in a `.minecraft` folder, exactly like PCL2 and
//! HMCL: `versions/<id>/<id>.json` is the version metadata, and turning on
//! version isolation only moves the per-version data under
//! `versions/<id>/`. The import pipeline therefore works unchanged.
//!
//! What BakaXL does **not** leave behind is a marker inside the game folder, so
//! detection happens on the launcher directory instead. BakaXL is portable and
//! does not register itself, so the caller probes candidate directories and
//! asks this module whether one of them is a BakaXL installation.

use std::path::{Path, PathBuf};

use super::ExistingFiles;

/// Entries that only a BakaXL installation has at its root.
///
/// `BakaXL.exe` is the launcher itself; the rest cover a folder whose
/// executable was renamed or is being updated in place.
const MARKERS: [&str; 4] = [
    "BakaXL.exe",
    "BakaXL.Core.dll",
    "BakaXL.Core",
    "BakaXL.Plugin",
];

/// Whether a listing looks like a BakaXL installation folder.
#[must_use]
pub fn matches(files: &ExistingFiles) -> bool {
    MARKERS.iter().any(|marker| files.contains(marker))
}

/// Game directories a BakaXL installation can import from.
///
/// The default layout keeps the game in `.minecraft` beside the launcher. A
/// portable install that was unpacked straight into a game folder is also
/// accepted, but only when it actually holds `versions/`.
#[must_use]
pub fn game_directories(root: &Path) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    for candidate in [root.join(".minecraft"), root.to_path_buf()] {
        if candidate.join("versions").is_dir() && !directories.contains(&candidate) {
            directories.push(candidate);
        }
    }
    directories
}

#[cfg(test)]
#[path = "bakaxl_tests.rs"]
mod tests;
