use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::InstanceId;

use super::*;

const UUID: &str = "069a79f444e94726a5befca90e38aaf5";
const HASH: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90";
const DAY: u64 = 24 * 60 * 60;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct FakeHost {
    dir: PathBuf,
}

impl FakeHost {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aether-skins-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
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

fn encode(text: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in text.as_bytes().chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, byte)| n | (u32::from(*byte) << (16 - 8 * i)));
        for i in 0..=chunk.len() {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
        for _ in chunk.len()..3 {
            out.push('=');
        }
    }
    out
}

fn skin_png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 13]);
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&64u32.to_be_bytes());
    bytes.extend_from_slice(&64u32.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4]);
    bytes
}

struct FakeNet {
    has_skin: bool,
    online: bool,
    picture: Vec<u8>,
    texts: Mutex<Vec<String>>,
    bytes: Mutex<Vec<String>>,
}

impl FakeNet {
    fn new() -> Self {
        Self {
            has_skin: true,
            online: true,
            picture: skin_png(),
            texts: Mutex::new(Vec::new()),
            bytes: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> usize {
        self.texts.lock().unwrap().len() + self.bytes.lock().unwrap().len()
    }
}

impl SkinNet for FakeNet {
    fn get_text(&self, url: &str, _limit: usize) -> Result<String, String> {
        assert!(is_allowed_url(url), "{url}");
        self.texts.lock().unwrap().push(url.to_owned());
        if !self.online {
            return Err("the skin service cannot be reached".to_owned());
        }
        let textures = if self.has_skin {
            format!(
                r#"{{"textures":{{"SKIN":{{"url":"http://textures.minecraft.net/texture/{HASH}"}}}}}}"#
            )
        } else {
            r#"{"textures":{}}"#.to_owned()
        };
        Ok(format!(
            r#"{{"id":"{UUID}","name":"Notch","properties":[{{"name":"textures","value":"{}"}}]}}"#,
            encode(&textures)
        ))
    }

    fn get_bytes(&self, url: &str, _limit: usize) -> Result<Vec<u8>, String> {
        assert!(is_allowed_url(url), "{url}");
        self.bytes.lock().unwrap().push(url.to_owned());
        if !self.online {
            return Err("the skin service cannot be reached".to_owned());
        }
        Ok(self.picture.clone())
    }
}

fn now() -> u64 {
    seconds(SystemTime::now())
}

#[test]
fn a_skin_is_fetched_and_returned_as_a_data_url() {
    let host = FakeHost::new();
    let net = FakeNet::new();

    let skin = skin_with(&net, &host, UUID, now()).unwrap().unwrap();

    assert!(skin.starts_with("data:image/png;base64,iVBORw0KGgo"));
    assert_eq!(
        net.texts.lock().unwrap()[0],
        format!("https://sessionserver.mojang.com/session/minecraft/profile/{UUID}")
    );
    assert_eq!(
        net.bytes.lock().unwrap()[0],
        format!("https://textures.minecraft.net/texture/{HASH}")
    );
}

#[test]
fn a_second_call_is_answered_from_disk() {
    let host = FakeHost::new();
    let net = FakeNet::new();
    let first = skin_with(&net, &host, UUID, now()).unwrap();
    let asked = net.requests();

    let second = skin_with(&net, &host, UUID, now() + 60).unwrap();

    assert_eq!(first, second);
    assert_eq!(net.requests(), asked);
}

#[test]
fn after_a_day_it_asks_again() {
    let host = FakeHost::new();
    let net = FakeNet::new();
    skin_with(&net, &host, UUID, now()).unwrap();

    skin_with(&net, &host, UUID, now() + DAY + 60).unwrap();

    assert_eq!(net.texts.lock().unwrap().len(), 2);
}

#[test]
fn a_player_without_a_skin_gets_none_and_is_not_asked_about_again_for_a_day() {
    let host = FakeHost::new();
    let mut net = FakeNet::new();
    net.has_skin = false;

    assert_eq!(skin_with(&net, &host, UUID, now()).unwrap(), None);
    assert_eq!(skin_with(&net, &host, UUID, now() + 60).unwrap(), None);
    assert_eq!(net.texts.lock().unwrap().len(), 1);
    assert!(net.bytes.lock().unwrap().is_empty());

    assert_eq!(
        skin_with(&net, &host, UUID, now() + DAY + 60).unwrap(),
        None
    );
    assert_eq!(net.texts.lock().unwrap().len(), 2);
}

#[test]
fn without_a_network_nothing_is_not_an_error() {
    let host = FakeHost::new();
    let mut net = FakeNet::new();
    net.online = false;

    assert_eq!(skin_with(&net, &host, UUID, now()).unwrap(), None);
}

#[test]
fn without_a_network_an_old_skin_is_still_used() {
    let host = FakeHost::new();
    let mut net = FakeNet::new();
    let kept = skin_with(&net, &host, UUID, now()).unwrap();
    assert!(kept.is_some());

    net.online = false;
    let later = skin_with(&net, &host, UUID, now() + 30 * DAY).unwrap();

    assert_eq!(later, kept);
}

#[test]
fn a_picture_that_is_not_a_skin_is_not_kept() {
    let host = FakeHost::new();
    let mut net = FakeNet::new();
    net.picture = b"<svg xmlns='http://www.w3.org/2000/svg' onload='alert(1)'/>".to_vec();

    assert_eq!(skin_with(&net, &host, UUID, now()).unwrap(), None);
    assert!(
        !host
            .dir
            .join("cache")
            .join("skins")
            .join(format!("{UUID}.png"))
            .exists()
    );
}

#[test]
fn a_planted_file_in_the_cache_is_not_served() {
    let host = FakeHost::new();
    let mut net = FakeNet::new();
    skin_with(&net, &host, UUID, now()).unwrap();
    let path = host
        .dir
        .join("cache")
        .join("skins")
        .join(format!("{UUID}.png"));
    fs::write(&path, b"<html>not a picture</html>").unwrap();
    net.online = false;

    assert_eq!(skin_with(&net, &host, UUID, now() + 60).unwrap(), None);
}

#[test]
fn only_a_player_id_is_accepted() {
    let host = FakeHost::new();
    let net = FakeNet::new();
    for bad in [
        "",
        "../x",
        "Notch",
        "069A79F444E94726A5BEFCA90E38AAF5",
        "069a79f4-44e9-4726-a5be-fca90e38aaf5",
    ] {
        assert!(skin_with(&net, &host, bad, now()).is_err(), "{bad}");
    }
    assert_eq!(net.requests(), 0);
}

#[test]
fn only_the_two_skin_addresses_are_contacted() {
    for good in [
        format!("https://sessionserver.mojang.com/session/minecraft/profile/{UUID}"),
        format!("https://textures.minecraft.net/texture/{HASH}"),
    ] {
        assert!(is_allowed_url(&good), "{good}");
    }
    for bad in [
        format!("http://sessionserver.mojang.com/session/minecraft/profile/{UUID}"),
        "https://sessionserver.mojang.com/session/minecraft/profile/../../x".to_owned(),
        format!("https://sessionserver.mojang.com@evil.test/session/minecraft/profile/{UUID}"),
        format!("https://textures.minecraft.net.evil.test/texture/{HASH}"),
        format!("https://textures.minecraft.net:444/texture/{HASH}"),
        "https://textures.minecraft.net/other".to_owned(),
        "https://api.minecraftservices.com/minecraft/profile".to_owned(),
        "https://modrinth.com/".to_owned(),
        String::new(),
    ] {
        assert!(!is_allowed_url(&bad), "{bad}");
    }
}
