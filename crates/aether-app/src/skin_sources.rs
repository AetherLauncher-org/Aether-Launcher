//! Skin sources for the download page: the services with a documented public API.
//!
//! Three sources are automatic: Mojang's own chain (player name → UUID → session profile →
//! `textures.minecraft.net`) and two authlib-injector services, LittleSkin and Ely.by, whose
//! documented endpoints are used exactly. NameMC, Skindex and Planet Minecraft have no such
//! API and are never scraped; [`open_skin_site`] only opens three fixed addresses.
//!
//! Every answer is untrusted. Requests go to a fixed allow-list of HTTPS hosts, redirects are
//! not followed, bodies are bounded, and a texture address from a profile is accepted only
//! when it is on the same source's documented texture host. The picture itself is checked
//! with `aether_content::skin::validate_skin_png` before the page ever sees it. Mojang's
//! texture address carries the picture's SHA-256, and `skins::download_by_name` checks it.
//!
//! A search result is kept behind an opaque token in [`SkinDownloadState`]; the page later
//! says only "add token" or "save token to a folder the user picks", never a path, an
//! address or the picture's bytes.

use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use aether_content::clean_text;
use aether_content::skin::{
    MAX_PROFILE_BYTES, MAX_SKIN_BYTES, base64_decode, is_player_name, is_uuid, trusted_texture_url,
    validate_skin_png,
};
use aether_core::InstanceId;
use serde::{Deserialize, Serialize};

use crate::host::Host;
use crate::platform;
use crate::skin_library::{self, AddResult};
use crate::skins::{PlayerSkin, SkinNet, UreqSkinNet, download_by_name};

/// How long the decoded `textures` document may be, in bytes.
const MAX_TEXTURES_BYTES: usize = 16 * 1024;

/// How many search results stay addressable at once. The page shows one at a time; the cap
/// keeps a long session from growing the map without bound.
const MAX_PENDING_SKINS: usize = 8;

/// The fixed home pages behind the "open in the browser" buttons.
const SKIN_SITES: [(&str, &str); 3] = [
    ("namemc", "https://namemc.com/"),
    ("skindex", "https://www.minecraftskins.com/"),
    ("planetminecraft", "https://www.planetminecraft.com/skins/"),
];

/// A source with a documented public skin API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkinSource {
    Mojang,
    LittleSkin,
    ElyBy,
}

impl SkinSource {
    /// `mojang`, `littleskin` or `elyby`; anything else is not a source.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "mojang" => Some(Self::Mojang),
            "littleskin" => Some(Self::LittleSkin),
            "elyby" => Some(Self::ElyBy),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mojang => "mojang",
            Self::LittleSkin => "littleskin",
            Self::ElyBy => "elyby",
        }
    }
}

/// What the skin sources need from the network. A trait so tests can answer without one.
pub trait SkinSourceNet: SkinNet {
    /// `POST` a JSON body, return the answer's text.
    ///
    /// # Errors
    ///
    /// When the service cannot be reached or sends more than `limit` bytes.
    fn post_json(&self, url: &str, body: &str, limit: usize) -> Result<String, String>;
}

/// `true` for the requests the download page's sources may make: the exact documented hosts
/// and path shapes, HTTPS only, no query, no port, no userinfo.
#[must_use]
pub fn is_allowed_source_url(url: &str) -> bool {
    if !url.starts_with("https://") {
        return false;
    }
    let littleskin_name = url == "https://littleskin.cn/api/yggdrasil/api/profiles/minecraft";
    let littleskin_profile = url
        .strip_prefix(
            "https://littleskin.cn/api/yggdrasil/sessionserver/session/minecraft/profile/",
        )
        .is_some_and(is_uuid);
    let littleskin_texture = url
        .strip_prefix("https://littleskin.cn/textures/")
        .is_some_and(is_texture_hash);
    let ely_name = url
        .strip_prefix("https://authserver.ely.by/api/users/profiles/minecraft/")
        .is_some_and(is_player_name);
    let ely_profile = url
        .strip_prefix("https://authserver.ely.by/session/profile/")
        .is_some_and(is_uuid);
    let ely_texture = url
        .strip_prefix("https://ely.by/storage/skins/")
        .map(|rest| rest.strip_suffix(".png").is_some_and(is_texture_hash))
        .unwrap_or_default();
    let mojang = crate::skins::is_allowed_url(url);
    (mojang
        || littleskin_name
        || littleskin_profile
        || littleskin_texture
        || ely_name
        || ely_profile
        || ely_texture)
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// The production [`SkinSourceNet`]: the same blocking `ureq` setup as the avatar lookup,
/// with the sources' wider (still fixed) allow-list.
pub struct UreqSkinSourceNet;

impl SkinNet for UreqSkinSourceNet {
    fn get_text(&self, url: &str, limit: usize) -> Result<String, String> {
        UreqSkinNet::get_text_on(url, limit, is_allowed_source_url)
    }

    fn get_bytes(&self, url: &str, limit: usize) -> Result<Vec<u8>, String> {
        UreqSkinNet::get_bytes_on(url, limit, is_allowed_source_url)
    }
}

impl SkinSourceNet for UreqSkinSourceNet {
    fn post_json(&self, url: &str, body: &str, limit: usize) -> Result<String, String> {
        UreqSkinNet::post_json_on(url, body, limit, is_allowed_source_url)
    }
}

/// `true` for 1 to 64 lower-case hexadecimal digits, the shape of a texture name.
fn is_texture_hash(hash: &str) -> bool {
    (1..=64).contains(&hash.len()) && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A texture address on `littleskin.cn`, as the profile may name it (`http` is upgraded).
fn trusted_littleskin_texture(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    let hash = path.strip_prefix("textures/")?;
    (host == "littleskin.cn" && is_texture_hash(hash))
        .then(|| format!("https://littleskin.cn/textures/{hash}"))
}

/// A texture address Ely.by may name: its own storage, or (documented proxying) Mojang's
/// texture host. `http` is upgraded.
fn trusted_ely_texture(url: &str) -> Option<String> {
    if let Some(mojang) = trusted_texture_url(url) {
        return Some(mojang);
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    let file = path.strip_prefix("storage/skins/")?;
    let hash = file.strip_suffix(".png")?;
    (host == "ely.by" && is_texture_hash(hash))
        .then(|| format!("https://ely.by/storage/skins/{hash}.png"))
}

#[derive(Deserialize)]
struct NameAnswer {
    id: String,
    name: String,
}

/// Reads a one-name answer (`{"id","name"}`); `Ok(None)` for an empty body, which the
/// services use for "no such player".
fn parse_name_answer(body: &str) -> Result<Option<(String, String)>, String> {
    if body.trim().is_empty() {
        return Ok(None);
    }
    if body.len() > MAX_PROFILE_BYTES {
        return Err("the skin service sent too much".to_owned());
    }
    let answer: NameAnswer = serde_json::from_str(body)
        .map_err(|_| "the skin service sent something unusable".to_owned())?;
    let id = answer.id.to_ascii_lowercase();
    if !is_uuid(&id) || !is_player_name(&answer.name) {
        return Err("the skin service sent something unusable".to_owned());
    }
    Ok(Some((id, answer.name)))
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    name: String,
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct Textures {
    textures: Option<TextureSet>,
}

#[derive(Deserialize)]
struct TextureSet {
    #[serde(rename = "SKIN")]
    skin: Option<Texture>,
}

#[derive(Deserialize)]
struct Texture {
    url: String,
    metadata: Option<TextureMetadata>,
}

#[derive(Deserialize)]
struct TextureMetadata {
    model: Option<String>,
}

/// The skin a profile names, before its address is trusted.
#[derive(Debug, Clone)]
struct SourceSkin {
    url: String,
    slim: bool,
}

/// Reads a profile from LittleSkin or Ely.by: the player's real name, and the skin they wear
/// (or `None` when the profile carries no `SKIN`). `trusted` turns the profile's texture
/// address into the HTTPS address to use, or `None` to refuse it.
fn skin_from_source_profile(
    body: &str,
    trusted: fn(&str) -> Option<String>,
) -> Result<(String, Option<SourceSkin>), String> {
    if body.len() > MAX_PROFILE_BYTES {
        return Err("the skin service sent too much".to_owned());
    }
    let profile: Profile = serde_json::from_str(body)
        .map_err(|_| "the skin service sent something unusable".to_owned())?;
    if !is_player_name(&profile.name) {
        return Err("the skin service sent something unusable".to_owned());
    }
    let Some(property) = profile.properties.iter().find(|p| p.name == "textures") else {
        return Ok((profile.name, None));
    };
    let decoded = base64_decode(&property.value, MAX_TEXTURES_BYTES)
        .ok_or("the skin service sent something unusable")?;
    let textures: Textures = serde_json::from_slice(&decoded)
        .map_err(|_| "the skin service sent something unusable".to_owned())?;
    let Some(skin) = textures.textures.and_then(|set| set.skin) else {
        return Ok((profile.name, None));
    };
    let url = trusted(&skin.url).ok_or("the skin service named a texture somewhere it may not")?;
    let slim = skin
        .metadata
        .and_then(|metadata| metadata.model)
        .is_some_and(|model| model == "slim");
    Ok((profile.name, Some(SourceSkin { url, slim })))
}

/// One downloaded skin, waiting behind a token.
#[derive(Clone)]
struct PendingSkin {
    source: SkinSource,
    name: String,
    bytes: Vec<u8>,
    slim: bool,
}

/// Search results the page has not acted on yet, behind opaque tokens.
#[derive(Default, Clone)]
pub struct SkinDownloadState {
    pending: Arc<Mutex<HashMap<String, PendingSkin>>>,
}

impl SkinDownloadState {
    fn remember(&self, skin: PendingSkin) -> Result<String, String> {
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| "the skin sources are busy".to_owned())?;
        if pending.len() >= MAX_PENDING_SKINS {
            pending.clear();
        }
        let token = InstanceId::new().to_string();
        pending.insert(token.clone(), skin);
        Ok(token)
    }

    fn get(&self, token: &str) -> Result<PendingSkin, String> {
        self.pending
            .lock()
            .map_err(|_| "the skin sources are busy".to_owned())?
            .get(token)
            .cloned()
            .ok_or_else(|| "this skin is no longer available; search for it again".to_owned())
    }
}

/// A search result: an opaque token, who wears the skin, which model it is made for, and the
/// checked picture as a `data:` URL. No address and no path crosses the boundary.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkinPreview {
    pub token: String,
    pub source: &'static str,
    pub name: String,
    pub slim: bool,
    pub image: String,
}

/// A skin saved into a folder the user picked: only the file's name, never its path.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedSkin {
    pub file_name: String,
}

/// The skin a player wears, from one of the documented sources.
fn download_with(
    net: &dyn SkinSourceNet,
    source: SkinSource,
    name: &str,
) -> Result<PendingSkin, String> {
    let name = name.trim();
    if !is_player_name(name) {
        return Err("that is not a player name".to_owned());
    }
    match source {
        SkinSource::Mojang => {
            let PlayerSkin { name, bytes, slim } = download_by_name(net, name)?;
            Ok(PendingSkin {
                source,
                name,
                bytes,
                slim,
            })
        }
        SkinSource::LittleSkin => {
            let body = net.post_json(
                "https://littleskin.cn/api/yggdrasil/api/profiles/minecraft",
                &format!("[\"{name}\"]"),
                MAX_PROFILE_BYTES,
            )?;
            if body.len() > MAX_PROFILE_BYTES {
                return Err("the skin service sent too much".to_owned());
            }
            let answers: Vec<NameAnswer> = if body.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&body)
                    .map_err(|_| "the skin service sent something unusable".to_owned())?
            };
            if answers.len() > 10 {
                return Err("the skin service sent too much".to_owned());
            }
            let answer = answers
                .iter()
                .find(|answer| answer.name.eq_ignore_ascii_case(name))
                .ok_or("no player has this name")?;
            let uuid = answer.id.to_ascii_lowercase();
            if !is_uuid(&uuid) || !is_player_name(&answer.name) {
                return Err("the skin service sent something unusable".to_owned());
            }
            let profile = net.get_text(
                &format!(
                    "https://littleskin.cn/api/yggdrasil/sessionserver/session/minecraft/profile/{uuid}"
                ),
                MAX_PROFILE_BYTES,
            )?;
            let (name, skin) = skin_from_source_profile(&profile, trusted_littleskin_texture)?;
            let bytes = texture_bytes(net, skin.as_ref())?;
            Ok(PendingSkin {
                source,
                name,
                bytes,
                slim: skin.is_some_and(|skin| skin.slim),
            })
        }
        SkinSource::ElyBy => {
            let body = net.get_text(
                &format!("https://authserver.ely.by/api/users/profiles/minecraft/{name}"),
                MAX_PROFILE_BYTES,
            )?;
            let Some((uuid, _)) = parse_name_answer(&body)? else {
                return Err("no player has this name".to_owned());
            };
            let profile = net.get_text(
                &format!("https://authserver.ely.by/session/profile/{uuid}"),
                MAX_PROFILE_BYTES,
            )?;
            let (name, skin) = skin_from_source_profile(&profile, trusted_ely_texture)?;
            let bytes = texture_bytes(net, skin.as_ref())?;
            Ok(PendingSkin {
                source,
                name,
                bytes,
                slim: skin.is_some_and(|skin| skin.slim),
            })
        }
    }
}

/// Takes the picture a profile named. A player without a custom skin is an error here; the
/// caller has already committed to showing one result.
fn texture_bytes(net: &dyn SkinSourceNet, skin: Option<&SourceSkin>) -> Result<Vec<u8>, String> {
    let skin = skin.ok_or("this player has no custom skin")?;
    let bytes = net.get_bytes(&skin.url, MAX_SKIN_BYTES)?;
    if skin.url.starts_with("https://textures.minecraft.net/") {
        crate::skins::verify_texture_hash(&skin.url, &bytes)?;
    }
    validate_skin_png(&bytes).map_err(|_| "this is not a Minecraft skin".to_owned())?;
    Ok(bytes)
}

/// Searches a source for the skin a player wears and keeps the result behind a token.
///
/// # Errors
///
/// For a source or name that is not one, a player who does not exist or has no custom skin,
/// a service that cannot be reached, or an answer that is not what the service documents.
pub fn search_with(
    net: &dyn SkinSourceNet,
    state: &SkinDownloadState,
    source: SkinSource,
    name: &str,
) -> Result<SkinPreview, String> {
    let PendingSkin {
        source,
        name,
        bytes,
        slim,
    } = download_with(net, source, name)?;
    let image = format!(
        "data:image/png;base64,{}",
        crate::schematics::base64(&bytes)
    );
    let token = state.remember(PendingSkin {
        source,
        name: name.clone(),
        bytes,
        slim,
    })?;
    Ok(SkinPreview {
        token,
        source: source.as_str(),
        name,
        slim,
        image,
    })
}

/// [`search_with`] against the real services.
///
/// # Errors
///
/// As [`search_with`].
pub fn search(
    state: &SkinDownloadState,
    source: SkinSource,
    name: &str,
) -> Result<SkinPreview, String> {
    search_with(&UreqSkinSourceNet, state, source, name)
}

/// Adds a searched skin to the library, recorded as coming from the download page.
///
/// # Errors
///
/// For a token this session no longer has, and for a picture that is no longer a skin or a
/// full library.
pub fn add_downloaded_with(
    host: &dyn Host,
    state: &SkinDownloadState,
    token: &str,
) -> Result<AddResult, String> {
    let skin = state.get(token)?;
    skin_library::add_bytes(host, &skin.bytes, &skin.name, skin.slim, "download")
}

/// [`add_downloaded_with`] against the real library.
///
/// # Errors
///
/// As [`add_downloaded_with`].
pub fn add_downloaded(
    host: &dyn Host,
    state: &SkinDownloadState,
    token: &str,
) -> Result<AddResult, String> {
    add_downloaded_with(host, state, token)
}

/// A safe file name for a skin: the player's name, with Windows' reserved names kept out.
fn skin_file_name(name: &str) -> String {
    let cleaned = clean_text(name, 32);
    let stem = if cleaned.is_empty() {
        "skin".to_owned()
    } else {
        cleaned
    };
    let upper = stem.to_ascii_uppercase();
    let numbered = ["COM", "LPT"].iter().any(|prefix| {
        upper
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.len() == 1 && matches!(rest.as_bytes()[0], b'1'..=b'9'))
    });
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered;
    let stem = if reserved {
        format!("skin-{stem}")
    } else {
        stem
    };
    format!("{stem}.png")
}

/// Writes `bytes` to `folder` under a new name; a file that is already there is never
/// replaced.
fn write_new_skin(folder: &Path, name: &str, bytes: &[u8]) -> Result<SavedSkin, String> {
    let file_name = skin_file_name(name);
    let target = folder.join(&file_name);
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            return Err("a file with this name is already there".to_owned());
        }
        Err(_) => return Err("cannot write the skin".to_owned()),
    };
    if file.write_all(bytes).is_err() {
        let _ = fs::remove_file(&target);
        return Err("cannot write the skin".to_owned());
    }
    Ok(SavedSkin { file_name })
}

/// Saves a searched skin into a folder the user picks in the native dialog. `Ok(None)` when
/// the dialog is cancelled. The folder's path stays in the backend.
///
/// # Errors
///
/// For a token this session no longer has, a picture that is no longer a skin, a file that is
/// already there, and a folder that cannot be written to.
pub fn save_downloaded_with(
    host: &dyn Host,
    state: &SkinDownloadState,
    token: &str,
) -> Result<Option<SavedSkin>, String> {
    let skin = state.get(token)?;
    validate_skin_png(&skin.bytes).map_err(|_| "this is not a Minecraft skin".to_owned())?;
    let Some(folder) = host.pick_folder("保存皮肤到哪个文件夹？")? else {
        return Ok(None);
    };
    write_new_skin(&folder, &skin.name, &skin.bytes).map(Some)
}

/// [`save_downloaded_with`] against the real dialog and disk.
///
/// # Errors
///
/// As [`save_downloaded_with`].
pub fn save_downloaded(
    host: &dyn Host,
    state: &SkinDownloadState,
    token: &str,
) -> Result<Option<SavedSkin>, String> {
    save_downloaded_with(host, state, token)
}

/// The fixed home page behind an "open in the browser" button, by key.
#[must_use]
pub fn skin_site_url(site: &str) -> Option<&'static str> {
    SKIN_SITES
        .iter()
        .find(|(key, _)| *key == site)
        .map(|(_, url)| *url)
}

/// Opens one of the three skin sites without an official API in the system browser. The
/// address is chosen here from a fixed list; the page cannot name another.
///
/// # Errors
///
/// For a key that is not one, or when the browser cannot be started.
pub fn open_skin_site(site: &str) -> Result<(), String> {
    let url = skin_site_url(site).ok_or("not a skin site".to_owned())?;
    platform::open_in_browser(url)
}

#[cfg(test)]
#[path = "skin_sources_tests.rs"]
mod tests;
