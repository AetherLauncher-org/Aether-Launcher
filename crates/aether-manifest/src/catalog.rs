//! Pure, bounded parsing of Mojang's Java Edition version_manifest_v2.json.
//! Network access lives in the desktop command, not in this crate.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::loader::validate_version_id;

/// The only catalog endpoint requested by the desktop backend.
pub const VERSION_CATALOG_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
/// Reject unexpectedly large responses before attempting JSON deserialization.
pub const MAX_CATALOG_BYTES: usize = 2 * 1024 * 1024;
const MAX_VERSIONS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionCatalog {
    pub latest: LatestVersions,
    pub versions: Vec<CatalogVersion>,
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("version catalog exceeds the 2 MiB size limit")]
    TooLarge,
    #[error("version catalog contains invalid JSON")]
    InvalidJson(#[source] serde_json::Error),
    #[error("version catalog contains invalid or conflicting entries")]
    InvalidEntries,
}

/// Parse only the official v2 shape; never silently accept a truncated catalog.
/// Mojang may add fields, so unknown JSON fields remain supported.
pub fn parse_version_catalog(input: &[u8]) -> Result<VersionCatalog, CatalogError> {
    if input.len() > MAX_CATALOG_BYTES {
        return Err(CatalogError::TooLarge);
    }
    let catalog: VersionCatalog =
        serde_json::from_slice(input).map_err(CatalogError::InvalidJson)?;
    if catalog.versions.is_empty() || catalog.versions.len() > MAX_VERSIONS {
        return Err(CatalogError::InvalidEntries);
    }

    let mut seen = HashSet::new();
    for version in &catalog.versions {
        let trusted_host = version.url.starts_with("https://piston-meta.mojang.com/")
            || version.url.starts_with("https://launchermeta.mojang.com/");
        if validate_version_id(&version.id).is_err()
            || !seen.insert(version.id.as_str())
            || !trusted_host
            || version.sha1.len() != 40
            || !version.sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
            || version.kind.is_empty()
            || version.release_time.is_empty()
        {
            return Err(CatalogError::InvalidEntries);
        }
    }
    if !seen.contains(catalog.latest.release.as_str())
        || !seen.contains(catalog.latest.snapshot.as_str())
    {
        return Err(CatalogError::InvalidEntries);
    }
    Ok(catalog)
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
