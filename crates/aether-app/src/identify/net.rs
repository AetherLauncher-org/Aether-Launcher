//! The network access content lookup is allowed: HTTPS to Modrinth's API and CDN, nothing else.

use std::time::Duration;

use aether_content::modrinth::{API_BASE, ICON_HOST, MAX_BODY_BYTES};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

/// What lookup needs from the network. A trait so tests can answer without one.
pub trait ContentNet {
    /// `POST` a JSON body, return the answer's text.
    fn post_json(&self, url: &str, body: &str) -> Result<String, String>;
    /// `GET`, return the answer's text.
    fn get_text(&self, url: &str) -> Result<String, String>;
    /// `GET`, return at most `limit` bytes; more than that is an error.
    fn get_bytes(&self, url: &str, limit: usize) -> Result<Vec<u8>, String>;
    /// `GET`, a reader over at most `limit` bytes of the body, for files too big to hold in
    /// memory. The reader fails once the limit is crossed. Only the production net does this.
    fn get_stream(&self, _url: &str, _limit: u64) -> Result<Box<dyn std::io::Read + Send>, String> {
        Err("streaming is not available".to_owned())
    }
}

/// `true` for the two places lookup may talk to: the API and the icon CDN, over HTTPS.
#[must_use]
pub fn is_allowed_url(url: &str) -> bool {
    let api = url
        .strip_prefix(API_BASE)
        .is_some_and(|rest| rest.starts_with('/'));
    let cdn = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(host, _)| host == ICON_HOST);
    (api || cdn) && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// The production [`ContentNet`]: blocking `ureq`, no redirects, bounded bodies, and a
/// `User-Agent` that names the launcher as Modrinth asks.
///
/// Errors say what failed, never the address or the body.
pub struct UreqNet;

impl UreqNet {
    fn agent() -> ureq::Agent {
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
    }
}

fn check(url: &str) -> Result<(), String> {
    if is_allowed_url(url) {
        Ok(())
    } else {
        Err("that address is not one the launcher contacts".to_owned())
    }
}

fn failed(error: &ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(429) => "the platform is limiting requests, try later".to_owned(),
        ureq::Error::StatusCode(code) => format!("the platform answered {code}"),
        _ => "the platform cannot be reached".to_owned(),
    }
}

impl ContentNet for UreqNet {
    fn post_json(&self, url: &str, body: &str) -> Result<String, String> {
        check(url)?;
        let mut response = Self::agent()
            .post(url)
            .header("content-type", "application/json")
            .send(body)
            .map_err(|error| failed(&error))?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES as u64)
            .read_to_string()
            .map_err(|error| failed(&error))
    }

    fn get_text(&self, url: &str) -> Result<String, String> {
        check(url)?;
        let mut response = Self::agent()
            .get(url)
            .call()
            .map_err(|error| failed(&error))?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES as u64)
            .read_to_string()
            .map_err(|error| failed(&error))
    }

    fn get_bytes(&self, url: &str, limit: usize) -> Result<Vec<u8>, String> {
        check(url)?;
        let mut response = Self::agent()
            .get(url)
            .call()
            .map_err(|error| failed(&error))?;
        response
            .body_mut()
            .with_config()
            .limit(limit as u64)
            .read_to_vec()
            .map_err(|error| failed(&error))
    }

    fn get_stream(&self, url: &str, limit: u64) -> Result<Box<dyn std::io::Read + Send>, String> {
        check(url)?;
        let response = Self::agent()
            .get(url)
            // a file download may take longer than a lookup; the size is bounded instead
            .config()
            .timeout_global(Some(DOWNLOAD_TIMEOUT))
            .build()
            .call()
            .map_err(|error| failed(&error))?;
        Ok(Box::new(
            response
                .into_body()
                .into_with_config()
                .limit(limit)
                .reader(),
        ))
    }
}
