//! A small NBT reader and writer, built for files that came from somewhere else.
//!
//! Schematics are downloaded, shared and renamed by people, so every read is bounded: how
//! deep the structure nests, how many values it holds, how long a string, list or array is
//! claimed to be compared with the bytes that actually remain, and (for compressed files)
//! how large the decompressed data may grow. A file that crosses a bound is an error, never
//! a crash or an allocation the file's header asked for.

use std::fmt::Write as _;
use std::io::Read;

use flate2::read::GzDecoder;
use thiserror::Error;

/// How deep compounds and lists may nest. Minecraft itself stops at 512; real files are
/// nowhere near that.
pub const MAX_DEPTH: usize = 64;
/// How many values (tags) one file may hold.
pub const MAX_NODES: usize = 4_000_000;
/// The most bytes a file may decompress to.
pub const MAX_DECOMPRESSED_BYTES: u64 = 96 * 1024 * 1024;

/// Failures when reading NBT.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum NbtError {
    #[error("the data ends before the structure is complete")]
    Truncated,
    #[error("unknown tag type {0}")]
    UnknownTag(u8),
    #[error("the structure nests deeper than {MAX_DEPTH} levels")]
    TooDeep,
    #[error("the file holds more than {MAX_NODES} values")]
    TooManyNodes,
    #[error("a length in the file is larger than the data that is left")]
    BadLength,
    #[error("the root of the file is not a compound")]
    RootNotCompound,
    #[error("the file expands to more than {} MiB", MAX_DECOMPRESSED_BYTES / (1024 * 1024))]
    TooLarge,
    #[error("the compressed data is damaged")]
    BadCompression,
    #[error("there is data after the end of the structure")]
    TrailingData,
}

/// One NBT value. Compounds keep their order, so a file written back looks like the one read.
#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Self::Byte(_) => 1,
            Self::Short(_) => 2,
            Self::Int(_) => 3,
            Self::Long(_) => 4,
            Self::Float(_) => 5,
            Self::Double(_) => 6,
            Self::ByteArray(_) => 7,
            Self::String(_) => 8,
            Self::List(_) => 9,
            Self::Compound(_) => 10,
            Self::IntArray(_) => 11,
            Self::LongArray(_) => 12,
        }
    }

    /// A child of a compound, by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Tag> {
        match self {
            Self::Compound(entries) => entries
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// Any whole-number tag as an `i64`.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Byte(v) => Some(i64::from(*v)),
            Self::Short(v) => Some(i64::from(*v)),
            Self::Int(v) => Some(i64::from(*v)),
            Self::Long(v) => Some(*v),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_i32(&self) -> Option<i32> {
        self.as_i64().and_then(|v| i32::try_from(v).ok())
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_list(&self) -> Option<&[Tag]> {
        match self {
            Self::List(items) => Some(items),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_compound(&self) -> Option<&[(String, Tag)]> {
        match self {
            Self::Compound(entries) => Some(entries),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_long_array(&self) -> Option<&[i64]> {
        match self {
            Self::LongArray(values) => Some(values),
            _ => None,
        }
    }
}

/// Builds a compound from `(name, tag)` pairs.
#[must_use]
pub fn compound<const N: usize>(entries: [(&str, Tag); N]) -> Tag {
    Tag::Compound(
        entries
            .into_iter()
            .map(|(name, tag)| (name.to_owned(), tag))
            .collect(),
    )
}

// --------------------------------------------------------------------------------------
// Reading
// --------------------------------------------------------------------------------------

/// Decompresses gzip data (recognised by its magic number), or returns plain data as is.
///
/// # Errors
///
/// [`NbtError::TooLarge`] when the data would expand past [`MAX_DECOMPRESSED_BYTES`];
/// [`NbtError::BadCompression`] when it is not valid gzip.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>, NbtError> {
    if bytes.len() < 2 || bytes[0] != 0x1f || bytes[1] != 0x8b {
        return if bytes.len() as u64 > MAX_DECOMPRESSED_BYTES {
            Err(NbtError::TooLarge)
        } else {
            Ok(bytes.to_vec())
        };
    }
    let mut out = Vec::new();
    GzDecoder::new(bytes)
        .take(MAX_DECOMPRESSED_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|_| NbtError::BadCompression)?;
    if out.len() as u64 > MAX_DECOMPRESSED_BYTES {
        return Err(NbtError::TooLarge);
    }
    Ok(out)
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
    nodes: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], NbtError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or(NbtError::BadLength)?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or(NbtError::Truncated)?;
        self.position = end;
        Ok(slice)
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.position
    }

    fn byte(&mut self) -> Result<u8, NbtError> {
        Ok(self.take(1)?[0])
    }

    fn short(&mut self) -> Result<i16, NbtError> {
        Ok(i16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| NbtError::Truncated)?,
        ))
    }

    fn int(&mut self) -> Result<i32, NbtError> {
        Ok(i32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| NbtError::Truncated)?,
        ))
    }

    fn long(&mut self) -> Result<i64, NbtError> {
        Ok(i64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| NbtError::Truncated)?,
        ))
    }

    fn string(&mut self) -> Result<String, NbtError> {
        let length = usize::from(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| NbtError::Truncated)?,
        ));
        // Java's "modified UTF-8" matches UTF-8 for everything real files contain.
        Ok(String::from_utf8_lossy(self.take(length)?).into_owned())
    }

    /// A length prefix for `unit`-byte elements, checked against what is actually left.
    fn array_length(&mut self, unit: usize) -> Result<usize, NbtError> {
        let length = usize::try_from(self.int()?).map_err(|_| NbtError::BadLength)?;
        if length
            .checked_mul(unit)
            .is_none_or(|bytes| bytes > self.remaining())
        {
            return Err(NbtError::BadLength);
        }
        Ok(length)
    }

    fn count_node(&mut self) -> Result<(), NbtError> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(NbtError::TooManyNodes);
        }
        Ok(())
    }

    fn payload(&mut self, id: u8, depth: usize) -> Result<Tag, NbtError> {
        self.count_node()?;
        if depth > MAX_DEPTH {
            return Err(NbtError::TooDeep);
        }
        Ok(match id {
            1 => Tag::Byte(i8::from_be_bytes([self.byte()?])),
            2 => Tag::Short(self.short()?),
            3 => Tag::Int(self.int()?),
            4 => Tag::Long(self.long()?),
            5 => Tag::Float(f32::from_bits(self.int()?.cast_unsigned())),
            6 => Tag::Double(f64::from_bits(self.long()?.cast_unsigned())),
            7 => {
                let length = self.array_length(1)?;
                Tag::ByteArray(self.take(length)?.iter().map(|&b| b as i8).collect())
            }
            8 => Tag::String(self.string()?),
            9 => {
                let element = self.byte()?;
                // A list of anything needs at least one byte per element, except lists of
                // empty compounds; the cap on `length` against remaining bytes still holds
                // because an empty compound is one byte (its End tag).
                let length = self.array_length(1)?;
                if element == 0 && length > 0 {
                    return Err(NbtError::UnknownTag(0));
                }
                let mut items = Vec::with_capacity(length.min(4096));
                for _ in 0..length {
                    items.push(self.payload(element, depth + 1)?);
                }
                Tag::List(items)
            }
            10 => {
                let mut entries = Vec::new();
                loop {
                    let child = self.byte()?;
                    if child == 0 {
                        break;
                    }
                    let name = self.string()?;
                    entries.push((name, self.payload(child, depth + 1)?));
                }
                Tag::Compound(entries)
            }
            11 => {
                let length = self.array_length(4)?;
                let mut values = Vec::with_capacity(length);
                for _ in 0..length {
                    values.push(self.int()?);
                }
                Tag::IntArray(values)
            }
            12 => {
                let length = self.array_length(8)?;
                let mut values = Vec::with_capacity(length);
                for _ in 0..length {
                    values.push(self.long()?);
                }
                Tag::LongArray(values)
            }
            other => return Err(NbtError::UnknownTag(other)),
        })
    }
}

/// Reads a whole NBT document (already decompressed): the root name and its compound.
///
/// # Errors
///
/// See [`NbtError`].
pub fn read(data: &[u8]) -> Result<(String, Tag), NbtError> {
    let mut reader = Reader {
        data,
        position: 0,
        nodes: 0,
    };
    if reader.byte()? != 10 {
        return Err(NbtError::RootNotCompound);
    }
    let name = reader.string()?;
    let root = reader.payload(10, 0)?;
    if reader.remaining() != 0 {
        return Err(NbtError::TrailingData);
    }
    Ok((name, root))
}

/// Decompresses if needed, then reads.
///
/// # Errors
///
/// See [`NbtError`].
pub fn read_maybe_gzip(bytes: &[u8]) -> Result<(String, Tag), NbtError> {
    read(&decompress(bytes)?)
}

// --------------------------------------------------------------------------------------
// Writing
// --------------------------------------------------------------------------------------

fn put_string(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    let length = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(&bytes[..usize::from(length)]);
}

fn put_payload(out: &mut Vec<u8>, tag: &Tag) {
    match tag {
        Tag::Byte(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Short(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Int(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Long(v) => out.extend_from_slice(&v.to_be_bytes()),
        Tag::Float(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        Tag::Double(v) => out.extend_from_slice(&v.to_bits().to_be_bytes()),
        Tag::ByteArray(values) => {
            out.extend_from_slice(
                &i32::try_from(values.len())
                    .unwrap_or(i32::MAX)
                    .to_be_bytes(),
            );
            out.extend(values.iter().map(|&v| v as u8));
        }
        Tag::String(value) => put_string(out, value),
        Tag::List(items) => {
            let element = items.first().map_or(0, Tag::id);
            out.push(element);
            out.extend_from_slice(&i32::try_from(items.len()).unwrap_or(i32::MAX).to_be_bytes());
            for item in items {
                put_payload(out, item);
            }
        }
        Tag::Compound(entries) => {
            for (name, value) in entries {
                out.push(value.id());
                put_string(out, name);
                put_payload(out, value);
            }
            out.push(0);
        }
        Tag::IntArray(values) => {
            out.extend_from_slice(
                &i32::try_from(values.len())
                    .unwrap_or(i32::MAX)
                    .to_be_bytes(),
            );
            for v in values {
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
        Tag::LongArray(values) => {
            out.extend_from_slice(
                &i32::try_from(values.len())
                    .unwrap_or(i32::MAX)
                    .to_be_bytes(),
            );
            for v in values {
                out.extend_from_slice(&v.to_be_bytes());
            }
        }
    }
}

/// Serialises a compound as an NBT document with the given root name (uncompressed).
#[must_use]
pub fn write(root_name: &str, root: &Tag) -> Vec<u8> {
    let mut out = vec![10];
    put_string(&mut out, root_name);
    put_payload(&mut out, root);
    out
}

/// Serialises and gzips.
#[must_use]
pub fn write_gzip(root_name: &str, root: &Tag) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write as _;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    // Writing into a Vec cannot fail.
    let _ = encoder.write_all(&write(root_name, root));
    encoder.finish().unwrap_or_default()
}

// --------------------------------------------------------------------------------------
// SNBT, for showing a block entity to a person
// --------------------------------------------------------------------------------------

fn bare_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'+'))
}

fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn snbt_into(out: &mut String, tag: &Tag, indent: usize, pretty: bool) {
    let newline = |out: &mut String, level: usize| {
        if pretty {
            out.push('\n');
            out.push_str(&"  ".repeat(level));
        }
    };
    match tag {
        Tag::Byte(v) => {
            let _ = write!(out, "{v}b");
        }
        Tag::Short(v) => {
            let _ = write!(out, "{v}s");
        }
        Tag::Int(v) => {
            let _ = write!(out, "{v}");
        }
        Tag::Long(v) => {
            let _ = write!(out, "{v}L");
        }
        Tag::Float(v) => {
            let _ = write!(out, "{v}f");
        }
        Tag::Double(v) => {
            let _ = write!(out, "{v}d");
        }
        Tag::String(v) => out.push_str(&quoted(v)),
        Tag::ByteArray(values) => {
            out.push_str("[B; ");
            out.push_str(
                &values
                    .iter()
                    .map(|v| format!("{v}b"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push(']');
        }
        Tag::IntArray(values) => {
            out.push_str("[I; ");
            out.push_str(
                &values
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push(']');
        }
        Tag::LongArray(values) => {
            out.push_str("[L; ");
            out.push_str(
                &values
                    .iter()
                    .map(|v| format!("{v}L"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            out.push(']');
        }
        Tag::List(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                    if !pretty {
                        out.push(' ');
                    }
                }
                newline(out, indent + 1);
                snbt_into(out, item, indent + 1, pretty);
            }
            newline(out, indent);
            out.push(']');
        }
        Tag::Compound(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (index, (key, value)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                    if !pretty {
                        out.push(' ');
                    }
                }
                newline(out, indent + 1);
                out.push_str(&if bare_key(key) {
                    key.clone()
                } else {
                    quoted(key)
                });
                out.push_str(": ");
                snbt_into(out, value, indent + 1, pretty);
            }
            newline(out, indent);
            out.push('}');
        }
    }
}

/// The tag in Minecraft's text form. `pretty` indents compounds and lists; the result is cut
/// at `max_chars` (on a character boundary) with an ellipsis, so a huge value cannot flood
/// the interface.
#[must_use]
pub fn to_snbt(tag: &Tag, pretty: bool, max_chars: usize) -> String {
    let mut out = String::new();
    snbt_into(&mut out, tag, 0, pretty);
    if out.chars().count() > max_chars {
        let mut cut: String = out.chars().take(max_chars).collect();
        cut.push('…');
        return cut;
    }
    out
}

#[cfg(test)]
#[path = "nbt_tests.rs"]
mod tests;
