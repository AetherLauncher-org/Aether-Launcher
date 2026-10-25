//! Verify installed Minecraft assets offline; no downloads or file mutations.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::verify::{ArtifactVerificationError, verify_local_artifact};
use crate::{ArtifactSource, DownloadArtifact, is_safe_relative_path};

const MAX_INDEX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_OBJECTS: usize = 250_000;
const MAX_OBJECT_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_NAME_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAssets {
    pub index_file: PathBuf,
    pub objects_verified: usize,
    pub bytes_verified: u64,
}

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("invalid asset index identifier")]
    UnsafeIndexId,
    #[error("invalid asset object hash for {0:?}")]
    InvalidHash(String),
    #[error("asset index is too large")]
    IndexTooLarge,
    #[error("asset index contains too many entries")]
    TooManyObjects,
    #[error("asset index contains an invalid logical object name")]
    InvalidObjectName,
    #[error("asset object is larger than supported limit")]
    ObjectTooLarge,
    #[error("asset objects with the same hash have contradictory sizes")]
    ConflictingObjectSizes,
    #[error("asset objects total size exceeds supported limit")]
    TotalSizeOverflow,
    #[error("invalid asset index JSON: {0}")]
    InvalidIndex(#[from] serde_json::Error),
    #[error("cannot read verified asset index: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Verification(#[from] ArtifactVerificationError),
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

/// Validate an asset index and all content-addressed objects with local SHA-1.
/// Object destinations come exclusively from validated digests, never from
/// untrusted logical filenames inside the index.
pub fn verify_installed_assets(
    game_directory: &Path,
    index_id: &str,
    index_sha1: &str,
    index_size: u64,
) -> Result<VerifiedAssets, AssetError> {
    if !is_safe_relative_path(Path::new(index_id)) || index_id.contains(['/', '\\']) {
        return Err(AssetError::UnsafeIndexId);
    }
    if index_size > MAX_INDEX_BYTES {
        return Err(AssetError::IndexTooLarge);
    }
    if !valid_sha1(index_sha1) {
        return Err(AssetError::InvalidHash(index_id.to_owned()));
    }
    let index_artifact = DownloadArtifact {
        destination: PathBuf::from("assets")
            .join("indexes")
            .join(format!("{index_id}.json")),
        size: index_size,
        sha1: index_sha1.to_owned(),
        sources: vec![ArtifactSource {
            provider: "mojang".to_owned(),
            url: "https://resources.download.minecraft.net/".to_owned(),
        }],
    };
    let index_file = verify_local_artifact(game_directory, &index_artifact)?;
    let index_bytes = fs::read(&index_file)?;
    if index_bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err(AssetError::IndexTooLarge);
    }
    let index: AssetIndex = serde_json::from_slice(&index_bytes)?;
    if index.objects.len() > MAX_OBJECTS {
        return Err(AssetError::TooManyObjects);
    }

    let mut distinct = HashMap::<String, u64>::new();
    let mut total = 0_u64;
    for (logical_name, object) in index.objects {
        if logical_name.is_empty()
            || logical_name.len() > MAX_NAME_BYTES
            || logical_name.chars().any(char::is_control)
        {
            return Err(AssetError::InvalidObjectName);
        }
        if !valid_sha1(&object.hash) {
            return Err(AssetError::InvalidHash(logical_name));
        }
        if object.size > MAX_OBJECT_BYTES {
            return Err(AssetError::ObjectTooLarge);
        }
        total = total
            .checked_add(object.size)
            .ok_or(AssetError::TotalSizeOverflow)?;
        if let Some(existing) = distinct.insert(object.hash.to_ascii_lowercase(), object.size)
            && existing != object.size
        {
            return Err(AssetError::ConflictingObjectSizes);
        }
    }

    let mut bytes_verified = 0_u64;
    for (hash, size) in &distinct {
        let object_artifact = DownloadArtifact {
            destination: PathBuf::from("assets")
                .join("objects")
                .join(&hash[..2])
                .join(hash),
            size: *size,
            sha1: hash.clone(),
            sources: vec![ArtifactSource {
                provider: "mojang".to_owned(),
                url: "https://resources.download.minecraft.net/".to_owned(),
            }],
        };
        verify_local_artifact(game_directory, &object_artifact)?;
        bytes_verified = bytes_verified
            .checked_add(*size)
            .ok_or(AssetError::TotalSizeOverflow)?;
    }
    Ok(VerifiedAssets {
        index_file,
        objects_verified: distinct.len(),
        bytes_verified,
    })
}

fn valid_sha1(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
#[path = "assets/assets_tests.rs"]
mod tests;
