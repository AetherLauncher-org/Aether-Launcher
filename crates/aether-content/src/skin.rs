//! Player skins: find a skin's address in Mojang's public profile and check the picture.
//!
//! The session server answers `GET /session/minecraft/profile/<uuid>` for any player without a
//! token. The answer carries one property, `textures`, a base-64 JSON document whose `SKIN`
//! entry names the picture on `textures.minecraft.net`. Both documents and the picture are
//! untrusted: sizes are limited, the address must be exactly the texture host and path shape,
//! and the picture must be a PNG of a skin's dimensions (64×64 or the old 64×32) before it is
//! kept, so a page that shows it never meets anything else.

use serde::Deserialize;

use crate::ContentError;

/// The only host a skin picture may come from.
pub const TEXTURE_HOST: &str = "textures.minecraft.net";

/// The profile answer, read at most this far, in bytes.
pub const MAX_PROFILE_BYTES: usize = 64 * 1024;

/// The decoded `textures` document, at most this long, in bytes.
const MAX_TEXTURES_BYTES: usize = 16 * 1024;

/// The biggest skin picture taken, in bytes. A real skin is a few KiB.
pub const MAX_SKIN_BYTES: usize = 64 * 1024;

/// The address of a player's profile on the session server. `uuid` is the 32-digit,
/// lower-case hexadecimal form.
#[must_use]
pub fn profile_url(uuid: &str) -> Option<String> {
    is_uuid(uuid)
        .then(|| format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"))
}

/// `true` for 32 lower-case hexadecimal digits.
#[must_use]
pub fn is_uuid(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Standard base-64 (with or without `=` padding). `None` for anything else, or when the
/// result would be longer than `max` bytes.
#[must_use]
pub fn base64_decode(text: &str, max: usize) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    if text.len() % 4 == 1 || text.len() / 4 * 3 > max + 3 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 2);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xFF).ok()?);
        }
    }
    (out.len() <= max).then_some(out)
}

/// `url` as an HTTPS texture address when it is exactly `http(s)://textures.minecraft.net/texture/<hex>`
/// (Mojang still hands out the `http` form); `None` otherwise.
#[must_use]
pub fn trusted_texture_url(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/')?;
    let hash = path.strip_prefix("texture/")?;
    let hash_ok = (1..=64).contains(&hash.len())
        && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (host == TEXTURE_HOST && hash_ok).then(|| format!("https://{TEXTURE_HOST}/texture/{hash}"))
}

#[derive(Deserialize)]
struct Profile {
    #[serde(default)]
    properties: Vec<Property>,
}

#[derive(Deserialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Deserialize)]
struct Textures {
    textures: Option<TextureSet>,
}

#[derive(Deserialize)]
struct TextureSet {
    #[serde(rename = "SKIN")]
    skin: Option<Skin>,
}

#[derive(Deserialize)]
struct Skin {
    url: String,
    metadata: Option<SkinMetadata>,
}

#[derive(Deserialize)]
struct SkinMetadata {
    model: Option<String>,
}

/// A skin as the profile names it: where the picture is, and whether the model has slim
/// (3-pixel, "Alex") arms. A skin that says nothing about its model is the classic one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinRef {
    pub url: String,
    pub slim: bool,
}

/// The longest player name Mojang allows, and the shortest.
pub const MIN_NAME: usize = 3;
pub const MAX_NAME: usize = 16;

/// `true` for a Minecraft player name: 3 to 16 letters, digits or underscores.
#[must_use]
pub fn is_player_name(text: &str) -> bool {
    (MIN_NAME..=MAX_NAME).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The address that turns a player name into a player id (`GET`, no token needed).
#[must_use]
pub fn name_lookup_url(name: &str) -> Option<String> {
    is_player_name(name)
        .then(|| format!("https://api.minecraftservices.com/minecraft/profile/lookup/name/{name}"))
}

#[derive(Deserialize)]
struct NameAnswer {
    id: String,
    name: String,
}

/// Reads the answer to a name lookup: the player's id (lower-case, 32 digits) and the name
/// with its real capitalisation.
///
/// # Errors
///
/// [`ContentError::TooLarge`] and [`ContentError::Malformed`].
pub fn parse_name_lookup(body: &str) -> Result<(String, String), ContentError> {
    if body.len() > MAX_PROFILE_BYTES {
        return Err(ContentError::TooLarge(MAX_PROFILE_BYTES));
    }
    let answer: NameAnswer = serde_json::from_str(body).map_err(|_| ContentError::Malformed)?;
    let id = answer.id.to_ascii_lowercase();
    if is_uuid(&id) && is_player_name(&answer.name) {
        Ok((id, answer.name))
    } else {
        Err(ContentError::Malformed)
    }
}

/// The skin's address from a profile answer: `Ok(None)` for a player with no custom skin.
///
/// # Errors
///
/// [`ContentError::TooLarge`] and [`ContentError::Malformed`]; the latter also for a skin whose
/// address is not a texture address, which is an odd answer and not "no skin".
pub fn skin_url_from_profile(body: &str) -> Result<Option<String>, ContentError> {
    Ok(skin_from_profile(body)?.map(|skin| skin.url))
}

/// Like [`skin_url_from_profile`], and also says which model the skin is made for.
///
/// # Errors
///
/// As [`skin_url_from_profile`].
pub fn skin_from_profile(body: &str) -> Result<Option<SkinRef>, ContentError> {
    if body.len() > MAX_PROFILE_BYTES {
        return Err(ContentError::TooLarge(MAX_PROFILE_BYTES));
    }
    let profile: Profile = serde_json::from_str(body).map_err(|_| ContentError::Malformed)?;
    let Some(property) = profile
        .properties
        .iter()
        .find(|property| property.name == "textures")
    else {
        return Ok(None);
    };
    let decoded =
        base64_decode(&property.value, MAX_TEXTURES_BYTES).ok_or(ContentError::Malformed)?;
    let textures: Textures =
        serde_json::from_slice(&decoded).map_err(|_| ContentError::Malformed)?;
    let Some(skin) = textures.textures.and_then(|set| set.skin) else {
        return Ok(None);
    };
    let url = trusted_texture_url(&skin.url).ok_or(ContentError::Malformed)?;
    let slim = skin
        .metadata
        .and_then(|metadata| metadata.model)
        .is_some_and(|model| model == "slim");
    Ok(Some(SkinRef { url, slim }))
}

/// Checks that `bytes` is a PNG of a skin: small, with the PNG signature, an `IHDR` first and
/// 64×64 or 64×32 pixels. The size is read from the header, so nothing is decoded here.
///
/// # Errors
///
/// [`ContentError::TooLarge`] and [`ContentError::Malformed`].
pub fn validate_skin_png(bytes: &[u8]) -> Result<(), ContentError> {
    if bytes.len() > MAX_SKIN_BYTES {
        return Err(ContentError::TooLarge(MAX_SKIN_BYTES));
    }
    let header_ok = bytes.len() >= 33
        && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && bytes[8..12] == [0, 0, 0, 13]
        && &bytes[12..16] == b"IHDR";
    if !header_ok {
        return Err(ContentError::Malformed);
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width == 64 && (height == 64 || height == 32) {
        Ok(())
    } else {
        Err(ContentError::Malformed)
    }
}

#[cfg(test)]
#[path = "skin_tests.rs"]
mod tests;
