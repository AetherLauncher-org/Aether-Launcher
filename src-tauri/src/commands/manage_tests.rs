use std::path::Path;

use super::*;

#[test]
fn the_loader_is_named_from_the_installed_version_id() {
    assert_eq!(
        detect_loader("fabric-loader-0.16.5-1.21.1").as_deref(),
        Some("Fabric 0.16.5")
    );
    assert_eq!(
        detect_loader("quilt-loader-0.26.0-1.21.1").as_deref(),
        Some("Quilt 0.26.0")
    );
    assert_eq!(
        detect_loader("1.21.1-forge-52.0.1").as_deref(),
        Some("Forge 52.0.1")
    );
    assert_eq!(
        detect_loader("neoforge-21.1.172").as_deref(),
        Some("NeoForge 21.1.172")
    );
    assert_eq!(
        detect_loader("26.3-forge-66.1.2").as_deref(),
        Some("Forge 66.1.2")
    );
    assert_eq!(detect_loader("1.21.1"), None);
    assert_eq!(detect_loader("26.2"), None);
    assert_eq!(detect_loader(""), None);
}

#[test]
fn folder_labels_map_to_folders_inside_the_game_directory() {
    let game = Path::new("game");
    assert_eq!(folder_for(game, "game"), Some(game.to_path_buf()));
    assert_eq!(folder_for(game, "mods"), Some(game.join("mods")));
    assert_eq!(folder_for(game, "saves"), Some(game.join("saves")));
    assert_eq!(
        folder_for(game, "schematics"),
        Some(game.join("schematics"))
    );
    assert_eq!(folder_for(game, "logs"), Some(game.join("logs")));
    assert_eq!(
        folder_for(game, "crash-reports"),
        Some(game.join("crash-reports"))
    );
    // Anything else, including a path, is not a folder the page may open.
    for label in [
        "",
        "..",
        "../mods",
        "C:\\Windows",
        "/etc",
        "Mods",
        "versions",
        "instance.json",
    ] {
        assert_eq!(folder_for(game, label), None, "{label}");
    }
}

#[test]
fn the_extended_length_prefix_is_dropped_but_network_paths_are_kept() {
    assert_eq!(
        plain(Path::new(r"\\?\C:\Users\me\x")),
        Path::new(r"C:\Users\me\x")
    );
    assert_eq!(
        plain(Path::new(r"\\?\UNC\server\share")),
        Path::new(r"\\?\UNC\server\share")
    );
    assert_eq!(plain(Path::new(r"C:\plain")), Path::new(r"C:\plain"));
}

#[test]
fn only_known_logs_with_plain_names_are_readable() {
    assert_eq!(log_source("latest", None), Ok(LogSource::Latest));
    assert_eq!(
        log_source("crash", Some("crash-1.txt".to_owned())),
        Ok(LogSource::CrashReport("crash-1.txt".to_owned()))
    );
    for (kind, name) in [
        ("crash", None),
        ("crash", Some("../x.txt")),
        ("crash", Some("a/b.txt")),
        ("other", Some("x.txt")),
        ("", None),
    ] {
        assert!(
            log_source(kind, name.map(str::to_owned)).is_err(),
            "{kind} {name:?}"
        );
    }
}

#[test]
fn the_dtos_use_camel_case_and_carry_no_path() {
    let details = serde_json::to_value(InstanceDetails {
        id: "x".to_owned(),
        name: "Survival".to_owned(),
        version: "1.21.1".to_owned(),
        state: "ready",
        loader: Some("Fabric 0.16.5".to_owned()),
        size_bytes: 10,
        counts: ContentCounts {
            mods: 1,
            resourcepacks: 2,
            shaderpacks: 3,
            saves: 4,
            schematics: 5,
        },
    })
    .unwrap();
    assert_eq!(details["sizeBytes"], 10);
    assert_eq!(details["counts"]["resourcepacks"], 2);
    assert!(details.get("root").is_none() && details.get("path").is_none());

    let item = serde_json::to_value(ContentItem {
        name: "a.jar".to_owned(),
        size_bytes: 1,
        enabled: false,
        is_folder: false,
    })
    .unwrap();
    assert_eq!(item["isFolder"], false);
    assert_eq!(item["sizeBytes"], 1);
}
