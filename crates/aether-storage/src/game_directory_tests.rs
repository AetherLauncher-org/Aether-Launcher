//! 批量补游戏目录的测试。
//!
//! 这条路径会改写用户已有的记录，所以断言集中在：只改该改的、改过的能读回来、
//! 改之前一定留下了备份。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use aether_core::{GameInstance, InstanceId, InstanceState, VersionId};

use super::{GameDirectoryError, apply_game_directory};
use crate::create::create_instance;

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "aether-gamedir-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("store").join("instances")).expect("store");
        fs::create_dir_all(root.join("game").join("versions")).expect("game");
        Self { root }
    }

    fn store(&self) -> PathBuf {
        self.root.join("store")
    }

    fn game(&self) -> PathBuf {
        self.root.join("game")
    }

    /// 造一条**没有**游戏目录的记录，模拟字段加入之前创建的实例。
    fn legacy_instance(&self, name: &str, version: &str) -> GameInstance {
        let id = InstanceId::new();
        let directory = self.store().join("instances").join(id.to_string());
        fs::create_dir_all(directory.join("minecraft")).expect("instance dir");
        let instance = GameInstance {
            id,
            name: name.to_owned(),
            version: VersionId::parse(version).expect("version"),
            root: directory,
            game_directory: None,
            state: InstanceState::Incomplete,
        };
        crate::update::write_instance_atomic(&instance.root, &instance).expect("record");
        instance
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn read_record(instance: &GameInstance) -> String {
    fs::read_to_string(instance.root.join("instance.json")).expect("record")
}

#[test]
fn writes_the_game_directory_into_legacy_records() {
    let fixture = Fixture::new("legacy");
    let first = fixture.legacy_instance("Survival", "1.21.1");
    let second = fixture.legacy_instance("Creative", "1.20.1");

    let outcome = apply_game_directory(&fixture.store(), &fixture.game()).expect("apply");

    assert_eq!(outcome.updated, 2);
    assert_eq!(outcome.skipped, 0);
    assert!(outcome.failed.is_empty());

    // 记录能被重新读出来，且指向刚选的目录。
    let store = crate::InstanceStore::open(&fixture.store()).expect("store");
    for instance in [&first, &second] {
        let loaded = store.load(instance.id).expect("reload");
        assert_eq!(
            loaded
                .game_directory
                .as_deref()
                .and_then(|path| path.canonicalize().ok()),
            Some(fixture.game().canonicalize().expect("canonical")),
            "{} should now point at the game directory",
            instance.name
        );
    }
}

#[test]
fn backs_up_every_record_it_is_about_to_change() {
    let fixture = Fixture::new("backup");
    let instance = fixture.legacy_instance("Survival", "1.21.1");
    let before = read_record(&instance);

    let outcome = apply_game_directory(&fixture.store(), &fixture.game()).expect("apply");

    let backup = outcome.backup.expect("a backup directory");
    let saved = backup
        .join(instance.root.file_name().expect("name"))
        .join("instance.json");
    assert_eq!(
        fs::read_to_string(&saved).expect("backup record"),
        before,
        "the backup must hold the record as it was before the change"
    );
    assert_ne!(
        read_record(&instance),
        before,
        "the live record should have been rewritten"
    );
}

#[test]
fn leaves_instances_that_already_have_a_game_directory_alone() {
    let fixture = Fixture::new("already-set");
    let created =
        create_instance(&fixture.store(), "Modern", "1.21.1", &fixture.game()).expect("create");
    let before = read_record(&created);

    let other = fixture.root.join("other-game");
    fs::create_dir_all(other.join("versions")).expect("other game");
    let outcome = apply_game_directory(&fixture.store(), &other).expect("apply");

    assert_eq!(outcome.updated, 0);
    assert_eq!(outcome.skipped, 1);
    assert!(
        outcome.backup.is_none(),
        "nothing to change, nothing to back up"
    );
    assert_eq!(
        read_record(&created),
        before,
        "the record must be untouched"
    );
}

#[test]
fn rejects_a_directory_that_is_not_a_game_directory() {
    let fixture = Fixture::new("bad-directory");
    let instance = fixture.legacy_instance("Survival", "1.21.1");
    let before = read_record(&instance);
    let empty = fixture.root.join("not-a-game");
    fs::create_dir_all(&empty).expect("empty");

    let error = apply_game_directory(&fixture.store(), &empty)
        .expect_err("a directory without versions/ must be rejected");

    assert!(matches!(error, GameDirectoryError::InvalidGameDirectory));
    assert_eq!(read_record(&instance), before, "nothing may be rewritten");
}

#[test]
fn a_missing_directory_is_rejected() {
    let fixture = Fixture::new("missing-directory");
    let absent = fixture.root.join("missing");

    let error = apply_game_directory(&fixture.store(), &absent)
        .expect_err("a missing directory must be rejected");

    assert!(matches!(error, GameDirectoryError::InvalidGameDirectory));
}

#[test]
fn nothing_to_do_is_not_an_error() {
    let fixture = Fixture::new("empty-store");
    let outcome = apply_game_directory(&fixture.store(), &fixture.game()).expect("apply");

    assert_eq!(outcome.updated, 0);
    assert_eq!(outcome.skipped, 0);
    assert!(outcome.failed.is_empty());
    assert!(outcome.backup.is_none());
}

fn _assert_path(_: &Path) {}
