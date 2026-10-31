use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::{GameInstance, InstanceId};

use super::*;

/// A PNG header declaring 16×16 pixels; the rest is not a picture, nothing here decodes it.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x10icon-bytes";
/// The same, declaring 65535×65535.
const HUGE_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\xFF\xFF\0\0\xFF\xFFicon-bytes";
const DAY: u64 = 24 * 60 * 60;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct FakeHost {
    dir: PathBuf,
}

impl FakeHost {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aether-identify-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
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

    fn pick_file(
        &self,
        _filter_name: &str,
        _extensions: &[&str],
    ) -> Result<Option<PathBuf>, String> {
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

    fn is_instance_running(&self, _id: InstanceId) -> Result<bool, String> {
        Ok(false)
    }

    fn emit_login(&self, _event: &crate::microsoft::LoginEvent) {}

    fn emit_install(&self, _progress: &crate::install::InstallProgress) {}
}

impl Drop for FakeHost {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn sha1_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha1::digest(bytes))
}

/// Answers like Modrinth for one known file and records what it was asked.
struct FakeNet {
    /// The hash Modrinth knows.
    known: String,
    icon: Vec<u8>,
    online: bool,
    posts: Mutex<Vec<(String, String)>>,
    gets: Mutex<Vec<String>>,
    images: Mutex<Vec<String>>,
}

impl FakeNet {
    fn knowing(bytes: &[u8]) -> Self {
        Self {
            known: sha1_hex(bytes),
            icon: PNG.to_vec(),
            online: true,
            posts: Mutex::new(Vec::new()),
            gets: Mutex::new(Vec::new()),
            images: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> usize {
        self.posts.lock().unwrap().len() + self.gets.lock().unwrap().len()
    }
}

impl ContentNet for FakeNet {
    fn post_json(&self, url: &str, body: &str) -> Result<String, String> {
        assert!(net::is_allowed_url(url), "{url}");
        self.posts
            .lock()
            .unwrap()
            .push((url.to_owned(), body.to_owned()));
        if !self.online {
            return Err("the platform cannot be reached".to_owned());
        }
        let asked: serde_json::Value = serde_json::from_str(body).unwrap();
        let mut answer = serde_json::Map::new();
        if asked["hashes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|hash| hash == self.known.as_str())
        {
            answer.insert(
                self.known.clone(),
                serde_json::json!({"project_id": "AANobbMI", "version_number": "0.6.3"}),
            );
        }
        Ok(serde_json::Value::Object(answer).to_string())
    }

    fn get_text(&self, url: &str) -> Result<String, String> {
        assert!(net::is_allowed_url(url), "{url}");
        self.gets.lock().unwrap().push(url.to_owned());
        if !self.online {
            return Err("the platform cannot be reached".to_owned());
        }
        Ok(serde_json::json!([{
            "id": "AANobbMI",
            "slug": "sodium",
            "title": "Sodium",
            "description": "A modern rendering engine",
            "project_type": "mod",
            "icon_url": "https://cdn.modrinth.com/data/AANobbMI/icon.png"
        }])
        .to_string())
    }

    fn get_bytes(&self, url: &str, _limit: usize) -> Result<Vec<u8>, String> {
        assert!(net::is_allowed_url(url), "{url}");
        self.images.lock().unwrap().push(url.to_owned());
        if !self.online {
            return Err("the platform cannot be reached".to_owned());
        }
        Ok(self.icon.clone())
    }
}

fn run(net: &FakeNet, host: &FakeHost, instance: &GameInstance, kind: &str, now: u64) -> Lookup {
    identify_with(net, host, &instance.id.to_string(), kind, now).unwrap()
}

#[test]
fn a_file_is_identified_by_its_hash_and_gets_a_title_and_an_icon() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/whatever-name.jar", b"sodium");
    host.write(&instance, "mods/unknown.jar", b"something else");
    let net = FakeNet::knowing(b"sodium");

    let lookup = run(&net, &host, &instance, "mods", 1000);

    assert!(!lookup.unreachable);
    assert_eq!(lookup.items.len(), 1);
    let item = &lookup.items[0];
    assert_eq!(item.name, "whatever-name.jar");
    assert_eq!(item.title, "Sodium");
    assert_eq!(item.summary, "A modern rendering engine");
    assert_eq!(item.version, "0.6.3");
    assert_eq!(item.project_slug, "sodium");
    assert_eq!(item.platform, Platform::Modrinth);
    assert!(
        item.icon
            .as_deref()
            .is_some_and(|icon| icon.starts_with("data:image/png;base64,"))
    );
}

#[test]
fn only_hashes_leave_the_machine() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/my-secret-name.jar", b"sodium");
    let net = FakeNet::knowing(b"sodium");

    run(&net, &host, &instance, "mods", 1000);

    let posts = net.posts.lock().unwrap();
    assert_eq!(posts.len(), 1);
    assert!(posts[0].1.contains(&sha1_hex(b"sodium")));
    assert!(!posts[0].1.contains("secret"));
    assert!(!posts[0].1.contains(&instance.id.to_string()));
    assert!(posts[0].0.starts_with("https://api.modrinth.com/v2/"));
}

#[test]
fn a_second_visit_is_answered_from_the_cache() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    host.write(&instance, "mods/b.jar", b"unknown");
    let net = FakeNet::knowing(b"sodium");

    let first = run(&net, &host, &instance, "mods", 1000);
    let asked = net.requests();
    let images = net.images.lock().unwrap().len();
    let second = run(&net, &host, &instance, "mods", 2000);

    assert_eq!(first, second);
    assert_eq!(net.requests(), asked);
    assert_eq!(net.images.lock().unwrap().len(), images);
}

#[test]
fn a_file_modrinth_does_not_know_is_asked_about_again_after_a_day_not_before() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/b.jar", b"unknown");
    let net = FakeNet::knowing(b"sodium");

    run(&net, &host, &instance, "mods", 1000);
    run(&net, &host, &instance, "mods", 1000 + DAY - 1);
    assert_eq!(net.posts.lock().unwrap().len(), 1);

    run(&net, &host, &instance, "mods", 1000 + DAY + 1);
    assert_eq!(net.posts.lock().unwrap().len(), 2);
}

#[test]
fn a_new_file_is_asked_about_without_asking_about_the_old_ones() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    let net = FakeNet::knowing(b"sodium");
    run(&net, &host, &instance, "mods", 1000);

    host.write(&instance, "mods/c.jar", b"brand new");
    run(&net, &host, &instance, "mods", 2000);

    let posts = net.posts.lock().unwrap();
    assert_eq!(posts.len(), 2);
    assert!(!posts[1].1.contains(&sha1_hex(b"sodium")));
    assert!(posts[1].1.contains(&sha1_hex(b"brand new")));
}

#[test]
fn without_a_network_the_cache_answers_and_the_result_says_so() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    let mut net = FakeNet::knowing(b"sodium");
    let online = run(&net, &host, &instance, "mods", 1000);
    assert_eq!(online.items.len(), 1);

    // A week later the entry is stale and the network is gone.
    net.online = false;
    let offline = run(&net, &host, &instance, "mods", 1000 + 8 * DAY);

    assert!(offline.unreachable);
    assert_eq!(offline.items.len(), 1);
    assert_eq!(offline.items[0].title, "Sodium");
    assert!(offline.items[0].icon.is_some());
}

#[test]
fn with_no_network_and_no_cache_the_list_is_just_empty() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    let mut net = FakeNet::knowing(b"sodium");
    net.online = false;

    let lookup = run(&net, &host, &instance, "mods", 1000);

    assert!(lookup.unreachable);
    assert!(lookup.items.is_empty());
}

#[test]
fn a_failed_lookup_does_not_turn_files_into_misses() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    let mut net = FakeNet::knowing(b"sodium");
    net.online = false;
    run(&net, &host, &instance, "mods", 1000);

    net.online = true;
    let lookup = run(&net, &host, &instance, "mods", 1001);

    assert!(!lookup.unreachable);
    assert_eq!(lookup.items.len(), 1);
}

#[test]
fn an_icon_that_is_not_an_image_is_not_kept() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    let mut net = FakeNet::knowing(b"sodium");
    net.icon = b"<svg xmlns='http://www.w3.org/2000/svg' onload='alert(1)'/>".to_vec();

    let lookup = run(&net, &host, &instance, "mods", 1000);

    assert_eq!(lookup.items.len(), 1);
    assert_eq!(lookup.items[0].icon, None);
}

#[test]
fn an_icon_declaring_a_huge_picture_is_not_kept() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    let mut net = FakeNet::knowing(b"sodium");
    net.icon = HUGE_PNG.to_vec();

    let lookup = run(&net, &host, &instance, "mods", 1000);

    assert_eq!(lookup.items[0].icon, None);
    assert!(
        !host
            .dir
            .join("cache")
            .join("icons")
            .join("AANobbMI")
            .exists()
    );
}

#[test]
fn a_link_planted_at_an_icon_name_is_replaced_not_followed() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    let victim = host.dir.join("victim.txt");
    fs::write(&victim, b"keep me").unwrap();
    let icons = host.dir.join("cache").join("icons");
    fs::create_dir_all(&icons).unwrap();
    // a link to a file that does not exist yet: `exists()` is false for it, and a plain write
    // would create the victim through the link
    let missing = host.dir.join("not-yet.txt");
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&missing, icons.join("AANobbMI")).is_ok();
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&missing, icons.join("AANobbMI")).is_ok();
    let net = FakeNet::knowing(b"sodium");

    run(&net, &host, &instance, "mods", 1000);

    assert_eq!(fs::read(&victim).unwrap(), b"keep me");
    assert!(
        !missing.exists() || !linked,
        "the launcher wrote through the link"
    );
}

#[test]
fn a_planted_oversized_or_fake_icon_file_is_not_served() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    let net = FakeNet::knowing(b"sodium");
    run(&net, &host, &instance, "mods", 1000);

    let icon = host.dir.join("cache").join("icons").join("AANobbMI");
    fs::write(&icon, b"not an image").unwrap();
    assert_eq!(
        run(&net, &host, &instance, "mods", 1001).items[0].icon,
        None
    );

    fs::write(&icon, vec![0x89; MAX_ICON_BYTES + 1]).unwrap();
    assert_eq!(
        run(&net, &host, &instance, "mods", 1002).items[0].icon,
        None
    );
}

#[test]
fn a_damaged_cache_is_ignored() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar", b"sodium");
    fs::create_dir_all(host.dir.join("cache")).unwrap();
    fs::write(host.dir.join("cache").join(CACHE_FILE), "{ not json").unwrap();
    let net = FakeNet::knowing(b"sodium");

    assert_eq!(run(&net, &host, &instance, "mods", 1000).items.len(), 1);
}

#[test]
fn folders_and_worlds_and_schematics_are_not_looked_up() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "resourcepacks/Unpacked/pack.mcmeta", b"{}");
    host.write(&instance, "resourcepacks/Packed.zip", b"sodium");
    host.write(&instance, "saves/World/level.dat", b"w");
    let net = FakeNet::knowing(b"sodium");

    let packs = run(&net, &host, &instance, "resourcepacks", 1000);
    assert_eq!(
        packs
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        ["Packed.zip"]
    );
    let hashes = net.posts.lock().unwrap()[0].1.clone();
    assert_eq!(hashes.matches("hashes").count(), 1);
    assert_eq!(hashes.matches(&sha1_hex(b"sodium")).count(), 1);

    for kind in ["saves", "schematics", "logs", "../mods", ""] {
        assert!(
            identify_with(&net, &host, &instance.id.to_string(), kind, 1000).is_err(),
            "{kind:?}"
        );
    }
}

#[test]
fn a_switched_off_mod_is_still_recognised() {
    let host = FakeHost::new();
    let instance = host.instance();
    host.write(&instance, "mods/a.jar.disabled", b"sodium");
    let net = FakeNet::knowing(b"sodium");

    let lookup = run(&net, &host, &instance, "mods", 1000);

    assert_eq!(lookup.items[0].name, "a.jar.disabled");
}

#[test]
fn an_unknown_instance_is_an_error() {
    let host = FakeHost::new();
    let net = FakeNet::knowing(b"x");
    assert!(identify_with(&net, &host, "not-an-instance", "mods", 0).is_err());
}

#[test]
fn the_result_carries_no_path_and_uses_camel_case() {
    let item = serde_json::to_value(Identified {
        name: "a.jar".to_owned(),
        platform: Platform::Modrinth,
        project_slug: "sodium".to_owned(),
        title: "Sodium".to_owned(),
        summary: String::new(),
        version: "1".to_owned(),
        icon: None,
    })
    .unwrap();
    assert_eq!(item["projectSlug"], "sodium");
    assert_eq!(item["platform"], "modrinth");
    assert!(item.get("path").is_none());
}

#[test]
fn only_modrinth_is_contacted() {
    for good in [
        "https://api.modrinth.com/v2/version_files",
        "https://api.modrinth.com/v2/projects?ids=%5B%22a%22%5D",
        "https://cdn.modrinth.com/data/AANobbMI/icon.png",
    ] {
        assert!(net::is_allowed_url(good), "{good}");
    }
    let bad: HashSet<&str> = [
        "http://api.modrinth.com/v2/version_files",
        "https://api.modrinth.com.evil.test/v2/x",
        "https://api.modrinth.com@evil.test/v2/x",
        "https://api.modrinth.com/v2x/x",
        "https://evil.test/https://api.modrinth.com/v2/x",
        "https://cdn.modrinth.com:444/x",
        "https://cdn.modrinth.com",
        "https://modrinth.com/project/x",
        "https://api.curseforge.com/v1/mods",
        "https://api.modrinth.com/v2/x y",
        "file:///etc/passwd",
        "",
    ]
    .into_iter()
    .collect();
    for url in bad {
        assert!(!net::is_allowed_url(url), "{url}");
    }
}

#[test]
fn a_project_page_can_only_be_opened_for_a_slug() {
    assert!(open_project_page("https://evil.test").is_err());
    assert!(open_project_page("a/b").is_err());
    assert!(open_project_page("").is_err());
}
