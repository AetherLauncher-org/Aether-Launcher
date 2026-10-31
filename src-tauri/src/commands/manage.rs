//! Tauri commands for managing an installed instance: details, rename, delete, its content
//! folders (mods, packs, shaders, worlds, schematics), its logs, and opening its folders.
//!
//! These only turn arguments and results into IPC. The operations, and the rules that guard
//! them (no paths from the frontend, nothing changes while the game runs, logs are redacted),
//! are in `aether_app::manage`.

use aether_app::identify::{self, Lookup};
use aether_app::instances::InstanceSummary;
use aether_app::manage::{self, ContentItem, InstanceDetails, LogContent, LogEntry};

use super::host::run;

#[tauri::command]
pub async fn instance_details(
    app: tauri::AppHandle,
    id: String,
) -> Result<InstanceDetails, String> {
    run(app, move |host| manage::instance_details(host, &id)).await
}

/// Renames an instance. Only the display name changes; the folder keeps its UUID.
#[tauri::command]
pub async fn rename_installed_instance(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<InstanceSummary, String> {
    run(app, move |host| manage::rename(host, &id, &name)).await
}

/// Deletes an instance and everything in its folder. A running instance cannot be deleted.
#[tauri::command]
pub async fn delete_installed_instance(app: tauri::AppHandle, id: String) -> Result<(), String> {
    run(app, move |host| manage::delete(host, &id)).await
}

#[tauri::command]
pub async fn list_instance_content(
    app: tauri::AppHandle,
    id: String,
    kind: String,
) -> Result<Vec<ContentItem>, String> {
    run(app, move |host| manage::list_content(host, &id, &kind)).await
}

/// Looks the mods, resource packs or shader packs of an instance up on Modrinth by file hash.
/// Only hashes leave the machine; the answer is cached, and with no network the cache answers.
#[tauri::command]
pub async fn identify_instance_content(
    app: tauri::AppHandle,
    id: String,
    kind: String,
) -> Result<Lookup, String> {
    run(app, move |host| identify::identify(host, &id, &kind)).await
}

/// Opens a project's page on modrinth.com. The address is built from a slug in the backend.
#[tauri::command]
pub async fn open_content_project(slug: String) -> Result<(), String> {
    identify::open_project_page(&slug)
}

/// Switches a mod on or off (`x.jar` ↔ `x.jar.disabled`).
#[tauri::command]
pub async fn set_instance_mod_enabled(
    app: tauri::AppHandle,
    id: String,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    run(app, move |host| {
        manage::set_mod_enabled(host, &id, &name, enabled)
    })
    .await
}

/// Deletes one item from a content folder.
#[tauri::command]
pub async fn remove_instance_content(
    app: tauri::AppHandle,
    id: String,
    kind: String,
    name: String,
) -> Result<(), String> {
    run(app, move |host| {
        manage::remove_content(host, &id, &kind, &name)
    })
    .await
}

#[tauri::command]
pub async fn list_instance_logs(
    app: tauri::AppHandle,
    id: String,
) -> Result<Vec<LogEntry>, String> {
    run(app, move |host| manage::list_logs(host, &id)).await
}

/// Reads a log or crash report with names, tokens, e-mail and IP addresses redacted.
#[tauri::command]
pub async fn read_instance_log(
    app: tauri::AppHandle,
    id: String,
    kind: String,
    name: Option<String>,
) -> Result<LogContent, String> {
    run(app, move |host| manage::read_log(host, &id, &kind, name)).await
}

/// Opens one of the instance's folders in the file manager.
#[tauri::command]
pub async fn open_instance_folder(
    app: tauri::AppHandle,
    id: String,
    target: String,
) -> Result<(), String> {
    run(app, move |host| manage::open_folder(host, &id, &target)).await
}
