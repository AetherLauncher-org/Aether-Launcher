use std::collections::{BTreeMap, HashSet};
use std::io::Cursor;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::InstanceId;
use super::*;
use crate::identify::net::is_allowed_url;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct FakeHost {
    dir: PathBuf,
    running: Mutex<HashSet<InstanceId>>,
}

impl FakeHost {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aether-install-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            running: Mutex::new(HashSet::new()),
        }
    }

    fn instance(&self, version: &str) -> GameInstance {
        aether_storage::create_instance(&self.dir, "Survival", version).unwrap()
    }

    fn game(&self, instance: &GameInstance) -> PathBuf {
        game_directory(instance)
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

/// A platform that answers from a table: version lists by project id, file bytes by address.
struct FakeNet {
    versions: BTreeMap<String, String>,
    files: BTreeMap<String, Vec<u8>>,
    asked: Mutex<Vec<String>>,
    search_answer: String,
}

impl FakeNet {
    fn new() -> Self {
        Self {
            versions: BTreeMap::new(),
            files: BTreeMap::new(),
            asked: Mutex::new(Vec::new()),
            search_answer: r#"{"hits":[],"total_hits":0}"#.to_owned(),
        }
    }

    /// Adds a project whose newest fitting version has one file with these bytes.
    fn project(
        mut self,
        id: &str,
        filename: &str,
        bytes: &[u8],
        loaders: &str,
        dependencies: &[&str],
    ) -> Self {
        let url = format!("https://cdn.modrinth.com/data/{id}/{filename}");
        let deps = dependencies
            .iter()
            .map(|dep| format!(r#"{{"project_id":"{dep}","dependency_type":"required"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        self.versions.insert(
            id.to_owned(),
            format!(
                r#"[{{"id":"V{id}","version_number":"1.0.0","version_type":"release","date_published":"2026-10-01T00:00:00Z",
                "game_versions":["1.21.1"],"loaders":[{loaders}],
                "files":[{{"url":"{url}","filename":"{filename}","size":{},"primary":true,
                           "hashes":{{"sha1":"{:x}","sha512":"{:x}"}}}}],
                "dependencies":[{deps}]}}]"#,
                bytes.len(),
                Sha1::digest(bytes),
                Sha512::digest(bytes),
            ),
        );
        self.files.insert(url, bytes.to_vec());
        self
    }

    fn note(&self, url: &str) {
        assert!(is_allowed_url(url), "{url}");
        self.asked.lock().unwrap().push(url.to_owned());
    }
}

impl ContentNet for FakeNet {
    fn post_json(&self, url: &str, _body: &str) -> Result<String, String> {
        self.note(url);
        Err("not used".to_owned())
    }

    fn get_text(&self, url: &str) -> Result<String, String> {
        self.note(url);
        if url.contains("/search?") {
            return Ok(self.search_answer.clone());
        }
        let id = url
            .strip_prefix("https://api.modrinth.com/v2/project/")
            .and_then(|rest| rest.split('/').next())
            .ok_or("unexpected address")?;
        self.versions.get(id).cloned().ok_or_else(|| "the platform answered 404".to_owned())
    }

    fn get_bytes(&self, url: &str, _limit: usize) -> Result<Vec<u8>, String> {
        self.note(url);
        self.files.get(url).cloned().ok_or_else(|| "the platform answered 404".to_owned())
    }

    fn get_stream(&self, url: &str, limit: u64) -> Result<Box<dyn Read + Send>, String> {
        self.note(url);
        let bytes = self.files.get(url).cloned().ok_or("the platform answered 404")?;
        // like the real reader: it fails once more than `limit` bytes have been read
        let _ = limit;
        Ok(Box::new(Cursor::new(bytes)))
    }
}

const FABRIC: &str = "fabric-loader-0.16.5-1.21.1";

fn mods(host: &FakeHost, instance: &GameInstance) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(host.game(instance).join("mods"))
        .map(|reader| {
            reader
                .flatten()
                .map(|item| item.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn the_game_version_and_loader_come_from_the_version_id() {
    fn case(id: &str, game: Option<&str>, loader: Option<&'static str>) {
        assert_eq!(
            target_of(id),
            Target {
                game_version: game.map(str::to_owned),
                loader
            },
            "{id}"
        );
    }
    case("1.21.1", Some("1.21.1"), None);
    case("24w14a", Some("24w14a"), None);
    case("fabric-loader-0.16.5-1.21.1", Some("1.21.1"), Some("fabric"));
    case("quilt-loader-0.26.0-1.20.4", Some("1.20.4"), Some("quilt"));
    case("1.20.1-forge-47.2.0", Some("1.20.1"), Some("forge"));
    case("neoforge-21.1.172", Some("1.21.1"), Some("neoforge"));
    case("neoforge-21.0.5", Some("1.21"), Some("neoforge"));
    case("neoforge-20.4.237", Some("1.20.4"), Some("neoforge"));
    case("neoforge-1.20.1-47.1.106", Some("1.20.1"), Some("neoforge"));
    // a scheme this code does not know: no guess
    case("neoforge-weird", None, Some("neoforge"));
    case("fabric-loader-0.16.5", None, Some("fabric"));
}

#[test]
fn a_mod_is_downloaded_checked_and_placed_with_its_required_dependency() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    let net = FakeNet::new()
        .project("AANobbMI", "sodium-1.0.0.jar", b"sodium bytes", r#""fabric""#, &["P7dR8mSH"])
        .project("P7dR8mSH", "fabric-api-1.0.0.jar", b"api bytes", r#""fabric""#, &[]);

    let report = install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, true).unwrap();

    assert_eq!(mods(&host, &instance), ["fabric-api-1.0.0.jar", "sodium-1.0.0.jar"]);
    assert_eq!(report.installed.len(), 2);
    assert!(report.skipped.is_empty());
    assert_eq!(
        fs::read(host.game(&instance).join("mods/sodium-1.0.0.jar")).unwrap(),
        b"sodium bytes"
    );
    // the versions request was narrowed to this game and loader
    let asked = net.asked.lock().unwrap();
    assert!(asked[0].contains("loaders=%5B%22fabric%22%5D&game_versions=%5B%221.21.1%22%5D"));
}

#[test]
fn dependencies_are_left_alone_unless_asked_for() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    let net = FakeNet::new()
        .project("AANobbMI", "sodium-1.0.0.jar", b"s", r#""fabric""#, &["P7dR8mSH"])
        .project("P7dR8mSH", "fabric-api-1.0.0.jar", b"a", r#""fabric""#, &[]);

    install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, false).unwrap();

    assert_eq!(mods(&host, &instance), ["sodium-1.0.0.jar"]);
}

#[test]
fn a_dependency_that_cannot_be_had_is_reported_and_the_mod_still_installs() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    let net = FakeNet::new().project("AANobbMI", "sodium-1.0.0.jar", b"s", r#""fabric""#, &["P7dR8mSH"]);

    let report = install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, true).unwrap();

    assert_eq!(report.installed.len(), 1);
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].project_id, "P7dR8mSH");
}

#[test]
fn a_dependency_cycle_ends() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    let net = FakeNet::new()
        .project("AAAAAAAA", "a.jar", b"a", r#""fabric""#, &["BBBBBBBB"])
        .project("BBBBBBBB", "b.jar", b"b", r#""fabric""#, &["AAAAAAAA"]);

    let report = install_with(&net, &host, &instance.id.to_string(), "AAAAAAAA", "mod", None, true).unwrap();

    assert_eq!(report.installed.len(), 2);
}

#[test]
fn a_file_with_the_wrong_hash_is_discarded_and_nothing_is_left() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    let mut net = FakeNet::new().project("AANobbMI", "sodium.jar", b"good bytes", r#""fabric""#, &[]);
    // same length, other content: the mirror (or a man in the middle) swapped the file
    net.files.insert(
        "https://cdn.modrinth.com/data/AANobbMI/sodium.jar".to_owned(),
        b"evil bytes".to_vec(),
    );

    let error = install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, false).unwrap_err();

    assert!(error.contains("checksum"), "{error}");
    assert_eq!(mods(&host, &instance), Vec::<String>::new());
}

#[test]
fn a_file_longer_or_shorter_than_listed_is_discarded() {
    for bytes in [b"much longer than listed".to_vec(), b"x".to_vec()] {
        let host = FakeHost::new();
        let instance = host.instance(FABRIC);
        let mut net = FakeNet::new().project("AANobbMI", "sodium.jar", b"ten bytes!", r#""fabric""#, &[]);
        net.files
            .insert("https://cdn.modrinth.com/data/AANobbMI/sodium.jar".to_owned(), bytes);

        assert!(install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, false).is_err());
        assert_eq!(mods(&host, &instance), Vec::<String>::new());
    }
}

#[test]
fn an_existing_file_is_never_overwritten() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    let existing = host.game(&instance).join("mods/sodium.jar");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, b"mine").unwrap();
    let net = FakeNet::new().project("AANobbMI", "sodium.jar", b"theirs", r#""fabric""#, &[]);

    let error = install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, false).unwrap_err();

    assert!(error.contains("already there"), "{error}");
    assert_eq!(fs::read(&existing).unwrap(), b"mine");
}

#[test]
fn a_link_at_the_file_name_is_not_followed() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    let victim = host.dir.join("victim.txt");
    fs::write(&victim, b"keep me").unwrap();
    let mods_dir = host.game(&instance).join("mods");
    fs::create_dir_all(&mods_dir).unwrap();
    #[cfg(unix)]
    let _ = std::os::unix::fs::symlink(&victim, mods_dir.join("sodium.jar"));
    #[cfg(windows)]
    let _ = std::os::windows::fs::symlink_file(&victim, mods_dir.join("sodium.jar"));
    let net = FakeNet::new().project("AANobbMI", "sodium.jar", b"theirs", r#""fabric""#, &[]);

    let _ = install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, false);

    assert_eq!(fs::read(&victim).unwrap(), b"keep me");
}

#[test]
fn nothing_is_installed_while_the_game_runs() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    host.running.lock().unwrap().insert(instance.id);
    let net = FakeNet::new().project("AANobbMI", "sodium.jar", b"s", r#""fabric""#, &[]);

    assert!(install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, false).is_err());
    assert!(net.asked.lock().unwrap().is_empty());
}

#[test]
fn a_mod_needs_a_loader_but_a_resource_pack_does_not() {
    let host = FakeHost::new();
    let instance = host.instance("1.21.1");
    let net = FakeNet::new()
        .project("AANobbMI", "sodium.jar", b"s", r#""fabric""#, &[])
        .project("RRRRRRRR", "Faithful.zip", b"pack", r#""minecraft""#, &[]);
    let id = instance.id.to_string();

    let error = install_with(&net, &host, &id, "AANobbMI", "mod", None, false).unwrap_err();
    assert!(error.contains("mod loader"), "{error}");

    install_with(&net, &host, &id, "RRRRRRRR", "resourcepack", None, false).unwrap();
    assert_eq!(fs::read(host.game(&instance).join("resourcepacks/Faithful.zip")).unwrap(), b"pack");
}

#[test]
fn a_data_pack_goes_into_the_chosen_world() {
    let host = FakeHost::new();
    let instance = host.instance("1.21.1");
    let world = host.game(&instance).join("saves/My World");
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), b"x").unwrap();
    let net = FakeNet::new().project("DDDDDDDD", "tweaks.zip", b"dp", r#""datapack""#, &[]);
    let id = instance.id.to_string();

    // without a world there is nowhere to put it, and nothing is asked of the platform
    assert!(install_with(&net, &host, &id, "DDDDDDDD", "datapack", None, false).is_err());
    assert!(net.asked.lock().unwrap().is_empty());
    // a name that is not a world of this instance is refused too
    assert!(install_with(&net, &host, &id, "DDDDDDDD", "datapack", Some("../x"), false).is_err());

    install_with(&net, &host, &id, "DDDDDDDD", "datapack", Some("My World"), false).unwrap();
    assert_eq!(fs::read(world.join("datapacks/tweaks.zip")).unwrap(), b"dp");
}

#[test]
fn a_file_of_the_wrong_kind_for_the_folder_is_not_installed() {
    let host = FakeHost::new();
    let instance = host.instance("1.21.1");
    // a "resource pack" whose file is an executable jar
    let net = FakeNet::new().project("RRRRRRRR", "pack.jar", b"x", r#""minecraft""#, &[]);

    let error = install_with(&net, &host, &instance.id.to_string(), "RRRRRRRR", "resourcepack", None, false)
        .unwrap_err();

    assert!(error.contains(".zip"), "{error}");
    assert!(!host.game(&instance).join("resourcepacks/pack.jar").exists());
}

#[test]
fn modpacks_and_unknown_types_are_refused() {
    let host = FakeHost::new();
    let instance = host.instance(FABRIC);
    let net = FakeNet::new();
    let id = instance.id.to_string();

    assert!(install_with(&net, &host, &id, "AANobbMI", "modpack", None, false).is_err());
    assert!(install_with(&net, &host, &id, "AANobbMI", "plugin", None, false).is_err());
    assert!(install_with(&net, &host, &id, "../x", "mod", None, false).is_err());
    assert!(install_with(&net, &host, "nonsense", "AANobbMI", "mod", None, false).is_err());
}

#[test]
fn a_project_with_no_version_for_this_game_is_an_error() {
    let host = FakeHost::new();
    let instance = host.instance("fabric-loader-0.16.5-1.19.2");
    let net = FakeNet::new().project("AANobbMI", "sodium.jar", b"s", r#""fabric""#, &[]);

    let error = install_with(&net, &host, &instance.id.to_string(), "AANobbMI", "mod", None, false).unwrap_err();

    assert!(error.contains("1.19.2"), "{error}");
}

#[test]
fn search_asks_for_the_type_and_drops_the_loader_where_it_does_not_apply() {
    let mut net = FakeNet::new();
    net.search_answer = r#"{"total_hits":1,"hits":[{"project_id":"AANobbMI","slug":"sodium","title":"Sodium",
        "description":"Fast","author":"jelly","downloads":5,"project_type":"mod"}]}"#
        .to_owned();

    let result = search(&net, &FakeHost::new(), &SearchRequest { text: "sodium", project_type: "mod", game_version: Some("1.21.1"), loader: Some("fabric"), sort: "downloads", offset: 0 }).unwrap();
    assert_eq!(result.hits[0].title, "Sodium");
    assert_eq!(result.total, 1);
    assert!(net.asked.lock().unwrap()[0].contains("categories%3Afabric"));

    search(&net, &FakeHost::new(), &SearchRequest { text: "faithful", project_type: "resourcepack", game_version: Some("1.21.1"), loader: Some("fabric"), sort: "relevance", offset: 0 }).unwrap();
    assert!(!net.asked.lock().unwrap()[1].contains("categories"));

    assert!(search(&net, &FakeHost::new(), &SearchRequest { text: "x", project_type: "plugin", game_version: None, loader: None, sort: "downloads", offset: 0 }).is_err());
    assert!(search(&net, &FakeHost::new(), &SearchRequest { text: "x", project_type: "mod", game_version: None, loader: None, sort: "popular", offset: 0 }).is_err());
}

#[test]
fn search_results_come_with_their_icons_and_a_missing_icon_is_not_an_error() {
    let host = FakeHost::new();
    let mut net = FakeNet::new();
    net.search_answer = r#"{"total_hits":2,"hits":[
        {"project_id":"AANobbMI","title":"Sodium","project_type":"mod","icon_url":"https://cdn.modrinth.com/data/AANobbMI/icon.png",
         "display_categories":["optimization","fabric"],"categories":["optimization","fabric"],"versions":["1.20.1","1.21.1"]},
        {"project_id":"P7dR8mSH","title":"Lithium","project_type":"mod","icon_url":"https://cdn.modrinth.com/data/P7dR8mSH/icon.png"}
    ]}"#
        .to_owned();
    // a 16x16 PNG header for the first; nothing for the second (the CDN answers 404)
    let mut icon = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x10".to_vec();
    icon.extend_from_slice(b"pixels");
    net.files.insert("https://cdn.modrinth.com/data/AANobbMI/icon.png".to_owned(), icon);

    let result = search(&net, &host, &SearchRequest { text: "", project_type: "mod", game_version: None, loader: None, sort: "downloads", offset: 0 }).unwrap();

    assert!(result.hits[0].icon.as_deref().is_some_and(|icon| icon.starts_with("data:image/png;base64,")));
    assert_eq!(result.hits[0].loaders, ["fabric"]);
    assert_eq!(result.hits[0].categories, ["optimization"]);
    assert_eq!(result.hits[0].newest_version.as_deref(), Some("1.21.1"));
    assert_eq!(result.hits[1].icon, None);

    // the second search finds the first icon in the cache: it is not fetched again
    let before = net.asked.lock().unwrap().iter().filter(|url| url.contains("AANobbMI/icon")).count();
    search(&net, &host, &SearchRequest { text: "", project_type: "mod", game_version: None, loader: None, sort: "downloads", offset: 0 }).unwrap();
    let after = net.asked.lock().unwrap().iter().filter(|url| url.contains("AANobbMI/icon")).count();
    assert_eq!(before, after);
}

#[test]
fn the_dtos_use_camel_case_and_carry_no_path_or_address() {
    let value = serde_json::to_value(InstallReport {
        installed: vec![InstalledFile {
            project_id: "AANobbMI".to_owned(),
            file_name: "sodium.jar".to_owned(),
            version: "1.0.0".to_owned(),
        }],
        skipped: vec![SkippedProject {
            project_id: "P7dR8mSH".to_owned(),
            reason: "x".to_owned(),
        }],
    })
    .unwrap();
    assert_eq!(value["installed"][0]["fileName"], "sodium.jar");
    assert_eq!(value["skipped"][0]["projectId"], "P7dR8mSH");
    assert!(!value.to_string().contains("http"));
}
