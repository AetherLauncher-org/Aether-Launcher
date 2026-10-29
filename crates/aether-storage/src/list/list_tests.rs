//! Fault-tolerance tests for the instance listing pass.
//!
//! Every test builds a real store under the OS temporary directory, plants both
//! healthy and deliberately broken instances, and then checks that
//!
//! * a broken instance is reported with one of the eight stable labels, and
//! * a broken instance never hides the healthy ones.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use aether_core::{GameInstance, InstanceId, InstanceState, VersionId};

use super::{ListError, MAX_INSTANCE_RECORD_BYTES, SkipReason, list_instances, paths_equal};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A unique temporary directory, removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "aether-storage-list-{label}-{}-{unique}",
            std::process::id()
        ));
        // A stale directory from an earlier failed run is wiped first so that the
        // assertions below never see its leftovers.
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temporary directory should be creatable");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn instances(&self) -> PathBuf {
        self.path.join("instances")
    }

    /// Creates `instances/` and returns it.
    fn create_store(&self) -> PathBuf {
        let instances = self.instances();
        fs::create_dir_all(&instances).expect("instances directory");
        instances
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The canonical UUID text of an instance id, as used for directory names.
fn id_text(id: InstanceId) -> String {
    serde_json::to_value(id)
        .expect("instance id should serialize")
        .as_str()
        .expect("instance id should serialize as a string")
        .to_owned()
}

/// Writes a healthy instance into `instances/<id>/instance.json`.
fn plant_instance(temp: &TempDir, name: &str) -> InstanceId {
    let id = InstanceId::new();
    let directory = temp.create_store().join(id_text(id));
    fs::create_dir_all(&directory).expect("instance dir");
    let directory = fs::canonicalize(&directory).expect("canonical instance dir");

    let instance = GameInstance {
        id,
        name: name.to_owned(),
        version: VersionId::parse("1.21.1").expect("version"),
        root: directory.clone(),
        state: InstanceState::Ready,
    };
    fs::write(
        directory.join("instance.json"),
        crate::encode_instance(&instance).expect("encode"),
    )
    .expect("write record");

    id
}

/// Creates `instances/<name>/` and returns it.
fn plant_directory(temp: &TempDir, name: &str) -> PathBuf {
    let directory = temp.create_store().join(name);
    fs::create_dir_all(&directory).expect("instance dir");
    directory
}

/// Name of a UUID-shaped directory that no healthy instance uses.
fn fixed_id(index: u8) -> String {
    format!(
        "{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}-{index:x}{index:x}{index:x}{index:x}-{index:x}{index:x}{index:x}{index:x}-{index:x}{index:x}{index:x}{index:x}-{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}{index:x}"
    )
}

/// Writes `raw` as the record of a UUID-shaped directory.
fn plant_raw_record(temp: &TempDir, directory_name: &str, raw: &[u8]) -> PathBuf {
    let directory = plant_directory(temp, directory_name);
    fs::write(directory.join("instance.json"), raw).expect("write record");
    directory
}

/// Writes a decodable record into a UUID-shaped directory.
fn plant_record(temp: &TempDir, directory_name: &str, instance: &GameInstance) -> PathBuf {
    let raw = crate::encode_instance(instance).expect("encode");
    plant_raw_record(temp, directory_name, raw.as_bytes())
}

/// Builds a record for `directory_name` whose id and root are correct.
fn consistent_instance(directory: &Path, name: &str) -> GameInstance {
    let canonical = fs::canonicalize(directory).expect("canonical dir");
    let directory_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .expect("directory name")
        .to_owned();
    let id = serde_json::from_value(serde_json::Value::String(directory_name)).expect("uuid id");

    GameInstance {
        id,
        name: name.to_owned(),
        version: VersionId::parse("1.21.1").expect("version"),
        root: canonical,
        state: InstanceState::Ready,
    }
}

/// Creates a directory symbolic link, returning `false` when the platform or the
/// current user cannot create links (so the caller can leave early).
#[cfg(windows)]
fn symlink_dir(original: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_dir(original, link).is_ok()
}

#[cfg(not(windows))]
fn symlink_dir(original: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(original, link).is_ok()
}

#[test]
fn lists_a_healthy_instance() {
    let temp = TempDir::new("single");
    let id = plant_instance(&temp, "Survival");

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.instances.len(), 1);
    assert_eq!(listing.instances[0].id, id);
    assert_eq!(listing.instances[0].name, "Survival");
    assert!(listing.skipped.is_empty());
}

#[test]
fn sorts_instances_by_lowercased_name() {
    let temp = TempDir::new("sorted");
    plant_instance(&temp, "charlie");
    plant_instance(&temp, "alpha");
    plant_instance(&temp, "Bravo");

    let listing = list_instances(temp.path()).expect("listing should work");

    let names: Vec<&str> = listing
        .instances
        .iter()
        .map(|instance| instance.name.as_str())
        .collect();
    assert_eq!(names, ["alpha", "Bravo", "charlie"]);
}

#[test]
fn sorts_skipped_entries_by_directory_name() {
    let temp = TempDir::new("skipped-order");
    temp.create_store();
    plant_directory(&temp, &fixed_id(2));
    plant_directory(&temp, &fixed_id(1));
    plant_directory(&temp, &fixed_id(3));

    let listing = list_instances(temp.path()).expect("listing should work");

    let ids: Vec<&str> = listing
        .skipped
        .iter()
        .map(|skipped| skipped.id.as_str())
        .collect();
    assert_eq!(ids, [fixed_id(1), fixed_id(2), fixed_id(3)]);
}

#[test]
fn empty_store_lists_nothing() {
    let temp = TempDir::new("empty");
    temp.create_store();

    let listing = list_instances(temp.path()).expect("listing should work");

    assert!(listing.instances.is_empty());
    assert!(listing.skipped.is_empty());
}

#[test]
fn missing_store_is_an_invalid_store() {
    let temp = TempDir::new("no-store");

    let error = list_instances(temp.path()).expect_err("must fail");
    assert!(matches!(error, ListError::InvalidStore));
}

#[test]
fn a_file_instead_of_the_store_is_an_invalid_store() {
    let temp = TempDir::new("store-file");
    fs::write(temp.instances(), "not a directory").expect("write file");

    let error = list_instances(temp.path()).expect_err("must fail");
    assert!(matches!(error, ListError::InvalidStore));
}

#[test]
fn a_linked_store_root_is_an_invalid_store() {
    let temp = TempDir::new("store-link");
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside dir");
    fs::create_dir_all(temp.instances()).expect("instances dir");
    fs::remove_dir(temp.instances()).expect("remove empty instances dir");
    if !symlink_dir(&outside, &temp.instances()) {
        return; // Symlink creation may be unavailable (privileges/filesystem).
    }

    let error = list_instances(temp.path()).expect_err("must fail");
    assert!(matches!(error, ListError::InvalidStore));
}

#[test]
fn a_symlinked_instance_is_not_followed() {
    let temp = TempDir::new("symlink");
    temp.create_store();
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside dir");
    let link = temp.instances().join(fixed_id(6));
    if !symlink_dir(&outside, &link) {
        return; // Symlink creation may be unavailable (privileges/filesystem).
    }

    let listing = list_instances(temp.path()).expect("listing should work");

    assert!(listing.instances.is_empty());
    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "symbolic-link");
}

#[test]
fn an_oversized_record_is_reported() {
    let temp = TempDir::new("too-large");
    let padding = "x".repeat(MAX_INSTANCE_RECORD_BYTES as usize + 16);
    plant_raw_record(&temp, &fixed_id(3), padding.as_bytes());

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "record-too-large");
}

#[test]
fn an_id_that_does_not_match_the_directory_is_reported() {
    let temp = TempDir::new("id-mismatch");
    let directory = plant_directory(&temp, &fixed_id(4));
    // Everything is consistent except the id, which belongs to another instance.
    let mut instance = consistent_instance(&directory, "Elsewhere");
    instance.id = InstanceId::new();
    plant_record(&temp, &fixed_id(4), &instance);

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "id-mismatch");
}

#[test]
fn an_id_in_a_different_case_is_accepted() {
    let temp = TempDir::new("id-case");
    let directory = plant_directory(&temp, &fixed_id(9).to_uppercase());
    let instance = consistent_instance(&directory, "CaseFold");
    plant_record(&temp, &fixed_id(9).to_uppercase(), &instance);

    let listing = list_instances(temp.path()).expect("listing should work");

    assert!(listing.skipped.is_empty(), "{:?}", listing.skipped);
    assert_eq!(listing.instances.len(), 1);
    assert_eq!(listing.instances[0].name, "CaseFold");
}

#[test]
fn a_root_that_points_elsewhere_is_reported() {
    let temp = TempDir::new("root-mismatch");
    let directory = plant_directory(&temp, &fixed_id(5));
    let mut instance = consistent_instance(&directory, "WrongRoot");
    instance.root = temp.path().join("somewhere-else");

    plant_record(&temp, &fixed_id(5), &instance);

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "root-mismatch");
}

#[test]
fn a_record_written_with_a_verbatim_prefix_is_accepted() {
    let temp = TempDir::new("verbatim-root");
    let directory = plant_directory(&temp, &fixed_id(7));
    // `strip_verbatim` only ever sees the canonical path of the instance
    // directory, so the canonical form must already carry the prefix that
    // `fs::canonicalize` appends on Windows; elsewhere there is nothing to strip.
    let expected_root = fs::canonicalize(&directory).expect("canonical dir");
    if !expected_root.to_string_lossy().starts_with(r"\\?\") {
        return; // Platform without a verbatim prefix in canonical paths.
    }

    let mut instance = consistent_instance(&directory, "Verbatim");
    instance.root = PathBuf::from(expected_root.to_string_lossy().trim_start_matches(r"\\?\"));
    plant_record(&temp, &fixed_id(7), &instance);

    let listing = list_instances(temp.path()).expect("listing should work");

    assert!(listing.skipped.is_empty(), "{:?}", listing.skipped);
    assert_eq!(listing.instances.len(), 1);
    assert_eq!(listing.instances[0].name, "Verbatim");
}

#[test]
fn verbatim_and_plain_spellings_of_a_path_are_equal() {
    assert!(paths_equal(
        Path::new(r"C:\store\instances\1b4e28ba-2fa1-11d2-883f-0016d3cca427"),
        Path::new(r"\\?\C:\store\instances\1b4e28ba-2fa1-11d2-883f-0016d3cca427"),
    ));
    assert!(paths_equal(
        Path::new(r"\\?\C:\store\one"),
        Path::new(r"\\?\C:\store\one"),
    ));
    assert!(!paths_equal(
        Path::new(r"C:\store\one"),
        Path::new(r"C:\store\two"),
    ));
}

#[test]
fn a_truncated_record_is_reported() {
    let temp = TempDir::new("truncated");
    let directory = plant_directory(&temp, &fixed_id(8));
    let instance = consistent_instance(&directory, "Truncated");
    let raw = crate::encode_instance(&instance).expect("encode");
    let truncated = &raw.as_bytes()[..raw.len() / 2];
    plant_raw_record(&temp, &fixed_id(8), truncated);

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "decode-failed");
}

#[test]
fn a_record_with_a_wrong_field_type_is_reported() {
    let temp = TempDir::new("wrong-type");
    plant_raw_record(&temp, &fixed_id(10), br#"{"id": 42}"#);

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "decode-failed");
}

#[test]
fn a_uuid_named_file_is_reported_as_not_a_directory() {
    let temp = TempDir::new("uuid-file");
    let instances = temp.create_store();
    fs::write(instances.join(fixed_id(1)), "x").expect("file");

    let listing = list_instances(temp.path()).expect("listing should work");

    assert!(listing.instances.is_empty());
    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "not-a-directory");
}

#[test]
fn an_instance_without_a_record_is_reported() {
    let temp = TempDir::new("missing-record");
    plant_directory(&temp, &fixed_id(2));

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "missing-record");
}

#[test]
fn a_record_that_is_a_directory_is_reported_as_unsafe() {
    let temp = TempDir::new("record-dir");
    let directory = plant_directory(&temp, &fixed_id(11));
    fs::create_dir(directory.join("instance.json")).expect("record dir");

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "unsafe-record");
}

#[test]
fn a_symlinked_record_is_reported_as_unsafe() {
    let temp = TempDir::new("record-link");
    let directory = plant_directory(&temp, &fixed_id(12));
    let outside = temp.path().join("outside.json");
    let instance = consistent_instance(&directory, "LinkedRecord");
    fs::write(&outside, crate::encode_instance(&instance).expect("encode"))
        .expect("outside record");
    if !symlink_dir(&outside, &directory.join("instance.json")) {
        return; // Symlink creation may be unavailable (privileges/filesystem).
    }

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.skipped.len(), 1);
    assert_eq!(listing.skipped[0].reason, "unsafe-record");
}

#[test]
fn one_broken_instance_does_not_hide_the_healthy_ones() {
    let temp = TempDir::new("resilient");
    plant_instance(&temp, "Alpha");
    plant_instance(&temp, "Beta");

    // Every failure mode at once, each in its own UUID-shaped directory.
    plant_raw_record(&temp, &fixed_id(1), b"{ this is not json");
    plant_directory(&temp, &fixed_id(2));
    let oversized = "y".repeat(MAX_INSTANCE_RECORD_BYTES as usize + 1);
    plant_raw_record(&temp, &fixed_id(3), oversized.as_bytes());
    let mismatched_dir = plant_directory(&temp, &fixed_id(4));
    let mut mismatched = consistent_instance(&mismatched_dir, "Mismatched");
    mismatched.id = InstanceId::new();
    plant_record(&temp, &fixed_id(4), &mismatched);
    let wrong_root_dir = plant_directory(&temp, &fixed_id(5));
    let mut wrong_root = consistent_instance(&wrong_root_dir, "WrongRoot");
    wrong_root.root = temp.path().join("elsewhere");
    plant_record(&temp, &fixed_id(5), &wrong_root);
    fs::write(temp.instances().join(fixed_id(6)), "stray").expect("stray file");

    let listing = list_instances(temp.path()).expect("listing should work");

    let mut names: Vec<&str> = listing
        .instances
        .iter()
        .map(|instance| instance.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["Alpha", "Beta"]);

    let reasons: Vec<&str> = listing
        .skipped
        .iter()
        .map(|skipped| skipped.reason)
        .collect();
    assert_eq!(
        reasons,
        [
            "decode-failed",
            "missing-record",
            "record-too-large",
            "id-mismatch",
            "root-mismatch",
            "not-a-directory",
        ]
    );
}

#[test]
fn skipped_reasons_are_stable_labels_without_paths() {
    let temp = TempDir::new("labels");
    let oversized = "z".repeat(MAX_INSTANCE_RECORD_BYTES as usize + 1);
    plant_raw_record(&temp, &fixed_id(1), oversized.as_bytes());
    plant_raw_record(&temp, &fixed_id(2), b"{}");
    plant_directory(&temp, &fixed_id(3));
    fs::write(temp.instances().join(fixed_id(4)), "stray").expect("stray file");

    let listing = list_instances(temp.path()).expect("listing should work");

    let allowed = [
        "not-a-directory",
        "symbolic-link",
        "missing-record",
        "record-too-large",
        "unsafe-record",
        "decode-failed",
        "id-mismatch",
        "root-mismatch",
    ];
    assert_eq!(listing.skipped.len(), 4);
    for skipped in &listing.skipped {
        assert!(
            allowed.contains(&skipped.reason),
            "unexpected reason label: {:?}",
            skipped.reason
        );
        assert!(
            !skipped.reason.contains('\\') && !skipped.reason.contains('/'),
            "reason must never contain a path: {:?}",
            skipped.reason
        );
    }
}

#[test]
fn non_uuid_entries_are_ignored_silently() {
    let temp = TempDir::new("noise");
    plant_instance(&temp, "Healthy");
    let instances = temp.instances();
    fs::write(instances.join("README.txt"), "hi").expect("file");
    fs::create_dir_all(instances.join("backup")).expect("dir");
    fs::create_dir_all(instances.join(".tmp")).expect("dir");
    // A directory that looks almost like a UUID but is not one byte-exact.
    fs::create_dir_all(instances.join("1b4e28ba-2fa1-11d2-883f-0016d3cca42")).expect("near miss");

    let listing = list_instances(temp.path()).expect("listing should work");

    assert_eq!(listing.instances.len(), 1);
    assert!(
        listing.skipped.is_empty(),
        "noise should not be reported: {:?}",
        listing.skipped
    );
}

#[test]
fn every_skip_reason_has_a_stable_label() {
    let expected = [
        (SkipReason::NotADirectory, "not-a-directory"),
        (SkipReason::SymbolicLink, "symbolic-link"),
        (SkipReason::MissingRecord, "missing-record"),
        (SkipReason::RecordTooLarge, "record-too-large"),
        (SkipReason::UnsafeRecord, "unsafe-record"),
        (SkipReason::DecodeFailed, "decode-failed"),
        (SkipReason::IdMismatch, "id-mismatch"),
        (SkipReason::RootMismatch, "root-mismatch"),
    ];

    for (reason, label) in expected {
        assert_eq!(reason.as_str(), label);
        assert_eq!(reason.to_string(), label);
        // The label is what the desktop shell stores in a `&'static str`.
        let forwarded: &'static str = reason.as_str();
        assert_eq!(forwarded, label);
    }
}
