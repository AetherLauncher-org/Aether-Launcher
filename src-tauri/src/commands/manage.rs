//! Managing one installed instance: details, rename, delete, its content folders (mods,
//! packs, shaders, worlds, schematics), its logs, and opening its folders.
//!
//! The frontend names an instance by id and a folder by a fixed label (`mods`, `saves`,
//! `logs`, …), and an item by a plain file name. It never supplies a path: the backend
//! resolves every folder from the verified instance record (`aether_storage::manage`), and
//! the only thing it hands to the operating system is a folder it built itself.
//!
//! Logs and crash reports are untrusted text that may hold the player's name, UUID, access
//! token fragments, e-mail addresses or IP addresses. They are redacted
//! (`aether_diagnose::redacted`) before they reach the frontend.

use std::path::{Path, PathBuf};

use aether_core::{GameInstance, InstanceId};
use aether_storage::manage::{
    ContentKind, LogSource, count_content, delete_instance, directory_size, game_directory,
    is_plain_name, list_content, list_logs, read_log, remove_content, set_mod_enabled,
};
use aether_storage::{InstanceStore, rename_instance};
use serde::Serialize;
use tauri::Manager;

use super::instances::InstanceSummary;
use super::launch::is_instance_running;

fn parse_id(id: &str) -> Result<InstanceId, String> {
    InstanceId::parse(id).map_err(|_| "invalid instance ID".to_owned())
}

fn store_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|_| "application data directory is unavailable".to_owned())?;
    std::fs::create_dir_all(base.join("instances"))
        .map_err(|_| "cannot prepare the instance store".to_owned())?;
    Ok(base)
}

fn load(app: &tauri::AppHandle, id: &str) -> Result<(PathBuf, GameInstance), String> {
    let id = parse_id(id)?;
    let base = store_root(app)?;
    let instance = InstanceStore::open(&base)
        .and_then(|store| store.load(id))
        .map_err(|error| format!("cannot open the instance: {error}"))?;
    Ok((base, instance))
}

fn parse_kind(label: &str) -> Result<ContentKind, String> {
    ContentKind::parse(label).ok_or_else(|| "unknown content folder".to_owned())
}

// --------------------------------------------------------------------------------------
// Details
// --------------------------------------------------------------------------------------

/// How many items each content folder holds.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ContentCounts {
    pub mods: usize,
    pub resourcepacks: usize,
    pub shaderpacks: usize,
    pub saves: usize,
    pub schematics: usize,
}

/// What the instance page shows at the top. No path of any kind.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDetails {
    pub id: String,
    pub name: String,
    pub version: String,
    pub state: &'static str,
    /// `Fabric 0.16.5`, `Forge 52.0.1`, … or `None` for plain Vanilla.
    pub loader: Option<String>,
    /// Size of the instance's game directory (saves, mods, logs). The shared game files
    /// (versions, libraries, assets) are not included.
    pub size_bytes: u64,
    pub counts: ContentCounts,
}

/// Names the mod loader from the installed version id, which the installer builds as
/// `fabric-loader-<loader>-<game>`, `quilt-loader-<loader>-<game>`, `<game>-forge-<ver>` or
/// `neoforge-<ver>`. Anything else is Vanilla.
pub(super) fn detect_loader(version: &str) -> Option<String> {
    if let Some(rest) = version.strip_prefix("fabric-loader-") {
        return Some(format!("Fabric {}", rest.split('-').next().unwrap_or(rest)));
    }
    if let Some(rest) = version.strip_prefix("quilt-loader-") {
        return Some(format!("Quilt {}", rest.split('-').next().unwrap_or(rest)));
    }
    if let Some(rest) = version.strip_prefix("neoforge-") {
        return Some(format!("NeoForge {rest}"));
    }
    version
        .split_once("-forge-")
        .map(|(_, loader)| format!("Forge {loader}"))
}

#[tauri::command]
pub async fn instance_details(
    app: tauri::AppHandle,
    id: String,
) -> Result<InstanceDetails, String> {
    let (_, instance) = load(&app, &id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let game = game_directory(&instance);
        let count = |kind| count_content(&game, kind);
        InstanceDetails {
            id: instance.id.to_string(),
            name: instance.name.clone(),
            version: instance.version.as_str().to_owned(),
            state: InstanceSummary::from_instance(&instance).state_label(),
            loader: detect_loader(instance.version.as_str()),
            size_bytes: directory_size(&game),
            counts: ContentCounts {
                mods: count(ContentKind::Mods),
                resourcepacks: count(ContentKind::ResourcePacks),
                shaderpacks: count(ContentKind::ShaderPacks),
                saves: count(ContentKind::Saves),
                schematics: count(ContentKind::Schematics),
            },
        }
    })
    .await
    .map_err(|_| "instance details task failed".to_owned())
}

// --------------------------------------------------------------------------------------
// Rename and delete
// --------------------------------------------------------------------------------------

/// Renames an instance. Only the display name changes; the folder keeps its UUID.
#[tauri::command]
pub async fn rename_installed_instance(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<InstanceSummary, String> {
    let (_, instance) = load(&app, &id)?;
    tauri::async_runtime::spawn_blocking(move || {
        rename_instance(&instance.root, &instance, name.trim())
            .map(|renamed| InstanceSummary::from_instance(&renamed))
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "rename task failed".to_owned())?
}

/// Deletes an instance and everything in its folder: worlds, mods, settings. The shared game
/// files are kept. A running instance cannot be deleted.
#[tauri::command]
pub async fn delete_installed_instance(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let (base, instance) = load(&app, &id)?;
    if is_instance_running(&app, instance.id)? {
        return Err("stop the game before deleting this instance".to_owned());
    }
    let natives = base
        .join("game")
        .join("natives")
        .join(instance.id.to_string());
    tauri::async_runtime::spawn_blocking(move || {
        delete_instance(&base, instance.id).map_err(|error| error.to_string())?;
        // The extracted natives belong to this instance alone. Not fatal if they are gone.
        let _ = std::fs::remove_dir_all(natives);
        Ok(())
    })
    .await
    .map_err(|_| "delete task failed".to_owned())?
}

// --------------------------------------------------------------------------------------
// Content
// --------------------------------------------------------------------------------------

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContentItem {
    pub name: String,
    pub size_bytes: u64,
    pub enabled: bool,
    pub is_folder: bool,
}

#[tauri::command]
pub async fn list_instance_content(
    app: tauri::AppHandle,
    id: String,
    kind: String,
) -> Result<Vec<ContentItem>, String> {
    let (_, instance) = load(&app, &id)?;
    let kind = parse_kind(&kind)?;
    tauri::async_runtime::spawn_blocking(move || {
        list_content(&game_directory(&instance), kind)
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| ContentItem {
                        name: entry.name,
                        size_bytes: entry.size,
                        enabled: entry.enabled,
                        is_folder: entry.is_folder,
                    })
                    .collect()
            })
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "listing task failed".to_owned())?
}

/// Switches a mod on or off (`x.jar` ↔ `x.jar.disabled`).
#[tauri::command]
pub async fn set_instance_mod_enabled(
    app: tauri::AppHandle,
    id: String,
    name: String,
    enabled: bool,
) -> Result<(), String> {
    let (_, instance) = load(&app, &id)?;
    if is_instance_running(&app, instance.id)? {
        return Err("stop the game before changing its mods".to_owned());
    }
    tauri::async_runtime::spawn_blocking(move || {
        set_mod_enabled(&game_directory(&instance), &name, enabled)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "mod switch task failed".to_owned())?
}

/// Deletes one item from a content folder.
#[tauri::command]
pub async fn remove_instance_content(
    app: tauri::AppHandle,
    id: String,
    kind: String,
    name: String,
) -> Result<(), String> {
    let (_, instance) = load(&app, &id)?;
    let kind = parse_kind(&kind)?;
    if is_instance_running(&app, instance.id)? {
        return Err("stop the game before deleting its files".to_owned());
    }
    tauri::async_runtime::spawn_blocking(move || {
        remove_content(&game_directory(&instance), kind, &name).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "removal task failed".to_owned())?
}

// --------------------------------------------------------------------------------------
// Logs
// --------------------------------------------------------------------------------------

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// `latest` or `crash`.
    pub kind: &'static str,
    pub name: String,
    pub size_bytes: u64,
    /// Seconds since the Unix epoch.
    pub modified: Option<u64>,
}

#[tauri::command]
pub async fn list_instance_logs(
    app: tauri::AppHandle,
    id: String,
) -> Result<Vec<LogEntry>, String> {
    let (_, instance) = load(&app, &id)?;
    tauri::async_runtime::spawn_blocking(move || {
        list_logs(&game_directory(&instance))
            .map(|files| {
                files
                    .into_iter()
                    .map(|file| LogEntry {
                        kind: match file.source {
                            LogSource::Latest => "latest",
                            LogSource::CrashReport(_) => "crash",
                        },
                        name: file.name,
                        size_bytes: file.size,
                        modified: file.modified,
                    })
                    .collect()
            })
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "log listing task failed".to_owned())?
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogContent {
    /// Redacted text: the end of the log, at most 256 KiB.
    pub text: String,
    /// `true` when the beginning of a longer log was left out.
    pub truncated: bool,
}

/// Reads a log or crash report with names, tokens, e-mail and IP addresses redacted.
/// `kind` is `latest` or `crash`; `name` is only used for `crash`.
#[tauri::command]
pub async fn read_instance_log(
    app: tauri::AppHandle,
    id: String,
    kind: String,
    name: Option<String>,
) -> Result<LogContent, String> {
    let (_, instance) = load(&app, &id)?;
    let source = log_source(&kind, name)?;
    tauri::async_runtime::spawn_blocking(move || {
        let log =
            read_log(&game_directory(&instance), &source).map_err(|error| error.to_string())?;
        Ok(LogContent {
            text: aether_diagnose::redacted(&log.text).text,
            truncated: log.truncated,
        })
    })
    .await
    .map_err(|_| "log reading task failed".to_owned())?
}

pub(super) fn log_source(kind: &str, name: Option<String>) -> Result<LogSource, String> {
    match (kind, name) {
        ("latest", _) => Ok(LogSource::Latest),
        ("crash", Some(name)) if is_plain_name(&name) => Ok(LogSource::CrashReport(name)),
        _ => Err("unknown log".to_owned()),
    }
}

// --------------------------------------------------------------------------------------
// Opening a folder
// --------------------------------------------------------------------------------------

/// The folder a label stands for, inside the game directory.
pub(super) fn folder_for(game: &Path, label: &str) -> Option<PathBuf> {
    match label {
        "game" => Some(game.to_path_buf()),
        "logs" => Some(game.join("logs")),
        "crash-reports" => Some(game.join("crash-reports")),
        other => ContentKind::parse(other).map(|kind| game.join(kind.folder())),
    }
}

/// Windows' extended-length prefix (`\\?\C:\…`) is something Explorer's opener dislikes.
pub(super) fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Opens one of the instance's folders in the file manager.
///
/// `target` is `game`, `logs`, `crash-reports`, or a content label. The folder is created
/// first if it does not exist yet, so the button always does something.
#[tauri::command]
pub async fn open_instance_folder(
    app: tauri::AppHandle,
    id: String,
    target: String,
) -> Result<(), String> {
    let (_, instance) = load(&app, &id)?;
    let game = game_directory(&instance);
    let folder = folder_for(&game, &target).ok_or_else(|| "unknown folder".to_owned())?;
    std::fs::create_dir_all(&folder).map_err(|_| "cannot create the folder".to_owned())?;
    // The folder must really be inside the instance (no link planted at a content name).
    let inside = std::fs::canonicalize(&folder)
        .and_then(|folder| std::fs::canonicalize(&instance.root).map(|root| (folder, root)))
        .is_ok_and(|(folder, root)| folder.starts_with(root));
    if !inside {
        return Err("that folder is not inside the instance".to_owned());
    }
    open_in_file_manager(&plain(&folder))
}

/// Hands a folder to the platform's file manager. The path is one argument, never
/// interpreted by a shell.
fn open_in_file_manager(folder: &Path) -> Result<(), String> {
    let mut command = if cfg!(target_os = "windows") {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(folder);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(folder);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(folder);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|_| "cannot open the file manager".to_owned())
}

#[cfg(test)]
#[path = "manage_tests.rs"]
mod tests;
