//! The Sponge schematic format (`.schem`), versions 1 to 3, as used by WorldEdit, FAWE and
//! Litematica's import. Block data is a byte array of variable-length integers indexing a
//! palette whose keys are block states written as `minecraft:oak_stairs[facing=north]`.
//!
//! Reading accepts versions 1 to 3. Writing produces version 2, which every tool that
//! reads `.schem` understands.

use std::collections::BTreeMap;

use crate::model::{
    BlockState, MAX_BLOCK_ENTITIES, MAX_PALETTE, Schematic, SchematicError, checked_volume,
};
use crate::nbt::{Tag, compound};

fn dimension(tag: Option<&Tag>, what: &'static str) -> Result<u32, SchematicError> {
    let value = tag
        .and_then(Tag::as_i64)
        .ok_or(SchematicError::Corrupt(what))?;
    // Sponge stores these as unsigned shorts, which read back negative past 32767.
    let value = if value < 0 { value + 65_536 } else { value };
    u32::try_from(value).map_err(|_| SchematicError::Corrupt(what))
}

fn varints(data: &[i8], count: usize) -> Result<Vec<u32>, SchematicError> {
    let mut out = Vec::with_capacity(count);
    let mut value = 0u64;
    let mut shift = 0u32;
    for &byte in data {
        let byte = byte as u8;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            out.push(
                u32::try_from(value)
                    .map_err(|_| SchematicError::Corrupt("a block index is too large"))?,
            );
            value = 0;
            shift = 0;
            if out.len() > count {
                break;
            }
        } else {
            shift += 7;
            if shift > 28 {
                return Err(SchematicError::Corrupt("a block index is too long"));
            }
        }
    }
    if shift != 0 || out.len() != count {
        return Err(SchematicError::Corrupt(
            "the block data does not match the size",
        ));
    }
    Ok(out)
}

fn put_varint(out: &mut Vec<i8>, mut value: u32) {
    loop {
        let low = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(low as i8);
            return;
        }
        out.push((low | 0x80) as i8);
    }
}

/// Reads a parsed `.schem` root (any of versions 1 to 3).
///
/// # Errors
///
/// [`SchematicError::Unsupported`], [`SchematicError::Corrupt`] or [`SchematicError::TooLarge`].
pub fn read(root: &Tag) -> Result<Schematic, SchematicError> {
    // Version 3 wraps everything in a `Schematic` compound.
    let body = root.get("Schematic").unwrap_or(root);
    let version = body
        .get("Version")
        .and_then(Tag::as_i32)
        .ok_or_else(|| SchematicError::Unsupported("not a Sponge schematic".to_owned()))?;
    if !(1..=3).contains(&version) {
        return Err(SchematicError::Unsupported(format!(
            "Sponge schematic version {version} (1 to 3 are supported)"
        )));
    }
    let size = [
        dimension(body.get("Width"), "the schematic has no width")?,
        dimension(body.get("Height"), "the schematic has no height")?,
        dimension(body.get("Length"), "the schematic has no length")?,
    ];
    let volume = usize::try_from(checked_volume(size)?)
        .map_err(|_| SchematicError::TooLarge("too many blocks".to_owned()))?;

    // Version 3 keeps the block fields in a `Blocks` compound.
    let blocks_tag = if version == 3 {
        body.get("Blocks")
    } else {
        Some(body)
    };
    let blocks_tag = blocks_tag.ok_or(SchematicError::Corrupt("the schematic has no blocks"))?;
    let palette_tag =
        blocks_tag
            .get("Palette")
            .and_then(Tag::as_compound)
            .ok_or(SchematicError::Corrupt(
                "the schematic has no block palette",
            ))?;
    if palette_tag.is_empty() || palette_tag.len() > MAX_PALETTE {
        return Err(SchematicError::Corrupt("the block palette has a bad size"));
    }
    let data_key = if version == 3 { "Data" } else { "BlockData" };
    let data = match blocks_tag.get(data_key) {
        Some(Tag::ByteArray(bytes)) => bytes,
        _ => return Err(SchematicError::Corrupt("the schematic has no block data")),
    };

    let mut states: Vec<Option<BlockState>> = vec![None; palette_tag.len()];
    for (text, id) in palette_tag {
        let id = id
            .as_i32()
            .and_then(|id| usize::try_from(id).ok())
            .filter(|&id| id < states.len())
            .ok_or(SchematicError::Corrupt(
                "the block palette numbers are not 0..n",
            ))?;
        states[id] = Some(BlockState::parse(text).ok_or(SchematicError::Corrupt(
            "a palette entry is not a block state",
        ))?);
    }

    let mut schematic = Schematic::empty("", size)?;
    let metadata = body.get("Metadata");
    let text = |key: &str| {
        metadata
            .and_then(|meta| meta.get(key))
            .and_then(Tag::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    schematic.name = text("Name");
    schematic.author = text("Author");
    schematic.data_version = body.get("DataVersion").and_then(Tag::as_i32);

    let mut mapped = Vec::with_capacity(states.len());
    for state in states {
        let state = state.ok_or(SchematicError::Corrupt(
            "the block palette numbers are not 0..n",
        ))?;
        mapped.push(schematic.intern(state)?);
    }
    let indices = varints(data, volume)?;
    for (slot, index) in schematic.blocks.iter_mut().zip(indices) {
        *slot = *mapped.get(index as usize).ok_or(SchematicError::Corrupt(
            "a block refers to a state outside the palette",
        ))?;
    }

    let entity_tags = blocks_tag
        .get("BlockEntities")
        .and_then(Tag::as_list)
        .unwrap_or_default();
    for entity in entity_tags.iter().take(MAX_BLOCK_ENTITIES) {
        let Some(Tag::IntArray(position)) = entity.get("Pos") else {
            continue;
        };
        let [Ok(x), Ok(y), Ok(z)] = [0, 1, 2].map(|i| {
            position
                .get(i)
                .map_or(Err(()), |v| u32::try_from(*v).map_err(|_| ()))
        }) else {
            continue;
        };
        let Some(index) = schematic.index(x, y, z) else {
            continue;
        };
        let Some(id) = entity.get("Id").and_then(Tag::as_str) else {
            continue;
        };
        // Version 3 nests the data under `Data`; earlier versions put it beside `Id`.
        let source: &[(String, Tag)] = if version == 3 {
            entity
                .get("Data")
                .and_then(Tag::as_compound)
                .unwrap_or_default()
        } else {
            entity.as_compound().unwrap_or_default()
        };
        let mut entries = vec![("id".to_owned(), Tag::String(id.to_owned()))];
        entries.extend(
            source
                .iter()
                .filter(|(key, _)| !matches!(key.as_str(), "Id" | "Pos" | "id"))
                .cloned(),
        );
        schematic
            .block_entities
            .insert(u32::try_from(index).unwrap_or(0), Tag::Compound(entries));
    }
    schematic.validate()?;
    Ok(schematic)
}

fn short(value: u32) -> Tag {
    // Written as an unsigned short, as the format says.
    Tag::Short(
        i16::try_from(value & 0xffff)
            .unwrap_or_else(|_| (value & 0xffff).wrapping_sub(65_536) as i16),
    )
}

/// Builds the root tag of a version 2 `.schem`.
///
/// # Errors
///
/// Anything [`Schematic::validate`] reports.
pub fn to_tag(schematic: &Schematic) -> Result<Tag, SchematicError> {
    schematic.validate()?;
    let schematic = schematic.compacted();

    let mut palette = Vec::with_capacity(schematic.palette.len());
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for (index, state) in schematic.palette.iter().enumerate() {
        let text = state.to_text();
        if seen.insert(text.clone(), ()).is_some() {
            return Err(SchematicError::Corrupt(
                "two palette entries are the same state",
            ));
        }
        palette.push((text, Tag::Int(i32::try_from(index).unwrap_or(0))));
    }
    let mut data = Vec::new();
    for &index in &schematic.blocks {
        put_varint(&mut data, index);
    }

    let entities = schematic
        .block_entities
        .iter()
        .filter_map(|(&index, data)| {
            let [x, y, z] = schematic.position(index);
            let Tag::Compound(entries) = data else {
                return None;
            };
            let id = entries
                .iter()
                .find(|(key, _)| key == "id")?
                .1
                .as_str()?
                .to_owned();
            let mut out = vec![
                ("Id".to_owned(), Tag::String(id)),
                (
                    "Pos".to_owned(),
                    Tag::IntArray(vec![
                        i32::try_from(x).unwrap_or(0),
                        i32::try_from(y).unwrap_or(0),
                        i32::try_from(z).unwrap_or(0),
                    ]),
                ),
            ];
            out.extend(entries.iter().filter(|(key, _)| key != "id").cloned());
            Some(Tag::Compound(out))
        })
        .collect();

    let mut root = vec![
        ("Version".to_owned(), Tag::Int(2)),
        (
            "DataVersion".to_owned(),
            Tag::Int(schematic.data_version.unwrap_or(0)),
        ),
        (
            "Metadata".to_owned(),
            compound([
                ("Name", Tag::String(schematic.name.clone())),
                ("Author", Tag::String(schematic.author.clone())),
            ]),
        ),
        ("Width".to_owned(), short(schematic.size[0])),
        ("Height".to_owned(), short(schematic.size[1])),
        ("Length".to_owned(), short(schematic.size[2])),
        ("Offset".to_owned(), Tag::IntArray(vec![0, 0, 0])),
        (
            "PaletteMax".to_owned(),
            Tag::Int(i32::try_from(palette.len()).unwrap_or(0)),
        ),
        ("Palette".to_owned(), Tag::Compound(palette)),
        ("BlockData".to_owned(), Tag::ByteArray(data)),
    ];
    if !schematic.block_entities.is_empty() {
        root.push(("BlockEntities".to_owned(), Tag::List(entities)));
    }
    Ok(Tag::Compound(root))
}

#[cfg(test)]
#[path = "sponge_tests.rs"]
mod tests;
