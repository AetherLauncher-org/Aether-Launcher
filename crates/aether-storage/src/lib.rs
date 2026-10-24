pub mod instances;
pub use instances::{InstanceLoadError, InstanceStore};

use aether_core::GameInstance;
use thiserror::Error;

pub fn encode_instance(instance: &GameInstance) -> Result<String, StorageError> {
    serde_json::to_string_pretty(instance).map_err(StorageError::Serialize)
}

pub fn decode_instance(input: &str) -> Result<GameInstance, StorageError> {
    serde_json::from_str(input).map_err(StorageError::Deserialize)
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to serialize game instance")]
    Serialize(#[source] serde_json::Error),
    #[error("failed to deserialize game instance")]
    Deserialize(#[source] serde_json::Error),
}

pub mod create;
pub mod import;
pub mod list;
pub mod update;

pub use create::{CreateError, create_instance};
pub use import::{
    ExistingFiles, ImportCandidate, ImportError, ImportSourceKind, detect, plan_candidates,
    scan_directory,
};
pub use list::{
    InstanceListing, ListError, MAX_INSTANCE_RECORD_BYTES, SkipReason, SkippedInstance,
    list_instances,
};
pub use update::{
    UpdateError, rename_instance, set_instance_state, set_instance_version, validate_instance_name,
    write_instance_atomic,
};
