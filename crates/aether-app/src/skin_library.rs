//! The skin library: skins the player keeps on this machine, to wear later or on several
//! accounts.
//!
//! A skin is a PNG file of 64×64 (or the old 64×32) pixels and at most 64 KiB, kept in
//! `<data>/skins/` under the SHA-256 of its bytes, so the same picture added twice is one skin.
//! A small `index.json` beside them gives each skin a name, a model (classic or slim arms) and
//! where it came from. A caller names a skin by its id (the hash); it never supplies a path.
//!
//! Skins come from a file the player picks with the system dialog, from a player's name (the
//! skin that player wears, from Mojang's public services) or, later, from a download. Whatever
//! the source, the bytes are checked before they are kept (`aether_content::skin`): the header
//! is read, nothing is decoded. Files in the folder are untrusted too: the index is read with a
//! size limit, a picture is checked again each time it is read, and nothing is written through
//! a link.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use aether_content::clean_text;
use aether_content::skin::{MAX_SKIN_BYTES, validate_skin_png};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::cachefile::{read_capped, write_replacing};
use crate::host::Host;
use crate::skins::{SkinNet, UreqSkinNet, download_by_name};

/// The most skins the library holds.
pub const MAX_SKINS: usize = 500;
const MAX_NAME_CHARS: usize = 32;
const MAX_INDEX_BYTES: u64 = 1024 * 1024;
const INDEX_FILE: &str = "index.json";

/// Serialises every read-modify-write of the index, so two commands cannot lose each other's
/// change.
static INDEX_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinEntry {
    /// The SHA-256 of the picture, 64 lower-case hexadecimal digits.
    pub id: String,
    pub name: String,
    /// Slim ("Alex") arms; classic ("Steve") otherwise.
    pub slim: bool,
    /// `file`, `player` or `download`.
    pub source: String,
    /// Seconds since the Unix epoch.
    pub added: u64,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    pub entry: SkinEntry,
    /// `false` when the library already had this exact picture; `entry` is then the existing
    /// skin and nothing changed.
    pub added: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    skins: Vec<SkinEntry>,
}

fn folder(host: &dyn Host) -> Result<PathBuf, String> {
    Ok(host.data_dir()?.join("skins"))
}

/// `true` for a skin id: 64 lower-case hexadecimal digits.
#[must_use]
pub fn is_skin_id(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn load_index(folder: &std::path::Path) -> Index {
    let mut index: Index = read_capped(&folder.join(INDEX_FILE), MAX_INDEX_BYTES)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    // an index someone edited: keep only well-formed, distinct entries, and no more than allowed
    let mut seen = BTreeSet::new();
    index.skins.retain(|skin| is_skin_id(&skin.id) && seen.insert(skin.id.clone()));
    index.skins.truncate(MAX_SKINS);
    for skin in &mut index.skins {
        skin.name = clean_text(&skin.name, MAX_NAME_CHARS);
        if !matches!(skin.source.as_str(), "file" | "player" | "download") {
            "file".clone_into(&mut skin.source);
        }
    }
    index
}

fn save_index(folder: &std::path::Path, index: &Index) -> Result<(), String> {
    fs::create_dir_all(folder).map_err(|_| "cannot create the skin folder".to_owned())?;
    let text = serde_json::to_vec(index).map_err(|_| "cannot save the skin library".to_owned())?;
    write_replacing(&folder.join(INDEX_FILE), &text)
        .map_err(|_| "cannot save the skin library".to_owned())
}

fn picture_path(folder: &std::path::Path, id: &str) -> PathBuf {
    folder.join(format!("{id}.png"))
}

/// A name for the list: cleaned, cut, and never empty.
fn display_name(wanted: &str) -> String {
    let name = clean_text(wanted, MAX_NAME_CHARS);
    if name.is_empty() { "皮肤".to_owned() } else { name }
}

// --------------------------------------------------------------------------------------
// Reading
// --------------------------------------------------------------------------------------

/// The skins in the library, newest first. Pictures are not included (see [`image`]).
///
/// # Errors
///
/// When the data directory is unavailable.
pub fn list(host: &dyn Host) -> Result<Vec<SkinEntry>, String> {
    let folder = folder(host)?;
    let _guard = INDEX_LOCK.lock().map_err(|_| "the skin library is busy".to_owned())?;
    let mut skins = load_index(&folder).skins;
    skins.sort_by(|a, b| b.added.cmp(&a.added).then_with(|| a.name.cmp(&b.name)));
    Ok(skins)
}

/// A skin's picture as a `data:` URL, checked again as it is read.
///
/// # Errors
///
/// For an id that is not a skin id, a skin that is not in the library, or a picture that is
/// missing or no longer a skin.
pub fn image(host: &dyn Host, id: &str) -> Result<String, String> {
    if !is_skin_id(id) {
        return Err("not a skin".to_owned());
    }
    let folder = folder(host)?;
    {
        let _guard = INDEX_LOCK.lock().map_err(|_| "the skin library is busy".to_owned())?;
        if !load_index(&folder).skins.iter().any(|skin| skin.id == id) {
            return Err("no such skin".to_owned());
        }
    }
    let bytes = read_capped(&picture_path(&folder, id), MAX_SKIN_BYTES as u64)
        .ok_or("the picture of this skin is missing")?;
    validate_skin_png(&bytes).map_err(|_| "the picture of this skin is damaged".to_owned())?;
    Ok(format!(
        "data:image/png;base64,{}",
        crate::schematics::base64(&bytes)
    ))
}

// --------------------------------------------------------------------------------------
// Adding
// --------------------------------------------------------------------------------------

/// Adds a picture to the library. The bytes are checked first; a picture the library already
/// has is not stored twice.
pub(crate) fn add_bytes(
    host: &dyn Host,
    bytes: &[u8],
    name: &str,
    slim: bool,
    source: &str,
) -> Result<AddResult, String> {
    validate_skin_png(bytes).map_err(|_| "this is not a Minecraft skin (a PNG of 64×64 or 64×32 pixels, up to 64 KiB)".to_owned())?;
    let id = format!("{:x}", Sha256::digest(bytes));
    let folder = folder(host)?;
    let _guard = INDEX_LOCK.lock().map_err(|_| "the skin library is busy".to_owned())?;
    let mut index = load_index(&folder);
    if let Some(existing) = index.skins.iter().find(|skin| skin.id == id) {
        return Ok(AddResult {
            entry: existing.clone(),
            added: false,
        });
    }
    if index.skins.len() >= MAX_SKINS {
        return Err(format!("the library is full ({MAX_SKINS} skins), remove some first"));
    }
    fs::create_dir_all(&folder).map_err(|_| "cannot create the skin folder".to_owned())?;
    write_replacing(&picture_path(&folder, &id), bytes)
        .map_err(|_| "cannot save the skin".to_owned())?;
    let entry = SkinEntry {
        id,
        name: display_name(name),
        slim,
        source: source.to_owned(),
        added: now(),
    };
    index.skins.push(entry.clone());
    if let Err(error) = save_index(&folder, &index) {
        let _ = fs::remove_file(picture_path(&folder, &entry.id));
        return Err(error);
    }
    Ok(AddResult { entry, added: true })
}

/// Adds a skin from a PNG file the player picks in the system dialog. `Ok(None)` when the
/// dialog is cancelled. The file's name (without `.png`) becomes the skin's name.
///
/// # Errors
///
/// For a file that cannot be read, is too big or is not a skin, and for a full library.
pub fn add_from_file(host: &dyn Host) -> Result<Option<AddResult>, String> {
    let Some(path) = host.pick_file("PNG", &["png"])? else {
        return Ok(None);
    };
    let bytes = read_capped(&path, MAX_SKIN_BYTES as u64)
        .ok_or("cannot read this file, or it is too big for a skin (64 KiB at most)")?;
    let name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    add_bytes(host, &bytes, name, false, "file").map(Some)
}

/// Adds the skin a player wears, found by name, from Mojang's public services.
///
/// # Errors
///
/// For a name that is not a player name, a player who does not exist or has no custom skin,
/// a service that cannot be reached, and a full library.
pub fn add_from_player_with(net: &dyn SkinNet, host: &dyn Host, name: &str) -> Result<AddResult, String> {
    let skin = download_by_name(net, name.trim())?;
    add_bytes(host, &skin.bytes, &skin.name, skin.slim, "player")
}

/// [`add_from_player_with`] against the real services.
///
/// # Errors
///
/// As [`add_from_player_with`].
pub fn add_from_player(host: &dyn Host, name: &str) -> Result<AddResult, String> {
    add_from_player_with(&UreqSkinNet, host, name)
}

// --------------------------------------------------------------------------------------
// Changing and removing
// --------------------------------------------------------------------------------------

fn change(host: &dyn Host, id: &str, apply: impl FnOnce(&mut SkinEntry)) -> Result<SkinEntry, String> {
    if !is_skin_id(id) {
        return Err("not a skin".to_owned());
    }
    let folder = folder(host)?;
    let _guard = INDEX_LOCK.lock().map_err(|_| "the skin library is busy".to_owned())?;
    let mut index = load_index(&folder);
    let entry = index
        .skins
        .iter_mut()
        .find(|skin| skin.id == id)
        .ok_or("no such skin")?;
    apply(entry);
    let changed = entry.clone();
    save_index(&folder, &index)?;
    Ok(changed)
}

/// Gives a skin another name.
///
/// # Errors
///
/// For a skin that is not in the library, or an empty name.
pub fn rename(host: &dyn Host, id: &str, name: &str) -> Result<SkinEntry, String> {
    let name = clean_text(name, MAX_NAME_CHARS);
    if name.is_empty() {
        return Err("a skin needs a name".to_owned());
    }
    change(host, id, |entry| entry.name = name)
}

/// Says which arms the skin is made for (slim or classic).
///
/// # Errors
///
/// For a skin that is not in the library.
pub fn set_model(host: &dyn Host, id: &str, slim: bool) -> Result<SkinEntry, String> {
    change(host, id, |entry| entry.slim = slim)
}

/// Takes a skin out of the library and deletes its picture.
///
/// # Errors
///
/// For a skin that is not in the library.
pub fn remove(host: &dyn Host, id: &str) -> Result<(), String> {
    if !is_skin_id(id) {
        return Err("not a skin".to_owned());
    }
    let folder = folder(host)?;
    let _guard = INDEX_LOCK.lock().map_err(|_| "the skin library is busy".to_owned())?;
    let mut index = load_index(&folder);
    let before = index.skins.len();
    index.skins.retain(|skin| skin.id != id);
    if index.skins.len() == before {
        return Err("no such skin".to_owned());
    }
    save_index(&folder, &index)?;
    // the index no longer lists it; a picture that cannot be deleted is only wasted space
    let _ = fs::remove_file(picture_path(&folder, id));
    Ok(())
}

#[cfg(test)]
#[path = "skin_library_tests.rs"]
mod tests;
