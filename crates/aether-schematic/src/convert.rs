//! Recognising a schematic file, and converting between formats and Minecraft data versions.
//!
//! Format conversion is exact: the same blocks, block states and block entities are written
//! in another container. Moving to another **data version** is not a full upgrade. The game's
//! own data fixers are not available here, so only changes that are certain are applied: a
//! few block renames and the 1.20.5 change to how containers store items. Everything else is
//! kept as it is and reported in [`Conversion::warnings`], so the user knows what to check.

use std::collections::HashMap;

use crate::model::{BlockState, Schematic, SchematicError};
use crate::nbt::{self, Tag};
use crate::{litematic, sponge, structure};

/// A schematic file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Litematica, `.litematic`.
    Litematic,
    /// Sponge, `.schem` (WorldEdit, FAWE).
    Sponge,
    /// The vanilla structure file, `.nbt`.
    Structure,
}

impl Format {
    pub const ALL: [Self; 3] = [Self::Litematic, Self::Sponge, Self::Structure];

    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Litematic => "litematic",
            Self::Sponge => "schem",
            Self::Structure => "nbt",
        }
    }

    /// The stable label used across IPC, the same as the extension.
    #[must_use]
    pub fn label(self) -> &'static str {
        self.extension()
    }

    #[must_use]
    pub fn parse(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.label() == label)
    }

    /// The format a file name suggests (`.schematic` is the legacy MCEdit format and is not
    /// supported, so it is not mapped here).
    #[must_use]
    pub fn from_file_name(name: &str) -> Option<Self> {
        let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
        Self::parse(&extension)
    }
}

/// Parses a file and recognises its format from what is inside, not from its name.
///
/// # Errors
///
/// [`SchematicError::Unsupported`] for a file that is NBT but not one of the three formats,
/// plus everything the format readers report.
pub fn read_any(bytes: &[u8]) -> Result<(Format, Schematic), SchematicError> {
    let (_, root) = nbt::read_maybe_gzip(bytes)?;
    let format = if root.get("Regions").is_some() {
        Format::Litematic
    } else if root.get("Schematic").is_some()
        || root.get("BlockData").is_some()
        || root
            .get("Palette")
            .is_some_and(|p| p.as_compound().is_some())
    {
        Format::Sponge
    } else if root.get("blocks").is_some() && root.get("size").is_some() {
        Format::Structure
    } else if root.get("Blocks").is_some() && root.get("Width").is_some() {
        return Err(SchematicError::Unsupported(
            "this is a legacy .schematic (MCEdit) file; convert it with WorldEdit first".to_owned(),
        ));
    } else {
        return Err(SchematicError::Unsupported(
            "the file is not a Litematica, Sponge or structure file".to_owned(),
        ));
    };
    let schematic = match format {
        Format::Litematic => litematic::read(&root)?,
        Format::Sponge => sponge::read(&root)?,
        Format::Structure => structure::read(&root)?,
    };
    Ok((format, schematic))
}

/// Serialises a schematic in a format (gzip-compressed, as all three are stored).
///
/// # Errors
///
/// Anything [`Schematic::validate`] reports.
pub fn write_as(
    schematic: &Schematic,
    format: Format,
    now_ms: i64,
) -> Result<Vec<u8>, SchematicError> {
    let tag = match format {
        Format::Litematic => litematic::to_tag(schematic, now_ms)?,
        Format::Sponge => sponge::to_tag(schematic)?,
        Format::Structure => structure::to_tag(schematic)?,
    };
    // A version 3 Sponge file names its root "Schematic"; the others have an empty name.
    Ok(nbt::write_gzip("", &tag))
}

/// Minecraft releases the interface offers as conversion targets: `(data version, name)`.
/// Any other number is accepted too; this is only the list people usually want.
pub const KNOWN_DATA_VERSIONS: &[(i32, &str)] = &[
    (2586, "1.16.5"),
    (2730, "1.17.1"),
    (2975, "1.18.2"),
    (3120, "1.19.2"),
    (3337, "1.19.4"),
    (3465, "1.20.1"),
    (3700, "1.20.4"),
    (3839, "1.20.6"),
    (3955, "1.21.1"),
    (4189, "1.21.4"),
];

/// A block renamed in a release: `(data version of that release, old id, new id)`.
///
/// Deliberately short. Only renames that are certain belong here; the 1.13 "flattening" and
/// friends need the game's full data tables and are not attempted.
const BLOCK_RENAMES: &[(i32, &str, &str)] = &[(3698, "minecraft:grass", "minecraft:short_grass")];

/// The data version from which containers store `count` and `components` instead of `Count`
/// and `tag` (Minecraft 1.20.5).
const ITEM_COMPONENTS_VERSION: i32 = 3837;
/// The data version from which signs keep their text in `front_text` / `back_text`.
const SIGN_FORMAT_VERSION: i32 = 3465;

/// The result of a conversion.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversion {
    pub bytes: Vec<u8>,
    pub format: Format,
    pub data_version: Option<i32>,
    /// How many blocks were given a new id.
    pub renamed_blocks: u64,
    /// Things the user should check, plain sentences.
    pub warnings: Vec<String>,
}

/// Converts a schematic file to another format and, optionally, another data version.
///
/// # Errors
///
/// See [`read_any`] and [`write_as`].
pub fn convert(
    bytes: &[u8],
    to: Format,
    target_data_version: Option<i32>,
    now_ms: i64,
) -> Result<Conversion, SchematicError> {
    let (from_format, mut schematic) = read_any(bytes)?;
    let mut warnings = Vec::new();
    let mut renamed_blocks = 0;

    if let Some(target) = target_data_version {
        match schematic.data_version.filter(|&version| version > 0) {
            Some(source) if source != target => {
                renamed_blocks = retarget(&mut schematic, source, target, &mut warnings);
            }
            Some(_) => {}
            None => warnings.push(
                "The file does not say which Minecraft version it was made for, so nothing was changed besides the version number."
                    .to_owned(),
            ),
        }
        schematic.data_version = Some(target);
        warnings.push(
            "Blocks are not checked against the target version's block list; a block that does not exist there will not load."
                .to_owned(),
        );
    }
    if from_format == Format::Litematic && to != Format::Litematic {
        warnings.push(
            "Litematica's own settings (such as placement and the region list) are not kept."
                .to_owned(),
        );
    }
    if to == Format::Structure {
        warnings.push(
            "A structure file cannot hold more than 48×48×48 blocks in a structure block."
                .to_owned(),
        );
    }
    let data_version = schematic.data_version;
    Ok(Conversion {
        bytes: write_as(&schematic, to, now_ms)?,
        format: to,
        data_version,
        renamed_blocks,
        warnings,
    })
}

/// The id a block has in the target version, if it differs.
fn renamed(name: &str, from: i32, to: i32) -> Option<&'static str> {
    for &(since, old, new) in BLOCK_RENAMES {
        if from < since && since <= to && name == old {
            return Some(new);
        }
        if to < since && since <= from && name == new {
            return Some(old);
        }
    }
    None
}

fn retarget(schematic: &mut Schematic, from: i32, to: i32, warnings: &mut Vec<String>) -> u64 {
    // Rename palette entries, and count the blocks that use each.
    let mut uses: HashMap<u32, u64> = HashMap::new();
    for &index in &schematic.blocks {
        *uses.entry(index).or_default() += 1;
    }
    let mut renamed_blocks = 0;
    for (index, state) in schematic.palette.iter_mut().enumerate() {
        if let Some(new_name) = renamed(&state.name, from, to) {
            renamed_blocks += uses
                .get(&u32::try_from(index).unwrap_or(0))
                .copied()
                .unwrap_or(0);
            *state = BlockState {
                name: new_name.to_owned(),
                properties: state.properties.clone(),
            };
        }
    }

    let upgrading = from < ITEM_COMPONENTS_VERSION && ITEM_COMPONENTS_VERSION <= to;
    let downgrading = to < ITEM_COMPONENTS_VERSION && ITEM_COMPONENTS_VERSION <= from;
    let mut dropped_item_data = false;
    let mut nested_containers = false;
    if upgrading || downgrading {
        for entity in schematic.block_entities.values_mut() {
            let Tag::Compound(entries) = entity else {
                continue;
            };
            for (key, value) in entries.iter_mut() {
                if key == "Items"
                    && let Tag::List(items) = value
                {
                    for item in items.iter_mut() {
                        if convert_item(item, upgrading, &mut dropped_item_data) {
                            nested_containers = true;
                        }
                    }
                }
            }
        }
    }
    if dropped_item_data {
        warnings.push(
            "Item data that changed format between these versions (names, enchantments, custom data) was left out of chests and other containers."
                .to_owned(),
        );
    }
    if nested_containers {
        warnings.push(
            "Containers inside containers (a shulker box in a chest) keep their old item format."
                .to_owned(),
        );
    }

    let has_sign = schematic.block_entities.values().any(|entity| {
        entity
            .get("id")
            .and_then(Tag::as_str)
            .is_some_and(|id| id.contains("sign"))
    });
    if has_sign && ((from < SIGN_FORMAT_VERSION) != (to < SIGN_FORMAT_VERSION)) {
        warnings.push(
            "Signs changed how they store text in 1.20; their text may need to be retyped."
                .to_owned(),
        );
    }
    renamed_blocks
}

/// Converts one item in a container. Returns `true` when the item holds a container of its
/// own (which is not converted).
fn convert_item(item: &mut Tag, upgrading: bool, dropped: &mut bool) -> bool {
    let Tag::Compound(entries) = item else {
        return false;
    };
    let mut nested = false;
    let mut converted = Vec::with_capacity(entries.len());
    for (key, value) in entries.drain(..) {
        match (upgrading, key.as_str()) {
            (true, "Count") => {
                converted.push(("count".to_owned(), Tag::Int(value.as_i32().unwrap_or(1))))
            }
            (true, "tag") => {
                *dropped = true;
                if value.get("BlockEntityTag").is_some() {
                    nested = true;
                }
            }
            (false, "count") => {
                let count = value.as_i32().unwrap_or(1).clamp(0, 127);
                converted.push((
                    "Count".to_owned(),
                    Tag::Byte(i8::try_from(count).unwrap_or(1)),
                ));
            }
            (false, "components") => *dropped = true,
            _ => converted.push((key, value)),
        }
    }
    *entries = converted;
    nested
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
