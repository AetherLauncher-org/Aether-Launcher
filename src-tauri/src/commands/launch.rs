//! Offline launch of an instance installed into the launcher-owned game tree.
//! IPC accepts typed instance IDs and an offline player name, never executable
//! paths, JVM arguments, URLs or filesystem roots from the frontend.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use aether_app::launch::current_environment;
use aether_auth::offline::{offline_uuid, validate_player_name};
use aether_core::InstanceId;
use aether_launch::{
    CoordinatorError, InstanceLaunchCoordinator, InstanceProcessStatus, InstanceRunState,
    RegistryError,
};
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

/// The launcher-owned game tree that the installer fills. It is never selected
/// by IPC and never points at an existing official `.minecraft`.
fn installed_game_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|_| "application data directory is unavailable".to_owned())?
        .join("game");
    if !path.join("versions").is_dir() {
        return Err(
            "no installed Minecraft versions were found; install a version first".to_owned(),
        );
    }
    std::fs::canonicalize(path).map_err(|error| format!("cannot open installed Minecraft: {error}"))
}

/// Normalise a user-supplied account UUID to 32 lowercase hex digits. Accepts
/// the plain 32-digit form and the 8-4-4-4-12 dashed form.
pub(super) fn normalize_uuid(text: &str) -> Result<String, String> {
    let text = text.trim();
    let compact: String = if text.len() == 36 {
        let groups: Vec<&str> = text.split('-').collect();
        let lengths: Vec<usize> = groups.iter().map(|group| group.len()).collect();
        if lengths != [8, 4, 4, 4, 12] {
            return Err("invalid account UUID".to_owned());
        }
        groups.concat()
    } else {
        text.to_owned()
    };
    if compact.len() != 32 || !compact.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid account UUID".to_owned());
    }
    Ok(compact.to_ascii_lowercase())
}

/// Launch only a verified managed instance, using backend-discovered Java.
///
/// With `account_id` the game starts as that signed-in Microsoft account: the backend
/// refreshes the sign-in and passes a fresh Minecraft token, which never crosses IPC.
/// Without it the game starts offline under `offline_name`; `account_uuid` is optional
/// there, and without it the standard offline UUID for the player name is used.
#[tauri::command]
pub async fn start_installed_instance(
    app: tauri::AppHandle,
    id: String,
    offline_name: String,
    memory_mib: u32,
    account_uuid: Option<String>,
    account_id: Option<String>,
) -> Result<InstanceProcessStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let id = parse_id(&id)?;
        let mut vars = BTreeMap::new();
        match account_id {
            Some(account) => {
                let credentials = aether_app::microsoft::launch_credentials_for(
                    &super::host::TauriHost::new(app.clone()),
                    &account,
                )?;
                vars.insert("auth_player_name".to_owned(), credentials.name);
                vars.insert("auth_uuid".to_owned(), credentials.uuid);
                vars.insert(
                    "auth_access_token".to_owned(),
                    credentials.access_token.expose().to_owned(),
                );
                vars.insert("user_type".to_owned(), "msa".to_owned());
                vars.insert("xuid".to_owned(), credentials.xuid.clone());
                vars.insert("auth_xuid".to_owned(), credentials.xuid);
            }
            None => {
                validate_player_name(&offline_name).map_err(|error| error.to_string())?;
                let uuid = match account_uuid {
                    Some(text) => normalize_uuid(&text)?,
                    None => offline_uuid(&offline_name)
                        .map_err(|error| error.to_string())?
                        .simple()
                        .to_string(),
                };
                vars.insert("auth_player_name".to_owned(), offline_name);
                vars.insert("auth_uuid".to_owned(), uuid);
                vars.insert("auth_access_token".to_owned(), "0".to_owned());
                vars.insert("user_type".to_owned(), "legacy".to_owned());
                vars.insert("xuid".to_owned(), String::new());
                vars.insert("auth_xuid".to_owned(), String::new());
            }
        }
        let root = instance_store_root(&app)?;
        let game = installed_game_directory(&app)?;
        vars.insert("version_type".to_owned(), "release".to_owned());
        vars.insert("clientid".to_owned(), String::new());
        vars.insert("launcher_name".to_owned(), "Aether".to_owned());
        vars.insert(
            "launcher_version".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        );

        let runtimes = aether_runtime::discover_java_runtimes(&[]);
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

/// Whether this session has a running game for the instance. Managing an instance's files
/// (deleting it, switching mods) is refused while its game runs.
pub(super) fn is_instance_running(app: &tauri::AppHandle, id: InstanceId) -> Result<bool, String> {
    let state = app.state::<LaunchState>();
    let mut guard = state
        .coordinator
        .lock()
        .map_err(|_| "game process registry is unavailable".to_owned())?;
    let Some(coordinator) = guard.as_mut() else {
        return Ok(false);
    };
    match coordinator.status(id) {
        Ok(status) => Ok(status.state == InstanceRunState::Running),
        Err(CoordinatorError::Registry(RegistryError::UnknownInstance)) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// Current state of a game started in this session; `None` when this session
/// never started the instance. Lets the UI notice when the game exits.
#[tauri::command]
pub async fn game_status(
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<InstanceProcessStatus>, String> {
    let id = parse_id(&id)?;
    let state = app.state::<LaunchState>();
    let mut guard = state
        .coordinator
        .lock()
        .map_err(|_| "game process registry is unavailable".to_owned())?;
    let Some(coordinator) = guard.as_mut() else {
        return Ok(None);
    };
    match coordinator.status(id) {
        Ok(status) => Ok(Some(status)),
        Err(CoordinatorError::Registry(RegistryError::UnknownInstance)) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
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

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
