//! Unit tests for the import planner.
//!
//! The file is pulled in with `#[path = "execute/execute_tests.rs"] mod tests;`
//! from `import/execute.rs`, so it is the *body* of the `tests` module and
//! `super::*` refers to `import::execute`. Do not wrap the contents in another
//! `mod tests { ... }`.
//!
//! Every test builds its own directory below `std::env::temp_dir()` and removes
//! it on drop, so nothing here touches the working directory or a real user
//! profile. The planner never writes, and several tests assert exactly that.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{
    ImportExecuteError, ImportPlan, ImportRequest, ImportSkipReason, MAX_IMPORT_VERSIONS,
    plan_folder_import, plan_prism_import,
};

/// Monotonic counter that keeps temporary directory names unique per process.
static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A throwaway source directory.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "aether-storage-import-plan-{label}-{}-{counter}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("source")).unwrap();
        Self { root }
    }

    fn source(&self) -> PathBuf {
        self.root.join("source")
    }

    /// Writes `versions/<id>/<id>.json` into the source directory.
    fn write_source_version(&self, id: &str, json: &str) -> PathBuf {
        let directory = self.source().join("versions").join(id);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("{id}.json"));
        fs::write(&path, json).unwrap();
        path
    }

    /// Writes a Prism instance with the given base version and loader component.
    fn write_prism_instance(&self, id: &str, name: &str, version: &str, loader_uid: Option<&str>) {
        let directory = self.source().join("instances").join(id);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            self.source().join("prismlauncher.cfg"),
            "InstanceDir=instances\n",
        )
        .unwrap();
        fs::write(directory.join("instance.cfg"), format!("name={name}\n")).unwrap();
        let loader = loader_uid
            .map(|uid| format!(r#",{{"uid":"{uid}","version":"0.16.0"}}"#))
            .unwrap_or_default();
        fs::write(
            directory.join("mmc-pack.json"),
            format!(
                r#"{{"components":[{{"uid":"net.minecraft","version":"{version}"}}{loader}]}}"#
            ),
        )
        .unwrap();
    }

    fn plan(&self, ids: &[&str]) -> ImportPlan {
        let owned: Vec<String> = ids.iter().map(|id| (*id).to_owned()).collect();
        plan_folder_import(&self.source(), &owned).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn skip_reason(plan: &ImportPlan, id: &str) -> Option<ImportSkipReason> {
    plan.skipped
        .iter()
        .find(|(skipped, _)| skipped == id)
        .map(|(_, reason)| *reason)
}

fn owned(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

#[test]
fn a_plain_version_becomes_an_install_request_named_after_it() {
    let fixture = Fixture::new("standalone");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);

    let plan = fixture.plan(&["1.20.1"]);

    assert_eq!(
        plan.requests,
        [ImportRequest {
            id: "1.20.1".to_owned(),
            version: "1.20.1".to_owned(),
            name: "1.20.1".to_owned(),
        }]
    );
    assert!(plan.skipped.is_empty());
    assert_eq!(plan.seed, fs::canonicalize(fixture.source()).unwrap());
}

#[test]
fn planning_never_writes_anything() {
    let fixture = Fixture::new("read-only");
    let json = fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);
    let before = fs::read(&json).unwrap();

    fixture.plan(&["1.20.1", "absent", "../escape"]);

    assert_eq!(fs::read(&json).unwrap(), before);
    let entries: Vec<_> = fs::read_dir(fixture.source()).unwrap().flatten().collect();
    assert_eq!(entries.len(), 1, "only the versions directory may exist");
    assert!(
        !fixture.source().join("versions").join("absent").exists(),
        "a missing version must not be created"
    );
}

#[test]
fn a_version_that_inherits_from_another_needs_a_loader() {
    let fixture = Fixture::new("loader");
    fixture.write_source_version("1.21.1", r#"{"id":"1.21.1"}"#);
    fixture.write_source_version(
        "fabric-loader-0.16-1.21.1",
        r#"{"id":"fabric-loader-0.16-1.21.1","inheritsFrom":"1.21.1"}"#,
    );

    let plan = fixture.plan(&["1.21.1", "fabric-loader-0.16-1.21.1"]);

    assert_eq!(plan.requests.len(), 1);
    assert_eq!(plan.requests[0].version, "1.21.1");
    assert_eq!(
        skip_reason(&plan, "fabric-loader-0.16-1.21.1"),
        Some(ImportSkipReason::NeedsLoader)
    );
}

#[test]
fn a_utf8_byte_order_mark_does_not_hide_a_loader() {
    let fixture = Fixture::new("bom");
    let mut bytes = b"\xEF\xBB\xBF".to_vec();
    bytes.extend_from_slice(br#"{"id":"x","inheritsFrom":"1.20.1"}"#);
    let directory = fixture.source().join("versions").join("x");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("x.json"), bytes).unwrap();

    let plan = fixture.plan(&["x"]);

    assert_eq!(skip_reason(&plan, "x"), Some(ImportSkipReason::NeedsLoader));
}

#[test]
fn a_version_whose_metadata_is_missing_is_reported() {
    let fixture = Fixture::new("missing");

    let plan = fixture.plan(&["1.21"]);

    assert!(plan.requests.is_empty());
    assert_eq!(
        skip_reason(&plan, "1.21"),
        Some(ImportSkipReason::MissingVersionJson)
    );
}

#[test]
fn an_identifier_that_is_not_a_single_path_segment_is_refused() {
    let fixture = Fixture::new("unsafe-id");

    let plan = fixture.plan(&["../escape", "CON", "a/b"]);

    assert!(plan.requests.is_empty());
    for id in ["../escape", "CON", "a/b"] {
        assert_eq!(
            skip_reason(&plan, id),
            Some(ImportSkipReason::UnsafeVersionId),
            "{id} must be refused"
        );
    }
}

#[test]
fn a_version_json_that_is_not_an_object_is_refused() {
    let fixture = Fixture::new("bad-json");
    fixture.write_source_version("broken", "{");

    let plan = fixture.plan(&["broken"]);

    assert_eq!(
        skip_reason(&plan, "broken"),
        Some(ImportSkipReason::InvalidVersionJson)
    );
}

#[test]
fn an_oversized_version_json_is_refused_without_reading_it() {
    let fixture = Fixture::new("big");
    let path = fixture.write_source_version("big", "{}");
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(super::MAX_VERSION_JSON_BYTES + 1).unwrap();

    let plan = fixture.plan(&["big"]);

    assert_eq!(
        skip_reason(&plan, "big"),
        Some(ImportSkipReason::VersionJsonTooLarge)
    );
}

#[test]
fn one_unusable_entry_does_not_stop_the_others() {
    let fixture = Fixture::new("partial");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);

    let plan = fixture.plan(&["1.20.1", "absent"]);

    assert_eq!(plan.requests.len(), 1);
    assert_eq!(
        skip_reason(&plan, "absent"),
        Some(ImportSkipReason::MissingVersionJson)
    );
}

#[test]
fn a_repeated_identifier_is_planned_once() {
    let fixture = Fixture::new("repeat");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);

    let plan = fixture.plan(&["1.20.1", "1.20.1"]);

    assert_eq!(plan.requests.len(), 1);
    assert!(plan.skipped.is_empty());
}

#[test]
fn the_selection_must_be_non_empty_and_bounded() {
    let fixture = Fixture::new("selection");

    assert!(matches!(
        plan_folder_import(&fixture.source(), &[]),
        Err(ImportExecuteError::NothingSelected)
    ));
    let too_many: Vec<String> = (0..=MAX_IMPORT_VERSIONS)
        .map(|index| index.to_string())
        .collect();
    assert!(matches!(
        plan_folder_import(&fixture.source(), &too_many),
        Err(ImportExecuteError::TooManyVersions)
    ));
}

#[test]
fn a_file_or_missing_directory_as_source_is_rejected() {
    let fixture = Fixture::new("file-source");
    let file = fixture.root.join("not-a-directory");
    fs::write(&file, b"{}").unwrap();

    for source in [file, fixture.root.join("absent")] {
        assert!(matches!(
            plan_folder_import(&source, &owned(&["1.20.1"])),
            Err(ImportExecuteError::InvalidSource)
        ));
        assert!(matches!(
            plan_prism_import(&source, &owned(&["a"])),
            Err(ImportExecuteError::InvalidSource)
        ));
    }
}

#[test]
fn a_vanilla_prism_instance_becomes_a_request_named_after_the_instance() {
    let fixture = Fixture::new("prism");
    fixture.write_prism_instance("survival", "Survival", "1.20.4", None);

    let plan = plan_prism_import(&fixture.source(), &owned(&["survival"])).unwrap();

    assert_eq!(
        plan.requests,
        [ImportRequest {
            id: "survival".to_owned(),
            version: "1.20.4".to_owned(),
            name: "Survival".to_owned(),
        }]
    );
    assert_eq!(plan.seed, fs::canonicalize(fixture.source()).unwrap());
}

#[test]
fn a_prism_instance_with_a_loader_needs_a_loader() {
    let fixture = Fixture::new("prism-loader");
    fixture.write_prism_instance(
        "modded",
        "Modded",
        "1.20.4",
        Some("net.fabricmc.fabric-loader"),
    );

    let plan = plan_prism_import(&fixture.source(), &owned(&["modded"])).unwrap();

    assert!(plan.requests.is_empty());
    assert_eq!(
        skip_reason(&plan, "modded"),
        Some(ImportSkipReason::NeedsLoader)
    );
}

#[test]
fn a_prism_instance_without_a_pack_or_with_an_unsafe_id_is_refused() {
    let fixture = Fixture::new("prism-bad");
    fixture.write_prism_instance("ok", "Ok", "1.20.4", None);
    fs::create_dir_all(fixture.source().join("instances").join("no-pack")).unwrap();

    let plan = plan_prism_import(&fixture.source(), &owned(&["no-pack", "../ok", "ok"])).unwrap();

    assert_eq!(
        skip_reason(&plan, "no-pack"),
        Some(ImportSkipReason::MissingPackMetadata)
    );
    assert_eq!(
        skip_reason(&plan, "../ok"),
        Some(ImportSkipReason::UnsafeVersionId)
    );
    assert_eq!(plan.requests.len(), 1);
}

#[test]
fn skip_reasons_have_stable_labels() {
    for (reason, label) in [
        (ImportSkipReason::UnsafeVersionId, "unsafe-version-id"),
        (ImportSkipReason::MissingVersionJson, "missing-version-json"),
        (
            ImportSkipReason::MissingPackMetadata,
            "missing-pack-metadata",
        ),
        (ImportSkipReason::UnsafeSource, "unsafe-source"),
        (
            ImportSkipReason::VersionJsonTooLarge,
            "version-json-too-large",
        ),
        (ImportSkipReason::InvalidVersionJson, "invalid-version-json"),
        (ImportSkipReason::NeedsLoader, "needs-loader"),
        (ImportSkipReason::NotOfficial, "not-official"),
        (ImportSkipReason::DuplicateName, "duplicate-name"),
        (ImportSkipReason::InvalidName, "invalid-name"),
        (ImportSkipReason::InstallFailed, "install-failed"),
        (ImportSkipReason::Cancelled, "cancelled"),
    ] {
        assert_eq!(reason.as_str(), label);
        assert_eq!(reason.to_string(), label);
    }
}

#[cfg(windows)]
#[test]
fn a_symlinked_version_json_is_refused() {
    let fixture = Fixture::new("symlink");
    let outside = fixture.root.join("outside.json");
    fs::write(&outside, r#"{"id":"linked"}"#).unwrap();
    let directory = fixture.source().join("versions").join("linked");
    fs::create_dir_all(&directory).unwrap();
    if std::os::windows::fs::symlink_file(&outside, directory.join("linked.json")).is_err() {
        // Creating a symlink needs developer mode or an elevated shell; the
        // metadata check is still exercised by the other tests.
        return;
    }

    let plan = fixture.plan(&["linked"]);

    assert!(plan.requests.is_empty());
    assert_eq!(
        skip_reason(&plan, "linked"),
        Some(ImportSkipReason::UnsafeSource)
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_version_json_is_refused() {
    let fixture = Fixture::new("symlink");
    let outside = fixture.root.join("outside.json");
    fs::write(&outside, r#"{"id":"linked"}"#).unwrap();
    let directory = fixture.source().join("versions").join("linked");
    fs::create_dir_all(&directory).unwrap();
    std::os::unix::fs::symlink(&outside, directory.join("linked.json")).unwrap();

    let plan = fixture.plan(&["linked"]);

    assert!(plan.requests.is_empty());
    assert_eq!(
        skip_reason(&plan, "linked"),
        Some(ImportSkipReason::UnsafeSource)
    );
}
