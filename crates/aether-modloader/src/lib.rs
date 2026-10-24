use serde::{Deserialize, Serialize};

pub mod fabric;
pub mod forge;
pub mod installer;
pub mod json;
pub mod neoforge;
pub mod quilt;
pub mod transport;

pub use installer::{InstallStep, MAX_VERSION_ID_LEN, ModLoaderInstallPlan, is_safe_version_id};
pub use transport::{MetaError, MetaFetcher, MetaSource, StaticFetcher};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModLoaderKind {
    Fabric,
    Forge,
    NeoForge,
    Quilt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModLoaderVersion {
    pub kind: ModLoaderKind,
    pub version: String,
    pub game_version: String,
}
