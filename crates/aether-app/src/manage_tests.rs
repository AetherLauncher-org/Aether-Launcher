use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::InstanceId;

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

// ---- the operations, over a host ----------------------------------------------------------

static NEXT: AtomicU64 = AtomicU64::new(0);

struct FakeHost {
    dir: PathBuf,
    running: Mutex<HashSet<InstanceId>>,
}

impl FakeHost {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aether-app-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            running: Mutex::new(HashSet::new()),
        }
    }

    fn instance(&self, name: &str) -> GameInstance {
        aether_storage::create_instance(&self.dir, name, "1.21.1").unwrap()
    }

    fn start(&self, instance: &GameInstance) {
        self.running.lock().unwrap().insert(instance.id);
    }

    fn write(&self, instance: &GameInstance, relative: &str, bytes: &[u8]) {
        let path = instance.root.join("minecraft").join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

impl Host for FakeHost {
    fn data_dir(&self) -> Result<PathBuf, String> {
        Ok(self.dir.clone())
    }

    fn is_instance_running(&self, id: InstanceId) -> Result<bool, String> {
        Ok(self.running.lock().unwrap().contains(&id))
    }

    fn emit_login(&self, _event: &crate::microsoft::LoginEvent) {}

    fn emit_install(&self, _progress: &crate::install::InstallProgress) {}
}

impl Drop for FakeHost {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn details_count_what_is_in_the_folders_and_name_the_loader() {
    let host = FakeHost::new();
    let instance = host.instance("Survival");
    host.write(&instance, "mods/a.jar", b"a");
    host.write(&instance, "mods/b.jar.disabled", b"b");
    host.write(&instance, "saves/World/level.dat", b"w");
    host.write(&instance, "schematics/House.litematic", b"h");

    let details = instance_details(&host, &instance.id.to_string()).unwrap();

    assert_eq!(details.name, "Survival");
    assert_eq!(details.loader, None);
    assert_eq!(
        details.counts,
        ContentCounts {
            mods: 2,
            resourcepacks: 0,
            shaderpacks: 0,
            saves: 1,
            schematics: 1
        }
    );
    // The four bytes written here, plus whatever a new instance starts with.
    assert!(details.size_bytes >= 4);
}

#[test]
fn an_id_that_is_not_an_instance_is_refused_everywhere() {
    let host = FakeHost::new();
    let ghost = InstanceId::new().to_string();

    for id in ["", "nonsense", "../x", ghost.as_str()] {
        assert!(instance_details(&host, id).is_err(), "{id}");
        assert!(rename(&host, id, "x").is_err());
        assert!(delete(&host, id).is_err());
        assert!(list_content(&host, id, "mods").is_err());
        assert!(list_logs(&host, id).is_err());
        assert!(open_folder(&host, id, "game").is_err());
    }
    assert_eq!(
        instance_details(&host, "nonsense").unwrap_err(),
        "invalid instance ID"
    );
}

#[test]
fn renaming_changes_the_name_only_and_refuses_a_taken_name() {
    let host = FakeHost::new();
    let first = host.instance("First");
    let second = host.instance("Second");

    let renamed = rename(&host, &first.id.to_string(), "  Hardcore  ").unwrap();

    assert_eq!(renamed.id(), first.id.to_string());
    assert!(first.root.join("instance.json").is_file());
    assert_eq!(
        instance_details(&host, &first.id.to_string()).unwrap().name,
        "Hardcore"
    );
    assert_eq!(
        rename(&host, &second.id.to_string(), "hardcore").unwrap_err(),
        "an instance with this name already exists"
    );
    // Giving an instance its own name back (even in another case) is not a clash.
    assert!(rename(&host, &first.id.to_string(), "HARDCORE").is_ok());
    assert!(rename(&host, &second.id.to_string(), "   ").is_err());
}

#[test]
fn deleting_removes_the_instance_and_its_natives_but_nothing_else() {
    let host = FakeHost::new();
    let doomed = host.instance("Doomed");
    let kept = host.instance("Kept");
    let natives = host.dir.join("game/natives").join(doomed.id.to_string());
    fs::create_dir_all(&natives).unwrap();
    fs::write(natives.join("lwjgl.dll"), b"x").unwrap();
    let shared = host.dir.join("game/versions/1.21.1/1.21.1.json");
    fs::create_dir_all(shared.parent().unwrap()).unwrap();
    fs::write(&shared, b"{}").unwrap();

    delete(&host, &doomed.id.to_string()).unwrap();

    assert!(!doomed.root.exists());
    assert!(!natives.exists());
    assert!(kept.root.join("instance.json").is_file());
    assert!(shared.is_file());
}

#[test]
fn nothing_changes_while_the_game_runs_but_reading_still_works() {
    let host = FakeHost::new();
    let instance = host.instance("Playing");
    let id = instance.id.to_string();
    host.write(&instance, "mods/a.jar", b"a");
    host.write(&instance, "saves/World/level.dat", b"w");
    host.start(&instance);

    for result in [
        delete(&host, &id),
        set_mod_enabled(&host, &id, "a.jar", false),
        remove_content(&host, &id, "saves", "World"),
    ] {
        assert!(result.unwrap_err().starts_with("stop the game"));
    }
    assert!(instance.root.join("minecraft/mods/a.jar").is_file());
    assert!(instance.root.join("minecraft/saves/World").is_dir());
    assert_eq!(list_content(&host, &id, "mods").unwrap().len(), 1);
    assert!(instance_details(&host, &id).is_ok());
}

#[test]
fn content_is_listed_switched_and_removed_by_label_and_plain_name() {
    let host = FakeHost::new();
    let instance = host.instance("Modded");
    let id = instance.id.to_string();
    host.write(&instance, "mods/a.jar", b"aa");

    let mods = list_content(&host, &id, "mods").unwrap();
    assert_eq!(
        mods,
        [ContentItem {
            name: "a.jar".to_owned(),
            size_bytes: 2,
            enabled: true,
            is_folder: false
        }]
    );

    set_mod_enabled(&host, &id, "a.jar", false).unwrap();
    assert!(!list_content(&host, &id, "mods").unwrap()[0].enabled);
    remove_content(&host, &id, "mods", "a.jar.disabled").unwrap();
    assert!(list_content(&host, &id, "mods").unwrap().is_empty());

    assert_eq!(
        list_content(&host, &id, "../mods").unwrap_err(),
        "unknown content folder"
    );
    assert!(remove_content(&host, &id, "mods", "../../instance.json").is_err());
    assert!(instance.root.join("instance.json").is_file());
}

#[test]
fn logs_are_redacted_before_they_leave_this_layer() {
    let host = FakeHost::new();
    let instance = host.instance("Logs");
    let id = instance.id.to_string();
    host.write(
        &instance,
        "logs/latest.log",
        b"Launching with --accessToken secrettoken1234567890 --username Steve\nDone\n",
    );
    host.write(&instance, "crash-reports/crash-1.txt", b"boom");

    let listed = list_logs(&host, &id).unwrap();
    assert_eq!(listed[0].kind, "latest");
    assert_eq!(listed[1].kind, "crash");

    let log = read_log(&host, &id, "latest", None).unwrap();
    assert!(!log.text.contains("secrettoken1234567890"), "{}", log.text);
    assert!(log.text.contains("Done"));
    assert_eq!(
        read_log(&host, &id, "crash", Some("crash-1.txt".to_owned()))
            .unwrap()
            .text,
        "boom"
    );
    assert!(read_log(&host, &id, "crash", Some("../x.txt".to_owned())).is_err());
    assert!(read_log(&host, &id, "other", None).is_err());
}

#[test]
fn only_known_folders_can_be_opened() {
    let host = FakeHost::new();
    let instance = host.instance("Folders");
    let id = instance.id.to_string();

    for target in [
        "",
        "..",
        "../mods",
        "C:\\Windows",
        "versions",
        "instance.json",
    ] {
        assert_eq!(
            open_folder(&host, &id, target).unwrap_err(),
            "unknown folder",
            "{target}"
        );
    }
}

#[test]
fn the_instance_list_shows_what_the_store_holds() {
    let host = FakeHost::new();
    host.instance("One");
    host.instance("Two");

    let listing = crate::instances::list(&host).unwrap();

    let mut names: Vec<String> = listing
        .instances()
        .iter()
        .map(|summary| {
            serde_json::to_value(summary).unwrap()["name"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    names.sort();
    assert_eq!(names, ["One", "Two"]);
    assert_eq!(listing.skipped_count(), 0);
}
