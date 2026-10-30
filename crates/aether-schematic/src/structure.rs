//! The vanilla structure file (`.nbt`), as written by structure blocks and used in
//! `data/<namespace>/structure/`: a size, a palette and a list of positioned blocks.
//!
//! Positions the file does not list are air. Only the first palette is read when a file has
//! several (the variants shipwrecks use).

use crate::model::{
    BlockState, MAX_BLOCK_ENTITIES, MAX_PALETTE, Schematic, SchematicError, checked_volume,
};
use crate::nbt::{Tag, compound};

fn int_list(tag: Option<&Tag>) -> Option<[i32; 3]> {
    let items = tag?.as_list()?;
    Some([
        items.first()?.as_i32()?,
        items.get(1)?.as_i32()?,
        items.get(2)?.as_i32()?,
    ])
}

fn read_state(entry: &Tag) -> Result<BlockState, SchematicError> {
    let name = entry
        .get("Name")
        .and_then(Tag::as_str)
        .ok_or(SchematicError::Corrupt("a palette entry has no name"))?;
    let mut state = BlockState::new(name);
    if let Some(properties) = entry.get("Properties").and_then(Tag::as_compound) {
        for (key, value) in properties {
            if let Some(value) = value.as_str() {
                state.properties.push((key.clone(), value.to_owned()));
            }
        }
    }
    Ok(state)
}

/// Reads a parsed structure file.
///
/// # Errors
///
/// [`SchematicError::Unsupported`], [`SchematicError::Corrupt`] or [`SchematicError::TooLarge`].
pub fn read(root: &Tag) -> Result<Schematic, SchematicError> {
    let size = int_list(root.get("size"))
        .ok_or_else(|| SchematicError::Unsupported("not a structure file".to_owned()))?;
    let size = [
        u32::try_from(size[0]).map_err(|_| SchematicError::Corrupt("a negative size"))?,
        u32::try_from(size[1]).map_err(|_| SchematicError::Corrupt("a negative size"))?,
        u32::try_from(size[2]).map_err(|_| SchematicError::Corrupt("a negative size"))?,
    ];
    checked_volume(size)?;
    let palette_tag = root
        .get("palette")
        .and_then(Tag::as_list)
        .or_else(|| {
            root.get("palettes")
                .and_then(Tag::as_list)
                .and_then(|variants| variants.first())
                .and_then(Tag::as_list)
        })
        .ok_or(SchematicError::Corrupt("the structure has no palette"))?;
    if palette_tag.is_empty() || palette_tag.len() > MAX_PALETTE {
        return Err(SchematicError::Corrupt("the palette has a bad size"));
    }

    let mut schematic = Schematic::empty("", size)?;
    schematic.data_version = root.get("DataVersion").and_then(Tag::as_i32);
    let mapped = palette_tag
        .iter()
        .map(|entry| schematic.intern(read_state(entry)?))
        .collect::<Result<Vec<_>, _>>()?;

    let blocks = root
        .get("blocks")
        .and_then(Tag::as_list)
        .ok_or(SchematicError::Corrupt("the structure has no blocks"))?;
    for block in blocks {
        let position =
            int_list(block.get("pos")).ok_or(SchematicError::Corrupt("a block has no position"))?;
        let state = block
            .get("state")
            .and_then(Tag::as_i32)
            .and_then(|state| usize::try_from(state).ok())
            .and_then(|state| mapped.get(state))
            .ok_or(SchematicError::Corrupt(
                "a block refers to a state outside the palette",
            ))?;
        let [Ok(x), Ok(y), Ok(z)] = position.map(u32::try_from) else {
            return Err(SchematicError::Corrupt("a block is outside the structure"));
        };
        let index = schematic
            .index(x, y, z)
            .ok_or(SchematicError::Corrupt("a block is outside the structure"))?;
        schematic.blocks[index] = *state;
        if let Some(Tag::Compound(entries)) = block.get("nbt")
            && schematic.block_entities.len() < MAX_BLOCK_ENTITIES
        {
            schematic.block_entities.insert(
                u32::try_from(index).unwrap_or(0),
                Tag::Compound(entries.clone()),
            );
        }
    }
    schematic.validate()?;
    Ok(schematic)
}

/// Builds the root tag of a structure file. Air blocks are left out, as the game does.
///
/// # Errors
///
/// Anything [`Schematic::validate`] reports.
pub fn to_tag(schematic: &Schematic) -> Result<Tag, SchematicError> {
    schematic.validate()?;
    let schematic = schematic.compacted();
    let palette = schematic
        .palette
        .iter()
        .map(|state| {
            let mut entries = vec![("Name".to_owned(), Tag::String(state.name.clone()))];
            if !state.properties.is_empty() {
                entries.push((
                    "Properties".to_owned(),
                    Tag::Compound(
                        state
                            .properties
                            .iter()
                            .map(|(key, value)| (key.clone(), Tag::String(value.clone())))
                            .collect(),
                    ),
                ));
            }
            Tag::Compound(entries)
        })
        .collect();
    let blocks = schematic
        .blocks
        .iter()
        .enumerate()
        .filter(|&(_, &state)| !schematic.palette[state as usize].is_air())
        .map(|(index, &state)| {
            let index = u32::try_from(index).unwrap_or(0);
            let [x, y, z] = schematic.position(index);
            let mut entries = vec![
                (
                    "pos".to_owned(),
                    Tag::List(vec![
                        Tag::Int(i32::try_from(x).unwrap_or(0)),
                        Tag::Int(i32::try_from(y).unwrap_or(0)),
                        Tag::Int(i32::try_from(z).unwrap_or(0)),
                    ]),
                ),
                (
                    "state".to_owned(),
                    Tag::Int(i32::try_from(state).unwrap_or(0)),
                ),
            ];
            if let Some(data) = schematic.block_entities.get(&index) {
                entries.push(("nbt".to_owned(), data.clone()));
            }
            Tag::Compound(entries)
        })
        .collect();
    let size = schematic
        .size
        .iter()
        .map(|&side| Tag::Int(i32::try_from(side).unwrap_or(0)))
        .collect();
    Ok(compound([
        ("DataVersion", Tag::Int(schematic.data_version.unwrap_or(0))),
        ("size", Tag::List(size)),
        ("palette", Tag::List(palette)),
        ("blocks", Tag::List(blocks)),
        ("entities", Tag::List(Vec::new())),
    ]))
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
