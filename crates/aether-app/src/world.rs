//! One world of an instance: what its `level.dat` says, which data packs it has, deleting a
//! data pack, and opening its folder.
//!
//! A caller names an instance by id and a world and a data pack by plain file names; it never
//! supplies a path. A world is whatever another program or a download left in `saves/`, so
//! `level.dat`, `pack.mcmeta` and the packs themselves are untrusted: everything is read with
//! size, depth and entry limits, nothing is unpacked to disk, and nothing in them is run.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use aether_content::{image_mime, image_size};
use aether_core::GameInstance;
use aether_schematic::nbt::{self, Tag};
use aether_storage::manage as store;
use aether_storage::manage::{game_directory, is_plain_name};
use serde::Serialize;

use crate::cachefile::read_capped;
use crate::host::Host;
use crate::instances::load;
use crate::platform;

/// The largest `level.dat` read (compressed). Real ones are a few KiB.
const MAX_LEVEL_DAT_BYTES: u64 = 4 * 1024 * 1024;
/// The largest `icon.png` shown; a world icon is 64×64.
const MAX_ICON_BYTES: u64 = 64 * 1024;
const MAX_ICON_SIDE: u32 = 512;
/// The largest `pack.mcmeta` read, in a folder or in a zip.
const MAX_MCMETA_BYTES: u64 = 64 * 1024;
/// More data packs than this are not listed.
const MAX_DATAPACKS: usize = 500;
/// A zip with more entries than this is not opened (a data pack has a few thousand at most).
const MAX_ZIP_ENTRIES: usize = 100_000;
const MAX_DESCRIPTION_CHARS: usize = 300;
const MAX_COMPONENT_DEPTH: usize = 8;

/// Whether a data pack is switched on in the world, from `level.dat`.
#[derive(Debug, Serialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum PackState {
    Enabled,
    Disabled,
    /// `level.dat` does not list the pack (never loaded yet) or could not be read.
    Unknown,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DatapackInfo {
    pub name: String,
    pub is_folder: bool,
    pub size_bytes: u64,
    pub description: Option<String>,
    pub pack_format: Option<i64>,
    pub state: PackState,
}

/// What the world page shows. No path of any kind.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorldDetails {
    /// The folder name.
    pub name: String,
    /// The name inside `level.dat`, which can differ from the folder.
    pub level_name: Option<String>,
    pub game_version: Option<String>,
    pub data_version: Option<i64>,
    /// `survival`, `creative`, `adventure` or `spectator`.
    pub game_mode: Option<&'static str>,
    pub hardcore: Option<bool>,
    /// `peaceful`, `easy`, `normal` or `hard`.
    pub difficulty: Option<&'static str>,
    /// Seconds since the Unix epoch.
    pub last_played: Option<u64>,
    pub size_bytes: u64,
    /// `data:image/png;base64,…`, when the world has a small real PNG icon.
    pub icon: Option<String>,
    pub datapacks: Vec<DatapackInfo>,
    pub datapacks_truncated: bool,
    /// Why `level.dat` could not be read, when it could not.
    pub unreadable: Option<String>,
}

fn refuse_while_running(host: &dyn Host, instance: &GameInstance, what: &str) -> Result<(), String> {
    if host.is_instance_running(instance.id)? {
        return Err(format!("stop the game before {what}"));
    }
    Ok(())
}

/// The folder of a world: a plain name that is a real folder (not a link) in `saves/` and has a
/// `level.dat`.
fn world_folder(game: &Path, world: &str) -> Result<PathBuf, String> {
    if !is_plain_name(world) {
        return Err("not a world name".to_owned());
    }
    let path = game.join("saves").join(world);
    let metadata = fs::symlink_metadata(&path).map_err(|_| "no such world".to_owned())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("no such world".to_owned());
    }
    Ok(path)
}

// --------------------------------------------------------------------------------------
// level.dat
// --------------------------------------------------------------------------------------

#[derive(Default)]
struct Level {
    level_name: Option<String>,
    game_version: Option<String>,
    data_version: Option<i64>,
    game_mode: Option<&'static str>,
    hardcore: Option<bool>,
    difficulty: Option<&'static str>,
    last_played: Option<u64>,
    enabled: Vec<String>,
    disabled: Vec<String>,
    has_pack_lists: bool,
}

fn strings(tag: Option<&Tag>) -> Option<Vec<String>> {
    let list = tag?.as_list()?;
    Some(
        list.iter()
            .filter_map(|item| item.as_str().map(str::to_owned))
            .take(MAX_DATAPACKS * 2)
            .collect(),
    )
}

fn read_level(path: &Path) -> Result<Level, String> {
    let bytes =
        read_capped(path, MAX_LEVEL_DAT_BYTES).ok_or_else(|| "level.dat is missing or too large".to_owned())?;
    let (_, root) = nbt::read_maybe_gzip(&bytes).map_err(|error| format!("level.dat: {error}"))?;
    let data = root.get("Data").ok_or_else(|| "level.dat has no world data".to_owned())?;

    let mut level = Level {
        level_name: data
            .get("LevelName")
            .and_then(Tag::as_str)
            .map(|name| aether_content::clean_text(name, 100)),
        game_version: data
            .get("Version")
            .and_then(|version| version.get("Name"))
            .and_then(Tag::as_str)
            .map(|name| aether_content::clean_text(name, 40)),
        data_version: data.get("DataVersion").and_then(Tag::as_i64),
        game_mode: data.get("GameType").and_then(Tag::as_i64).and_then(|mode| match mode {
            0 => Some("survival"),
            1 => Some("creative"),
            2 => Some("adventure"),
            3 => Some("spectator"),
            _ => None,
        }),
        hardcore: data.get("hardcore").and_then(Tag::as_i64).map(|flag| flag != 0),
        difficulty: data.get("Difficulty").and_then(Tag::as_i64).and_then(|value| match value {
            0 => Some("peaceful"),
            1 => Some("easy"),
            2 => Some("normal"),
            3 => Some("hard"),
            _ => None,
        }),
        last_played: data
            .get("LastPlayed")
            .and_then(Tag::as_i64)
            .and_then(|millis| u64::try_from(millis / 1000).ok()),
        ..Level::default()
    };
    if let Some(packs) = data.get("DataPacks") {
        level.enabled = strings(packs.get("Enabled")).unwrap_or_default();
        level.disabled = strings(packs.get("Disabled")).unwrap_or_default();
        level.has_pack_lists = packs.get("Enabled").is_some() || packs.get("Disabled").is_some();
    }
    Ok(level)
}

fn state_of(level: Option<&Level>, name: &str) -> PackState {
    let Some(level) = level.filter(|level| level.has_pack_lists) else {
        return PackState::Unknown;
    };
    let id = format!("file/{name}");
    if level.enabled.contains(&id) {
        PackState::Enabled
    } else if level.disabled.contains(&id) {
        PackState::Disabled
    } else {
        PackState::Unknown
    }
}

// --------------------------------------------------------------------------------------
// pack.mcmeta
// --------------------------------------------------------------------------------------

/// The plain text of a description, which is a string or a text component (an object with
/// `text` and `extra`, or an array of those).
fn component_text(value: &serde_json::Value, depth: usize, out: &mut String) {
    if depth > MAX_COMPONENT_DEPTH || out.chars().count() > MAX_DESCRIPTION_CHARS {
        return;
    }
    match value {
        serde_json::Value::String(text) => out.push_str(text),
        serde_json::Value::Array(items) => {
            for item in items.iter().take(32) {
                component_text(item, depth + 1, out);
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(text)) = map.get("text") {
                out.push_str(text);
            }
            if let Some(extra) = map.get("extra") {
                component_text(extra, depth + 1, out);
            }
        }
        _ => {}
    }
}

fn parse_mcmeta(bytes: &[u8]) -> (Option<String>, Option<i64>) {
    let Ok(root) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return (None, None);
    };
    let Some(pack) = root.get("pack") else {
        return (None, None);
    };
    let mut description = String::new();
    if let Some(value) = pack.get("description") {
        component_text(value, 0, &mut description);
    }
    let description = aether_content::clean_text(&description, MAX_DESCRIPTION_CHARS);
    (
        (!description.is_empty()).then_some(description),
        pack.get("pack_format").and_then(serde_json::Value::as_i64),
    )
}

/// `pack.mcmeta` of a pack that is a folder or a zip. Only that one entry is read.
fn read_mcmeta(path: &Path, is_folder: bool) -> Option<Vec<u8>> {
    if is_folder {
        return read_capped(&path.join("pack.mcmeta"), MAX_MCMETA_BYTES);
    }
    let file = fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return None;
    }
    let entry = archive.by_name("pack.mcmeta").ok()?;
    if entry.size() > MAX_MCMETA_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    entry.take(MAX_MCMETA_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_MCMETA_BYTES).then_some(bytes)
}

fn list_datapacks(world: &Path, level: Option<&Level>) -> (Vec<DatapackInfo>, bool) {
    let Ok(reader) = fs::read_dir(world.join("datapacks")) else {
        return (Vec::new(), false);
    };
    let mut packs = Vec::new();
    let mut truncated = false;
    for item in reader.flatten() {
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Ok(metadata) = fs::symlink_metadata(item.path()) else {
            continue;
        };
        if !is_plain_name(&name) || metadata.file_type().is_symlink() {
            continue;
        }
        let is_folder = metadata.is_dir();
        let is_zip = Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"));
        if !is_folder && !is_zip {
            continue;
        }
        if packs.len() >= MAX_DATAPACKS {
            truncated = true;
            break;
        }
        let (description, pack_format) = read_mcmeta(&item.path(), is_folder)
            .map(|bytes| parse_mcmeta(&bytes))
            .unwrap_or_default();
        packs.push(DatapackInfo {
            size_bytes: if is_folder {
                store::directory_size(&item.path())
            } else {
                metadata.len()
            },
            state: state_of(level, &name),
            description,
            pack_format,
            is_folder,
            name,
        });
    }
    packs.sort_by_key(|pack| pack.name.to_lowercase());
    (packs, truncated)
}

fn world_icon(world: &Path) -> Option<String> {
    let bytes = read_capped(&world.join("icon.png"), MAX_ICON_BYTES)?;
    if image_mime(&bytes) != Some("image/png") {
        return None;
    }
    let (width, height) = image_size(&bytes)?;
    ((1..=MAX_ICON_SIDE).contains(&width) && (1..=MAX_ICON_SIDE).contains(&height))
        .then(|| format!("data:image/png;base64,{}", crate::schematics::base64(&bytes)))
}

// --------------------------------------------------------------------------------------
// The operations
// --------------------------------------------------------------------------------------

/// The details of one world. Measures the folder and opens each data pack's metadata, so call
/// it off the interface thread. A `level.dat` that cannot be read is not an error: the details
/// come back without it and say why in `unreadable`.
///
/// # Errors
///
/// For an unknown instance, or a world name that is not a world folder of the instance.
pub fn world_details(host: &dyn Host, id: &str, world: &str) -> Result<WorldDetails, String> {
    let (_, instance) = load(host, id)?;
    let folder = world_folder(&game_directory(&instance), world)?;
    let (level, unreadable) = match read_level(&folder.join("level.dat")) {
        Ok(level) => (Some(level), None),
        Err(reason) => (None, Some(reason)),
    };
    let (datapacks, datapacks_truncated) = list_datapacks(&folder, level.as_ref());
    let level = level.unwrap_or_default();
    Ok(WorldDetails {
        name: world.to_owned(),
        level_name: level.level_name,
        game_version: level.game_version,
        data_version: level.data_version,
        game_mode: level.game_mode,
        hardcore: level.hardcore,
        difficulty: level.difficulty,
        last_played: level.last_played,
        size_bytes: store::directory_size(&folder),
        icon: world_icon(&folder),
        datapacks,
        datapacks_truncated,
        unreadable,
    })
}

/// Deletes one data pack of a world: a zip, or a folder with everything in it.
///
/// # Errors
///
/// While the instance's game runs, for an unknown world, or for a pack name that is not a plain
/// name of an existing pack (a link is refused).
pub fn remove_datapack(host: &dyn Host, id: &str, world: &str, name: &str) -> Result<(), String> {
    let (_, instance) = load(host, id)?;
    refuse_while_running(host, &instance, "removing a data pack")?;
    let folder = world_folder(&game_directory(&instance), world)?;
    if !is_plain_name(name) {
        return Err("not a data pack name".to_owned());
    }
    let path = folder.join("datapacks").join(name);
    let metadata = fs::symlink_metadata(&path).map_err(|_| "no such data pack".to_owned())?;
    if metadata.file_type().is_symlink() {
        return Err("that data pack is a link and is not removed".to_owned());
    }
    let result = if metadata.is_dir() {
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    };
    result.map_err(|_| "cannot remove the data pack".to_owned())
}

/// Opens the world's folder in the file manager. The folder must really be inside the
/// instance (a link planted at the world's name is refused).
///
/// # Errors
///
/// For an unknown world, a folder outside the instance, or a file manager that will not start.
pub fn open_world_folder(host: &dyn Host, id: &str, world: &str) -> Result<(), String> {
    let (_, instance) = load(host, id)?;
    let folder = world_folder(&game_directory(&instance), world)?;
    let inside = fs::canonicalize(&folder)
        .and_then(|folder| fs::canonicalize(&instance.root).map(|root| (folder, root)))
        .is_ok_and(|(folder, root)| folder.starts_with(root));
    if !inside {
        return Err("that folder is not inside the instance".to_owned());
    }
    platform::open_in_file_manager(&platform::plain(&folder))
}

#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;
