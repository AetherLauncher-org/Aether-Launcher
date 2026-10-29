use std::fs;
use std::path::PathBuf;

use super::{game_directories, matches};
use crate::import::ExistingFiles;

#[test]
fn detects_a_launcher_folder_by_its_executable() {
    assert!(matches(&ExistingFiles::new([
        "BakaXL.exe",
        "BakaXL.Core.dll",
        "Themes",
    ])));
    assert!(matches(&ExistingFiles::new(["BakaXL.Plugin"])));
}

#[test]
fn a_game_folder_is_not_a_bakaxl_installation() {
    assert!(!matches(&ExistingFiles::new([
        "versions",
        "versions/1.20.1/1.20.1.json",
    ])));
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("aether-bakaxl-{}-{label}", std::process::id()))
}

#[test]
fn the_default_game_directory_sits_beside_the_launcher() {
    let root = temp_root("default");
    fs::create_dir_all(root.join(".minecraft").join("versions")).expect("mkdir");
    fs::write(root.join("BakaXL.exe"), b"stub").expect("write");

    assert_eq!(game_directories(&root), vec![root.join(".minecraft")]);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_portable_install_inside_a_game_folder_is_accepted() {
    let root = temp_root("portable");
    fs::create_dir_all(root.join("versions")).expect("mkdir");
    fs::write(root.join("BakaXL.exe"), b"stub").expect("write");

    assert_eq!(game_directories(&root), vec![root.clone()]);

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_launcher_folder_without_versions_yields_nothing() {
    let root = temp_root("empty");
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(root.join("BakaXL.exe"), b"stub").expect("write");

    assert!(game_directories(&root).is_empty());

    fs::remove_dir_all(root).expect("cleanup");
}
