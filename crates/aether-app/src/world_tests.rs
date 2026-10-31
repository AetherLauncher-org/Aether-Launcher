use std::collections::HashSet;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::InstanceId;
use aether_schematic::nbt::compound;

use super::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct FakeHost {
    dir: PathBuf,
    running: Mutex<HashSet<InstanceId>>,
}

impl FakeHost {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aether-world-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            running: Mutex::new(HashSet::new()),
        }
    }

    fn instance(&self) -> GameInstance {
        aether_storage::create_instance(&self.dir, "Survival", "1.21.1").unwrap()
    }

    fn write(&self, instance: &GameInstance, relative: &str, bytes: &[u8]) {
        let path = instance.root.join("minecraft").join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

impl Host for FakeHost {
    fn emit_import_item(&self, _event: &crate::import::ImportItemEvent) {}

    fn pick_folder(&self, _title: &str) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }

    fn pick_file(&self, _filter_name: &str, _extensions: &[&str]) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }

    fn known_folders(&self) -> crate::host::KnownFolders {
        crate::host::KnownFolders {
            desktop: None,
            downloads: None,
            documents: None,
            home: None,
        }
    }

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

fn strings(items: &[&str]) -> Tag {
    Tag::List(items.iter().map(|item| Tag::String((*item).to_owned())).collect())
}

fn level_dat() -> Vec<u8> {
    let data = compound([
        ("LevelName", Tag::String("My World".to_owned())),
        ("DataVersion", Tag::Int(3955)),
        ("GameType", Tag::Int(1)),
        ("hardcore", Tag::Byte(0)),
        ("Difficulty", Tag::Byte(2)),
        ("LastPlayed", Tag::Long(1_761_900_000_000)),
        ("Version", compound([("Name", Tag::String("1.21.1".to_owned()))])),
        (
            "DataPacks",
            compound([
                ("Enabled", strings(&["vanilla", "file/tweaks.zip", "file/folder-pack"])),
                ("Disabled", strings(&["file/off.zip"])),
            ]),
        ),
    ]);
    nbt::write_gzip("", &compound([("Data", data)]))
}

fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut bytes);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, content) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap();
    }
    bytes.into_inner()
}

const MCMETA: &[u8] = br#"{"pack":{"pack_format":48,"description":"Better trading"}}"#;

fn world(host: &FakeHost, instance: &GameInstance) {
    host.write(instance, "saves/My World/level.dat", &level_dat());
    host.write(
        instance,
        "saves/My World/datapacks/tweaks.zip",
        &zip_with(&[("pack.mcmeta", MCMETA), ("data/x/y.json", b"{}")]),
    );
    host.write(
        instance,
        "saves/My World/datapacks/folder-pack/pack.mcmeta",
        br#"{"pack":{"pack_format":26,"description":[{"text":"Two "},{"text":"parts","extra":[{"text":"!"}]}]}}"#,
    );
    host.write(
        instance,
        "saves/My World/datapacks/off.zip",
        &zip_with(&[("pack.mcmeta", MCMETA)]),
    );
    host.write(instance, "saves/My World/datapacks/readme.txt", b"not a pack");
}

#[test]
fn the_world_page_reads_level_dat_and_lists_the_packs_with_their_state() {
    let host = FakeHost::new();
    let instance = host.instance();
    world(&host, &instance);

    let details = world_details(&host, &instance.id.to_string(), "My World").unwrap();

    assert_eq!(details.level_name.as_deref(), Some("My World"));
    assert_eq!(details.game_version.as_deref(), Some("1.21.1"));
    assert_eq!(details.data_version, Some(3955));
    assert_eq!(details.game_mode, Some("creative"));
    assert_eq!(details.hardcore, Some(false));
    assert_eq!(details.difficulty, Some("normal"));
    assert_eq!(details.last_played, Some(1_761_900_000));
    assert_eq!(details.unreadable, None);

    let names: Vec<&str> = details.datapacks.iter().map(|pack| pack.name.as_str()).collect();
    assert_eq!(names, ["folder-pack", "off.zip", "tweaks.zip"]);
    let by_name = |name: &str| details.datapacks.iter().find(|pack| pack.name == name).unwrap();
    assert_eq!(by_name("tweaks.zip").state, PackState::Enabled);
    assert_eq!(by_name("tweaks.zip").description.as_deref(), Some("Better trading"));
    assert_eq!(by_name("tweaks.zip").pack_format, Some(48));
    assert_eq!(by_name("off.zip").state, PackState::Disabled);
    assert_eq!(by_name("folder-pack").state, PackState::Enabled);
    assert!(by_name("folder-pack").is_folder);
    assert_eq!(by_name("folder-pack").description.as_deref(), Some("Two parts!"));
    assert!(!details.datapacks_truncated);
}

#[test]
fn a_pack_that_level_dat_does_not_list_is_unknown() {
    let host = FakeHost::new();
    let instance = host.instance();
    world(&host, &instance);
    host.write(
        &instance,
        "saves/My World/datapacks/new.zip",
        &zip_with(&[("pack.mcmeta", MCMETA)]),
    );

    let details = world_details(&host, &instance.id.to_string(), "My World").unwrap();

    let new = details.datapacks.iter().find(|pack| pack.name == "new.zip").unwrap();
    assert_eq!(new.state, PackState::Unknown);
}

#[test]
fn a_damaged_level_dat_still_lists_the_packs_and_says_why() {
    let host = FakeHost::new();
    let instance = host.instance();
    world(&host, &instance);
    host.write(&instance, "saves/My World/level.dat", b"\x1f\x8bnot really gzip");

    let details = world_details(&host, &instance.id.to_string(), "My World").unwrap();

    assert!(details.unreadable.is_some());
    assert_eq!(details.level_name, None);
    assert_eq!(details.datapacks.len(), 3);
    assert!(details.datapacks.iter().all(|pack| pack.state == PackState::Unknown));
}

#[test]
fn a_pack_with_no_or_broken_metadata_is_listed_without_a_description() {
    let host = FakeHost::new();
    let instance = host.instance();
    world(&host, &instance);
    host.write(&instance, "saves/My World/datapacks/bare.zip", &zip_with(&[("data/a.json", b"{}")]));
    host.write(&instance, "saves/My World/datapacks/junk.zip", b"this is not a zip");
    host.write(
        &instance,
        "saves/My World/datapacks/big.zip",
        &zip_with(&[("pack.mcmeta", &vec![b' '; 70 * 1024])]),
    );

    let details = world_details(&host, &instance.id.to_string(), "My World").unwrap();

    for name in ["bare.zip", "junk.zip", "big.zip"] {
        let pack = details.datapacks.iter().find(|pack| pack.name == name).unwrap();
        assert_eq!((pack.description.as_deref(), pack.pack_format), (None, None), "{name}");
    }
}

#[test]
fn only_a_world_folder_of_the_instance_is_a_world() {
    let host = FakeHost::new();
    let instance = host.instance();
    world(&host, &instance);
    host.write(&instance, "saves/not-a-folder", b"x");
    let id = instance.id.to_string();

    for name in ["", "..", "../x", "My World/..", "a/b", "C:\\Windows", "missing", "not-a-folder"] {
        assert!(world_details(&host, &id, name).is_err(), "{name:?}");
        assert!(remove_datapack(&host, &id, name, "tweaks.zip").is_err(), "{name:?}");
        assert!(open_world_folder(&host, &id, name).is_err(), "{name:?}");
    }
    assert!(world_details(&host, "nonsense", "My World").is_err());
}

#[test]
fn a_data_pack_can_be_removed_unless_the_game_runs() {
    let host = FakeHost::new();
    let instance = host.instance();
    world(&host, &instance);
    let id = instance.id.to_string();
    let packs = instance.root.join("minecraft/saves/My World/datapacks");

    host.running.lock().unwrap().insert(instance.id);
    assert!(remove_datapack(&host, &id, "My World", "tweaks.zip").is_err());
    assert!(packs.join("tweaks.zip").exists());

    host.running.lock().unwrap().clear();
    remove_datapack(&host, &id, "My World", "tweaks.zip").unwrap();
    remove_datapack(&host, &id, "My World", "folder-pack").unwrap();
    assert!(!packs.join("tweaks.zip").exists());
    assert!(!packs.join("folder-pack").exists());
    assert!(packs.join("off.zip").exists());
}

#[test]
fn only_plain_names_of_existing_packs_are_removed() {
    let host = FakeHost::new();
    let instance = host.instance();
    world(&host, &instance);
    let id = instance.id.to_string();

    for name in ["", "..", "../level.dat", "a/b", "missing.zip"] {
        assert!(remove_datapack(&host, &id, "My World", name).is_err(), "{name:?}");
    }
    assert!(instance.root.join("minecraft/saves/My World/level.dat").exists());
}

#[test]
fn descriptions_are_plain_cut_text() {
    let long = format!(r#"{{"pack":{{"pack_format":1,"description":"{}"}}}}"#, "x".repeat(2000));
    let (description, format) = parse_mcmeta(long.as_bytes());
    assert_eq!(description.map(|text| text.chars().count()), Some(MAX_DESCRIPTION_CHARS));
    assert_eq!(format, Some(1));

    assert_eq!(parse_mcmeta(b"nope"), (None, None));
    assert_eq!(parse_mcmeta(br#"{"pack":{}}"#), (None, None));
    // control characters in a description do not reach the page
    let (description, _) = parse_mcmeta(br#"{"pack":{"description":"a\nb\u0000c"}}"#);
    assert_eq!(description.as_deref(), Some("a b c"));
}

#[test]
fn the_dto_uses_camel_case_and_carries_no_path() {
    let value = serde_json::to_value(WorldDetails {
        name: "W".to_owned(),
        level_name: None,
        game_version: None,
        data_version: None,
        game_mode: Some("survival"),
        hardcore: None,
        difficulty: None,
        last_played: None,
        size_bytes: 1,
        icon: None,
        datapacks: vec![DatapackInfo {
            name: "a.zip".to_owned(),
            is_folder: false,
            size_bytes: 2,
            description: None,
            pack_format: Some(48),
            state: PackState::Enabled,
        }],
        datapacks_truncated: false,
        unreadable: None,
    })
    .unwrap();
    assert_eq!(value["gameMode"], "survival");
    assert_eq!(value["datapacksTruncated"], false);
    assert_eq!(value["datapacks"][0]["packFormat"], 48);
    assert_eq!(value["datapacks"][0]["state"], "enabled");
    assert!(value.get("path").is_none() && value.get("root").is_none());
}
