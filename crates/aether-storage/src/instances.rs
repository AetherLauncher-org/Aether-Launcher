//! Read installed instances from an application-owned directory by UUID.
//! Untrusted arbitrary paths never become a lookup argument.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use aether_core::{GameInstance, InstanceId};
use thiserror::Error;

use crate::{StorageError, decode_instance};

const MAX_INSTANCE_BYTES: u64 = 128 * 1024;

#[derive(Debug, Error)]
pub enum InstanceLoadError {
    #[error("instance store must contain an application-owned instances directory")]
    InvalidStore,
    #[error("instance directory does not match its expected UUID location")]
    UnsafeDirectory,
    #[error("instance record is not a regular file or is a symlink")]
    UnsafeRecord,
    #[error("instance record exceeds 128 KiB")]
    TooLarge,
    #[error("instance record UUID does not match requested instance")]
    IdMismatch,
    #[error("instance root does not match its managed directory")]
    RootMismatch,
    #[error("cannot read instance record: {0}")]
    Io(#[from] io::Error),
    #[error("cannot parse instance record: {0}")]
    Decode(#[from] StorageError),
}

/// The store's base is configured by the backend, *not* supplied via IPC.
/// The layout is <base>/instances/<uuid>/instance.json.
pub struct InstanceStore {
    instances_directory: PathBuf,
}

impl InstanceStore {
    pub fn open(base: &Path) -> Result<Self, InstanceLoadError> {
        let base = fs::canonicalize(base)?;
        let instances = base.join("instances");
        let metadata =
            fs::symlink_metadata(&instances).map_err(|_| InstanceLoadError::InvalidStore)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(InstanceLoadError::InvalidStore);
        }
        let instances_directory = fs::canonicalize(&instances)?;
        if !instances_directory.starts_with(&base) {
            return Err(InstanceLoadError::InvalidStore);
        }
        Ok(Self {
            instances_directory,
        })
    }

    /// Load a single instance using a typed UUID, rejecting redirects through
    /// symlinks and mismatched instance identities/roots.
    pub fn load(&self, id: InstanceId) -> Result<GameInstance, InstanceLoadError> {
        let directory = self.instances_directory.join(id.to_string());
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(InstanceLoadError::UnsafeDirectory);
        }
        let canonical_directory = fs::canonicalize(&directory)?;
        if !canonical_directory.starts_with(&self.instances_directory) {
            return Err(InstanceLoadError::UnsafeDirectory);
        }

        let file = directory.join("instance.json");
        let metadata = fs::symlink_metadata(&file)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(InstanceLoadError::UnsafeRecord);
        }
        if metadata.len() > MAX_INSTANCE_BYTES {
            return Err(InstanceLoadError::TooLarge);
        }

        let bytes = fs::read(&file)?;
        if bytes.len() as u64 > MAX_INSTANCE_BYTES {
            return Err(InstanceLoadError::TooLarge);
        }
        let input = std::str::from_utf8(&bytes).map_err(|error| {
            InstanceLoadError::Io(io::Error::new(io::ErrorKind::InvalidData, error))
        })?;
        let instance = decode_instance(input)?;
        if instance.id != id {
            return Err(InstanceLoadError::IdMismatch);
        }

        let actual_root = fs::canonicalize(&instance.root)?;
        if actual_root != canonical_directory {
            return Err(InstanceLoadError::RootMismatch);
        }
        Ok(instance)
    }
}

#[cfg(test)]
#[path = "instances/instances_tests.rs"]
mod tests;
