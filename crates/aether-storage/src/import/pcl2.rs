//! PCL2 (Plain Craft Launcher 2) layout recognition.
//!
//! PCL2 keeps its own configuration under `PCL/` next to the game directories,
//! and version metadata in `<game>/versions/<id>/<id>.json`. The two facts this
//! module relies on:
//!
//! * a strong marker file exists (`PCL/Setup.ini`, or the older flat `PCL.ini`);
//! * for every version folder, `<id>.json` sits beside the folder's `jar/`
//!   and `natives/` subdirectories.
//!
//! `Setup.ini` parsing is deliberately shallow: only `key=value` lines are
//! understood, the first `=` splits, surrounding quotes are stripped, and
//! unknown keys are ignored. Real PCL2 configuration has nested structures and
//! comment conventions this module does not attempt to model; anything richer
//! than a flat key/value map would not change the import plan.

use std::path::PathBuf;

use super::{ExistingFiles, ImportCandidate, ImportSourceKind};

/// Marker files that identify a PCL2 installation.
pub const MARKERS: [&str; 2] = ["PCL/Setup.ini", "PCL.ini"];

/// Keys of `Setup.ini` this module understands.
///
/// Every recognised key is advisory only; the plan is built from the directory
/// layout, never from a configuration value, because configuration values are
/// user-editable and may point outside the tree.
pub const KNOWN_KEYS: [&str; 5] = [
    "setup_version",
    "launch_folder_index",
    "launcher_visibility",
    "update_channel",
    "language",
];

/// Whether the listing carries a PCL2 marker.
#[must_use]
pub fn matches(files: &ExistingFiles) -> bool {
    MARKERS.iter().any(|marker| files.contains(marker))
}

/// Parsed, flat view of a `Setup.ini` file.
#[must_use]
pub fn parse_setup_ini(content: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || line.starts_with('[')
        {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        let value = value.trim().trim_matches('"').trim().to_owned();
        pairs.push((key.to_owned(), value));
    }

    pairs
}

/// Looks up one key of an already parsed `Setup.ini`.
#[must_use]
pub fn setup_value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

/// Builds the PCL2 candidates for a listing.
#[must_use]
pub fn plan(files: &ExistingFiles) -> Vec<ImportCandidate> {
    super::plan_candidates(ImportSourceKind::Pcl2, files)
}

/// Game directories a PCL2 tree may keep its versions in, in priority order.
///
/// Used only for diagnostics; [`plan`] discovers the actual directories from the
/// listing instead of assuming one of these exists.
#[must_use]
pub fn candidate_game_directories() -> Vec<PathBuf> {
    vec![PathBuf::new(), PathBuf::from(".minecraft")]
}

#[cfg(test)]
mod inline_tests {
    use super::parse_setup_ini;

    #[test]
    fn ignores_comments_and_sections() {
        let pairs = parse_setup_ini("[Setup]\n; comment\n# other\nkey=value\n");
        assert_eq!(pairs, vec![("key".to_owned(), "value".to_owned())]);
    }
}
