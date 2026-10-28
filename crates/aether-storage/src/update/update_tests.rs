//! Behavioural tests for atomic instance-record updates.
//!
//! Every test builds a real directory under the OS temporary directory and then
//! asserts only on observable state: the returned [`GameInstance`], the bytes in
//! `instance.json`, or the listing of the instance directory. Nothing here looks
//! at internal helpers, so the tests stay valid while the implementation is
//! reshaped.
//!
//! The guarantees encoded below are the ones the module promises:
//!
//! * a successful write publishes a complete record and leaves no `*.tmp` behind,
//! * a target that is not an existing plain directory is refused before anything
//!   is written, so a symbolic link can never redirect a write,
//! * an update touches exactly one field of the record that is already on disk,
//! * validation rejects the names and versions that Windows itself would mangle.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use aether_core::{GameInstance, InstanceId, InstanceState, VersionId};

use super::{
    INSTANCE_RECORD_FILE, INSTANCE_RECORD_TEMP_FILE, MAX_INSTANCE_NAME_CHARS,
    MAX_INSTANCE_RECORD_BYTES, UpdateError, rename_instance, set_instance_state,
    set_instance_version, validate_instance_name, write_instance_atomic,
};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A unique temporary directory, removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "aether-storage-update-{label}-{}-{unique}",
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

    /// The instance record inside this directory.
    fn record(&self) -> PathBuf {
        self.path.join(INSTANCE_RECORD_FILE)
    }

    /// The temporary file used by the atomic replace.
    fn staging(&self) -> PathBuf {
        self.path.join(INSTANCE_RECORD_TEMP_FILE)
    }

    /// The raw bytes currently stored in `instance.json`.
    fn record_bytes(&self) -> Vec<u8> {
        fs::read(self.record()).expect("the record should exist")
    }

    /// The record decoded from disk.
    fn stored(&self) -> GameInstance {
        let raw = fs::read_to_string(self.record()).expect("the record should exist");
        crate::decode_instance(&raw).expect("the stored record should decode")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The entry names of a directory, sorted so assertions do not depend on order.
fn entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("the directory should be readable")
        .map(|entry| {
            entry
                .expect("the entry should be readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort_unstable();
    names
}

/// A valid instance rooted at `directory`.
fn instance_in(directory: &Path, name: &str) -> GameInstance {
    GameInstance {
        id: InstanceId::new(),
        name: name.to_owned(),
        version: VersionId::parse("1.21.1").expect("the sample version should parse"),
        root: directory.to_path_buf(),
        game_directory: None,
        state: InstanceState::Ready,
    }
}

/// Creates a directory symbolic link, returning `false` when the platform or the
/// current user cannot create one. On Windows this needs
/// `SeCreateSymbolicLinkPrivilege` or Developer Mode, so callers skip instead of
/// failing when the link could not be created.
#[cfg(windows)]
fn symlink_dir(original: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_dir(original, link).is_ok()
}

#[cfg(not(windows))]
fn symlink_dir(original: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(original, link).is_ok()
}

/// Creates a file symbolic link, returning `false` when the platform or the
/// current user cannot create one. On Windows this needs
/// `SeCreateSymbolicLinkPrivilege` or Developer Mode, so callers skip instead of
/// failing when the link could not be created.
#[cfg(windows)]
fn symlink_file(original: &Path, link: &Path) -> bool {
    std::os::windows::fs::symlink_file(original, link).is_ok()
}

#[cfg(not(windows))]
fn symlink_file(original: &Path, link: &Path) -> bool {
    std::os::unix::fs::symlink(original, link).is_ok()
}

#[test]
fn write_creates_the_record_inside_the_directory() {
    let temp = TempDir::new("write");
    let instance = instance_in(temp.path(), "Survival");

    write_instance_atomic(temp.path(), &instance).expect("the write should succeed");

    assert!(
        temp.record().is_file(),
        "instance.json should be a regular file"
    );
    assert_eq!(temp.stored(), instance);
    assert!(
        !temp.staging().exists(),
        "the temporary file must not survive a successful write"
    );
    assert_eq!(entries(temp.path()), [INSTANCE_RECORD_FILE.to_owned()]);
}

#[test]
fn write_replaces_the_previous_record() {
    let temp = TempDir::new("replace");
    let mut instance = instance_in(temp.path(), "First");
    write_instance_atomic(temp.path(), &instance).expect("the first write should succeed");

    instance.name = "Second".to_owned();
    instance.state = InstanceState::Running;
    write_instance_atomic(temp.path(), &instance).expect("the second write should succeed");

    let stored = temp.stored();
    assert_eq!(stored, instance, "the second write must win");
    assert_eq!(stored.name, "Second");
    assert!(
        !temp.staging().exists(),
        "the temporary file must not survive an overwrite"
    );
    assert_eq!(entries(temp.path()), [INSTANCE_RECORD_FILE.to_owned()]);
}

#[test]
fn write_rejects_a_target_that_is_not_a_plain_directory() {
    let temp = TempDir::new("unsafe-target");

    // A directory that does not exist yet.
    let missing = temp.path().join("not-there");
    let instance = instance_in(&missing, "Survival");
    let error = write_instance_atomic(&missing, &instance).expect_err("a missing directory fails");
    assert!(
        matches!(error, UpdateError::UnsafeDirectory),
        "expected UnsafeDirectory for a missing directory, got {error:?}"
    );
    assert!(
        !missing.exists(),
        "nothing may be created for a missing directory"
    );

    // A regular file standing in for the instance directory.
    let file = temp.path().join("a-file");
    fs::write(&file, b"{\"untouched\":true}").expect("the file should be writable");
    let instance = instance_in(&file, "Survival");
    let error = write_instance_atomic(&file, &instance).expect_err("a regular file fails");
    assert!(
        matches!(error, UpdateError::UnsafeDirectory),
        "expected UnsafeDirectory for a regular file, got {error:?}"
    );
    assert_eq!(
        fs::read(&file).expect("the file should still exist"),
        b"{\"untouched\":true}",
        "a file passed as the instance directory must not be overwritten"
    );

    assert_eq!(
        entries(temp.path()),
        ["a-file".to_owned()],
        "only the planted file may exist afterwards"
    );
}

#[test]
fn write_rejects_a_symlinked_directory() {
    let temp = TempDir::new("link");
    let real = temp.path().join("real");
    fs::create_dir(&real).expect("the real directory should be creatable");
    let link = temp.path().join("link");
    // Windows needs SeCreateSymbolicLinkPrivilege or Developer Mode to create a
    // directory link; when it is unavailable the test degrades to a skip.
    if !symlink_dir(&real, &link) {
        return;
    }
    let instance = instance_in(&link, "Survival");

    let error =
        write_instance_atomic(&link, &instance).expect_err("a symbolic link must be refused");

    assert!(
        matches!(error, UpdateError::UnsafeDirectory),
        "expected UnsafeDirectory for a symbolic link, got {error:?}"
    );
    assert_eq!(
        entries(&real),
        Vec::<String>::new(),
        "a write through a symbolic link must not touch the link target"
    );
}

#[test]
fn write_refuses_a_symlink_at_the_temporary_path() {
    let temp = TempDir::new("temp-link");
    let instance = instance_in(temp.path(), "Survival");
    write_instance_atomic(temp.path(), &instance).expect("the first write should succeed");
    let published = temp.record_bytes();

    // A file outside the instance directory that the temporary name is redirected
    // to: opening it for writing would truncate it, so it must never be opened.
    let target = temp.path().join("outside.json");
    fs::write(&target, b"{\"must\":\"survive\"}").expect("the target should be writable");
    // Windows needs SeCreateSymbolicLinkPrivilege or Developer Mode to create a
    // link; when it is unavailable the test degrades to a skip.
    if !symlink_file(&target, &temp.staging()) {
        return;
    }

    let mut updated = instance.clone();
    updated.name = "Second".to_owned();
    let error = write_instance_atomic(temp.path(), &updated)
        .expect_err("a symbolic link at the temporary name must be refused");

    assert!(
        matches!(error, UpdateError::UnsafeDirectory),
        "expected UnsafeDirectory for a link at the temporary path, got {error:?}"
    );
    assert_eq!(
        fs::read(&target).expect("the link target should still exist"),
        b"{\"must\":\"survive\"}",
        "the write must not follow the link and truncate its target"
    );
    assert_eq!(
        temp.record_bytes(),
        published,
        "the published record must survive a refused write"
    );
}

#[test]
fn rename_changes_only_the_persisted_name() {
    let temp = TempDir::new("rename");
    let instance = instance_in(temp.path(), "Old");
    write_instance_atomic(temp.path(), &instance).expect("the write should succeed");
    let before = temp.stored();

    let updated =
        rename_instance(temp.path(), &instance, "New").expect("the rename should succeed");

    assert_eq!(updated.name, "New");
    assert_eq!(updated.id, before.id, "the id must survive a rename");
    assert_eq!(updated.version, before.version, "the version must survive");
    assert_eq!(updated.root, before.root, "the root must survive a rename");
    assert_eq!(
        updated.state, before.state,
        "the state must survive a rename"
    );

    let stored = temp.stored();
    assert_eq!(stored, updated, "the returned instance must match the disk");
    assert_eq!(stored.name, "New");
}

#[test]
fn rename_rejects_invalid_names_and_keeps_the_record() {
    let temp = TempDir::new("rename-bad");
    let instance = instance_in(temp.path(), "Keep");
    write_instance_atomic(temp.path(), &instance).expect("the write should succeed");
    let before = temp.record_bytes();
    let too_long = "n".repeat(MAX_INSTANCE_NAME_CHARS + 1);

    for name in ["a/b", "CON", "", too_long.as_str()] {
        let error = rename_instance(temp.path(), &instance, name)
            .expect_err("an invalid name must be rejected");
        assert!(
            matches!(error, UpdateError::InvalidName(_)),
            "{name:?} must be rejected with InvalidName, got {error:?}"
        );
        assert_eq!(
            temp.record_bytes(),
            before,
            "a rejected rename must not touch the record"
        );
    }
}

#[test]
fn set_state_persists_the_new_state() {
    let temp = TempDir::new("state");
    let instance = instance_in(temp.path(), "Survival");
    write_instance_atomic(temp.path(), &instance).expect("the write should succeed");

    let updated = set_instance_state(temp.path(), &instance, InstanceState::Running)
        .expect("the state change should succeed");

    assert_eq!(updated.state, InstanceState::Running);
    assert_eq!(updated.id, instance.id);
    assert_eq!(updated.name, "Survival");

    let stored = temp.stored();
    assert_eq!(
        stored.state,
        InstanceState::Running,
        "the state must be persisted"
    );
    assert_eq!(stored, updated);
    assert!(
        !temp.staging().exists(),
        "the temporary file must not survive an update"
    );
}

#[test]
fn set_version_persists_the_new_version() {
    let temp = TempDir::new("version");
    let instance = instance_in(temp.path(), "Survival");
    write_instance_atomic(temp.path(), &instance).expect("the write should succeed");

    let updated = set_instance_version(temp.path(), &instance, "1.20.1")
        .expect("the version change should succeed");

    assert_eq!(updated.version.as_str(), "1.20.1");
    assert_eq!(updated.id, instance.id);

    let stored = temp.stored();
    assert_eq!(
        stored.version.as_str(),
        "1.20.1",
        "the version must be persisted"
    );
    assert_eq!(stored, updated);
}

#[test]
fn set_version_rejects_invalid_versions_and_keeps_the_record() {
    let temp = TempDir::new("version-bad");
    let instance = instance_in(temp.path(), "Survival");
    write_instance_atomic(temp.path(), &instance).expect("the write should succeed");
    let before = temp.record_bytes();
    let too_long = "v".repeat(65);

    for version in ["../1.20", "a/b", "", too_long.as_str()] {
        let error = set_instance_version(temp.path(), &instance, version)
            .expect_err("an invalid version must be rejected");
        assert!(
            matches!(error, UpdateError::InvalidName(_)),
            "{version:?} must be rejected with InvalidName, got {error:?}"
        );
        assert_eq!(
            temp.record_bytes(),
            before,
            "a rejected version change must not touch the record"
        );
    }
}

#[test]
fn update_without_a_record_reports_io() {
    let temp = TempDir::new("no-record");
    let instance = instance_in(temp.path(), "Survival");

    let error = set_instance_state(temp.path(), &instance, InstanceState::Failed)
        .expect_err("an update needs an existing record");

    // The directory itself is fine; only `instance.json` is missing, so this is an
    // I/O failure (`NotFound`) and not an unsafe directory.
    assert!(
        matches!(error, UpdateError::Io(_)),
        "expected Io for a missing record, got {error:?}"
    );
    assert_eq!(
        entries(temp.path()),
        Vec::<String>::new(),
        "a failed update must not leave a record behind"
    );
}

#[test]
fn an_oversized_record_is_rejected_as_too_large() {
    let temp = TempDir::new("too-large");
    let oversized =
        vec![
            b'x';
            usize::try_from(MAX_INSTANCE_RECORD_BYTES).expect("the cap should fit in usize") + 1
        ];
    fs::write(temp.record(), &oversized).expect("the raw record should be writable");
    let instance = instance_in(temp.path(), "Survival");

    let error = set_instance_state(temp.path(), &instance, InstanceState::Running)
        .expect_err("an oversized record must be rejected");

    assert!(
        matches!(error, UpdateError::TooLarge),
        "expected TooLarge for an oversized record, got {error:?}"
    );
    assert_eq!(
        temp.record_bytes(),
        oversized,
        "the oversized record must be left alone"
    );
}

#[test]
fn set_state_refuses_a_handle_with_a_foreign_id() {
    let temp = TempDir::new("foreign-id");
    let resident = instance_in(temp.path(), "Resident");
    write_instance_atomic(temp.path(), &resident).expect("the write should succeed");
    let before = temp.record_bytes();

    // Same directory and same root, but the caller's handle carries another
    // identity: the stored record must not be silently re-labelled.
    let intruder = GameInstance {
        id: InstanceId::new(),
        ..instance_in(temp.path(), "Intruder")
    };
    let outcome = set_instance_state(temp.path(), &intruder, InstanceState::Running);

    // `UnsafeDirectory` is the documented contract for an identity mismatch: the
    // directory is not a safe target for the update that was requested.
    assert!(
        matches!(outcome, Err(UpdateError::UnsafeDirectory)),
        "expected UnsafeDirectory for a foreign id, got {outcome:?}"
    );
    assert_eq!(
        temp.record_bytes(),
        before,
        "a refused update must leave the record byte-identical"
    );
}

#[test]
fn set_state_refuses_a_record_whose_root_points_elsewhere() {
    let temp = TempDir::new("foreign-root");
    let elsewhere = temp.path().join("elsewhere");
    fs::create_dir(&elsewhere).expect("the foreign root should be creatable");

    // Written by hand: a record whose id is perfectly consistent with the handle
    // but which claims to live in another directory, i.e. a record copied in from
    // somewhere else. `write_instance_atomic` is not used because it would only
    // serialize whatever it is handed, and the point here is the mismatch.
    let id = InstanceId::new();
    let instance = GameInstance {
        id,
        name: "Copied".to_owned(),
        version: VersionId::parse("1.21.1").expect("the sample version should parse"),
        root: elsewhere.clone(),
        game_directory: None,
        state: InstanceState::Ready,
    };
    let raw = crate::encode_instance(&instance).expect("the record should encode");
    fs::write(temp.record(), raw.as_bytes()).expect("the raw record should be writable");
    let before = temp.record_bytes();

    let outcome = set_instance_state(temp.path(), &instance, InstanceState::Running);

    assert!(
        matches!(outcome, Err(UpdateError::UnsafeDirectory)),
        "expected UnsafeDirectory for a foreign root, got {outcome:?}"
    );
    assert_eq!(
        temp.record_bytes(),
        before,
        "a refused update must leave the record byte-identical"
    );
    assert_eq!(
        entries(&elsewhere),
        Vec::<String>::new(),
        "the directory the record points at must not be written to"
    );
}

#[test]
fn accepts_ordinary_names() {
    let sixty_four = "a".repeat(64);

    for name in [
        "Survival",
        "1.21 Fabric",
        "我的世界",
        "a.b",
        "CONman",
        sixty_four.as_str(),
    ] {
        let result = validate_instance_name(name);
        assert!(result.is_ok(), "{name:?} should be valid, got {result:?}");
    }
}

#[test]
fn rejects_blank_and_overlong_names() {
    let too_long = "n".repeat(MAX_INSTANCE_NAME_CHARS + 1);

    for name in ["", "   ", " \t ", too_long.as_str()] {
        let result = validate_instance_name(name);
        assert!(
            matches!(result, Err(UpdateError::InvalidName(_))),
            "{name:?} must be rejected, got {result:?}"
        );
    }
}

#[test]
fn rejects_forbidden_and_control_characters() {
    for forbidden in ['\\', '/', ':', '*', '?', '"', '<', '>', '|'] {
        let name = format!("a{forbidden}b");
        let result = validate_instance_name(&name);
        assert!(
            matches!(result, Err(UpdateError::InvalidName(_))),
            "{name:?} must be rejected, got {result:?}"
        );
    }

    for control in ['\u{0}', '\u{7}', '\n', '\r', '\u{1f}', '\u{7f}'] {
        let name = format!("a{control}b");
        let result = validate_instance_name(&name);
        assert!(
            matches!(result, Err(UpdateError::InvalidName(_))),
            "{name:?} must be rejected, got {result:?}"
        );
    }

    // U+0085 lives above ASCII, so a check written as `c < ' '` alone would let
    // it through; `is_control` covers all of U+0000..=U+001F and U+007F..=U+009F.
    assert!(
        '\u{85}'.is_control(),
        "U+0085 must count as a control character"
    );
    let c1 = "a\u{85}b";
    let result = validate_instance_name(c1);
    assert!(
        matches!(result, Err(UpdateError::InvalidName(_))),
        "a C1 control character must be rejected, got {result:?}"
    );
}

#[test]
fn rejects_path_edges_and_reserved_device_names() {
    for name in [".", "..", "name.", "name "] {
        let result = validate_instance_name(name);
        assert!(
            matches!(result, Err(UpdateError::InvalidName(_))),
            "{name:?} must be rejected, got {result:?}"
        );
    }

    for name in [
        "CON",
        "con",
        "Nul",
        "aux",
        "COM1",
        "com9",
        "LPT1",
        "lpt9",
        "CON.txt",
        "Lpt9",
        "aux.tar.gz",
    ] {
        let result = validate_instance_name(name);
        assert!(
            matches!(result, Err(UpdateError::InvalidName(_))),
            "{name:?} must be rejected as a device name, got {result:?}"
        );
    }
}

#[test]
fn name_limit_counts_characters_not_bytes() {
    assert_eq!(
        MAX_INSTANCE_NAME_CHARS, 64,
        "the documented name limit is 64 characters"
    );

    // 64 characters but 192 bytes: a `name.len()` based check would reject this.
    let sixty_four = "世界".repeat(32);
    assert_eq!(sixty_four.chars().count(), 64);
    assert_eq!(sixty_four.len(), 192);
    let result = validate_instance_name(&sixty_four);
    assert!(
        result.is_ok(),
        "64 multi-byte characters sit exactly on the limit and must be accepted, got {result:?}"
    );

    // One character more, with the same byte-heavy prefix, must be rejected.
    let sixty_five = format!("{sixty_four}世");
    assert_eq!(sixty_five.chars().count(), 65);
    let result = validate_instance_name(&sixty_five);
    assert!(
        matches!(result, Err(UpdateError::InvalidName(_))),
        "65 characters must be rejected even though they are only 3 bytes over the 64-character string, got {result:?}"
    );
}
