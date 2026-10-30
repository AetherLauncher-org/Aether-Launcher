//! Opening and converting the schematics of an instance.
//!
//! The frontend names an instance and a plain file name from the instance's `schematics/`
//! folder; the backend reads the file (`aether_storage::schematic_files`), parses it
//! (`aether_schematic`) and answers with a compact picture the 3D viewer can draw: the block
//! palette, the blocks run-length encoded, and the block entities with their data already
//! turned into text and item lists. File contents are untrusted: every size involved is
//! capped here and in the parser, and nothing in a schematic is ever executed.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use aether_schematic::nbt::{self, Tag};
use aether_schematic::{Format, KNOWN_DATA_VERSIONS, Schematic, convert_file, read_any};
use aether_storage::manage::game_directory;
use aether_storage::schematic_files::{read_schematic_file, safe_stem, write_new_schematic};
use serde::Serialize;

use super::manage::load_instance;

/// The most bytes of run-length data sent to the frontend. A build that needs more is too
/// busy to preview, and a request for it would be a request for an enormous message.
const MAX_RUN_BYTES: usize = 16 * 1024 * 1024;
/// The most block entities described. The rest are counted, not sent.
const MAX_ENTITIES_SHOWN: usize = 20_000;
const MAX_ITEMS_PER_ENTITY: usize = 64;
const MAX_ENTITY_TEXT_CHARS: usize = 6_000;
const MAX_ITEM_TEXT_CHARS: usize = 1_500;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Property {
    pub key: String,
    pub value: String,
}

/// One block state in the schematic's palette.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PaletteEntry {
    /// `minecraft:oak_stairs`
    pub name: String,
    pub properties: Vec<Property>,
    pub air: bool,
    /// How many blocks use this state.
    pub count: u64,
}

/// An item in a container.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub slot: Option<i32>,
    pub id: String,
    pub count: i32,
    /// The item's extra data (`components` or the older `tag`) as text, if it has any.
    pub data: Option<String>,
}

/// A chest, sign, furnace…: where it is, what it is, all its data as text, and the items it
/// holds.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlockEntityView {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub id: String,
    pub snbt: String,
    pub items: Vec<ItemView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicView {
    pub file_name: String,
    /// `litematic`, `schem` or `nbt`.
    pub format: &'static str,
    pub name: String,
    pub author: String,
    pub description: String,
    pub data_version: Option<i32>,
    pub size: [u32; 3],
    pub solid_blocks: u64,
    pub palette: Vec<PaletteEntry>,
    /// Base64 of pairs of little-endian `u32`: a palette index and how many blocks in a row
    /// have it, in index order `(y * sizeZ + z) * sizeX + x`.
    pub runs: String,
    pub run_count: u32,
    pub block_entities: Vec<BlockEntityView>,
    /// How many block entities there were in total, when more than are listed.
    pub block_entity_total: usize,
}

// --------------------------------------------------------------------------------------
// Building the view
// --------------------------------------------------------------------------------------

/// Run-length encodes block indices as little-endian `(value, count)` `u32` pairs.
pub(super) fn encode_runs(blocks: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut iter = blocks.iter().copied();
    let Some(mut value) = iter.next() else {
        return out;
    };
    let mut count = 1u32;
    for next in iter {
        if next == value && count < u32::MAX {
            count += 1;
        } else {
            out.extend_from_slice(&value.to_le_bytes());
            out.extend_from_slice(&count.to_le_bytes());
            value = next;
            count = 1;
        }
    }
    out.extend_from_slice(&value.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out
}

/// Standard base64 with padding.
pub(super) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let group = (u32::from(chunk[0]) << 16)
            | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for position in 0..4 {
            if position <= chunk.len() {
                out.push(char::from(
                    ALPHABET[((group >> (18 - 6 * position)) & 0x3f) as usize],
                ));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn item_count(item: &Tag) -> i32 {
    item.get("count")
        .or_else(|| item.get("Count"))
        .and_then(Tag::as_i32)
        .unwrap_or(1)
}

fn describe_items(entity: &Tag) -> Vec<ItemView> {
    let Some(items) = entity.get("Items").and_then(Tag::as_list) else {
        return Vec::new();
    };
    items
        .iter()
        .take(MAX_ITEMS_PER_ENTITY)
        .filter_map(|item| {
            let id = item.get("id").and_then(Tag::as_str)?.to_owned();
            let data = item
                .get("components")
                .or_else(|| item.get("tag"))
                .map(|data| nbt::to_snbt(data, true, MAX_ITEM_TEXT_CHARS));
            Some(ItemView {
                slot: item.get("Slot").and_then(Tag::as_i32),
                id,
                count: item_count(item),
                data,
            })
        })
        .collect()
}

pub(super) fn view_of(
    file_name: &str,
    format: Format,
    schematic: &Schematic,
) -> Result<SchematicView, String> {
    let mut counts = vec![0u64; schematic.palette.len()];
    for &index in &schematic.blocks {
        if let Some(slot) = counts.get_mut(index as usize) {
            *slot += 1;
        }
    }
    let palette = schematic
        .palette
        .iter()
        .zip(&counts)
        .map(|(state, &count)| PaletteEntry {
            name: state.name.clone(),
            properties: state
                .properties
                .iter()
                .map(|(key, value)| Property {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect(),
            air: state.is_air(),
            count,
        })
        .collect();

    let runs = encode_runs(&schematic.blocks);
    if runs.len() > MAX_RUN_BYTES {
        return Err("this schematic is too detailed to preview".to_owned());
    }

    let block_entities = schematic
        .block_entities
        .iter()
        .take(MAX_ENTITIES_SHOWN)
        .map(|(&index, data)| {
            let [x, y, z] = schematic.position(index);
            BlockEntityView {
                x,
                y,
                z,
                id: data
                    .get("id")
                    .and_then(Tag::as_str)
                    .unwrap_or("unknown")
                    .to_owned(),
                snbt: nbt::to_snbt(data, true, MAX_ENTITY_TEXT_CHARS),
                items: describe_items(data),
            }
        })
        .collect();

    Ok(SchematicView {
        file_name: file_name.to_owned(),
        format: format.label(),
        name: schematic.name.clone(),
        author: schematic.author.clone(),
        description: schematic.description.clone(),
        data_version: schematic.data_version.filter(|&version| version > 0),
        size: schematic.size,
        solid_blocks: schematic.solid_count(),
        palette,
        run_count: u32::try_from(runs.len() / 8).unwrap_or(u32::MAX),
        runs: base64(&runs),
        block_entities,
        block_entity_total: schematic.block_entities.len(),
    })
}

/// Reads and parses one schematic of an instance.
pub(super) fn open_schematic(game_dir: &Path, name: &str) -> Result<SchematicView, String> {
    let bytes = read_schematic_file(game_dir, name).map_err(|error| error.to_string())?;
    let (format, schematic) = read_any(&bytes).map_err(|error| error.to_string())?;
    view_of(name, format, &schematic)
}

// --------------------------------------------------------------------------------------
// Converting
// --------------------------------------------------------------------------------------

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConversionView {
    /// The new file, next to the original.
    pub file_name: String,
    pub format: &'static str,
    pub data_version: Option<i32>,
    pub renamed_blocks: u64,
    pub warnings: Vec<String>,
}

/// The lowest and highest data version accepted as a target.
const DATA_VERSION_RANGE: std::ops::RangeInclusive<i32> = 1..=1_000_000;

/// Converts a schematic of the instance and saves the result as a new file beside it. The
/// original is never touched.
pub(super) fn convert_in(
    game_dir: &Path,
    name: &str,
    format: &str,
    data_version: Option<i32>,
) -> Result<ConversionView, String> {
    let target = Format::parse(format).ok_or_else(|| "unknown schematic format".to_owned())?;
    if data_version.is_some_and(|version| !DATA_VERSION_RANGE.contains(&version)) {
        return Err("that Minecraft data version is out of range".to_owned());
    }
    let bytes = read_schematic_file(game_dir, name).map_err(|error| error.to_string())?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(0));
    let converted =
        convert_file(&bytes, target, data_version, now_ms).map_err(|error| error.to_string())?;

    let stem = Path::new(name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("schematic");
    let file_name = write_new_schematic(
        game_dir,
        &format!("{} (converted)", safe_stem(stem)),
        converted.format.extension(),
        &converted.bytes,
    )
    .map_err(|error| error.to_string())?;
    Ok(ConversionView {
        file_name,
        format: converted.format.label(),
        data_version: converted.data_version,
        renamed_blocks: converted.renamed_blocks,
        warnings: converted.warnings,
    })
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FormatOption {
    pub label: &'static str,
    pub name: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VersionOption {
    pub data_version: i32,
    pub name: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ConversionTargets {
    pub formats: Vec<FormatOption>,
    pub versions: Vec<VersionOption>,
}

fn format_name(format: Format) -> &'static str {
    match format {
        Format::Litematic => "Litematica (.litematic)",
        Format::Sponge => "Sponge / WorldEdit (.schem)",
        Format::Structure => "原版结构文件 (.nbt)",
    }
}

pub(super) fn targets() -> ConversionTargets {
    ConversionTargets {
        formats: Format::ALL
            .into_iter()
            .map(|format| FormatOption {
                label: format.label(),
                name: format_name(format),
            })
            .collect(),
        versions: KNOWN_DATA_VERSIONS
            .iter()
            .map(|&(data_version, name)| VersionOption { data_version, name })
            .collect(),
    }
}

// --------------------------------------------------------------------------------------
// Commands
// --------------------------------------------------------------------------------------

/// Opens a schematic for the 3D viewer.
#[tauri::command]
pub async fn read_instance_schematic(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<SchematicView, String> {
    let (_, instance) = load_instance(&app, &id)?;
    tauri::async_runtime::spawn_blocking(move || open_schematic(&game_directory(&instance), &name))
        .await
        .map_err(|_| "schematic task failed".to_owned())?
}

/// Converts a schematic to another format and, optionally, another Minecraft data version,
/// saving the result as a new file in the same folder.
#[tauri::command]
pub async fn convert_instance_schematic(
    app: tauri::AppHandle,
    id: String,
    name: String,
    format: String,
    data_version: Option<i32>,
) -> Result<ConversionView, String> {
    let (_, instance) = load_instance(&app, &id)?;
    tauri::async_runtime::spawn_blocking(move || {
        convert_in(&game_directory(&instance), &name, &format, data_version)
    })
    .await
    .map_err(|_| "conversion task failed".to_owned())?
}

/// The formats and Minecraft versions the conversion dialog offers.
#[tauri::command]
pub fn schematic_conversion_targets() -> ConversionTargets {
    targets()
}

#[cfg(test)]
#[path = "schematics_tests.rs"]
mod tests;
