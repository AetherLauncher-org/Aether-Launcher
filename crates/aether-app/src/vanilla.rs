//! Cache an official Mojang version JSON in the application's private game tree.
//!
//! The catalog fixes both the metadata URL and expected SHA-1. Neither a URL
//! nor a destination path may be provided by the frontend. This is deliberately
//! separate from installing client JARs, libraries, assets and natives.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_download::transport::{HttpTransport, TransportRequest};
use aether_download::verify_reader;
use aether_manifest::catalog::VersionCatalog;
use aether_manifest::parse_version_manifest;

use super::catalog::fetch_catalog;

const MAX_VERSION_JSON_BYTES: usize = 8 * 1024 * 1024;
const METADATA_TIMEOUT_SECS: u64 = 20;
static STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// The caller passes a version ID, never a URL. The catalog was freshly
/// validated against a fixed Mojang endpoint before this function is called.
fn fetch_verified_metadata(
    transport: &dyn HttpTransport,
    catalog: &VersionCatalog,
    version: &str,
) -> Result<Vec<u8>, String> {
    let selected = catalog
        .versions
        .iter()
        .find(|item| item.id == version)
        .ok_or_else(|| "version is absent from the official Mojang catalog".to_owned())?;

    let response = transport
        .get(&TransportRequest {
            url: selected.url.clone(),
            range: None,
            timeout_secs: METADATA_TIMEOUT_SECS,
        })
        .map_err(|error| format!("cannot retrieve official version JSON: {error}"))?;
    if response
        .content_length
        .is_some_and(|length| length > MAX_VERSION_JSON_BYTES as u64)
    {
        return Err("version JSON exceeds the 8 MiB limit".to_owned());
    }

    let mut bytes = Vec::new();
    response
        .body
        .take((MAX_VERSION_JSON_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read official version JSON".to_owned())?;
    if bytes.len() > MAX_VERSION_JSON_BYTES {
        return Err("version JSON exceeds the 8 MiB limit".to_owned());
    }

    let expected_sha1 = selected.sha1.to_ascii_lowercase();
    verify_reader(bytes.as_slice(), bytes.len() as u64, &expected_sha1)
        .map_err(|_| "official version JSON failed SHA-1 verification".to_owned())?;

    let json =
        std::str::from_utf8(&bytes).map_err(|_| "official version JSON is not UTF-8".to_owned())?;
    let parsed = parse_version_manifest(json)
        .map_err(|_| "official version JSON has an invalid schema".to_owned())?;
    if parsed.id != version || parsed.inherits_from.is_some() {
        return Err("version JSON declares an unexpected version or parent".to_owned());
    }
    // This command is only the first Vanilla install stage. Do not cache
    // loader manifests or versions without a declared official client.
    let client = parsed
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.client.as_ref())
        .ok_or_else(|| "version JSON has no official client download".to_owned())?;
    if !client.url.starts_with("https://")
        || client.sha1.len() != 40
        || !client.sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
        || client.size == 0
    {
        return Err("version JSON has invalid client download metadata".to_owned());
    }
    Ok(bytes)
}

/// Reject an existing symlink or non-directory, and create a missing folder.
/// The data root must be exclusively managed by the trusted launcher process.
fn ensure_private_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err("installer destination contains a link or non-directory".to_owned())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|_| "cannot create installer directory".to_owned())
        }
        Err(_) => Err("cannot inspect installer directory".to_owned()),
    }
}

/// Publish only verified bytes, never overwriting an existing cached manifest.
/// An invalid existing file is reported for explicit repair rather than
/// replacing arbitrary files in the destination tree.
fn cache_verified_metadata(
    game_root: &Path,
    version: &str,
    bytes: &[u8],
    expected_sha1: &str,
) -> Result<(), String> {
    // Version identifiers and digests are derived from parse_version_catalog.
    // Do not infer file destinations from any JSON-provided path.
    ensure_private_directory(game_root)?;
    let versions = game_root.join("versions");
    ensure_private_directory(&versions)?;
    let directory = versions.join(version);
    ensure_private_directory(&directory)?;
    let destination = directory.join(format!("{version}.json"));

    match fs::symlink_metadata(&destination) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("cached version JSON is not a regular file".to_owned());
            }
            let cached = fs::File::open(&destination)
                .map_err(|_| "cannot read cached version JSON".to_owned())?;
            verify_reader(
                cached,
                bytes.len() as u64,
                &expected_sha1.to_ascii_lowercase(),
            )
            .map_err(|_| "cached version JSON is damaged; repair required".to_owned())?;
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("cannot inspect cached version JSON".to_owned()),
    }

    let sequence = STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let staging = directory.join(format!(
        ".aether-{}-{sequence}.json.tmp",
        std::process::id()
    ));
    let result = (|| -> Result<(), String> {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .map_err(|_| "cannot create private staging file".to_owned())?;
        output
            .write_all(bytes)
            .and_then(|()| output.sync_all())
            .map_err(|_| "cannot persist version JSON".to_owned())?;
        drop(output);
        // Only the launcher modifies its private game tree. A second check
        // prevents replacing an existing file during normal operation.
        if fs::symlink_metadata(&destination).is_ok() {
            return Err("version JSON was created concurrently".to_owned());
        }
        fs::rename(&staging, &destination)
            .map_err(|_| "cannot publish verified version JSON".to_owned())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

// still used by src-tauri's modloader/import until they move
pub fn prepare_version(
    transport: &dyn HttpTransport,
    game_root: &Path,
    version: &str,
) -> Result<String, String> {
    let catalog = fetch_catalog(transport)?;
    prepare_version_from_catalog(transport, &catalog, game_root, version)
}

/// Same as [`prepare_version`] for a catalog the caller already holds, so
/// installing many versions in a row reads the catalog once.
pub(super) fn prepare_version_from_catalog(
    transport: &dyn HttpTransport,
    catalog: &VersionCatalog,
    game_root: &Path,
    version: &str,
) -> Result<String, String> {
    let selected = catalog
        .versions
        .iter()
        .find(|item| item.id == version)
        .ok_or_else(|| "version is absent from the official Mojang catalog".to_owned())?;
    let digest = selected.sha1.clone();
    let bytes = fetch_verified_metadata(transport, catalog, version)?;
    cache_verified_metadata(game_root, version, &bytes, &digest)?;
    Ok(version.to_owned())
}

#[cfg(test)]
#[path = "vanilla_tests.rs"]
mod tests;
