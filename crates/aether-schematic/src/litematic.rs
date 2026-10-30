//! Litematica's `.litematic` format (version 5 to 7): gzip-compressed NBT with one or more
//! named regions, each a palette plus a bit-packed array of palette indices.
//!
//! Regions are merged into a single box on reading. A region's `Size` may be negative (the
//! region was selected "backwards"); its lowest corner is then `Position + Size + 1` on that
//! axis. Block entities are stored with region-local coordinates.

use crate::model::{
    BlockState, MAX_BLOCK_ENTITIES, MAX_PALETTE, Schematic, SchematicError, bits_for,
    checked_volume,
};
use crate::nbt::{Tag, compound};

const MAX_REGIONS: usize = 4096;
/// The highest format version written; the readers accept 5 to 7.
const WRITTEN_VERSION: i32 = 6;

fn triple(tag: Option<&Tag>) -> Option<[i32; 3]> {
    let tag = tag?;
    Some([
        tag.get("x")?.as_i32()?,
        tag.get("y")?.as_i32()?,
        tag.get("z")?.as_i32()?,
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

/// Reads value number `index` of a tightly packed array (entries may span two longs).
fn packed_value(longs: &[i64], index: u64, bits: u32) -> u64 {
    let start = index * u64::from(bits);
    let word = usize::try_from(start / 64).unwrap_or(usize::MAX);
    let offset = (start % 64) as u32;
    let mask = (1u64 << bits) - 1;
    let mut value = (longs[word].cast_unsigned()) >> offset;
    if offset + bits > 64 {
        value |= longs[word + 1].cast_unsigned() << (64 - offset);
    }
    value & mask
}

struct Region {
    min: [i64; 3],
    size: [u32; 3],
    palette: Vec<BlockState>,
    states: Vec<i64>,
    tile_entities: Vec<Tag>,
}

fn read_region(tag: &Tag) -> Result<Region, SchematicError> {
    let position =
        triple(tag.get("Position")).ok_or(SchematicError::Corrupt("a region has no position"))?;
    let size = triple(tag.get("Size")).ok_or(SchematicError::Corrupt("a region has no size"))?;
    if size.contains(&0) {
        return Err(SchematicError::Corrupt("a region is empty"));
    }
    let mut min = [0i64; 3];
    let mut abs = [0u32; 3];
    for axis in 0..3 {
        let side = i64::from(size[axis]);
        min[axis] = i64::from(position[axis]) + if side < 0 { side + 1 } else { 0 };
        abs[axis] = u32::try_from(side.abs())
            .map_err(|_| SchematicError::Corrupt("a region is too large"))?;
    }
    checked_volume(abs)?;

    let palette_tags = tag
        .get("BlockStatePalette")
        .and_then(Tag::as_list)
        .ok_or(SchematicError::Corrupt("a region has no block palette"))?;
    if palette_tags.is_empty() || palette_tags.len() > MAX_PALETTE {
        return Err(SchematicError::Corrupt(
            "a region's block palette has a bad size",
        ));
    }
    let palette = palette_tags
        .iter()
        .map(read_state)
        .collect::<Result<Vec<_>, _>>()?;
    let states = tag
        .get("BlockStates")
        .and_then(Tag::as_long_array)
        .ok_or(SchematicError::Corrupt("a region has no block data"))?
        .to_vec();
    let volume = u64::from(abs[0]) * u64::from(abs[1]) * u64::from(abs[2]);
    let needed = (volume * u64::from(bits_for(palette.len()))).div_ceil(64);
    if (states.len() as u64) < needed {
        return Err(SchematicError::Corrupt(
            "a region's block data is too short",
        ));
    }
    let tile_entities = tag
        .get("TileEntities")
        .and_then(Tag::as_list)
        .map(<[Tag]>::to_vec)
        .unwrap_or_default();
    Ok(Region {
        min,
        size: abs,
        palette,
        states,
        tile_entities,
    })
}

/// Reads a parsed `.litematic` root into a [`Schematic`].
///
/// # Errors
///
/// [`SchematicError::Unsupported`] for another format version; [`SchematicError::Corrupt`] and
/// [`SchematicError::TooLarge`] for a file that does not hold together or is too big.
pub fn read(root: &Tag) -> Result<Schematic, SchematicError> {
    let version = root
        .get("Version")
        .and_then(Tag::as_i32)
        .ok_or_else(|| SchematicError::Unsupported("not a Litematica file".to_owned()))?;
    if !(5..=7).contains(&version) {
        return Err(SchematicError::Unsupported(format!(
            "Litematica format version {version} (5 to 7 are supported)"
        )));
    }
    let region_tags = root
        .get("Regions")
        .and_then(Tag::as_compound)
        .ok_or(SchematicError::Corrupt("the file has no regions"))?;
    if region_tags.is_empty() || region_tags.len() > MAX_REGIONS {
        return Err(SchematicError::Corrupt(
            "the file has a bad number of regions",
        ));
    }
    let regions = region_tags
        .iter()
        .map(|(_, tag)| read_region(tag))
        .collect::<Result<Vec<_>, _>>()?;

    // The box that holds every region.
    let mut low = [i64::MAX; 3];
    let mut high = [i64::MIN; 3];
    for region in &regions {
        for axis in 0..3 {
            low[axis] = low[axis].min(region.min[axis]);
            high[axis] = high[axis].max(region.min[axis] + i64::from(region.size[axis]));
        }
    }
    let mut size = [0u32; 3];
    for axis in 0..3 {
        size[axis] = u32::try_from(high[axis] - low[axis])
            .map_err(|_| SchematicError::TooLarge("the regions are far apart".to_owned()))?;
    }
    let meta = root.get("Metadata");
    let text = |key: &str| {
        meta.and_then(|meta| meta.get(key))
            .and_then(Tag::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let mut schematic = Schematic::empty(&text("Name"), size)?;
    schematic.author = text("Author");
    schematic.description = text("Description");
    schematic.data_version = root.get("MinecraftDataVersion").and_then(Tag::as_i32);

    for region in &regions {
        let bits = bits_for(region.palette.len());
        let mapped = region
            .palette
            .iter()
            .map(|state| schematic.intern(state.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let offset = [
            u32::try_from(region.min[0] - low[0]).unwrap_or(0),
            u32::try_from(region.min[1] - low[1]).unwrap_or(0),
            u32::try_from(region.min[2] - low[2]).unwrap_or(0),
        ];
        let [sx, sy, sz] = region.size;
        let air: Vec<bool> = region.palette.iter().map(BlockState::is_air).collect();
        let mut local = 0u64;
        for y in 0..sy {
            for z in 0..sz {
                for x in 0..sx {
                    let value = packed_value(&region.states, local, bits) as usize;
                    local += 1;
                    let Some(&is_air) = air.get(value) else {
                        return Err(SchematicError::Corrupt(
                            "a block refers to a state outside the palette",
                        ));
                    };
                    if is_air {
                        continue;
                    }
                    if let Some(index) =
                        schematic.index(offset[0] + x, offset[1] + y, offset[2] + z)
                    {
                        schematic.blocks[index] = mapped[value];
                    }
                }
            }
        }
        for entity in &region.tile_entities {
            if schematic.block_entities.len() >= MAX_BLOCK_ENTITIES {
                break;
            }
            let Some([x, y, z]) = triple(Some(entity)) else {
                continue;
            };
            let (Ok(x), Ok(y), Ok(z)) = (u32::try_from(x), u32::try_from(y), u32::try_from(z))
            else {
                continue;
            };
            if x >= region.size[0] || y >= region.size[1] || z >= region.size[2] {
                continue;
            }
            if let (Some(index), Tag::Compound(entries)) = (
                schematic.index(offset[0] + x, offset[1] + y, offset[2] + z),
                entity,
            ) {
                let rest: Vec<(String, Tag)> = entries
                    .iter()
                    .filter(|(key, _)| !matches!(key.as_str(), "x" | "y" | "z"))
                    .cloned()
                    .collect();
                schematic
                    .block_entities
                    .insert(u32::try_from(index).unwrap_or(0), Tag::Compound(rest));
            }
        }
    }
    schematic.validate()?;
    Ok(schematic)
}

fn xyz(x: i32, y: i32, z: i32) -> Tag {
    compound([("x", Tag::Int(x)), ("y", Tag::Int(y)), ("z", Tag::Int(z))])
}

fn side(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// Builds the root tag of a `.litematic` for the schematic: one region at the origin.
///
/// `now_ms` stamps the creation and modification times, so the output is reproducible.
///
/// # Errors
///
/// Anything [`Schematic::validate`] reports.
pub fn to_tag(schematic: &Schematic, now_ms: i64) -> Result<Tag, SchematicError> {
    schematic.validate()?;
    let schematic = schematic.compacted();
    let [sx, sy, sz] = schematic.size;
    let bits = bits_for(schematic.palette.len());
    let mut longs =
        vec![0u64; (schematic.blocks.len() as u64 * u64::from(bits)).div_ceil(64) as usize];
    for (index, &value) in schematic.blocks.iter().enumerate() {
        let start = index as u64 * u64::from(bits);
        let word = (start / 64) as usize;
        let offset = (start % 64) as u32;
        longs[word] |= u64::from(value) << offset;
        if offset + bits > 64 {
            longs[word + 1] |= u64::from(value) >> (64 - offset);
        }
    }

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

    let tile_entities = schematic
        .block_entities
        .iter()
        .filter_map(|(&index, data)| {
            let [x, y, z] = schematic.position(index);
            let Tag::Compound(entries) = data else {
                return None;
            };
            let mut out = vec![
                ("x".to_owned(), Tag::Int(side(x))),
                ("y".to_owned(), Tag::Int(side(y))),
                ("z".to_owned(), Tag::Int(side(z))),
            ];
            out.extend(
                entries
                    .iter()
                    .filter(|(key, _)| !matches!(key.as_str(), "x" | "y" | "z"))
                    .cloned(),
            );
            Some(Tag::Compound(out))
        })
        .collect();

    let name = if schematic.name.is_empty() {
        "Main"
    } else {
        &schematic.name
    };
    let region = compound([
        ("Position", xyz(0, 0, 0)),
        ("Size", xyz(side(sx), side(sy), side(sz))),
        ("BlockStatePalette", Tag::List(palette)),
        (
            "BlockStates",
            Tag::LongArray(longs.into_iter().map(u64::cast_signed).collect()),
        ),
        ("TileEntities", Tag::List(tile_entities)),
        ("Entities", Tag::List(Vec::new())),
        ("PendingBlockTicks", Tag::List(Vec::new())),
        ("PendingFluidTicks", Tag::List(Vec::new())),
    ]);
    Ok(Tag::Compound(vec![
        (
            "MinecraftDataVersion".to_owned(),
            Tag::Int(schematic.data_version.unwrap_or(0)),
        ),
        ("Version".to_owned(), Tag::Int(WRITTEN_VERSION)),
        ("SubVersion".to_owned(), Tag::Int(1)),
        (
            "Metadata".to_owned(),
            compound([
                ("Name", Tag::String(name.to_owned())),
                ("Author", Tag::String(schematic.author.clone())),
                ("Description", Tag::String(schematic.description.clone())),
                ("TimeCreated", Tag::Long(now_ms)),
                ("TimeModified", Tag::Long(now_ms)),
                ("EnclosingSize", xyz(side(sx), side(sy), side(sz))),
                ("RegionCount", Tag::Int(1)),
                (
                    "TotalVolume",
                    Tag::Int(side(sx.saturating_mul(sy).saturating_mul(sz))),
                ),
                (
                    "TotalBlocks",
                    Tag::Int(i32::try_from(schematic.solid_count()).unwrap_or(i32::MAX)),
                ),
            ]),
        ),
        (
            "Regions".to_owned(),
            Tag::Compound(vec![(name.to_owned(), region)]),
        ),
    ]))
}

#[cfg(test)]
#[path = "litematic_tests.rs"]
mod tests;
