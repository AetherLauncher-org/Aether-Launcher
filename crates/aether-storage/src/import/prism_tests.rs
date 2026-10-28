use std::fs;
use std::path::PathBuf;

use super::{
    PrismInstance, has_config, instance_directory, instance_name, list_instances,
    locate_instances_root, matches, parse_pack, read_instance,
};
use crate::import::ExistingFiles;

const PACK: &str = r#"{
    "components": [
        { "uid": "org.lwjgl3", "version": "3.2.2", "dependencyOnly": true },
        { "uid": "net.minecraft", "version": "1.16.1", "important": true },
        { "uid": "net.fabricmc.intermediary", "version": "1.16.1" },
        { "uid": "net.fabricmc.fabric-loader", "version": "0.18.4", "important": true }
    ],
    "formatVersion": 1
}"#;

#[test]
fn reads_the_instance_directory_from_the_launcher_config() {
    let config = "InstanceDir=instances\nLastUsedGroupForNewInstance=\n";
    assert_eq!(instance_directory(config).as_deref(), Some("instances"));
}

#[test]
fn instance_directory_ignores_sections_comments_and_other_keys() {
    let config = "# comment\n[General]\nInstanceDir=\nOther=1\n";
    assert_eq!(instance_directory(config), None);

    let config = "[General]\nInstanceDir=my instances\n";
    assert_eq!(instance_directory(config).as_deref(), Some("my instances"));
}

#[test]
fn parses_the_base_version_and_loader_from_a_pack() {
    let pack = parse_pack(PACK).expect("pack");
    assert_eq!(pack.minecraft_version, "1.16.1");
    assert_eq!(pack.loader.as_deref(), Some("Fabric 0.18.4"));
}

#[test]
fn a_vanilla_pack_has_no_loader() {
    let json = r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"}]}"#;
    let pack = parse_pack(json).expect("pack");
    assert_eq!(pack.minecraft_version, "1.20.1");
    assert_eq!(pack.loader, None);
}

#[test]
fn a_pack_without_minecraft_is_rejected() {
    let json = r#"{"components":[{"uid":"org.lwjgl3","version":"3.2.2"}]}"#;
    assert!(parse_pack(json).is_none());
    assert!(parse_pack("not json").is_none());
}

#[test]
fn a_byte_order_mark_is_tolerated() {
    let json = format!("\u{feff}{PACK}");
    assert!(parse_pack(&json).is_some());
}

#[test]
fn reads_the_display_name_from_instance_cfg() {
    let config = "InstanceType=OneSix\niconKey=default\nname=MCSRRanked-Windows-1.16.1\n";
    assert_eq!(
        instance_name(config).as_deref(),
        Some("MCSRRanked-Windows-1.16.1")
    );
    assert_eq!(instance_name("InstanceType=OneSix\n"), None);
}

#[test]
fn detects_a_launcher_by_its_config_or_its_instances() {
    assert!(matches(&ExistingFiles::new(["multimc.cfg", "MultiMC.exe"])));
    assert!(matches(&ExistingFiles::new([
        "instances",
        "instances/abc/mmc-pack.json",
    ])));
    // A plain game folder is not a Prism installation.
    assert!(!matches(&ExistingFiles::new([
        "versions",
        "versions/1.20.1/1.20.1.json",
    ])));
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("aether-prism-{}-{label}", std::process::id()))
}

#[test]
fn reads_one_instance_folder() {
    let root = temp_root("one");
    let instance = root.join("instances").join("MCSR");
    fs::create_dir_all(&instance).expect("mkdir");
    fs::write(instance.join("mmc-pack.json"), PACK).expect("pack");
    fs::write(
        instance.join("instance.cfg"),
        "InstanceType=OneSix\nname=速通 1.16.1\n",
    )
    .expect("cfg");

    let read = read_instance(&instance).expect("instance");
    assert_eq!(
        read,
        PrismInstance {
            id: "MCSR".to_owned(),
            name: "速通 1.16.1".to_owned(),
            minecraft_version: "1.16.1".to_owned(),
            loader: Some("Fabric 0.18.4".to_owned()),
        }
    );
    assert_eq!(read.display_name(), "速通 1.16.1（1.16.1 + Fabric 0.18.4）");

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn an_instance_without_a_pack_is_ignored() {
    let root = temp_root("nopack");
    let instance = root.join("instances").join("empty");
    fs::create_dir_all(&instance).expect("mkdir");

    assert!(read_instance(&instance).is_none());

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn lists_instances_under_the_configured_directory() {
    let root = temp_root("list");
    let instances = root.join("custom-instances");
    for (id, name) in [("b", "第二个"), ("a", "第一个")] {
        let directory = instances.join(id);
        fs::create_dir_all(&directory).expect("mkdir");
        fs::write(directory.join("mmc-pack.json"), PACK).expect("pack");
        fs::write(directory.join("instance.cfg"), format!("name={name}\n")).expect("cfg");
    }
    // A stray folder without a pack must not show up.
    fs::create_dir_all(instances.join("stray")).expect("mkdir");
    fs::write(root.join("multimc.cfg"), "InstanceDir=custom-instances\n").expect("cfg");

    assert_eq!(locate_instances_root(&root), Some(instances.clone()));

    let listed = list_instances(&root);
    assert_eq!(
        listed
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "b"]
    );

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn a_folder_without_instances_is_not_a_launcher() {
    let root = temp_root("none");
    fs::create_dir_all(&root).expect("mkdir");
    assert_eq!(locate_instances_root(&root), None);
    assert!(list_instances(&root).is_empty());
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn only_a_config_file_makes_a_folder_a_launcher() {
    let root = temp_root("config");
    fs::create_dir_all(root.join("instances")).expect("mkdir");
    assert!(
        !has_config(&root),
        "an instances folder alone is not a launcher"
    );

    fs::write(root.join("multimc.cfg"), "InstanceDir=instances\n").expect("write");
    assert!(has_config(&root));

    fs::remove_dir_all(root).expect("cleanup");
}
