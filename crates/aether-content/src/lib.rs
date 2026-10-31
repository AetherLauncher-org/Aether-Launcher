//! What the content platforms say about a file the player put in an instance.
//!
//! A mod, resource pack or shader pack is just a file on disk. A platform that hosts the
//! same bytes knows its name, summary and icon. This crate builds the requests that ask for
//! that (by file hash, never by file name) and reads the answers. It does no I/O: the caller
//! sends the requests, so what may be contacted and how is decided in one place
//! (`aether-app`).
//!
//! Everything a platform sends back is untrusted. Bodies have a size limit, ids are checked
//! against the alphabet the platform uses, text is stripped of control characters and cut to
//! a length, icon addresses must be on the platform's own CDN, and an image is recognised by
//! its first bytes (never by what the server claims; SVG is not accepted, it can carry
//! script). Nothing in an answer is executed, and no answer changes what is trusted: the
//! hash that was asked for is the identity, the platform only adds a label.
//!
//! [`skin`] does the same for a player's skin (Mojang's public session server), which the
//! account avatar is drawn from.
//!
//! Only Modrinth is implemented. CurseForge needs an API key that must not ship in the
//! client; see `docs/modules/content-metadata.md`.

pub mod browse;
pub mod modrinth;
pub mod skin;

use serde::Serialize;

/// Where an [`Identity`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Modrinth,
}

/// Why a request could not be built or an answer could not be used.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ContentError {
    #[error("the answer is larger than {0} bytes")]
    TooLarge(usize),
    #[error("the answer is not what the platform documents")]
    Malformed,
    #[error("too many items in one request (at most {0})")]
    TooMany(usize),
    #[error("not a SHA-1 hash")]
    InvalidHash,
    #[error("not a project id")]
    InvalidId,
}

/// Removes control characters (a newline becomes a space), trims, and cuts to `max` characters.
#[must_use]
pub fn clean_text(text: &str, max: usize) -> String {
    let flattened: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let mut out = String::new();
    let mut last_space = true;
    for c in flattened.chars() {
        if c == ' ' {
            if !last_space {
                out.push(c);
            }
            last_space = true;
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out.trim_end().chars().take(max).collect()
}

/// The image type of `bytes`, from its first bytes: PNG, JPEG, GIF or WebP. SVG and anything
/// else is `None`.
#[must_use]
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// The pixel size `(width, height)` an image declares in its header: PNG, GIF, WebP or JPEG.
/// Only the header is read, nothing is decoded. `None` for anything else or a damaged header.
#[must_use]
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let be32 = |at: usize| {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let le16 = |at: usize| {
        bytes
            .get(at..at + 2)
            .map(|b| u32::from(u16::from_le_bytes([b[0], b[1]])))
    };
    let le24 = |at: usize| {
        bytes
            .get(at..at + 3)
            .map(|b| u32::from(b[0]) | (u32::from(b[1]) << 8) | (u32::from(b[2]) << 16))
    };
    match image_mime(bytes)? {
        "image/png" => (bytes.get(12..16)? == b"IHDR").then_some((be32(16)?, be32(20)?)),
        "image/gif" => Some((le16(6)?, le16(8)?)),
        "image/webp" => match bytes.get(12..16)? {
            b"VP8 " => {
                // frame tag (3 bytes), start code 9d 01 2a, then 14-bit sizes
                (bytes.get(23..26)? == [0x9D, 0x01, 0x2A])
                    .then_some((le16(26)? & 0x3FFF, le16(28)? & 0x3FFF))
            }
            b"VP8L" => (*bytes.get(20)? == 0x2F).then(|| {
                let bits = u32::from_le_bytes([
                    *bytes.get(21)?,
                    *bytes.get(22)?,
                    *bytes.get(23)?,
                    *bytes.get(24)?,
                ]);
                Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
            })?,
            b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        },
        "image/jpeg" => {
            let mut at = 2;
            loop {
                if *bytes.get(at)? != 0xFF {
                    return None;
                }
                let marker = *bytes.get(at + 1)?;
                if marker == 0xFF {
                    at += 1;
                    continue;
                }
                let length = usize::from(u16::from_be_bytes([
                    *bytes.get(at + 2)?,
                    *bytes.get(at + 3)?,
                ]));
                // start-of-frame markers carry the size; 0xC4, 0xC8 and 0xCC are not frames
                if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                    let height = u32::from(u16::from_be_bytes([
                        *bytes.get(at + 5)?,
                        *bytes.get(at + 6)?,
                    ]));
                    let width = u32::from(u16::from_be_bytes([
                        *bytes.get(at + 7)?,
                        *bytes.get(at + 8)?,
                    ]));
                    return Some((width, height));
                }
                if length < 2 {
                    return None;
                }
                at += 2 + length;
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_sizes_come_from_the_headers() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&300u32.to_be_bytes());
        png.extend_from_slice(&200u32.to_be_bytes());
        assert_eq!(image_size(&png), Some((300, 200)));

        assert_eq!(image_size(b"GIF89a\x40\x01\xC8\x00"), Some((320, 200)));

        // VP8X: 24-bit (size - 1) at 24 and 27
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\x0A\0\0\0\0\0\0\0".to_vec();
        webp.extend_from_slice(&[0x3F, 0x01, 0x00, 0xC7, 0x00, 0x00]);
        assert_eq!(image_size(&webp), Some((320, 200)));

        // VP8L: signature 0x2f then 14 + 14 bits of (size - 1)
        let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2F".to_vec();
        let bits: u32 = 319 | (199 << 14);
        lossless.extend_from_slice(&bits.to_le_bytes());
        assert_eq!(image_size(&lossless), Some((320, 200)));

        // JPEG: an APP0 segment, then SOF0 (height before width)
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00,
            0xC8, 0x01, 0x40, 0x03,
        ];
        assert_eq!(image_size(&jpeg), Some((320, 200)));
    }

    #[test]
    fn damaged_or_foreign_headers_have_no_size() {
        assert_eq!(image_size(b"\x89PNG\r\n\x1a\n"), None);
        assert_eq!(image_size(b"<svg/>"), None);
        assert_eq!(image_size(&[0xFF, 0xD8, 0xFF]), None);
        assert_eq!(image_size(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00]), None);
        assert_eq!(image_size(b""), None);
    }

    #[test]
    fn text_loses_control_characters_and_is_cut() {
        assert_eq!(clean_text("  a\nb\t\u{0}c  ", 10), "a b c");
        assert_eq!(clean_text("abcdef", 3), "abc");
        assert_eq!(clean_text("日本語のテキスト", 3), "日本語");
        assert_eq!(clean_text("", 5), "");
    }

    #[test]
    fn images_are_recognised_by_their_bytes() {
        assert_eq!(image_mime(b"\x89PNG\r\n\x1a\nrest"), Some("image/png"));
        assert_eq!(image_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(image_mime(b"GIF89a.."), Some("image/gif"));
        assert_eq!(image_mime(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            image_mime(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            None
        );
        assert_eq!(image_mime(b"<!doctype html>"), None);
        assert_eq!(image_mime(b""), None);
    }
}
