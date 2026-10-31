//! Offline launch of an instance installed into the launcher-owned game tree.
//! IPC accepts typed instance IDs and an offline player name, never executable
//! paths, JVM arguments, URLs or filesystem roots from the frontend.

use aether_app::launch::{self, LaunchState};
use aether_launch::InstanceProcessStatus;
use tauri::Manager;

use super::host::run;

/// Launch only a verified managed instance, using backend-discovered Java.
///
/// With `account_id` the game starts as that signed-in Microsoft account: the backend
/// refreshes the sign-in and passes a fresh Minecraft token, which never crosses IPC.
/// Without it the game starts offline under `offline_name`; `account_uuid` is optional
/// there, and without it the standard offline UUID for the player name is used.
#[tauri::command]
pub(crate) async fn start_installed_instance(
    app: tauri::AppHandle,
    id: String,
    offline_name: String,
    memory_mib: u32,
    account_uuid: Option<String>,
    account_id: Option<String>,
) -> Result<InstanceProcessStatus, String> {
    let handle = app.clone();
    run(app, move |host| {
        launch::start(
            host,
            &handle.state::<LaunchState>(),
            &id,
            &offline_name,
            memory_mib,
            account_uuid.as_deref(),
            account_id.as_deref(),
        )
    })
    .await
}

/// Current state of a game started in this session; `None` when this session
/// never started the instance. Lets the UI notice when the game exits.
#[tauri::command]
pub(crate) async fn game_status(
    app: tauri::AppHandle,
    id: String,
) -> Result<Option<InstanceProcessStatus>, String> {
    let handle = app.clone();
    run(app, move |_host| {
        launch::status(&handle.state::<LaunchState>(), &id)
    })
    .await
}

/// Stop only a game process launched by this application session.
#[tauri::command]
pub(crate) async fn stop_installed_instance(
    app: tauri::AppHandle,
    id: String,
) -> Result<InstanceProcessStatus, String> {
    let handle = app.clone();
    run(app, move |_host| {
        launch::stop(&handle.state::<LaunchState>(), &id)
    })
    .await
}
