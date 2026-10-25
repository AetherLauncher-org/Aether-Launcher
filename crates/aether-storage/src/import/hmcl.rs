//! HMCL (Hello Minecraft! Launcher) layout recognition.
//!
//! HMCL is a portable launcher: everything it owns lives inside the game
//! directory it was pointed at, so the directory listing alone is enough to
//! recognise it. This module deliberately keeps the recognition algorithm
//! small; the reasons are worth stating because each of them is a decision not
//! to do work.
//!
//! # Only strong markers count
//!
//! A directory that merely *looks* like a Minecraft install (`versions/`,
//! `assets/`, `libraries/`) is not evidence of HMCL: vanilla installs, other
//! launchers and hand-made folders share that shape. `detect` therefore keys off
//! files that only HMCL writes — `.hmcl.json` (the modern settings file),
//! `hmcl.json` (hand-placed / older layout) and `hmclversion.cfg` (the updater
//! bookkeeping file written by the standalone build). The launcher lets users
//! relocate its configuration, so [`ExistingFiles::contains_file_named`] is used
//! for the basename match and the marker may appear at any depth the scan
//! reached. Weakening this to "has a `versions/` folder" would make HMCL win
//! detection against unrelated trees and produce junk candidates.
//!
//! # The version layout is delegated, not re-derived
//!
//! HMCL's on-disk version layout is the same one PCL2 and vanilla use:
//! `versions/<id>/<id>.json` beside the game directory. Re-implementing that
//! parse here would give two parsers that can drift apart, so [`plan`] simply
//! forwards to [`super::plan_candidates`] with [`ImportSourceKind::Hmcl`]. The
//! only HMCL-specific fact is the *label* stamped on the candidate.
//!
//! # Nothing large is copied
//!
//! A candidate is a plan, not a transfer: it lists the metadata file the import
//! would create and lets the caller decide what to skip. Game jars, libraries,
//! assets and native archives are never copied out of another launcher's
//! directory — they are resolved and downloaded later from the manifest
//! pipeline. Copying them would duplicate hundreds of megabytes, break version
//! manifests that point at shared library coordinates, and risk carrying over
//! files the user did not ask for. The cost of re-deriving them is a download
//! that the launcher already knows how to do.
//!
//! # Configuration parsing is coarse on purpose
//!
//! [`parse_config`] only understands two shapes: flat `key=value` lines (the
//! `hmclversion.cfg` style) and top-level keys of a flat JSON object (the
//! `.hmcl.json` style). It does not model HMCL's settings schema, it does not
//! recurse into arrays or nested objects, and it does not validate values. That
//! is sufficient because the import plan never depends on configuration — a
//! configuration value can be edited by the user or point outside the scanned
//! root, so trusting it would make the plan both wrong and unsafe. The parse
//! exists for diagnostics and future display only.
//!
//! # Explicit boundaries
//!
//! This module does **not** support:
//!
//! * parsing the full HMCL JSON configuration structure (nested objects, arrays
//!   of game directories, per-instance overrides) — only top-level scalar-ish
//!   keys are surfaced;
//! * non-standard custom game directories configured inside HMCL, or any
//!   directory the scan could not reach within [`super::MAX_SCAN_DEPTH`];
//! * archive / modpack import (`.zip`, `.mrpack`, `.hmcl` bundles) — those are a
//!   different pipeline entirely;
//! * migrating dependencies, libraries, assets, saves or options;
//! * recognising the portable "world-only" or server-only folder shapes.

use std::path::PathBuf;

use super::{ExistingFiles, ImportCandidate, ImportSourceKind};

/// Marker files that identify an HMCL installation.
///
/// Matched by file name at any scanned depth, because HMCL allows its settings
/// and updater files to move with the installation.
pub const MARKERS: [&str; 3] = [".hmcl.json", "hmcl.json", "hmclversion.cfg"];

/// Keys of `hmclversion.cfg` this module understands, for diagnostics.
pub const KNOWN_KEYS: [&str; 2] = ["version", "updatechannel"];

/// Whether the listing carries an HMCL marker.
#[must_use]
pub fn matches(files: &ExistingFiles) -> bool {
    MARKERS
        .iter()
        .any(|marker| files.contains_file_named(marker))
}

/// Builds the HMCL candidates for a listing.
///
/// The version layout is shared with the other launchers, so this forwards to
/// [`super::plan_candidates`] and only injects the HMCL label.
#[must_use]
pub fn plan(files: &ExistingFiles) -> Vec<ImportCandidate> {
    super::plan_candidates(ImportSourceKind::Hmcl, files)
}

/// Coarse parse of an HMCL configuration file.
///
/// Handles flat `key=value` lines and the top-level keys of a flat JSON object.
/// Values are returned verbatim; nothing is validated, coerced or applied. See
/// the module documentation for why this is intentionally shallow.
#[must_use]
pub fn parse_config(content: &str) -> Vec<(String, String)> {
    let trimmed = content.trim_start();
    if trimmed.starts_with('{') {
        parse_flat_json_keys(trimmed)
    } else {
        parse_key_value_lines(content)
    }
}

/// Parses `key=value` lines, skipping comments, sections and blank lines.
fn parse_key_value_lines(content: &str) -> Vec<(String, String)> {
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

        let key = key.trim().trim_matches('"').trim();
        if key.is_empty() {
            continue;
        }

        let value = value.trim().trim_matches('"').trim().to_owned();
        pairs.push((key.to_owned(), value));
    }

    pairs
}

/// Parses the top-level scalar keys of a JSON object without a JSON parser.
///
/// This is not a JSON parser, and it does not recurse: braces and brackets are
/// counted so that keys inside a nested object or array are visible to the
/// scanner but only keys of the root object (`depth == 1`) are recorded. A key
/// whose value is a nested structure is reported with an empty value rather than
/// fabricating a flattened form of it.
fn parse_flat_json_keys(content: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut chars = content.chars().peekable();
    let mut depth = 0_usize;

    while let Some(character) = chars.next() {
        match character {
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            '"' => {
                let Some(text) = read_json_string(&mut chars) else {
                    break;
                };
                if depth == 1 && consume_colon(&mut chars) {
                    pairs.push((text, read_json_scalar(&mut chars)));
                }
            }
            _ => {}
        }
    }

    pairs
}

/// Reads a JSON string body, returning `None` on an unterminated string.
fn read_json_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    let mut buffer = String::new();
    let mut escaped = false;

    for character in chars.by_ref() {
        if escaped {
            buffer.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '"' => return Some(buffer),
            other => buffer.push(other),
        }
    }

    None
}

/// Consumes whitespace and then an `:` separator, if one follows.
fn consume_colon(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    while matches!(chars.peek(), Some(next) if next.is_whitespace()) {
        chars.next();
    }
    if chars.peek() == Some(&':') {
        chars.next();
        true
    } else {
        false
    }
}

/// Reads one scalar value, leaving any structural character for the caller.
fn read_json_scalar(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    while matches!(chars.peek(), Some(next) if next.is_whitespace()) {
        chars.next();
    }

    match chars.peek() {
        // A nested object or array is not modelled; report it as empty and let
        // the main loop track its depth.
        Some('{' | '[') => return String::new(),
        Some('"') => {
            chars.next();
            return read_json_string(chars).unwrap_or_default();
        }
        None => return String::new(),
        _ => {}
    }

    let mut buffer = String::new();
    while let Some(&character) = chars.peek() {
        if matches!(character, ',' | '}' | ']') {
            break;
        }
        buffer.push(character);
        chars.next();
    }
    buffer.trim().to_owned()
}

/// Game directories an HMCL tree may keep its versions in, for diagnostics.
///
/// [`plan`] discovers the real directories from the listing rather than
/// assuming one of these exists.
#[must_use]
pub fn candidate_game_directories() -> Vec<PathBuf> {
    vec![PathBuf::new(), PathBuf::from(".minecraft")]
}

#[cfg(test)]
mod inline_tests {
    use super::{parse_config, parse_flat_json_keys};

    #[test]
    fn parses_flat_key_value_lines() {
        let pairs =
            parse_config("[General]\n; note\nversion=3.5.3\n\"updatechannel\"=\"stable\"\n");
        assert_eq!(
            pairs,
            vec![
                ("version".to_owned(), "3.5.3".to_owned()),
                ("updatechannel".to_owned(), "stable".to_owned()),
            ]
        );
    }

    #[test]
    fn parses_top_level_json_keys_without_recursing() {
        let pairs = parse_flat_json_keys(
            r#"{"last": "1.20.1", "nested": {"a": "b"}, "count": 3, "list": ["x"]}"#,
        );
        assert_eq!(
            pairs,
            vec![
                ("last".to_owned(), "1.20.1".to_owned()),
                ("nested".to_owned(), String::new()),
                ("count".to_owned(), "3".to_owned()),
                ("list".to_owned(), String::new()),
            ]
        );
    }
}
