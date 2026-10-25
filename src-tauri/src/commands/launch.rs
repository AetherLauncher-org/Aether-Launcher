//! Offline launch of an already installed official Minecraft game.
//! IPC accepts typed instance IDs and an offline player name, never executable
//! paths, JVM arguments, URLs or filesystem roots from the frontend.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use aether_auth::offline::offline_uuid;
use aether_core::{InstanceId, InstanceState};
use aether_download::assets::verify_installed_assets;
use aether_download::verify::verify_local_artifact;
use aether_download::{ArtifactSource, DownloadArtifact};
use aether_launch::{InstanceLaunchCoordinator, InstanceProcessStatus};
use aether_manifest::load_resolved_version_manifest;
use aether_manifest::rules::Environment;
use aether_storage::{InstanceStore, set_instance_state};
use tauri::Manager;

/// Owned for the lifetime of the Tauri application, not per IPC invocation.
/// The registry prevents a duplicate launch from losing an earlier child handle.
#[derive(Default)]
pub struct LaunchState {
    coordinator: Mutex<Option<InstanceLaunchCoordinator>>,
}

fn parse_id(id: &str) -> Result<InstanceId, String> {
    InstanceId::parse(id).map_err(|_| "invalid instance ID".to_owned())
}

fn instance_store_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("application storage unavailable: {error}"))?;
    std::fs::create_dir_all(base.join("instances"))
        .map_err(|error| format!("instance storage unavailable: {error}"))?;
    Ok(base)
}

/// Resolve the standard official launcher game directory. It is never selected
/// by IPC; this initial implementation intentionally does not run arbitrary imports.
fn installed_game_directory() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|base| base.join(".minecraft"));
    #[cfg(target_os = "linux")]
    let path = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|base| base.join(".minecraft"));
    #[cfg(target_os = "macos")]
    let path = std::env::var_os("HOME").map(PathBuf::from).map(|base| {
        base.join("Library")
            .join("Application Support")
            .join("minecraft")
    });
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    let path: Option<PathBuf> = None;

    let path = path.ok_or_else(|| "cannot locate the official Minecraft directory".to_owned())?;
    if !path.join("versions").is_dir() {
        return Err("no installed official Minecraft versions were found".to_owned());
    }
    std::fs::canonicalize(path).map_err(|error| format!("cannot open installed Minecraft: {error}"))
}

pub(super) fn current_environment() -> Environment {
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

/// Verify the client JAR and assets before considering a previously created
/// instance installed. Library/native verification happens again when launching.
#[tauri::command]
pub async fn activate_installed_instance(app: tauri::AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let id = parse_id(&id)?;
        let store_root = instance_store_root(&app)?;
        let game_root = installed_game_directory()?;
        let instance = InstanceStore::open(&store_root)
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
    })
    .await
    .map_err(|error| format!("installation validation task failed: {error}"))?
}

/// Launch only a verified managed instance, using backend-discovered Java.
/// This MVP supports offline single-player only; Microsoft auth is not faked.
#[tauri::command]
pub async fn start_installed_instance(
    app: tauri::AppHandle,
    id: String,
    offline_name: String,
    memory_mib: u32,
) -> Result<InstanceProcessStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let id = parse_id(&id)?;
        let uuid = offline_uuid(&offline_name).map_err(|error| error.to_string())?;
        let root = instance_store_root(&app)?;
        let game = installed_game_directory()?;
        let mut vars = BTreeMap::new();
        vars.insert("auth_player_name".to_owned(), offline_name);
        vars.insert("auth_uuid".to_owned(), uuid.simple().to_string());
        vars.insert("auth_access_token".to_owned(), "0".to_owned());
        vars.insert("user_type".to_owned(), "legacy".to_owned());
        vars.insert("version_type".to_owned(), "release".to_owned());
        vars.insert("clientid".to_owned(), String::new());
        vars.insert("xuid".to_owned(), String::new());
        vars.insert("launcher_name".to_owned(), "Aether".to_owned());
        vars.insert(
            "launcher_version".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        );

        let runtimes = aether_runtime::discover_java_runtimes();
        let state = app.state::<LaunchState>();
        let mut guard = state
            .coordinator
            .lock()
            .map_err(|_| "game process registry is unavailable".to_owned())?;
        if guard.is_none() {
            *guard = Some(
                InstanceLaunchCoordinator::open(&root, &game).map_err(|error| error.to_string())?,
            );
        }
        guard
            .as_mut()
            .ok_or_else(|| "game launcher unavailable".to_owned())?
            .start(
                id,
                memory_mib,
                &runtimes,
                &current_environment(),
                &vars,
                &[],
            )
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("game launch task failed: {error}"))?
}

/// Stop only a game process launched by this application session.
#[tauri::command]
pub async fn stop_installed_instance(
    app: tauri::AppHandle,
    id: String,
) -> Result<InstanceProcessStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let id = parse_id(&id)?;
        let state = app.state::<LaunchState>();
        let mut guard = state
            .coordinator
            .lock()
            .map_err(|_| "game process registry is unavailable".to_owned())?;
        guard
            .as_mut()
            .ok_or_else(|| "no game was started in this session".to_owned())?
            .stop(id)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("game stop task failed: {error}"))?
}
