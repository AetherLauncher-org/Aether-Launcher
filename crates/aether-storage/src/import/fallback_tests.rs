//! 游戏目录校验的测试。
//!
//! 这些断言守住一条：**只有真的含有该版本的目录才算数**，免得把实例指到
//! 一个空目录或别人的 `.minecraft` 上。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use super::{first_directory_with_version, has_version, looks_like_game_directory};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_root(label: &str) -> PathBuf {
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "aether-fallback-{label}-{}-{unique}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp root");
    path
}

fn game_with_versions(root: &Path, name: &str, versions: &[&str]) -> PathBuf {
    let game = root.join(name);
    fs::create_dir_all(game.join("versions")).expect("versions dir");
    for version in versions {
        fs::create_dir_all(game.join("versions").join(version)).expect("version dir");
    }
    game
}

#[test]
fn accepts_a_directory_that_holds_the_version() {
    let root = temp_root("hit");
    let game = game_with_versions(&root, "game", &["1.20.1"]);

    assert!(has_version(&game, "1.20.1"));
    assert!(looks_like_game_directory(&game));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn rejects_a_directory_without_that_version() {
    let root = temp_root("wrong-version");
    let game = game_with_versions(&root, "game", &["1.21.4"]);

    assert!(!has_version(&game, "1.20.1"));
    // 目录本身仍然像个游戏目录，只是没有这个版本。
    assert!(looks_like_game_directory(&game));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn rejects_a_directory_without_a_versions_folder() {
    let root = temp_root("no-versions");
    let empty = root.join("empty");
    fs::create_dir_all(&empty).expect("empty dir");

    assert!(!looks_like_game_directory(&empty));
    assert!(!has_version(&empty, "1.20.1"));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn rejects_version_names_that_are_not_a_single_segment() {
    let root = temp_root("bad-version");
    let game = game_with_versions(&root, "game", &["1.20.1"]);

    for version in ["", "..", ".", "../1.20.1", "a/b", "a\\b"] {
        assert!(
            !has_version(&game, version),
            "{version:?} must not be accepted as a version"
        );
    }

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn picks_the_first_candidate_that_really_has_the_version() {
    let root = temp_root("first");
    let without = game_with_versions(&root, "first", &["1.21.4"]);
    let with = game_with_versions(&root, "second", &["1.20.1"]);

    let found = first_directory_with_version(&[without, with.clone()], "1.20.1");

    assert_eq!(found.as_deref(), Some(with.as_path()));

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn reports_nothing_when_no_candidate_matches() {
    let root = temp_root("miss");
    let game = game_with_versions(&root, "game", &["1.20.1"]);

    assert!(first_directory_with_version(&[game], "9.9.9").is_none());
    assert!(first_directory_with_version(&[], "1.20.1").is_none());

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_missing_directory_is_rejected() {
    let root = temp_root("missing");
    let absent = root.join("nope");
    assert!(!looks_like_game_directory(&absent));
    assert!(!has_version(&absent, "1.20.1"));
    fs::remove_dir_all(root).expect("cleanup");
}
