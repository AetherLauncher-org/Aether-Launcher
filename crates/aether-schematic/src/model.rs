//! The format-independent picture of a schematic: a box of block states plus the block
//! entities (chests, signs, …) that sit in it.

use std::collections::{BTreeMap, HashMap};

use thiserror::Error;

use crate::nbt::{NbtError, Tag};

/// The most blocks a schematic may hold (its bounding box, air included). Four bytes each,
/// so this is a 32 MB array at the limit, and it is what a small machine can still draw.
pub const MAX_VOLUME: u64 = 8_000_000;
/// The longest side. A bound on its own because a thin schematic can pass the volume check.
pub const MAX_SIDE: u32 = 4096;
/// The most distinct block states.
pub const MAX_PALETTE: usize = 65_536;
/// The most block entities kept.
pub const MAX_BLOCK_ENTITIES: usize = 200_000;

/// Failures when reading, building or converting a schematic.
#[derive(Debug, Error, PartialEq)]
pub enum SchematicError {
    #[error(transparent)]
    Nbt(#[from] NbtError),
    #[error("this file is not a schematic this launcher can read: {0}")]
    Unsupported(String),
    #[error("the schematic is damaged: {0}")]
    Corrupt(&'static str),
    #[error("the schematic is too large ({0})")]
    TooLarge(String),
}

/// One block state: the block's id and its properties (`facing=north`, …).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockState {
    pub name: String,
    pub properties: Vec<(String, String)>,
}

impl BlockState {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            properties: Vec::new(),
        }
    }

    #[must_use]
    pub fn air() -> Self {
        Self::new("minecraft:air")
    }

    /// Air in any of its three forms.
    #[must_use]
    pub fn is_air(&self) -> bool {
        matches!(
            self.name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
    }

    /// Parses `minecraft:oak_stairs[facing=north,half=bottom]`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (name, rest) = match text.split_once('[') {
            Some((name, rest)) => (name, Some(rest.strip_suffix(']')?)),
            None => (text, None),
        };
        if name.is_empty() {
            return None;
        }
        let mut properties = Vec::new();
        for pair in rest.into_iter().flat_map(|rest| rest.split(',')) {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=')?;
            properties.push((key.trim().to_owned(), value.trim().to_owned()));
        }
        Some(Self {
            name: name.to_owned(),
            properties,
        })
    }

    /// The inverse of [`parse`](Self::parse), properties sorted so equal states print equal.
    #[must_use]
    pub fn to_text(&self) -> String {
        if self.properties.is_empty() {
            return self.name.clone();
        }
        let mut sorted = self.properties.clone();
        sorted.sort();
        let inner: Vec<String> = sorted.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{}[{}]", self.name, inner.join(","))
    }
}

/// A schematic. Blocks are indexed `(y * size_z + z) * size_x + x`; palette entry 0 is air.
#[derive(Debug, Clone, PartialEq)]
pub struct Schematic {
    pub name: String,
    pub author: String,
    pub description: String,
    /// The Minecraft data version the blocks and block entities were written for.
    pub data_version: Option<i32>,
    pub size: [u32; 3],
    pub palette: Vec<BlockState>,
    pub blocks: Vec<u32>,
    /// Block entity data by block index, as a compound that includes its `id`.
    pub block_entities: BTreeMap<u32, Tag>,
}

/// Checks a size against the limits and returns its volume.
///
/// # Errors
///
/// [`SchematicError::TooLarge`] past [`MAX_SIDE`] or [`MAX_VOLUME`].
pub fn checked_volume(size: [u32; 3]) -> Result<u64, SchematicError> {
    if size.iter().any(|&side| side > MAX_SIDE) {
        return Err(SchematicError::TooLarge(format!(
            "a side is longer than {MAX_SIDE} blocks"
        )));
    }
    let volume = u64::from(size[0]) * u64::from(size[1]) * u64::from(size[2]);
    if volume > MAX_VOLUME {
        return Err(SchematicError::TooLarge(format!(
            "{volume} blocks; the limit is {MAX_VOLUME}"
        )));
    }
    Ok(volume)
}

impl Schematic {
    /// An empty (all air) schematic of the given size.
    ///
    /// # Errors
    ///
    /// [`SchematicError::TooLarge`].
    pub fn empty(name: &str, size: [u32; 3]) -> Result<Self, SchematicError> {
        let volume = checked_volume(size)?;
        Ok(Self {
            name: name.to_owned(),
            author: String::new(),
            description: String::new(),
            data_version: None,
            size,
            palette: vec![BlockState::air()],
            blocks: vec![0; usize::try_from(volume).unwrap_or(0)],
            block_entities: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn volume(&self) -> usize {
        self.blocks.len()
    }

    /// The index of a block, or `None` outside the box.
    #[must_use]
    pub fn index(&self, x: u32, y: u32, z: u32) -> Option<usize> {
        if x >= self.size[0] || y >= self.size[1] || z >= self.size[2] {
            return None;
        }
        Some(
            ((y as usize * self.size[2] as usize) + z as usize) * self.size[0] as usize
                + x as usize,
        )
    }

    /// The coordinates of a block index.
    #[must_use]
    pub fn position(&self, index: u32) -> [u32; 3] {
        let (sx, sz) = (self.size[0], self.size[2]);
        let x = index % sx;
        let z = (index / sx) % sz;
        let y = index / (sx * sz);
        [x, y, z]
    }

    /// The palette index of a state, adding it when new.
    ///
    /// # Errors
    ///
    /// [`SchematicError::TooLarge`] when the palette would pass [`MAX_PALETTE`].
    pub fn intern(&mut self, state: BlockState) -> Result<u32, SchematicError> {
        if let Some(found) = self.palette.iter().position(|existing| *existing == state) {
            return Ok(u32::try_from(found).unwrap_or(0));
        }
        if self.palette.len() >= MAX_PALETTE {
            return Err(SchematicError::TooLarge(
                "too many different blocks".to_owned(),
            ));
        }
        self.palette.push(state);
        Ok(u32::try_from(self.palette.len() - 1).unwrap_or(0))
    }

    /// Blocks that are not air.
    #[must_use]
    pub fn solid_count(&self) -> u64 {
        let air: Vec<bool> = self.palette.iter().map(BlockState::is_air).collect();
        self.blocks
            .iter()
            .filter(|&&index| !air.get(index as usize).copied().unwrap_or(true))
            .count() as u64
    }

    /// Checks the invariants every reader and writer relies on: the block array matches the
    /// size, every index is inside the palette, and entity positions are inside the box.
    ///
    /// # Errors
    ///
    /// [`SchematicError::Corrupt`] or [`SchematicError::TooLarge`].
    pub fn validate(&self) -> Result<(), SchematicError> {
        if u64::try_from(self.blocks.len()).unwrap_or(u64::MAX) != checked_volume(self.size)? {
            return Err(SchematicError::Corrupt(
                "the block count does not match the size",
            ));
        }
        if self.palette.is_empty() {
            return Err(SchematicError::Corrupt("the block palette is empty"));
        }
        let limit = u32::try_from(self.palette.len()).unwrap_or(u32::MAX);
        if self.blocks.iter().any(|&index| index >= limit) {
            return Err(SchematicError::Corrupt(
                "a block refers to a state outside the palette",
            ));
        }
        let volume = u32::try_from(self.blocks.len()).unwrap_or(u32::MAX);
        if self.block_entities.keys().any(|&index| index >= volume) {
            return Err(SchematicError::Corrupt(
                "a block entity is outside the schematic",
            ));
        }
        Ok(())
    }

    /// Moves palette entry 0 to air if it is not already, and drops palette entries nothing
    /// uses, so a written file has the compact, air-first palette every format expects.
    #[must_use]
    pub fn compacted(&self) -> Self {
        let mut used = vec![false; self.palette.len()];
        for &index in &self.blocks {
            if let Some(slot) = used.get_mut(index as usize) {
                *slot = true;
            }
        }
        let mut palette = vec![BlockState::air()];
        let mut remap: HashMap<u32, u32> = HashMap::new();
        for (old, state) in self.palette.iter().enumerate() {
            if !used[old] {
                continue;
            }
            let old = u32::try_from(old).unwrap_or(0);
            if let Some(found) = palette.iter().position(|existing| existing == state) {
                remap.insert(old, u32::try_from(found).unwrap_or(0));
            } else {
                palette.push(state.clone());
                remap.insert(old, u32::try_from(palette.len() - 1).unwrap_or(0));
            }
        }
        Self {
            palette,
            blocks: self
                .blocks
                .iter()
                .map(|index| remap.get(index).copied().unwrap_or(0))
                .collect(),
            ..self.clone()
        }
    }
}

/// Bits per entry for a packed array of `palette_len` states (never below two).
#[must_use]
pub fn bits_for(palette_len: usize) -> u32 {
    let needed = usize::BITS - palette_len.saturating_sub(1).leading_zeros();
    needed.max(2)
}
