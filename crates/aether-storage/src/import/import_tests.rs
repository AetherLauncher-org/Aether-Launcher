//! Unit tests for the import detection and planning layer.
//!
//! The file is pulled in with `#[path = "import_tests.rs"] mod tests;` from
//! `import/mod.rs`, so it is the *body* of the `tests` module: `super::*` refers
//! to `import`. Do not wrap the contents in another `mod tests { ... }`.
//!
//! Most tests feed a hand-built [`ExistingFiles`] snapshot to the pure
//! `detect` / `plan_candidates` functions. Only the `scan_directory` tests touch
//! the file system, and they use a unique directory below
//! `std::env::temp_dir()` that is removed on drop — never the working directory
//! and never a real user profile.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::hmcl;
use super::pcl2;
use super::{ImportError, ImportSourceKind, detect, plan_candidates, scan_directory};

/// Monotonic counter that keeps temporary directory names unique per process.
static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A temporary directory removed when it goes out of scope.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a fresh, uniquely named directory below the OS temp directory.
    fn new(label: &str) -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aether-storage-import-{label}-{}-{counter}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    /// Root of the temporary directory.
    fn path(&self) -> &Path {
        &self.path
    }

    /// Creates a file (and its parents) inside the temporary directory.
    fn touch(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, b"{}").unwrap();
        path
    }

    /// Creates an empty directory inside the temporary directory.
    fn mkdir(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// --- marker detection -------------------------------------------------------

#[test]
fn detects_hmcl_from_dot_config_marker() {
    let snapshot = super::ExistingFiles::new([".hmcl.json", "versions/1.20.1/1.20.1.json"]);
    assert_eq!(detect(&snapshot), Some(ImportSourceKind::Hmcl));
}

#[test]
fn detects_hmcl_from_hmcl_json_marker() {
    let snapshot = super::ExistingFiles::new(["hmcl.json"]);
    assert_eq!(detect(&snapshot), Some(ImportSourceKind::Hmcl));
}

#[test]
fn detects_hmcl_from_hmclversion_cfg() {
    let snapshot = super::ExistingFiles::new(["hmclversion.cfg"]);
    assert_eq!(detect(&snapshot), Some(ImportSourceKind::Hmcl));
}

#[test]
fn detects_hmcl_marker_in_a_subdirectory() {
    // HMCL keeps its settings next to the launcher, which may be one level up
    // from the scanned root.
    let snapshot = super::ExistingFiles::new(["HMCL-3.5/hmclversion.cfg"]);
    assert_eq!(detect(&snapshot), Some(ImportSourceKind::Hmcl));
}

#[test]
fn returns_none_without_any_launcher_marker() {
    let snapshot = super::ExistingFiles::new([
        "versions/1.20.1/1.20.1.json",
        "assets/index.json",
        "libraries/com/example/lib.jar",
    ]);
    assert_eq!(detect(&snapshot), None);
}

#[test]
fn pcl2_marker_wins_over_hmcl_marker() {
    let snapshot = super::ExistingFiles::new(["PCL/Setup.ini", ".hmcl.json"]);
    assert_eq!(detect(&snapshot), Some(ImportSourceKind::Pcl2));
}

#[test]
fn hmcl_marker_does_not_leak_into_pcl2_matching() {
    let snapshot = super::ExistingFiles::new([".hmcl.json"]);
    assert!(!pcl2::matches(&snapshot));
    assert!(hmcl::matches(&snapshot));
}

// --- planning ---------------------------------------------------------------

#[test]
fn plans_a_candidate_from_version_metadata() {
    let snapshot = super::ExistingFiles::new([".hmcl.json", "versions/1.20.1/1.20.1.json"]);
    let candidates = plan_candidates(ImportSourceKind::Hmcl, &snapshot);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].version_id, "1.20.1");
    assert_eq!(candidates[0].source_kind, ImportSourceKind::Hmcl);
    assert_eq!(candidates[0].game_directory, PathBuf::new());
    assert_eq!(
        candidates[0].planned_relative_paths(),
        vec!["versions/1.20.1/1.20.1.json".to_owned()]
    );
}

#[test]
fn single_version_is_not_isolated() {
    let snapshot = super::ExistingFiles::new([
        ".hmcl.json",
        "versions/1.20.1/1.20.1.json",
        "versions/1.20.1/1.20.1.jar",
    ]);
    let candidates = hmcl::plan(&snapshot);

    assert_eq!(candidates.len(), 1);
    assert!(!candidates[0].version_isolation);
}

#[test]
fn multiple_versions_are_isolated() {
    let snapshot = super::ExistingFiles::new([
        ".hmcl.json",
        "versions/1.20.1/1.20.1.json",
        "versions/1.21/1.21.json",
        "versions/1.16.5/1.16.5.json",
    ]);
    let candidates = hmcl::plan(&snapshot);

    assert_eq!(candidates.len(), 3);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.version_isolation)
    );

    let mut ids: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.version_id.as_str())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["1.16.5", "1.20.1", "1.21"]);
}

#[test]
fn ignores_version_folders_without_matching_metadata() {
    // `versions/1.20.1/1.20.1.jar` alone is not enough: only the JSON counts.
    let snapshot = super::ExistingFiles::new([
        ".hmcl.json",
        "versions/1.20.1/1.20.1.jar",
        "versions/1.21/modified.json",
    ]);
    assert!(hmcl::plan(&snapshot).is_empty());
}

#[test]
fn discovers_versions_below_a_minecraft_directory() {
    let snapshot =
        super::ExistingFiles::new([".hmcl.json", ".minecraft/versions/1.20.1/1.20.1.json"]);
    let candidates = hmcl::plan(&snapshot);

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].game_directory, PathBuf::from(".minecraft"));
}

#[test]
fn existing_targets_are_skipped_not_overwritten() {
    let snapshot = super::ExistingFiles::new([".hmcl.json", "versions/1.20.1/1.20.1.json"]);
    let mut candidates = hmcl::plan(&snapshot);
    assert_eq!(candidates.len(), 1);

    let destination = super::ExistingFiles::new(["versions/1.20.1/1.20.1.json"]);
    let candidate = candidates.remove(0).with_existing(&destination);

    assert_eq!(
        candidate.existing_targets,
        vec!["versions/1.20.1/1.20.1.json".to_owned()]
    );
    assert!(candidate.missing_targets.is_empty());
    assert!(candidate.is_fully_present());
}

#[test]
fn absent_targets_stay_in_missing_targets() {
    let snapshot = super::ExistingFiles::new([".hmcl.json", "versions/1.20.1/1.20.1.json"]);
    let mut candidates = hmcl::plan(&snapshot);
    let candidate = candidates
        .remove(0)
        .with_existing(&super::ExistingFiles::new(["versions/1.21/1.21.json"]));

    assert!(candidate.existing_targets.is_empty());
    assert_eq!(
        candidate.missing_targets,
        vec!["versions/1.20.1/1.20.1.json".to_owned()]
    );
    assert!(!candidate.is_fully_present());
}

#[test]
fn existing_match_is_case_insensitive() {
    let snapshot = super::ExistingFiles::new([".hmcl.json", "versions/1.20.1/1.20.1.json"]);
    let mut candidates = hmcl::plan(&snapshot);
    let candidate = candidates
        .remove(0)
        .with_existing(&super::ExistingFiles::new(["VERSIONS/1.20.1/1.20.1.JSON"]));

    assert!(candidate.is_fully_present());
}

// --- directory scanning -----------------------------------------------------

#[test]
fn scan_directory_builds_relative_slash_paths() {
    let temp = TempDir::new("snapshot");
    temp.touch(".hmcl.json");
    temp.touch("versions/1.20.1/1.20.1.json");
    temp.mkdir("versions/1.20.1/natives");

    let snapshot = scan_directory(temp.path()).unwrap();

    assert!(snapshot.contains(".hmcl.json"));
    assert!(snapshot.contains("versions/1.20.1/1.20.1.json"));
    assert!(snapshot.contains("versions/1.20.1/natives"));
    assert_eq!(detect(&snapshot), Some(ImportSourceKind::Hmcl));
    assert_eq!(plan_candidates(ImportSourceKind::Hmcl, &snapshot).len(), 1);
}

#[test]
fn scan_directory_rejects_a_file_as_root() {
    let temp = TempDir::new("file-root");
    let file = temp.touch("not-a-directory");

    match scan_directory(&file) {
        Err(ImportError::InvalidRoot) => {}
        other => panic!("expected InvalidRoot, got {other:?}"),
    }
}

#[test]
fn scan_directory_rejects_a_missing_root() {
    let temp = TempDir::new("missing-root");
    let missing = temp.path().join("does-not-exist");

    match scan_directory(&missing) {
        Err(ImportError::InvalidRoot) => {}
        other => panic!("expected InvalidRoot, got {other:?}"),
    }
}

#[cfg(windows)]
#[test]
fn scan_directory_skips_symlinks() {
    let temp = TempDir::new("symlink");
    let scan = temp.mkdir("scan");
    let target = temp.mkdir("outside-target");
    fs::write(target.join("secret.json"), b"{}").unwrap();

    let link = scan.join("linked");
    if std::os::windows::fs::symlink_dir(&target, &link).is_err() {
        // Creating a symlink needs developer mode or an elevated shell; the
        // behaviour under test is still covered by the file-metadata check.
        return;
    }

    let snapshot = scan_directory(&scan).unwrap();
    assert!(!snapshot.contains("linked"));
    assert!(
        !snapshot
            .entries
            .iter()
            .any(|entry| entry.ends_with("secret.json"))
    );
}

#[cfg(unix)]
#[test]
fn scan_directory_skips_symlinks() {
    let temp = TempDir::new("symlink");
    let scan = temp.mkdir("scan");
    let target = temp.mkdir("outside-target");
    fs::write(target.join("secret.json"), b"{}").unwrap();
    std::os::unix::fs::symlink(&target, scan.join("linked")).unwrap();

    let snapshot = scan_directory(&scan).unwrap();
    assert!(!snapshot.contains("linked"));
    assert!(
        !snapshot
            .entries
            .iter()
            .any(|entry| entry.ends_with("secret.json"))
    );
}

#[test]
fn scan_directory_stops_at_max_scan_depth() {
    let temp = TempDir::new("depth");
    // levels 1..=MAX_SCAN_DEPTH are listed, the file below the deepest allowed
    // directory is not.
    let mut relative = String::new();
    for level in 1..=super::MAX_SCAN_DEPTH + 1 {
        if !relative.is_empty() {
            relative.push('/');
        }
        relative.push_str(&format!("level{level}"));
    }
    temp.touch(&format!("{relative}/too-deep.json"));

    let snapshot = scan_directory(temp.path()).unwrap();

    assert!(snapshot.contains("level1"));
    assert!(
        !snapshot
            .entries
            .iter()
            .any(|entry| entry.ends_with("too-deep.json"))
    );
}
