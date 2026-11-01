//! The authlib-injector artifact manifest: pure, bounded parsing of the JSON that names
//! a release's build number, version and SHA-256 checksum.
//!
//! The manifest is the only trust root for the agent JAR. The download URL is **never**
//! taken from it — the caller rebuilds the URL from the parsed build number and version —
//! and only the SHA-256 here is used to verify whatever bytes a mirror returns. This keeps
//! a compromised or tampered manifest from pointing the launcher at an arbitrary URL.

use crate::http::HttpResponse;
use serde::Deserialize;

/// The response body must stay well under this; a manifest is a few hundred bytes.
pub const MAX_MANIFEST_BODY: usize = 8 * 1024;

/// The stable shape `{"build_number":56,"version":"1.2.8","release_time":"...",
/// "download_url":"...","checksums":{"sha256":"<64 hex>"}}`. `download_url` is read only to
/// be ignored: it must never be used to fetch the JAR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactManifest {
    pub build_number: u32,
    pub version: String,
    pub sha256: String,
}

/// Strict bounded shape: two through four numeric parts, at most eight digits each.
fn valid_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    version.len() <= 32
        && (2..=4).contains(&parts.len())
        && parts.iter().all(|part| {
            (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_digit())
        })
}

/// `build_number` must be a small positive integer; the SHA-256 must be exactly 64 lowercase
/// hex digits. Anything else is refused so a malformed manifest cannot name a bogus artifact.
fn valid_sha256(sha256: &str) -> bool {
    sha256.len() == 64
        && sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Parses a `2xx` manifest body into an [`ArtifactManifest`].
///
/// # Errors
///
/// - a non-`2xx` status → [`ERR_STATUS`];
/// - a body over [`MAX_MANIFEST_BODY`] → [`ERR_TOO_LARGE`];
/// - malformed JSON, a bad build number, a bad version, or a bad checksum → [`ERR_INVALID`].
pub const ERR_STATUS: &str = "authlib-status";
pub const ERR_TOO_LARGE: &str = "authlib-too-large";
pub const ERR_INVALID: &str = "authlib-invalid-manifest";

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct Wire {
    build_number: u32,
    version: String,
    checksums: WireChecksums,
}

#[derive(Deserialize)]
struct WireChecksums {
    sha256: String,
}

pub fn parse_artifact_manifest(response: &HttpResponse) -> Result<ArtifactManifest, &'static str> {
    if !response.is_success() {
        return Err(ERR_STATUS);
    }
    if response.body.len() > MAX_MANIFEST_BODY {
        return Err(ERR_TOO_LARGE);
    }
    let wire: Wire = serde_json::from_str(&response.body).map_err(|_| ERR_INVALID)?;
    if !(1..=999_999).contains(&wire.build_number)
        || !valid_version(&wire.version)
        || !valid_sha256(&wire.checksums.sha256)
    {
        return Err(ERR_INVALID);
    }
    Ok(ArtifactManifest {
        build_number: wire.build_number,
        version: wire.version,
        sha256: wire.checksums.sha256,
    })
}

#[cfg(test)]
#[path = "authlib_tests.rs"]
mod tests;
