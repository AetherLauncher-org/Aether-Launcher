//! Unit tests for the import execution layer.
//!
//! The file is pulled in with `#[path = "execute/execute_tests.rs"] mod tests;`
//! from `import/execute.rs`, so it is the *body* of the `tests` module and
//! `super::*` refers to `import::execute`. Do not wrap the contents in another
//! `mod tests { ... }`.
//!
//! Every test builds its own directory below `std::env::temp_dir()` and removes
//! it on drop, so nothing here touches the working directory or a real user
//! profile.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{ImportExecuteError, ImportSkipReason, import_versions};

/// Monotonic counter that keeps temporary directory names unique per process.
static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A throwaway layout with a `source/`, `game/` and `store/` directory.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "aether-storage-import-exec-{label}-{}-{counter}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        for directory in ["source", "game", "store"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        Self { root }
    }

    fn source(&self) -> PathBuf {
        self.root.join("source")
    }

    fn game(&self) -> PathBuf {
        self.root.join("game")
    }

    fn store(&self) -> PathBuf {
        self.root.join("store")
    }

    /// Writes `versions/<id>/<id>.json` into the source directory.
    fn write_source_version(&self, id: &str, json: &str) -> PathBuf {
        let directory = self.source().join("versions").join(id);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("{id}.json"));
        fs::write(&path, json).unwrap();
        path
    }

    /// Reads a version JSON out of the managed game tree.
    fn read_game_version(&self, id: &str) -> Option<String> {
        fs::read_to_string(
            self.game()
                .join("versions")
                .join(id)
                .join(format!("{id}.json")),
        )
        .ok()
    }

    fn import(&self, ids: &[&str]) -> super::ImportOutcome {
        let owned: Vec<String> = ids.iter().map(|id| (*id).to_owned()).collect();
        import_versions(&self.source(), &self.game(), &self.store(), &owned).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn skip_reason(outcome: &super::ImportOutcome, id: &str) -> Option<ImportSkipReason> {
    outcome
        .skipped
        .iter()
        .find(|(skipped, _)| skipped == id)
        .map(|(_, reason)| *reason)
}

#[test]
fn imports_a_standalone_version_into_the_game_tree() {
    let fixture = Fixture::new("standalone");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);

    let outcome = fixture.import(&["1.20.1"]);

    assert_eq!(outcome.imported.len(), 1);
    assert!(outcome.skipped.is_empty());
    assert_eq!(outcome.imported[0].version.as_str(), "1.20.1");
    assert_eq!(outcome.imported[0].name, "1.20.1");
    assert_eq!(
        fixture.read_game_version("1.20.1").as_deref(),
        Some(r#"{"id":"1.20.1"}"#)
    );
}

#[test]
fn copies_every_ancestor_of_an_inheriting_version() {
    let fixture = Fixture::new("chain");
    fixture.write_source_version("1.21.1", r#"{"id":"1.21.1"}"#);
    fixture.write_source_version(
        "fabric-loader-0.16",
        r#"{"id":"fabric-loader-0.16","inheritsFrom":"1.21.1"}"#,
    );

    let outcome = fixture.import(&["fabric-loader-0.16"]);

    assert_eq!(outcome.imported.len(), 1);
    assert!(fixture.read_game_version("fabric-loader-0.16").is_some());
    assert!(
        fixture.read_game_version("1.21.1").is_some(),
        "the parent manifest has to be present or the loader cannot resolve the child"
    );
}

#[test]
fn an_existing_version_json_is_kept_and_an_instance_is_still_created() {
    let fixture = Fixture::new("already-installed");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1","source":"foreign"}"#);

    // 从文件夹导入时源和目标常常是同一个 `.minecraft`，那个版本早就装好了。
    let installed = fixture.game().join("versions").join("1.20.1");
    fs::create_dir_all(&installed).unwrap();
    fs::write(
        installed.join("1.20.1.json"),
        r#"{"id":"1.20.1","source":"official"}"#,
    )
    .unwrap();

    let outcome = fixture.import(&["1.20.1"]);

    // 文件已存在不是错误：直接给这个版本建实例，不再报「重名」。
    assert_eq!(outcome.imported.len(), 1);
    assert!(outcome.skipped.is_empty());
    assert_eq!(
        fixture.read_game_version("1.20.1").as_deref(),
        Some(r#"{"id":"1.20.1","source":"official"}"#),
        "the installed version must be left untouched"
    );
}

#[test]
fn reports_a_version_whose_metadata_is_missing() {
    let fixture = Fixture::new("missing");
    let outcome = fixture.import(&["1.21"]);

    assert!(outcome.imported.is_empty());
    assert_eq!(
        skip_reason(&outcome, "1.21"),
        Some(ImportSkipReason::MissingVersionJson)
    );
}

#[test]
fn reports_a_missing_parent_instead_of_half_importing() {
    let fixture = Fixture::new("missing-parent");
    fixture.write_source_version("child", r#"{"id":"child","inheritsFrom":"absent"}"#);

    let outcome = fixture.import(&["child"]);

    assert!(outcome.imported.is_empty());
    assert_eq!(
        skip_reason(&outcome, "child"),
        Some(ImportSkipReason::MissingVersionJson)
    );
    assert!(
        fixture.read_game_version("child").is_none(),
        "nothing may be published when the chain cannot be read completely"
    );
}

#[test]
fn rejects_an_identifier_that_is_not_a_single_path_segment() {
    let fixture = Fixture::new("unsafe-id");
    let outcome = fixture.import(&["../escape", "CON", "a/b"]);

    assert!(outcome.imported.is_empty());
    for id in ["../escape", "CON", "a/b"] {
        assert_eq!(
            skip_reason(&outcome, id),
            Some(ImportSkipReason::UnsafeVersionId),
            "{id} must be refused"
        );
    }
}

#[test]
fn detects_an_inheritance_cycle() {
    let fixture = Fixture::new("cycle");
    fixture.write_source_version("one", r#"{"id":"one","inheritsFrom":"two"}"#);
    fixture.write_source_version("two", r#"{"id":"two","inheritsFrom":"one"}"#);

    let outcome = fixture.import(&["one"]);

    assert!(outcome.imported.is_empty());
    assert_eq!(
        skip_reason(&outcome, "one"),
        Some(ImportSkipReason::InheritanceCycle)
    );
}

#[test]
fn rejects_a_version_json_that_is_not_an_object() {
    let fixture = Fixture::new("bad-json");
    fixture.write_source_version("broken", "{");

    let outcome = fixture.import(&["broken"]);

    assert!(outcome.imported.is_empty());
    assert_eq!(
        skip_reason(&outcome, "broken"),
        Some(ImportSkipReason::InvalidVersionJson)
    );
}

#[test]
fn a_second_import_of_the_same_version_is_reported_as_a_duplicate() {
    let fixture = Fixture::new("duplicate");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);

    assert_eq!(fixture.import(&["1.20.1"]).imported.len(), 1);
    let second = fixture.import(&["1.20.1"]);

    assert!(second.imported.is_empty());
    assert_eq!(
        skip_reason(&second, "1.20.1"),
        Some(ImportSkipReason::DuplicateName)
    );
}

#[test]
fn one_failing_version_does_not_stop_the_others() {
    let fixture = Fixture::new("partial");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);

    let outcome = fixture.import(&["1.20.1", "absent"]);

    assert_eq!(outcome.imported.len(), 1);
    assert_eq!(
        skip_reason(&outcome, "absent"),
        Some(ImportSkipReason::MissingVersionJson)
    );
}

#[test]
fn an_empty_selection_is_an_error() {
    let fixture = Fixture::new("empty");
    let error =
        import_versions(&fixture.source(), &fixture.game(), &fixture.store(), &[]).unwrap_err();
    assert!(matches!(error, ImportExecuteError::NothingSelected));
}

#[test]
fn a_file_as_source_is_rejected() {
    let fixture = Fixture::new("file-source");
    let file = fixture.root.join("not-a-directory");
    fs::write(&file, b"{}").unwrap();

    let error = import_versions(
        &file,
        &fixture.game(),
        &fixture.store(),
        &["1.20.1".to_owned()],
    )
    .unwrap_err();

    assert!(matches!(error, ImportExecuteError::InvalidSource));
}

#[test]
fn a_missing_game_directory_aborts_the_import() {
    let fixture = Fixture::new("missing-game");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);
    let game = fixture.root.join("game-not-created");

    let error = import_versions(
        &fixture.source(),
        &game,
        &fixture.store(),
        &["1.20.1".to_owned()],
    )
    .expect_err("an absent game directory must be rejected");

    assert!(
        matches!(error, ImportExecuteError::InvalidGameRoot),
        "expected InvalidGameRoot, got {error:?}"
    );
    assert!(
        !game.exists(),
        "the game directory must not be created as a side effect"
    );
}

#[test]
fn a_repeated_identifier_is_imported_once() {
    let fixture = Fixture::new("repeat");
    fixture.write_source_version("1.20.1", r#"{"id":"1.20.1"}"#);

    let outcome = fixture.import(&["1.20.1", "1.20.1"]);

    assert_eq!(outcome.imported.len(), 1);
    assert!(outcome.skipped.is_empty());
}

#[test]
fn skip_reasons_have_stable_labels() {
    for (reason, label) in [
        (ImportSkipReason::UnsafeVersionId, "unsafe-version-id"),
        (ImportSkipReason::MissingVersionJson, "missing-version-json"),
        (ImportSkipReason::UnsafeSource, "unsafe-source"),
        (
            ImportSkipReason::VersionJsonTooLarge,
            "version-json-too-large",
        ),
        (ImportSkipReason::InvalidVersionJson, "invalid-version-json"),
        (ImportSkipReason::InheritanceTooDeep, "inheritance-too-deep"),
        (ImportSkipReason::InheritanceCycle, "inheritance-cycle"),
        (ImportSkipReason::DuplicateName, "duplicate-name"),
        (ImportSkipReason::InvalidName, "invalid-name"),
        (ImportSkipReason::Io, "io"),
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

    let outcome = fixture.import(&["linked"]);

    assert!(outcome.imported.is_empty());
    assert_eq!(
        skip_reason(&outcome, "linked"),
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

    let outcome = fixture.import(&["linked"]);

    assert!(outcome.imported.is_empty());
    assert_eq!(
        skip_reason(&outcome, "linked"),
        Some(ImportSkipReason::UnsafeSource)
    );
}
