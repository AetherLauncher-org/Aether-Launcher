//! Reading, converting and writing Minecraft schematics: Litematica (`.litematic`), Sponge
//! (`.schem`) and vanilla structure (`.nbt`) files.
//!
//! The crate does no file or network access. It takes bytes and returns a [`Schematic`] (or
//! bytes), and every limit that protects against a hostile file lives here: nesting, value
//! counts, decompressed size, block volume. See `docs/modules/schematics.md`.

pub mod convert;
pub mod litematic;
pub mod model;
pub mod nbt;
pub mod sponge;
pub mod structure;

pub use convert::{
    Conversion, Format, KNOWN_DATA_VERSIONS, convert as convert_file, read_any, write_as,
};
pub use model::{BlockState, Schematic, SchematicError};
