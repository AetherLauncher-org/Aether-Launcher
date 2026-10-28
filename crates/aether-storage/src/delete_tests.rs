//! 删除实例记录的测试。
//!
//! 最重要的断言只有一条：**删实例不能碰到游戏目录里的任何文件**。用户的
//! `versions/`、`saves/`、投影都在 `.minecraft` 里，删记录不等于卸载游戏。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use aether_core::InstanceId;

use super::{DeleteError, delete_instance};
use crate::create::create_instance;

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "aether-delete-{label}-{}-{unique}",
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
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// 造一个带游戏文件的实例：游戏目录里有版本、存档和一个投影。
fn instance_with_game_files(fixture: &Fixture) -> aether_core::GameInstance {
    let version = "1.21.1";
    let version_directory = fixture.game().join("versions").join(version);
    fs::create_dir_all(&version_directory).expect("version dir");
    fs::write(version_directory.join(format!("{version}.json")), b"{}").expect("version json");
    fs::create_dir_all(fixture.game().join("saves").join("world")).expect("saves");
    fs::write(
        fixture.game().join("saves").join("world").join("level.dat"),
        b"x",
    )
    .expect("save");
    fs::create_dir_all(fixture.game().join("schematics")).expect("schematics");
    fs::write(
        fixture.game().join("schematics").join("castle.litematic"),
        b"x",
    )
    .expect("schematic");

    create_instance(&fixture.store(), "Survival", version, &fixture.game()).expect("create")
}

/// 游戏目录下所有条目的相对路径，用来证明删除前后完全一致。
fn list_game_tree(game: &Path) -> Vec<String> {
    let mut entries = Vec::new();
    collect(game, game, &mut entries);
    entries.sort();
    entries
}

fn collect(base: &Path, directory: &Path, out: &mut Vec<String>) {
    let Ok(read) = fs::read_dir(directory) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        out.push(
            path.strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/"),
        );
        if path.is_dir() {
            collect(base, &path, out);
        }
    }
}

#[test]
fn removes_the_record_and_leaves_the_game_untouched() {
    let fixture = Fixture::new("happy");
    let instance = instance_with_game_files(&fixture);
    let before = list_game_tree(&fixture.game());
    assert!(!before.is_empty(), "the fixture needs game files");
    let expected = fs::canonicalize(&instance.root).expect("canonical");

    let removed = delete_instance(&fixture.store(), instance.id).expect("delete");

    assert_eq!(removed, expected);
    assert!(!instance.root.exists(), "the record directory must be gone");
    assert_eq!(
        list_game_tree(&fixture.game()),
        before,
        "the game directory must be byte-for-byte untouched"
    );
}

#[test]
fn the_store_and_its_other_instances_survive() {
    let fixture = Fixture::new("siblings");
    let first = instance_with_game_files(&fixture);
    let second = create_instance(&fixture.store(), "Creative", "1.20.1", &fixture.game())
        .expect("second instance");

    delete_instance(&fixture.store(), first.id).expect("delete");

    assert!(fixture.store().join("instances").is_dir());
    assert!(second.root.join("instance.json").is_file());
}

#[test]
fn a_missing_identifier_is_reported() {
    let fixture = Fixture::new("missing");
    let error = delete_instance(&fixture.store(), InstanceId::new()).expect_err("missing");

    assert!(matches!(error, DeleteError::NotFound));
}

#[test]
fn an_unsafe_store_is_rejected() {
    let fixture = Fixture::new("bad-store");
    let missing = fixture.root.join("no-such-store");

    let error = delete_instance(&missing, InstanceId::new()).expect_err("bad store");

    assert!(matches!(error, DeleteError::InvalidStore));
}

#[test]
fn an_entry_that_is_not_a_directory_is_rejected() {
    let fixture = Fixture::new("not-instance");
    let id = InstanceId::new();
    fs::write(fixture.store().join("instances").join(id.to_string()), b"x").expect("file");

    let error = delete_instance(&fixture.store(), id).expect_err("not a directory");

    assert!(matches!(error, DeleteError::UnsafeDirectory));
}
