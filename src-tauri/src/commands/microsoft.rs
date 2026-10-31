//! Tauri commands for Microsoft sign-in and the accounts it creates.
//!
//! Only IPC glue. The sign-in flows (device code, and the browser flow with a loopback
//! listener), the tokens' storage in the operating system's vault, and every rule that keeps
//! tokens out of what the frontend sees are in `aether_app::microsoft`. Progress reaches the
//! frontend as `login://status` events, sent by [`TauriHost`](super::host::TauriHost).

use aether_app::microsoft::{
    self, BrowserLoginDto, DeviceCodeDto, MicrosoftAccountDto, MicrosoftLoginState,
};

use super::host::{run, run_shared};

/// Whether a sign-in can be started at all (an Azure application id is configured).
#[tauri::command]
pub fn microsoft_login_available() -> bool {
    microsoft::login_available()
}

/// Requests a device code and starts polling in the background. Returns the code to show the
/// user; the outcome arrives as `login://status` events.
#[tauri::command]
pub async fn begin_microsoft_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, MicrosoftLoginState>,
) -> Result<DeviceCodeDto, String> {
    let state = state.inner().clone();
    run_shared(app, move |host| microsoft::begin_device_login(host, &state)).await
}

/// Opens the Microsoft sign-in page in the browser and waits for it to come back
/// (authorization code with PKCE and a one-shot loopback listener). The frontend passes
/// nothing.
#[tauri::command]
pub async fn begin_microsoft_browser_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, MicrosoftLoginState>,
) -> Result<BrowserLoginDto, String> {
    let state = state.inner().clone();
    run_shared(app, move |host| {
        microsoft::begin_browser_login(host, &state)
    })
    .await
}

/// Opens the sign-in page of the running sign-in in the user's browser. The frontend passes
/// nothing: the URL is the one made or received for the current attempt.
#[tauri::command]
pub fn open_microsoft_verification_page(
    state: tauri::State<'_, MicrosoftLoginState>,
) -> Result<(), String> {
    microsoft::open_verification_page(state.inner())
}

/// Stops the running sign-in, if any. Returns whether one was running.
#[tauri::command]
pub fn cancel_microsoft_login(state: tauri::State<'_, MicrosoftLoginState>) -> bool {
    microsoft::cancel_login(state.inner())
}

/// The Microsoft accounts signed in on this computer (metadata only).
#[tauri::command]
pub async fn list_microsoft_accounts(
    app: tauri::AppHandle,
) -> Result<Vec<MicrosoftAccountDto>, String> {
    run(app, |host| microsoft::accounts(host)).await
}

/// A player's skin as a `data:` PNG URL for the account avatar, or `None` when there is no
/// custom skin or it cannot be had. `uuid` is the 32-digit player id; no token is involved.
#[tauri::command]
pub async fn account_skin(app: tauri::AppHandle, uuid: String) -> Result<Option<String>, String> {
    run(app, move |host| aether_app::skins::skin_for(host, &uuid)).await
}

/// Signs an account out: deletes its refresh token from the vault and its metadata.
#[tauri::command]
pub async fn remove_microsoft_account(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run(app, move |host| microsoft::remove_account(host, &id)).await
}
