use std::path::{Path, PathBuf};

use aether_core::{InstanceId, InstanceState};
use aether_download::assets::verify_installed_assets;
use aether_download::verify::verify_local_artifact;
use aether_download::{ArtifactSource, DownloadArtifact};
use aether_manifest::load_resolved_version_manifest;
use aether_manifest::rules::Environment;
use aether_storage::{InstanceStore, set_instance_state};

pub fn current_environment() -> Environment {
    Environment {
        os_name: if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "macos") {
            "osx"
        } else {
            "linux"
        }
        .to_owned(),
        os_arch: std::env::consts::ARCH.to_owned(),
        os_version: String::new(),
        features: Default::default(),
    }
}

/// Verify the client JAR and assets of an instance and mark it ready.
/// Library and native verification happens again when launching.
pub fn activate_instance(
    store_root: &Path,
    game_root: &Path,
    id: InstanceId,
) -> Result<(), String> {
    let game_root = std::fs::canonicalize(game_root)
        .map_err(|error| format!("cannot open installed Minecraft: {error}"))?;
    let instance = InstanceStore::open(store_root)
        .map_err(|error| error.to_string())?
        .load(id)
        .map_err(|error| error.to_string())?;
    let resolved = load_resolved_version_manifest(&game_root, instance.version.as_str())
        .map_err(|error| error.to_string())?;
    let client_version = resolved
        .client_jar_version
        .ok_or_else(|| "installed version has no verified client JAR metadata".to_owned())?;
    let client = resolved
        .manifest
        .downloads
        .as_ref()
        .and_then(|download| download.client.as_ref())
        .ok_or_else(|| "installed version has no client metadata".to_owned())?;
    let artifact = DownloadArtifact {
        destination: PathBuf::from("versions")
            .join(&client_version)
            .join(format!("{client_version}.jar")),
        size: client.size,
        sha1: client.sha1.clone(),
        sources: vec![ArtifactSource {
            provider: "mojang".to_owned(),
            url: client.url.clone(),
        }],
    };
    verify_local_artifact(&game_root, &artifact).map_err(|error| error.to_string())?;
    if let Some(index) = &resolved.manifest.asset_index {
        verify_installed_assets(&game_root, &index.id, &index.sha1, index.size)
            .map_err(|error| error.to_string())?;
    }
    if instance.state != InstanceState::Ready {
        set_instance_state(&instance.root, &instance, InstanceState::Ready)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
