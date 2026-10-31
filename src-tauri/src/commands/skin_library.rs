//! Tauri commands for the skin library: skins kept on this machine, added from a file the
//! player picks, from a player's name, and renamed, given a model, or removed.
//!
//! These only turn arguments and results into IPC. A skin is named by its id (the SHA-256 of
//! its picture); no path crosses the boundary, and every picture is checked in
//! `aether_app::skin_library` before it is kept or shown.

use aether_app::skin_library::{self, AddResult, SkinEntry};

use super::host::run;

#[tauri::command]
pub async fn list_skins(app: tauri::AppHandle) -> Result<Vec<SkinEntry>, String> {
    run(app, |host| skin_library::list(host)).await
}

/// A skin's picture as a `data:` URL.
#[tauri::command]
pub async fn skin_library_image(app: tauri::AppHandle, id: String) -> Result<String, String> {
    run(app, move |host| skin_library::image(host, &id)).await
}

/// Adds a skin from a PNG the player picks in the system dialog. `null` when cancelled.
#[tauri::command]
pub async fn add_skin_from_file(app: tauri::AppHandle) -> Result<Option<AddResult>, String> {
    run(app, |host| skin_library::add_from_file(host)).await
}

/// Adds the skin a player wears, found by name.
#[tauri::command]
pub async fn add_skin_from_player(app: tauri::AppHandle, name: String) -> Result<AddResult, String> {
    run(app, move |host| skin_library::add_from_player(host, &name)).await
}

#[tauri::command]
pub async fn rename_skin(app: tauri::AppHandle, id: String, name: String) -> Result<SkinEntry, String> {
    run(app, move |host| skin_library::rename(host, &id, &name)).await
}

#[tauri::command]
pub async fn set_skin_model(app: tauri::AppHandle, id: String, slim: bool) -> Result<SkinEntry, String> {
    run(app, move |host| skin_library::set_model(host, &id, slim)).await
}

#[tauri::command]
pub async fn remove_skin(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run(app, move |host| skin_library::remove(host, &id)).await
}
