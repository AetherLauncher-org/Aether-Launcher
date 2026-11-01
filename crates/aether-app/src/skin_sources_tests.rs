use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::InstanceId;
use sha2::{Digest as _, Sha256};

use super::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

const UUID: &str = "069a79f444e94726a5befca90e38aaf5";

struct FakeHost {
    dir: std::path::PathBuf,
    picked: Mutex<Option<std::path::PathBuf>>,
}

impl FakeHost {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aether-skinsrc-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self {
            dir,
            picked: Mutex::new(None),
        }
    }
}

impl Host for FakeHost {
    fn emit_import_item(&self, _event: &crate::import::ImportItemEvent) {}

    fn pick_folder(&self, _title: &str) -> Result<Option<std::path::PathBuf>, String> {
        Ok(self.picked.lock().unwrap().take())
    }

    fn pick_file(
        &self,
        _filter_name: &str,
        _extensions: &[&str],
    ) -> Result<Option<std::path::PathBuf>, String> {
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

    fn data_dir(&self) -> Result<std::path::PathBuf, String> {
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
        let _ = std::fs::remove_dir_all(&self.dir);
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

fn profile_with(texture_url: &str, slim: bool, name: &str) -> String {
    let model = if slim {
        r#","metadata":{"model":"slim"}"#
    } else {
        ""
    };
    let textures = format!(
        r#"{{"timestamp":1,"profileId":"{UUID}","profileName":"{name}","textures":{{"SKIN":{{"url":"{texture_url}"{model}}}}}}}"#
    );
    format!(
        r#"{{"id":"{UUID}","name":"{name}","properties":[{{"name":"textures","value":"{}"}}]}}"#,
        encode(&textures)
    )
}

/// The three services, answering from a table.
struct FakeNet {
    answers: BTreeMap<String, String>,
    posts: BTreeMap<String, String>,
    pictures: BTreeMap<String, Vec<u8>>,
    asked: Mutex<Vec<String>>,
}

impl FakeNet {
    fn mojang(picture: Vec<u8>, slim: bool) -> Self {
        let hash = format!("{:x}", Sha256::digest(&picture));
        let mut answers = BTreeMap::new();
        answers.insert(
            "https://api.minecraftservices.com/minecraft/profile/lookup/name/notch".to_owned(),
            format!(r#"{{"id":"{UUID}","name":"Notch"}}"#),
        );
        answers.insert(
            format!("https://sessionserver.mojang.com/session/minecraft/profile/{UUID}"),
            profile_with(
                &format!("https://textures.minecraft.net/texture/{hash}"),
                slim,
                "Notch",
            ),
        );
        let mut pictures = BTreeMap::new();
        pictures.insert(
            format!("https://textures.minecraft.net/texture/{hash}"),
            picture,
        );
        Self {
            answers,
            posts: BTreeMap::new(),
            pictures,
            asked: Mutex::new(Vec::new()),
        }
    }

    fn littleskin(picture: Vec<u8>, slim: bool) -> Self {
        let hash = "b".repeat(64);
        let mut posts = BTreeMap::new();
        posts.insert(
            "https://littleskin.cn/api/yggdrasil/api/profiles/minecraft".to_owned(),
            format!(r#"[{{"id":"{UUID}","name":"Notch"}}]"#),
        );
        let mut answers = BTreeMap::new();
        answers.insert(
            format!(
                "https://littleskin.cn/api/yggdrasil/sessionserver/session/minecraft/profile/{UUID}"
            ),
            profile_with(
                &format!("https://littleskin.cn/textures/{hash}"),
                slim,
                "Notch",
            ),
        );
        let mut pictures = BTreeMap::new();
        pictures.insert(format!("https://littleskin.cn/textures/{hash}"), picture);
        Self {
            answers,
            posts,
            pictures,
            asked: Mutex::new(Vec::new()),
        }
    }

    fn ely(picture: Vec<u8>, slim: bool) -> Self {
        let hash = "c".repeat(32);
        let mut answers = BTreeMap::new();
        answers.insert(
            "https://authserver.ely.by/api/users/profiles/minecraft/notch".to_owned(),
            format!(r#"{{"id":"{UUID}","name":"Notch"}}"#),
        );
        answers.insert(
            format!("https://authserver.ely.by/session/profile/{UUID}"),
            profile_with(
                &format!("https://ely.by/storage/skins/{hash}.png"),
                slim,
                "Notch",
            ),
        );
        let mut pictures = BTreeMap::new();
        pictures.insert(format!("https://ely.by/storage/skins/{hash}.png"), picture);
        Self {
            answers,
            posts: BTreeMap::new(),
            pictures,
            asked: Mutex::new(Vec::new()),
        }
    }
}

impl SkinNet for FakeNet {
    fn get_text(&self, url: &str, _limit: usize) -> Result<String, String> {
        assert!(is_allowed_source_url(url), "{url}");
        self.asked.lock().unwrap().push(url.to_owned());
        self.answers
            .get(url)
            .cloned()
            .ok_or_else(|| "the skin service answered 404".to_owned())
    }

    fn get_bytes(&self, url: &str, _limit: usize) -> Result<Vec<u8>, String> {
        assert!(is_allowed_source_url(url), "{url}");
        self.asked.lock().unwrap().push(url.to_owned());
        self.pictures
            .get(url)
            .cloned()
            .ok_or_else(|| "the skin service answered 404".to_owned())
    }
}

impl SkinSourceNet for FakeNet {
    fn post_json(&self, url: &str, _body: &str, _limit: usize) -> Result<String, String> {
        assert!(is_allowed_source_url(url), "{url}");
        self.asked.lock().unwrap().push(url.to_owned());
        self.posts
            .get(url)
            .cloned()
            .ok_or_else(|| "the skin service answered 404".to_owned())
    }
}

#[test]
fn only_the_documented_source_addresses_are_contacted() {
    for good in [
        "https://littleskin.cn/api/yggdrasil/api/profiles/minecraft".to_owned(),
        format!(
            "https://littleskin.cn/api/yggdrasil/sessionserver/session/minecraft/profile/{UUID}"
        ),
        format!("https://littleskin.cn/textures/{}", "a".repeat(64)),
        "https://authserver.ely.by/api/users/profiles/minecraft/notch".to_owned(),
        format!("https://authserver.ely.by/session/profile/{UUID}"),
        format!("https://ely.by/storage/skins/{}.png", "b".repeat(32)),
        format!("https://textures.minecraft.net/texture/{}", "c".repeat(64)),
    ] {
        assert!(is_allowed_source_url(&good), "{good}");
    }
    for bad in [
        "http://littleskin.cn/api/yggdrasil/api/profiles/minecraft".to_owned(),
        "https://littleskin.cn/api/yggdrasil/api/profiles/minecraft?extra=1".to_owned(),
        "https://littleskin.cn.evil.test/textures/abc".to_owned(),
        "https://authserver.ely.by/session/profile/not-a-uuid".to_owned(),
        "https://ely.by/storage/skins/../escape.png".to_owned(),
        "https://ely.by/storage/skins/not-hex.png".to_owned(),
        "https://skinsystem.ely.by/skins/notch.png".to_owned(),
        "https://namemc.com/".to_owned(),
        String::new(),
    ] {
        assert!(!is_allowed_source_url(&bad), "{bad}");
    }
}

#[test]
fn the_three_sources_find_a_player_and_return_a_preview() {
    for (source, net) in [
        (SkinSource::Mojang, FakeNet::mojang(skin_png(1), false)),
        (
            SkinSource::LittleSkin,
            FakeNet::littleskin(skin_png(2), true),
        ),
        (SkinSource::ElyBy, FakeNet::ely(skin_png(3), true)),
    ] {
        let state = SkinDownloadState::default();
        let preview = search_with(&net, &state, source, " notch ").unwrap();
        assert_eq!(preview.source, source.as_str());
        assert_eq!(preview.name, "Notch");
        assert_eq!(preview.slim, source != SkinSource::Mojang);
        assert!(
            preview
                .image
                .starts_with("data:image/png;base64,iVBORw0KGgo")
        );
        assert!(!preview.token.is_empty());
        assert!(skin_file_name(&preview.name).ends_with(".png"));
    }
}

#[test]
fn an_ely_texture_proxied_from_mojang_is_accepted() {
    let picture = skin_png(4);
    let hash = format!("{:x}", Sha256::digest(&picture));
    let mut net = FakeNet::ely(picture, false);
    net.answers.insert(
        format!("https://authserver.ely.by/session/profile/{UUID}"),
        profile_with(
            &format!("http://textures.minecraft.net/texture/{hash}"),
            false,
            "Notch",
        ),
    );
    net.pictures.insert(
        format!("https://textures.minecraft.net/texture/{hash}"),
        skin_png(4),
    );
    let state = SkinDownloadState::default();

    let preview = search_with(&net, &state, SkinSource::ElyBy, "notch").unwrap();

    assert_eq!(preview.name, "Notch");
}

#[test]
fn a_player_without_a_skin_or_with_an_odd_answer_is_refused() {
    let state = SkinDownloadState::default();
    let mut net = FakeNet::littleskin(skin_png(5), false);
    net.answers.insert(
        format!(
            "https://littleskin.cn/api/yggdrasil/sessionserver/session/minecraft/profile/{UUID}"
        ),
        profile_with("https://evil.test/skin.png", false, "Notch"),
    );
    assert_eq!(
        search_with(&net, &state, SkinSource::LittleSkin, "notch").unwrap_err(),
        "the skin service named a texture somewhere it may not"
    );

    let mut no_skin = FakeNet::ely(skin_png(6), false);
    no_skin.answers.insert(
        format!("https://authserver.ely.by/session/profile/{UUID}"),
        format!(r#"{{"id":"{UUID}","name":"Notch"}}"#),
    );
    assert_eq!(
        search_with(&no_skin, &state, SkinSource::ElyBy, "notch").unwrap_err(),
        "this player has no custom skin"
    );

    let mut unknown = FakeNet::ely(skin_png(7), false);
    unknown.answers.insert(
        "https://authserver.ely.by/api/users/profiles/minecraft/notch".to_owned(),
        String::new(),
    );
    assert_eq!(
        search_with(&unknown, &state, SkinSource::ElyBy, "notch").unwrap_err(),
        "no player has this name"
    );
}

#[test]
fn a_bad_name_or_source_reaches_no_service() {
    let state = SkinDownloadState::default();
    let net = FakeNet::mojang(skin_png(8), false);

    for bad in ["", "ab", "no way", "x".repeat(17).as_str()] {
        assert!(
            search_with(&net, &state, SkinSource::Mojang, bad).is_err(),
            "{bad:?}"
        );
    }
    assert!(net.asked.lock().unwrap().is_empty());
    assert_eq!(SkinSource::parse("namemc"), None);
}

#[test]
fn a_token_adds_the_skin_to_the_library_as_a_download() {
    let host = FakeHost::new();
    let state = SkinDownloadState::default();
    let net = FakeNet::littleskin(skin_png(9), true);
    let preview = search_with(&net, &state, SkinSource::LittleSkin, "notch").unwrap();

    let added = add_downloaded_with(&host, &state, &preview.token).unwrap();

    assert!(added.added);
    assert_eq!(added.entry.source, "download");
    assert!(added.entry.slim);
    assert!(skin_library::list(&host).unwrap().contains(&added.entry));
    let again = add_downloaded_with(&host, &state, &preview.token).unwrap();
    assert!(!again.added);
}

#[test]
fn an_unknown_token_adds_and_saves_nothing() {
    let host = FakeHost::new();
    let state = SkinDownloadState::default();

    assert!(add_downloaded_with(&host, &state, "nope").is_err());
    assert!(save_downloaded_with(&host, &state, "nope").is_err());
    assert!(skin_library::list(&host).unwrap().is_empty());
}

#[test]
fn a_skin_is_saved_to_the_picked_folder_without_overwriting() {
    let host = FakeHost::new();
    let folder = host.dir.join("pictures");
    std::fs::create_dir_all(&folder).unwrap();
    *host.picked.lock().unwrap() = Some(folder.clone());
    let state = SkinDownloadState::default();
    let net = FakeNet::ely(skin_png(10), false);
    let preview = search_with(&net, &state, SkinSource::ElyBy, "notch").unwrap();

    let saved = save_downloaded_with(&host, &state, &preview.token)
        .unwrap()
        .unwrap();

    assert_eq!(saved.file_name, "Notch.png");
    assert_eq!(
        std::fs::read(folder.join("Notch.png")).unwrap(),
        skin_png(10)
    );
    *host.picked.lock().unwrap() = Some(folder.clone());
    assert_eq!(
        save_downloaded_with(&host, &state, &preview.token).unwrap_err(),
        "a file with this name is already there"
    );
    assert_eq!(
        std::fs::read(folder.join("Notch.png")).unwrap(),
        skin_png(10)
    );
}

#[test]
fn cancelling_the_folder_dialog_saves_nothing() {
    let host = FakeHost::new();
    let state = SkinDownloadState::default();
    let net = FakeNet::mojang(skin_png(11), false);
    let preview = search_with(&net, &state, SkinSource::Mojang, "notch").unwrap();

    assert_eq!(
        save_downloaded_with(&host, &state, &preview.token).unwrap(),
        None
    );
    assert!(!host.dir.join("pictures").join("Notch.png").exists());
}

#[test]
fn windows_reserved_player_names_get_a_safe_file_name() {
    assert_eq!(skin_file_name("con"), "skin-con.png");
    assert_eq!(skin_file_name("COM1"), "skin-COM1.png");
    assert_eq!(skin_file_name("Notch"), "Notch.png");
    assert_eq!(skin_file_name(""), "skin.png");
}

#[test]
fn only_three_fixed_site_addresses_can_be_opened() {
    assert_eq!(skin_site_url("namemc"), Some("https://namemc.com/"));
    assert_eq!(
        skin_site_url("skindex"),
        Some("https://www.minecraftskins.com/")
    );
    assert_eq!(
        skin_site_url("planetminecraft"),
        Some("https://www.planetminecraft.com/skins/")
    );
    for bad in ["https://evil.test", "namemc/extra", "", "curseforge"] {
        assert_eq!(skin_site_url(bad), None, "{bad:?}");
    }
}

#[test]
fn foreign_and_malformed_texture_urls_are_rejected_before_downloading() {
    for url in [
        "https://littleskin.cn.evil.test/textures/abc",
        "https://littleskin.cn@evil.test/textures/abc",
        "https://littleskin.cn:444/textures/abc",
        "https://littleskin.cn/textures/../abc",
        "https://littleskin.cn/textures/abc?next=evil",
        "https://littleskin.cn/textures/abc#fragment",
        "file:///skin.png",
        "https://ely.by/storage/skins/abc.png",
    ] {
        let mut net = FakeNet::littleskin(skin_png(1), false);
        net.answers.insert(
            format!(
                "https://littleskin.cn/api/yggdrasil/sessionserver/session/minecraft/profile/{UUID}"
            ),
            profile_with(url, false, "Notch"),
        );
        assert!(
            search_with(
                &net,
                &SkinDownloadState::default(),
                SkinSource::LittleSkin,
                "notch"
            )
            .is_err(),
            "{url}"
        );
        assert_eq!(net.asked.lock().unwrap().len(), 2);
    }
    assert!(trusted_ely_texture("https://littleskin.cn/textures/abc").is_none());
    assert!(trusted_ely_texture("https://ely.by:443/storage/skins/abc.png").is_none());
    assert!(!is_allowed_source_url(
        "http://ely.by/storage/skins/abc.png"
    ));
}

#[test]
fn oversized_documents_and_invalid_pngs_are_refused() {
    let mut net = FakeNet::littleskin(skin_png(1), false);
    net.posts.insert(
        "https://littleskin.cn/api/yggdrasil/api/profiles/minecraft".to_owned(),
        " ".repeat(MAX_PROFILE_BYTES + 1),
    );
    assert!(
        search_with(
            &net,
            &SkinDownloadState::default(),
            SkinSource::LittleSkin,
            "notch"
        )
        .is_err()
    );
    assert!(
        skin_from_source_profile(&" ".repeat(MAX_PROFILE_BYTES + 1), trusted_ely_texture).is_err()
    );
    assert!(
        skin_from_source_profile(
            &profile_with("https://ely.by/storage/skins/abc.png", false, "../bad"),
            trusted_ely_texture
        )
        .is_err()
    );
    for bytes in [
        b"<html>not a skin</html>".to_vec(),
        vec![0; MAX_SKIN_BYTES + 1],
    ] {
        let net = FakeNet::ely(bytes, false);
        assert!(
            search_with(
                &net,
                &SkinDownloadState::default(),
                SkinSource::ElyBy,
                "notch"
            )
            .is_err()
        );
    }
}

#[test]
fn proxied_mojang_textures_still_require_their_sha256() {
    let original = skin_png(1);
    let hash = format!("{:x}", Sha256::digest(&original));
    let mut net = FakeNet::ely(original, false);
    net.answers.insert(
        format!("https://authserver.ely.by/session/profile/{UUID}"),
        profile_with(
            &format!("https://textures.minecraft.net/texture/{hash}"),
            false,
            "Notch",
        ),
    );
    net.pictures.insert(
        format!("https://textures.minecraft.net/texture/{hash}"),
        skin_png(2),
    );
    assert_eq!(
        search_with(
            &net,
            &SkinDownloadState::default(),
            SkinSource::ElyBy,
            "notch"
        )
        .unwrap_err(),
        "the skin does not match its texture hash"
    );
}

#[test]
fn the_pending_preview_store_is_bounded() {
    let net = FakeNet::ely(skin_png(1), false);
    let state = SkinDownloadState::default();
    let first = search_with(&net, &state, SkinSource::ElyBy, "notch").unwrap();
    for _ in 0..MAX_PENDING_SKINS {
        search_with(&net, &state, SkinSource::ElyBy, "notch").unwrap();
    }
    assert!(state.get(&first.token).is_err());
    assert_eq!(state.pending.lock().unwrap().len(), 1);
}

#[test]
fn the_dtos_use_camel_case_and_carry_no_path_or_address() {
    let preview = SkinPreview {
        token: "token".to_owned(),
        source: "mojang",
        name: "Notch".to_owned(),
        slim: true,
        image: "data:image/png;base64,xxx".to_owned(),
    };
    let saved = SavedSkin {
        file_name: "Notch.png".to_owned(),
    };
    let preview = serde_json::to_value(preview).unwrap();
    let saved = serde_json::to_value(saved).unwrap();
    assert_eq!(preview["source"], "mojang");
    assert_eq!(saved["fileName"], "Notch.png");
    assert!(!saved.to_string().contains('\\'));
}
