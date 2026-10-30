//! Behavioural tests for creating a new managed instance.
//!
//! Every test builds a real store under the OS temporary directory and asserts
//! only on observable state: the returned [`GameInstance`], the recorded bytes,
//! or the entries of a directory. The guarantees encoded here are:
//!
//! * one successful call produces exactly `<base>/instances/<uuid>/` containing
//!   `instance.json` and an empty `minecraft/`,
//! * the directory name is the instance id, and `root` is its canonical path,
//! * a base that is missing, a file or a symbolic link is refused before anything
//!   is written,
//! * invalid input and duplicate names are refused without leaving a directory
//!   behind, and an existing healthy instance is never disturbed.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use aether_core::InstanceState;

use super::{CreateError, create_instance};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A unique temporary directory holding one store, removed on drop.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "aether-storage-create-{label}-{}-{unique}",
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

    /// `<base>/instances`, the directory holding one subdirectory per instance.
    fn instances(&self) -> PathBuf {
        self.path.join("instances")
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

/// The canonical path of the directory an instance should have been given.
fn expected_root(temp: &TempDir, id: impl std::fmt::Display) -> PathBuf {
    fs::canonicalize(temp.instances().join(id.to_string())).expect("the instance directory exists")
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

#[test]
fn create_persists_the_requested_instance() {
    let temp = TempDir::new("happy");

    let instance =
        create_instance(temp.path(), "Survival", "1.21.1").expect("creation should succeed");

    assert_eq!(instance.name, "Survival");
    assert_eq!(instance.version.as_str(), "1.21.1");
    assert_eq!(instance.state, InstanceState::Incomplete);

    let directory = temp.instances().join(instance.id.to_string());
    assert!(
        directory.is_dir(),
        "the instance should live in a directory named after its id"
    );
    assert_eq!(
        instance.root,
        fs::canonicalize(&directory).expect("the instance directory should canonicalize"),
        "root must be the canonical path of the instance directory"
    );

    let raw = fs::read_to_string(instance.root.join("instance.json"))
        .expect("the record should have been written");
    let stored = crate::decode_instance(&raw).expect("the record should decode");
    assert_eq!(stored, instance, "the record must round-trip exactly");
    assert_eq!(stored.root, instance.root, "the stored root must match");
}

#[test]
fn create_builds_the_minecraft_directory_with_default_options() {
    let temp = TempDir::new("skeleton");

    let instance =
        create_instance(temp.path(), "Skeleton", "1.21.1").expect("creation should succeed");

    let minecraft = instance.root.join("minecraft");
    assert!(minecraft.is_dir(), "minecraft/ should be a directory");
    // 全新实例只放一个 options.txt：跳过辅助功能向导、直接进中文。
    assert_eq!(
        entries(&minecraft),
        ["options.txt".to_owned()],
        "minecraft/ should hold the default options only"
    );
    let options = std::fs::read_to_string(minecraft.join("options.txt")).expect("options.txt");
    assert!(options.contains("lang:zh_cn"), "unexpected: {options}");
    assert!(options.contains("onboardAccessibility:false"));
    assert_eq!(
        entries(&instance.root),
        ["instance.json".to_owned(), "minecraft".to_owned()],
        "the instance directory should hold the record and the skeleton only"
    );
}

#[test]
fn a_new_instance_keeps_language_case_for_old_versions() {
    let temp = TempDir::new("old-version");

    // 1.10 及更早只认大写的 zh_CN。
    let old = create_instance(temp.path(), "Old", "1.10.2").expect("creation should succeed");
    let options =
        std::fs::read_to_string(old.root.join("minecraft").join("options.txt")).expect("options");
    assert!(options.contains("lang:zh_CN"), "unexpected: {options}");
}

#[test]
fn create_does_not_overwrite_an_existing_options_file() {
    let temp = TempDir::new("existing-options");

    // 整合包常常自带 options.txt，不能被启动器的默认值覆盖。
    let first = create_instance(temp.path(), "First", "1.21.1").expect("creation should succeed");
    let minecraft = first.root.join("minecraft");
    let options = minecraft.join("options.txt");
    std::fs::write(&options, "lang:en_us\n").expect("write");

    super::write_default_options(&minecraft, "1.21.1").expect("writing should succeed");
    assert_eq!(
        std::fs::read_to_string(&options).expect("read"),
        "lang:en_us\n"
    );
}

#[test]
fn create_makes_the_instances_directory_when_it_is_missing() {
    let temp = TempDir::new("fresh-store");
    assert!(
        !temp.instances().exists(),
        "the store starts without an instances/ directory"
    );

    let instance =
        create_instance(temp.path(), "First", "1.21.1").expect("creation should succeed");

    assert!(temp.instances().is_dir(), "instances/ should be created");
    assert!(instance.root.is_dir());
}

#[test]
fn create_rejects_a_missing_base() {
    let temp = TempDir::new("missing-base");
    let missing = temp.path().join("no-such-base");

    let error =
        create_instance(&missing, "Survival", "1.21.1").expect_err("a missing base must fail");

    assert!(
        matches!(error, CreateError::UnsafeBase),
        "expected UnsafeBase, got {error:?}"
    );
    assert!(
        !missing.exists(),
        "a rejected base must not be created as a side effect"
    );
}

#[test]
fn create_rejects_a_regular_file_as_base() {
    let temp = TempDir::new("file-base");
    let file = temp.path().join("not-a-directory");
    fs::write(&file, b"not a store").expect("the file should be writable");

    let error = create_instance(&file, "Survival", "1.21.1").expect_err("a file base must fail");

    assert!(
        matches!(error, CreateError::UnsafeBase),
        "expected UnsafeBase, got {error:?}"
    );
    assert_eq!(
        fs::read(&file).expect("the file should still exist"),
        b"not a store",
        "a file passed as the store root must not be modified"
    );
    assert!(!temp.instances().exists(), "nothing may be created");
}

#[test]
fn create_rejects_a_symlinked_base() {
    let temp = TempDir::new("link-base");
    let real = temp.path().join("real");
    fs::create_dir(&real).expect("the real store should be creatable");
    let link = temp.path().join("link");
    // Windows needs SeCreateSymbolicLinkPrivilege or Developer Mode to create a
    // directory link; when it is unavailable the test degrades to a skip.
    if !symlink_dir(&real, &link) {
        return;
    }

    let error = create_instance(&link, "Survival", "1.21.1").expect_err("a link base must fail");

    assert!(
        matches!(error, CreateError::UnsafeBase),
        "expected UnsafeBase for a symlinked base, got {error:?}"
    );
    assert_eq!(
        entries(&real),
        Vec::<String>::new(),
        "a write through a symbolic link must not touch the link target"
    );
}

#[test]
fn create_rejects_a_symlinked_instances_directory() {
    let temp = TempDir::new("link-instances");
    // A real directory outside the store that `<base>/instances` is redirected to.
    let real_instances = temp.path().join("real-instances");
    fs::create_dir(&real_instances).expect("the real instances directory should be creatable");
    // Windows needs SeCreateSymbolicLinkPrivilege or Developer Mode to create a
    // directory link; when it is unavailable the test degrades to a skip.
    if !symlink_dir(&real_instances, &temp.instances()) {
        return;
    }

    let error = create_instance(temp.path(), "Survival", "1.21.1")
        .expect_err("a symlinked instances directory must be refused");

    assert!(
        matches!(error, CreateError::UnsafeBase),
        "expected UnsafeBase for a symlinked instances directory, got {error:?}"
    );
    assert_eq!(
        entries(&real_instances),
        Vec::<String>::new(),
        "creation through a symbolic link must not touch the link target"
    );
}

#[test]
fn create_rejects_a_duplicate_name_ignoring_case() {
    let temp = TempDir::new("duplicate");
    create_instance(temp.path(), "Survival", "1.21.1").expect("the first creation should succeed");
    let before = entries(&temp.instances());

    let error = create_instance(temp.path(), "survival", "1.20.1")
        .expect_err("a case-insensitive duplicate must be rejected");

    assert!(
        matches!(error, CreateError::DuplicateName(_)),
        "expected DuplicateName, got {error:?}"
    );
    assert_eq!(
        entries(&temp.instances()),
        before,
        "a rejected duplicate must not leave a directory behind"
    );
}

#[test]
fn create_rejects_invalid_names_without_creating_anything() {
    let temp = TempDir::new("bad-name");
    let too_long = "n".repeat(65);

    for name in ["", "  ", "a/b", "CON", "name.", too_long.as_str()] {
        let error = create_instance(temp.path(), name, "1.21.1")
            .expect_err("an invalid name must be rejected");
        assert!(
            matches!(error, CreateError::InvalidName(_)),
            "{name:?} must be rejected with InvalidName, got {error:?}"
        );
        assert_eq!(
            entries(temp.path()),
            Vec::<String>::new(),
            "{name:?} must not create anything under the base"
        );
    }
}

#[test]
fn create_rejects_invalid_versions_without_creating_anything() {
    let temp = TempDir::new("bad-version");
    let too_long = "v".repeat(65);

    for version in ["", "  ", "../1.21", "a\\b", "1.\u{7}", too_long.as_str()] {
        let error = create_instance(temp.path(), "Survival", version)
            .expect_err("an invalid version must be rejected");
        assert!(
            matches!(error, CreateError::InvalidVersion(_)),
            "{version:?} must be rejected with InvalidVersion, got {error:?}"
        );
        assert_eq!(
            entries(temp.path()),
            Vec::<String>::new(),
            "{version:?} must not create anything under the base"
        );
    }
}

#[test]
fn two_instances_get_distinct_ids_and_roots() {
    let temp = TempDir::new("two");

    let first = create_instance(temp.path(), "Alpha", "1.21.1").expect("the first should succeed");
    let second = create_instance(temp.path(), "Beta", "1.20.1").expect("the second should succeed");

    assert_ne!(first.id, second.id, "each instance needs its own id");
    assert_ne!(first.root, second.root, "each instance needs its own root");
    assert!(first.root.is_dir());
    assert!(second.root.is_dir());

    let mut expected = vec![first.id.to_string(), second.id.to_string()];
    expected.sort_unstable();
    assert_eq!(
        entries(&temp.instances()),
        expected,
        "the store must hold exactly the two instance directories"
    );
}

#[test]
fn a_corrupt_record_does_not_block_a_new_instance() {
    let temp = TempDir::new("corrupt-neighbour");
    let instances = temp.instances();
    fs::create_dir_all(&instances).expect("the instances directory should be creatable");
    let corrupt = instances.join("1b4e28ba-2fa1-11d2-883f-0016d3cca427");
    fs::create_dir(&corrupt).expect("the corrupt instance directory should be creatable");
    let corrupt_record = corrupt.join("instance.json");
    fs::write(&corrupt_record, b"{ this is not json")
        .expect("the corrupt record should be writable");

    let created = create_instance(temp.path(), "Fresh", "1.21.1")
        .expect("a corrupt neighbour must not block creation");

    assert_eq!(created.name, "Fresh");
    assert!(created.root.is_dir());
    assert_eq!(
        fs::read(&corrupt_record).expect("the corrupt record should still exist"),
        b"{ this is not json",
        "the corrupt neighbour must be left alone"
    );
}

#[test]
fn every_instance_directory_is_named_after_its_id() {
    let temp = TempDir::new("id-named");

    let first = create_instance(temp.path(), "Alpha", "1.21.1").expect("the first should succeed");
    let second = create_instance(temp.path(), "Beta", "1.20.1").expect("the second should succeed");

    for instance in [&first, &second] {
        let name = instance
            .root
            .file_name()
            .and_then(OsStr::to_str)
            .expect("the root should end in a UTF-8 directory name");
        assert_eq!(
            name,
            instance.id.to_string(),
            "the directory name must be the instance id"
        );
        assert_eq!(
            instance.root,
            expected_root(&temp, instance.id),
            "the root must be the canonical <base>/instances/<id> path"
        );
    }
}

#[test]
fn creating_a_second_instance_leaves_the_first_record_untouched() {
    let temp = TempDir::new("untouched");
    let first = create_instance(temp.path(), "Alpha", "1.21.1").expect("the first should succeed");
    let record = first.root.join("instance.json");
    let before = fs::read(&record).expect("the first record should exist");

    let second = create_instance(temp.path(), "Beta", "1.20.1").expect("the second should succeed");

    assert_eq!(
        fs::read(&record).expect("the first record should still exist"),
        before,
        "creating a second instance must not rewrite the first record"
    );
    assert_ne!(first.id, second.id);
}
