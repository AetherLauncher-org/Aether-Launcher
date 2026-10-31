//! Managing one installed instance: details, rename, delete, its content folders (mods,
//! packs, shaders, worlds, schematics), its logs, and opening its folders.
//!
//! A caller names an instance by id and a folder by a fixed label (`mods`, `saves`, `logs`,
//! …), and an item by a plain file name. It never supplies a path: every folder is resolved
//! from the verified instance record (`aether_storage::manage`), and the only thing handed to
//! the operating system is a folder built here.
//!
//! Logs and crash reports are untrusted text that may hold the player's name, UUID, access
//! token fragments, e-mail addresses or IP addresses. They are redacted
//! (`aether_diagnose::redacted`) before they leave this layer.

use std::path::{Path, PathBuf};

use aether_core::GameInstance;
use aether_storage::manage as store;
use aether_storage::manage::{ContentKind, LogSource, game_directory, is_plain_name};
use serde::Serialize;

use crate::host::Host;
use crate::instances::{InstanceSummary, load, state_label};
use crate::platform;

fn parse_kind(label: &str) -> Result<ContentKind, String> {
    ContentKind::parse(label).ok_or_else(|| "unknown content folder".to_owned())
}

fn refuse_while_running(
    host: &dyn Host,
    instance: &GameInstance,
    what: &str,
) -> Result<(), String> {
    if host.is_instance_running(instance.id)? {
        return Err(format!("stop the game before {what}"));
    }
    Ok(())
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
#[must_use]
pub fn detect_loader(version: &str) -> Option<String> {
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

/// The details of an instance. Measures the game directory, so call it off the interface
/// thread.
///
/// # Errors
///
/// For an id that is not an instance.
pub fn instance_details(host: &dyn Host, id: &str) -> Result<InstanceDetails, String> {
    let (_, instance) = load(host, id)?;
    let game = game_directory(&instance);
    let count = |kind| store::count_content(&game, kind);
    Ok(InstanceDetails {
        id: instance.id.to_string(),
        name: instance.name.clone(),
        version: instance.version.as_str().to_owned(),
        state: state_label(&instance.state),
        loader: detect_loader(instance.version.as_str()),
        size_bytes: store::directory_size(&game),
        counts: ContentCounts {
            mods: count(ContentKind::Mods),
            resourcepacks: count(ContentKind::ResourcePacks),
            shaderpacks: count(ContentKind::ShaderPacks),
            saves: count(ContentKind::Saves),
            schematics: count(ContentKind::Schematics),
        },
    })
}

// --------------------------------------------------------------------------------------
// Rename and delete
// --------------------------------------------------------------------------------------

/// Renames an instance. Only the display name changes; the folder keeps its UUID.
///
/// No two instances may share a name (compared without regard to ASCII case, as when one is
/// created); the store's own rename only checks that the name is valid, so the rule is kept
/// here and an interface's own check is a convenience, not the guard.
///
/// # Errors
///
/// For an unknown instance, a name the store refuses (empty, too long) or one another
/// instance already has.
pub fn rename(host: &dyn Host, id: &str, name: &str) -> Result<InstanceSummary, String> {
    let (base, instance) = load(host, id)?;
    let wanted = name.trim();
    let taken = aether_storage::list_instances(&base)
        .map_err(|error| format!("cannot list instances: {error}"))?
        .instances
        .iter()
        .any(|other| other.id != instance.id && other.name.eq_ignore_ascii_case(wanted));
    if taken {
        return Err("an instance with this name already exists".to_owned());
    }
    aether_storage::rename_instance(&instance.root, &instance, name.trim())
        .map(|renamed| InstanceSummary::from_instance(&renamed))
        .map_err(|error| error.to_string())
}

/// Deletes an instance and everything in its folder: worlds, mods, settings. The shared game
/// files are kept, and so is nothing of this instance's extracted natives.
///
/// # Errors
///
/// While the instance's game runs, for an unknown instance, or when the folder cannot be
/// removed.
pub fn delete(host: &dyn Host, id: &str) -> Result<(), String> {
    let (base, instance) = load(host, id)?;
    refuse_while_running(host, &instance, "deleting this instance")?;
    let natives = base
        .join("game")
        .join("natives")
        .join(instance.id.to_string());
    store::delete_instance(&base, instance.id).map_err(|error| error.to_string())?;
    // The extracted natives belong to this instance alone. Not fatal if they are gone.
    let _ = std::fs::remove_dir_all(natives);
    Ok(())
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

/// Lists one kind of content (`mods`, `resourcepacks`, `shaderpacks`, `saves`, `schematics`).
///
/// # Errors
///
/// For an unknown instance or label, or a folder that cannot be read.
pub fn list_content(host: &dyn Host, id: &str, kind: &str) -> Result<Vec<ContentItem>, String> {
    let (_, instance) = load(host, id)?;
    let kind = parse_kind(kind)?;
    store::list_content(&game_directory(&instance), kind)
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
}

/// Switches a mod on or off (`x.jar` ↔ `x.jar.disabled`).
///
/// # Errors
///
/// While the instance's game runs, or for a name that is not a plain mod file name.
pub fn set_mod_enabled(host: &dyn Host, id: &str, name: &str, enabled: bool) -> Result<(), String> {
    let (_, instance) = load(host, id)?;
    refuse_while_running(host, &instance, "changing its mods")?;
    store::set_mod_enabled(&game_directory(&instance), name, enabled)
        .map_err(|error| error.to_string())
}

/// Deletes one item from a content folder.
///
/// # Errors
///
/// While the instance's game runs, or for an unknown label or a name that is not plain.
pub fn remove_content(host: &dyn Host, id: &str, kind: &str, name: &str) -> Result<(), String> {
    let (_, instance) = load(host, id)?;
    let kind = parse_kind(kind)?;
    refuse_while_running(host, &instance, "deleting its files")?;
    store::remove_content(&game_directory(&instance), kind, name).map_err(|error| error.to_string())
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

/// The latest log and the crash reports of an instance.
///
/// # Errors
///
/// For an unknown instance or a folder that cannot be read.
pub fn list_logs(host: &dyn Host, id: &str) -> Result<Vec<LogEntry>, String> {
    let (_, instance) = load(host, id)?;
    store::list_logs(&game_directory(&instance))
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
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LogContent {
    /// Redacted text: the end of the log, at most 256 KiB.
    pub text: String,
    /// `true` when the beginning of a longer log was left out.
    pub truncated: bool,
}

/// Reads a log or crash report with names, tokens, e-mail and IP addresses redacted. `kind` is
/// `latest` or `crash`; `name` is only used for `crash`.
///
/// # Errors
///
/// For an unknown log, a name that is not plain, or a log that does not exist.
pub fn read_log(
    host: &dyn Host,
    id: &str,
    kind: &str,
    name: Option<String>,
) -> Result<LogContent, String> {
    let (_, instance) = load(host, id)?;
    let source = log_source(kind, name)?;
    let log =
        store::read_log(&game_directory(&instance), &source).map_err(|error| error.to_string())?;
    Ok(LogContent {
        text: aether_diagnose::redacted(&log.text).text,
        truncated: log.truncated,
    })
}

pub fn log_source(kind: &str, name: Option<String>) -> Result<LogSource, String> {
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
#[must_use]
pub fn folder_for(game: &Path, label: &str) -> Option<PathBuf> {
    match label {
        "game" => Some(game.to_path_buf()),
        "logs" => Some(game.join("logs")),
        "crash-reports" => Some(game.join("crash-reports")),
        other => ContentKind::parse(other).map(|kind| game.join(kind.folder())),
    }
}

/// Opens one of the instance's folders in the file manager. `target` is `game`, `logs`,
/// `crash-reports`, or a content label. The folder is created first if it does not exist yet,
/// so the button always does something, and it must really be inside the instance (a link
/// planted at a content name is refused).
///
/// # Errors
///
/// For an unknown label, a folder outside the instance, or a file manager that will not start.
pub fn open_folder(host: &dyn Host, id: &str, target: &str) -> Result<(), String> {
    let (_, instance) = load(host, id)?;
    let game = game_directory(&instance);
    let folder = folder_for(&game, target).ok_or_else(|| "unknown folder".to_owned())?;
    std::fs::create_dir_all(&folder).map_err(|_| "cannot create the folder".to_owned())?;
    let inside = std::fs::canonicalize(&folder)
        .and_then(|folder| std::fs::canonicalize(&instance.root).map(|root| (folder, root)))
        .is_ok_and(|(folder, root)| folder.starts_with(root));
    if !inside {
        return Err("that folder is not inside the instance".to_owned());
    }
    platform::open_in_file_manager(&platform::plain(&folder))
}

#[cfg(test)]
#[path = "manage_tests.rs"]
mod tests;
