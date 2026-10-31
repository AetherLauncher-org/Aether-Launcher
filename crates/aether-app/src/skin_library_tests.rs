use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::InstanceId;

use super::*;
use crate::skins::is_allowed_url;

static NEXT: AtomicU64 = AtomicU64::new(0);

const UUID: &str = "069a79f444e94726a5befca90e38aaf5";
const HASH: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f90";

struct FakeHost {
    dir: PathBuf,
    picked: Mutex<Option<PathBuf>>,
}

impl FakeHost {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aether-skinlib-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            picked: Mutex::new(None),
        }
    }

    fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.dir.join(name);
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Host for FakeHost {
    fn emit_import_item(&self, _event: &crate::import::ImportItemEvent) {}

    fn pick_folder(&self, _title: &str) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }

    fn pick_file(&self, _filter_name: &str, _extensions: &[&str]) -> Result<Option<PathBuf>, String> {
        Ok(self.picked.lock().unwrap().take())
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

/// A PNG header declaring a skin; `tag` makes pictures differ.
fn skin_png(tag: u8) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 13]);
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&64u32.to_be_bytes());
    bytes.extend_from_slice(&64u32.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, tag]);
    bytes
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

/// The services, answering from a table.
struct FakeNet {
    answers: BTreeMap<String, String>,
    pictures: BTreeMap<String, Vec<u8>>,
    asked: Mutex<Vec<String>>,
}

impl FakeNet {
    fn knowing_notch(slim: bool, picture: Vec<u8>) -> Self {
        let model = if slim { r#","metadata":{"model":"slim"}"# } else { "" };
        let textures = format!(
            r#"{{"textures":{{"SKIN":{{"url":"http://textures.minecraft.net/texture/{HASH}"{model}}}}}}}"#
        );
        let mut answers = BTreeMap::new();
        answers.insert(
            "https://api.minecraftservices.com/minecraft/profile/lookup/name/notch".to_owned(),
            format!(r#"{{"id":"{UUID}","name":"Notch"}}"#),
        );
        answers.insert(
            format!("https://sessionserver.mojang.com/session/minecraft/profile/{UUID}"),
            format!(
                r#"{{"id":"{UUID}","name":"Notch","properties":[{{"name":"textures","value":"{}"}}]}}"#,
                encode(&textures)
            ),
        );
        let mut pictures = BTreeMap::new();
        pictures.insert(format!("https://textures.minecraft.net/texture/{HASH}"), picture);
        Self {
            answers,
            pictures,
            asked: Mutex::new(Vec::new()),
        }
    }
}

impl SkinNet for FakeNet {
    fn get_text(&self, url: &str, _limit: usize) -> Result<String, String> {
        assert!(is_allowed_url(url), "{url}");
        self.asked.lock().unwrap().push(url.to_owned());
        self.answers
            .get(url)
            .cloned()
            .ok_or_else(|| "the skin service answered 404".to_owned())
    }

    fn get_bytes(&self, url: &str, _limit: usize) -> Result<Vec<u8>, String> {
        assert!(is_allowed_url(url), "{url}");
        self.asked.lock().unwrap().push(url.to_owned());
        self.pictures
            .get(url)
            .cloned()
            .ok_or_else(|| "the skin service answered 404".to_owned())
    }
}

#[test]
fn a_skin_added_from_a_picked_file_is_kept_under_its_hash_and_named_after_the_file() {
    let host = FakeHost::new();
    let path = host.file("Cool Hoodie.png", &skin_png(1));
    *host.picked.lock().unwrap() = Some(path);

    let result = add_from_file(&host).unwrap().unwrap();

    assert!(result.added);
    assert_eq!(result.entry.name, "Cool Hoodie");
    assert_eq!(result.entry.source, "file");
    assert!(!result.entry.slim);
    assert!(is_skin_id(&result.entry.id));
    assert_eq!(list(&host).unwrap(), vec![result.entry.clone()]);
    assert!(image(&host, &result.entry.id).unwrap().starts_with("data:image/png;base64,iVBORw0KGgo"));
}

#[test]
fn cancelling_the_dialog_adds_nothing() {
    let host = FakeHost::new();

    assert_eq!(add_from_file(&host).unwrap(), None);
    assert!(list(&host).unwrap().is_empty());
}

#[test]
fn the_same_picture_twice_is_one_skin() {
    let host = FakeHost::new();
    let first = add_bytes(&host, &skin_png(1), "One", false, "file").unwrap();

    let second = add_bytes(&host, &skin_png(1), "Other name", true, "player").unwrap();

    assert!(first.added);
    assert!(!second.added);
    assert_eq!(second.entry, first.entry);
    assert_eq!(list(&host).unwrap().len(), 1);
}

#[test]
fn only_skins_are_kept() {
    let host = FakeHost::new();
    let mut wrong_size = skin_png(1);
    wrong_size[16..20].copy_from_slice(&128u32.to_be_bytes());
    let mut too_big = skin_png(2);
    too_big.resize(MAX_SKIN_BYTES + 1, 0);

    for bad in [b"not a png".to_vec(), wrong_size, too_big, Vec::new()] {
        assert!(add_bytes(&host, &bad, "x", false, "file").is_err());
    }
    assert!(list(&host).unwrap().is_empty());

    // a file the dialog returned that is far too big is not read whole
    let path = host.file("huge.png", &vec![0u8; MAX_SKIN_BYTES * 4]);
    *host.picked.lock().unwrap() = Some(path);
    assert!(add_from_file(&host).is_err());
}

#[test]
fn a_player_is_found_by_name_and_the_model_is_kept() {
    let host = FakeHost::new();
    let net = FakeNet::knowing_notch(true, skin_png(7));

    // the name's capitalisation does not matter to the lookup; the answer's does to the list
    let result = add_from_player_with(&net, &host, " notch ").unwrap();

    assert_eq!(result.entry.name, "Notch");
    assert_eq!(result.entry.source, "player");
    assert!(result.entry.slim);
    assert_eq!(net.asked.lock().unwrap().len(), 3);
}

#[test]
fn a_name_that_is_not_a_player_name_asks_nothing() {
    let host = FakeHost::new();
    let net = FakeNet::knowing_notch(false, skin_png(7));

    for bad in ["", "ab", "../etc", "a/b", "x".repeat(17).as_str(), "no way"] {
        assert!(add_from_player_with(&net, &host, bad).is_err(), "{bad:?}");
    }
    assert!(net.asked.lock().unwrap().is_empty());
}

#[test]
fn an_unknown_player_and_a_picture_that_is_not_a_skin_are_refused() {
    let host = FakeHost::new();

    let unknown = FakeNet::knowing_notch(false, skin_png(7));
    assert_eq!(
        add_from_player_with(&unknown, &host, "nobody").unwrap_err(),
        "no player has this name"
    );

    let junk = FakeNet::knowing_notch(false, b"<svg onload=alert(1)>".to_vec());
    assert!(add_from_player_with(&junk, &host, "notch").is_err());
    assert!(list(&host).unwrap().is_empty());
}

#[test]
fn a_skin_can_be_renamed_and_its_model_changed() {
    let host = FakeHost::new();
    let id = add_bytes(&host, &skin_png(1), "One", false, "file").unwrap().entry.id;

    let renamed = rename(&host, &id, "  A\nnew name  ").unwrap();
    let slim = set_model(&host, &id, true).unwrap();

    assert_eq!(renamed.name, "A new name");
    assert!(slim.slim);
    assert_eq!(list(&host).unwrap()[0], slim);
    assert!(rename(&host, &id, "   ").is_err());
    assert!(rename(&host, &"0".repeat(64), "x").is_err());
    assert!(set_model(&host, "../x", true).is_err());
}

#[test]
fn a_removed_skin_is_gone_with_its_picture() {
    let host = FakeHost::new();
    let id = add_bytes(&host, &skin_png(1), "One", false, "file").unwrap().entry.id;
    let kept = add_bytes(&host, &skin_png(2), "Two", false, "file").unwrap().entry.id;

    remove(&host, &id).unwrap();

    assert_eq!(list(&host).unwrap().len(), 1);
    assert!(image(&host, &id).is_err());
    assert!(!host.dir.join("skins").join(format!("{id}.png")).exists());
    assert!(image(&host, &kept).is_ok());
    assert!(remove(&host, &id).is_err());
}

#[test]
fn only_a_skin_id_reaches_the_disk() {
    let host = FakeHost::new();
    add_bytes(&host, &skin_png(1), "One", false, "file").unwrap();
    let victim = host.file("victim.png", &skin_png(9));

    for bad in ["", "..", "../victim", "index", "a".repeat(63).as_str(), "A".repeat(64).as_str()] {
        assert!(image(&host, bad).is_err(), "{bad:?}");
        assert!(remove(&host, bad).is_err(), "{bad:?}");
        assert!(rename(&host, bad, "x").is_err(), "{bad:?}");
    }
    assert!(victim.exists());
    // a well-formed id that is not in the library is not read from the disk either
    fs::write(host.dir.join("skins").join(format!("{}.png", "b".repeat(64))), skin_png(5)).unwrap();
    assert!(image(&host, &"b".repeat(64)).is_err());
}

#[test]
fn a_picture_swapped_on_disk_is_not_served() {
    let host = FakeHost::new();
    let id = add_bytes(&host, &skin_png(1), "One", false, "file").unwrap().entry.id;
    let path = host.dir.join("skins").join(format!("{id}.png"));

    fs::write(&path, b"<html>not a skin</html>").unwrap();
    assert!(image(&host, &id).is_err());
    fs::write(&path, vec![0x89; MAX_SKIN_BYTES + 1]).unwrap();
    assert!(image(&host, &id).is_err());
}

#[test]
fn an_edited_index_cannot_make_the_library_misbehave() {
    let host = FakeHost::new();
    let good = add_bytes(&host, &skin_png(1), "One", false, "file").unwrap().entry;
    let index = format!(
        r#"{{"skins":[
            {{"id":"{}","name":"One","slim":false,"source":"file","added":1}},
            {{"id":"{}","name":"Dup","slim":false,"source":"file","added":2}},
            {{"id":"../../evil","name":"Path","slim":false,"source":"file","added":3}},
            {{"id":"{}","name":"Odd\nsource","slim":true,"source":"<script>","added":4}}
        ]}}"#,
        good.id,
        good.id,
        "c".repeat(64)
    );
    fs::write(host.dir.join("skins").join("index.json"), index).unwrap();

    let skins = list(&host).unwrap();

    assert_eq!(skins.len(), 2);
    assert!(skins.iter().all(|skin| is_skin_id(&skin.id)));
    let odd = skins.iter().find(|skin| skin.id == "c".repeat(64)).unwrap();
    assert_eq!(odd.name, "Odd source");
    assert_eq!(odd.source, "file");

    // a damaged index is an empty library, not an error
    fs::write(host.dir.join("skins").join("index.json"), b"{{{").unwrap();
    assert!(list(&host).unwrap().is_empty());
}

#[test]
fn the_library_is_limited() {
    let host = FakeHost::new();
    let folder = host.dir.join("skins");
    fs::create_dir_all(&folder).unwrap();
    let skins: Vec<SkinEntry> = (0..MAX_SKINS)
        .map(|n| SkinEntry {
            id: format!("{n:064x}"),
            name: format!("Skin {n}"),
            slim: false,
            source: "file".to_owned(),
            added: n as u64,
        })
        .collect();
    fs::write(folder.join("index.json"), serde_json::to_vec(&Index { skins }).unwrap()).unwrap();

    let error = add_bytes(&host, &skin_png(1), "One", false, "file").unwrap_err();

    assert!(error.contains("full"), "{error}");
    assert_eq!(list(&host).unwrap().len(), MAX_SKINS);
}

#[test]
fn the_dto_uses_camel_case_and_carries_no_path() {
    let value = serde_json::to_value(AddResult {
        entry: SkinEntry {
            id: "a".repeat(64),
            name: "One".to_owned(),
            slim: true,
            source: "file".to_owned(),
            added: 5,
        },
        added: true,
    })
    .unwrap();
    assert_eq!(value["entry"]["slim"], true);
    assert_eq!(value["added"], true);
    assert!(!value.to_string().contains('\\') && !value.to_string().contains(".png"));
}
