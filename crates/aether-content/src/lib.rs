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
//! Only Modrinth is implemented. CurseForge needs an API key that must not ship in the
//! client; see `docs/modules/content-metadata.md`.

pub mod modrinth;

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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(image_mime(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);
        assert_eq!(image_mime(b"<!doctype html>"), None);
        assert_eq!(image_mime(b""), None);
    }
}
