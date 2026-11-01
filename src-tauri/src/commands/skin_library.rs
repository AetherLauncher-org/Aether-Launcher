//! Tauri commands for the skin library: skins kept on this machine, added from a file the
//! player picks, from a player's name, and renamed, given a model, or removed.
//!
//! These only turn arguments and results into IPC. A skin is named by its id (the SHA-256 of
//! its picture); no path crosses the boundary, and every picture is checked in
//! `aether_app::skin_library` before it is kept or shown.

use aether_app::skin_library::{self, AddResult, SkinEntry};
use aether_app::skin_sources::{self, SavedSkin, SkinDownloadState, SkinPreview, SkinSource};
use tauri::Manager;

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
pub async fn add_skin_from_player(
    app: tauri::AppHandle,
    name: String,
) -> Result<AddResult, String> {
    run(app, move |host| skin_library::add_from_player(host, &name)).await
}

#[tauri::command]
pub async fn rename_skin(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<SkinEntry, String> {
    run(app, move |host| skin_library::rename(host, &id, &name)).await
}

#[tauri::command]
pub async fn set_skin_model(
    app: tauri::AppHandle,
    id: String,
    slim: bool,
) -> Result<SkinEntry, String> {
    run(app, move |host| skin_library::set_model(host, &id, slim)).await
}

/// Makes a library skin the skin of a signed-in Microsoft account. The account's token stays in
/// the backend.
#[tauri::command]
pub async fn wear_skin(
    app: tauri::AppHandle,
    id: String,
    account_id: String,
) -> Result<(), String> {
    run(app, move |host| {
        aether_app::skin_wear::wear(host, &id, &account_id)
    })
    .await
}

#[tauri::command]
pub async fn remove_skin(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run(app, move |host| skin_library::remove(host, &id)).await
}

/// Finds the skin a player wears on one of the documented sources (Mojang, LittleSkin,
/// Ely.by). The result is kept behind an opaque token; no address crosses the boundary.
#[tauri::command]
pub async fn search_skin(
    app: tauri::AppHandle,
    source: String,
    name: String,
) -> Result<SkinPreview, String> {
    let source = SkinSource::parse(&source).ok_or("unknown skin source".to_owned())?;
    let handle = app.clone();
    run(app, move |_host| {
        skin_sources::search(&handle.state::<SkinDownloadState>(), source, &name)
    })
    .await
}

/// Adds a skin found on the download page to the library, recorded as `download`.
#[tauri::command]
pub async fn add_downloaded_skin(
    app: tauri::AppHandle,
    token: String,
) -> Result<AddResult, String> {
    let handle = app.clone();
    run(app, move |host| {
        skin_sources::add_downloaded(host, &handle.state::<SkinDownloadState>(), &token)
    })
    .await
}

/// Saves a skin found on the download page into a folder the user picks. `null` when the
/// dialog is cancelled; the folder's path never reaches the page.
#[tauri::command]
pub async fn save_downloaded_skin(
    app: tauri::AppHandle,
    token: String,
) -> Result<Option<SavedSkin>, String> {
    let handle = app.clone();
    run(app, move |host| {
        skin_sources::save_downloaded(host, &handle.state::<SkinDownloadState>(), &token)
    })
    .await
}

/// Opens NameMC, Skindex or Planet Minecraft by key. Only these three fixed addresses can
/// be opened; the page cannot name a URL.
#[tauri::command]
pub async fn open_skin_site(site: String) -> Result<(), String> {
    skin_sources::open_skin_site(&site)
}
