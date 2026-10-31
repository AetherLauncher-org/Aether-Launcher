//! A signed-in player's skin, for the account avatar.
//!
//! The picture comes from Mojang's public session server (no token is involved, the player
//! UUID is public) and from `textures.minecraft.net`, the only two addresses this module
//! contacts, over HTTPS without redirects. Everything is checked before it is kept (see
//! `aether_content::skin`): a skin is a PNG of 64×64 or 64×32 pixels and a few KiB, nothing
//! else. The page receives it as a `data:` URL, so its content security policy stays closed
//! to the network.
//!
//! A skin is kept for a day in `<data>/cache/skins`, a player without one is remembered for
//! a day too, and with no network a kept skin is used however old it is. Offline accounts
//! have no skin to ask for and do not come here.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use aether_content::skin::{
    MAX_PROFILE_BYTES, MAX_SKIN_BYTES, TEXTURE_HOST, is_uuid, profile_url, skin_url_from_profile,
    validate_skin_png,
};

use crate::host::Host;

/// How long a skin, or the lack of one, is trusted without asking again, in seconds.
const TTL: u64 = 24 * 60 * 60;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// What skin lookup needs from the network. A trait so tests can answer without one.
pub trait SkinNet {
    fn get_text(&self, url: &str, limit: usize) -> Result<String, String>;
    fn get_bytes(&self, url: &str, limit: usize) -> Result<Vec<u8>, String>;
}

/// `true` for the two places skin lookup may talk to.
#[must_use]
pub fn is_allowed_url(url: &str) -> bool {
    let session = url
        .strip_prefix("https://sessionserver.mojang.com/session/minecraft/profile/")
        .is_some_and(is_uuid);
    let texture = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(host, path)| host == TEXTURE_HOST && path.starts_with("texture/"));
    (session || texture) && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// The production [`SkinNet`]: blocking `ureq`, no redirects, bounded bodies.
pub struct UreqSkinNet;

impl UreqSkinNet {
    fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
        if !is_allowed_url(url) {
            return Err("that address is not one the launcher contacts".to_owned());
        }
        let config = ureq::config::Config::builder()
            .max_redirects(0)
            .user_agent(concat!(
                "AetherLauncher/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/AetherLauncher-org/Aether-Launcher)"
            ))
            .timeout_global(Some(REQUEST_TIMEOUT))
            .build();
        ureq::Agent::new_with_config(config)
            .get(url)
            .call()
            .map_err(|error| match error {
                ureq::Error::StatusCode(code) => format!("the skin service answered {code}"),
                _ => "the skin service cannot be reached".to_owned(),
            })
    }
}

impl SkinNet for UreqSkinNet {
    fn get_text(&self, url: &str, limit: usize) -> Result<String, String> {
        Self::get(url)?
            .body_mut()
            .with_config()
            .limit(limit as u64)
            .read_to_string()
            .map_err(|_| "the skin service sent something unusable".to_owned())
    }

    fn get_bytes(&self, url: &str, limit: usize) -> Result<Vec<u8>, String> {
        Self::get(url)?
            .body_mut()
            .with_config()
            .limit(limit as u64)
            .read_to_vec()
            .map_err(|_| "the skin service sent something unusable".to_owned())
    }
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn age_of(path: &Path, now: u64) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(now.saturating_sub(seconds(modified)))
}

/// A kept skin as a `data:` URL, if the file is still a valid skin picture.
fn kept_skin(path: &Path) -> Option<String> {
    let bytes = crate::cachefile::read_capped(path, MAX_SKIN_BYTES as u64)?;
    validate_skin_png(&bytes).ok()?;
    Some(format!(
        "data:image/png;base64,{}",
        crate::schematics::base64(&bytes)
    ))
}

fn fetch(net: &dyn SkinNet, uuid: &str) -> Result<Option<Vec<u8>>, String> {
    let url = profile_url(uuid).ok_or_else(|| "not a player id".to_owned())?;
    let profile = net.get_text(&url, MAX_PROFILE_BYTES)?;
    let Some(texture) = skin_url_from_profile(&profile).map_err(|error| error.to_string())? else {
        return Ok(None);
    };
    let bytes = net.get_bytes(&texture, MAX_SKIN_BYTES)?;
    validate_skin_png(&bytes).map_err(|error| error.to_string())?;
    Ok(Some(bytes))
}

/// The skin of the player `uuid` (32 lower-case hexadecimal digits) as a `data:` PNG URL, or
/// `None` when the player has no custom skin or none can be had (the avatar then falls back
/// to a letter). It is never an error for the network to be down.
///
/// # Errors
///
/// For a `uuid` that is not a player id, or when the data directory is unavailable.
pub fn skin_for(host: &dyn Host, uuid: &str) -> Result<Option<String>, String> {
    skin_with(&UreqSkinNet, host, uuid, seconds(SystemTime::now()))
}

pub(crate) fn skin_with(
    net: &dyn SkinNet,
    host: &dyn Host,
    uuid: &str,
    now: u64,
) -> Result<Option<String>, String> {
    if !is_uuid(uuid) {
        return Err("not a player id".to_owned());
    }
    let dir: PathBuf = host.data_dir()?.join("cache").join("skins");
    let picture = dir.join(format!("{uuid}.png"));
    let none = dir.join(format!("{uuid}.none"));

    if age_of(&picture, now).is_some_and(|age| age < TTL)
        && let Some(skin) = kept_skin(&picture)
    {
        return Ok(Some(skin));
    }
    if age_of(&none, now).is_some_and(|age| age < TTL) {
        return Ok(None);
    }

    match fetch(net, uuid) {
        Ok(Some(bytes)) => {
            let _ = fs::remove_file(&none);
            if fs::create_dir_all(&dir).is_ok() {
                let _ = crate::cachefile::write_replacing(&picture, &bytes);
            }
            Ok(Some(format!(
                "data:image/png;base64,{}",
                crate::schematics::base64(&bytes)
            )))
        }
        Ok(None) => {
            let _ = fs::remove_file(&picture);
            if fs::create_dir_all(&dir).is_ok() {
                let _ = crate::cachefile::write_replacing(&none, b"");
            }
            Ok(None)
        }
        // No network, a limit, an odd answer: show what was kept, however old.
        Err(_) => Ok(kept_skin(&picture)),
    }
}

#[cfg(test)]
#[path = "skins_tests.rs"]
mod tests;
