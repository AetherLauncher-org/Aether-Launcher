//! Importing instances from an existing Minecraft installation.
//!
//! The user reaches this feature through two branches, and both keep the
//! frontend out of the file system:
//!
//! * **from a folder** — [`pick_import_folder`] opens the native folder picker
//!   in the backend, scans what the user chose, and returns a preview;
//! * **from another launcher** — [`detect_launcher_installations`] looks for
//!   PCL2, HMCL, BakaXL, Prism Launcher and MultiMC in a bounded set of likely
//!   places; [`pick_launcher_folder`] does the same for a folder the user
//!   selects when detection finds nothing.
//!
//! Neither branch accepts a path from the frontend. A scanned source is kept in
//! [`ImportState`] behind an opaque token, and [`import_versions`] only accepts
//! that token plus the identifiers the user ticked. The destination is always
//! the launcher-owned game tree and instance store.
//!
//! What an import actually copies is documented in
//! `aether_storage::import::execute`: version metadata only, never game files.
//! The client JAR, libraries and assets are fetched afterwards by the existing
//! `install_vanilla_client_libraries` command, which already falls back to the
//! BMCLAPI mirror.
//!
//! # Two shapes of source
//!
//! PCL2, HMCL and BakaXL keep the game in a `.minecraft` folder and describe a
//! version with `versions/<id>/<id>.json`, so one pipeline imports all three.
//! Prism Launcher and MultiMC describe a version with `mmc-pack.json` and store
//! no version JSON that could be copied, which is why [`PendingSource`] has a
//! second variant and [`import_versions`] dispatches on it.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aether_core::InstanceId;
use aether_download::transport::KernelHttpTransport;
use aether_download::transport::http::UreqKernel;
use aether_storage::import::{bakaxl, hmcl, pcl2, prism};
use aether_storage::{
    ExistingFiles, ImportCandidate, ImportError, ImportSourceKind, MAX_IMPORT_VERSIONS,
    plan_candidates, scan_game_directory,
};
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

use super::install::EventReporter;
use super::instances::InstanceSummary;

/// How many scanned sources stay addressable at once.
///
/// The UI is modal and only ever imports from the source it just previewed, so a
/// handful is plenty; the cap exists so a long session cannot grow the map
/// without bound.
const MAX_PENDING_SOURCES: usize = 32;

/// How many launcher installations one detection pass reports.
const MAX_LAUNCHER_INSTALLATIONS: usize = 8;

/// How many game folders one PCL2 installation may contribute.
const MAX_LAUNCHER_GAME_DIRECTORIES: usize = 4;

/// How many directories one detection pass may inspect.
const MAX_LAUNCHER_PROBE_DIRECTORIES: usize = 1500;

/// Scanned sources the user has not imported from yet.
///
/// Cloneable so a blocking task can own a handle while the command keeps the
/// application state borrow-free.
#[derive(Default, Clone)]
pub struct ImportState {
    sources: Arc<Mutex<HashMap<String, PendingSource>>>,
}

/// One scanned source, addressed by its token.
///
/// The variant decides how [`import_versions`] turns the selection into managed
/// instances, so a token can never be replayed against the wrong pipeline.
enum PendingSource {
    /// 一个 `.minecraft` 形状的游戏目录。导入就是把实例记录指向**它本身**：
    /// 版本元数据写回它的 `versions/`，游戏文件原地读取，不复制。
    GameDirectory { root: PathBuf },
    /// Prism/MultiMC 启动器文件夹；实例按 pack 重建，绑定到各自的游戏目录。
    PrismLauncher {
        root: PathBuf,
        /// 这个启动器的游戏目录（`instances/`），实例各有一份 `.minecraft`。
        game_directory: PathBuf,
    },
}

impl ImportState {
    /// Stores `source` under a fresh opaque token.
    fn remember(&self, source: PendingSource) -> Result<String, String> {
        let mut sources = self
            .sources
            .lock()
            .map_err(|_| "import state is unavailable".to_owned())?;
        if sources.len() >= MAX_PENDING_SOURCES {
            sources.clear();
        }
        let token = InstanceId::new().to_string();
        sources.insert(token.clone(), source);
        Ok(token)
    }

    /// Removes and returns the source behind `token`.
    ///
    /// A token is single-use: the same preview cannot be imported twice by
    /// accident, and a stale token from a previous dialog is rejected instead of
    /// silently importing from a folder the user has since forgotten about.
    fn take(&self, token: &str) -> Result<PendingSource, String> {
        let mut sources = self
            .sources
            .lock()
            .map_err(|_| "import state is unavailable".to_owned())?;
        sources.remove(token).ok_or_else(|| {
            "this import source is no longer available; choose the folder again".to_owned()
        })
    }
}

/// One importable unit a source offers.
///
/// A version identifier for a game directory, an instance folder name for
/// Prism/MultiMC. Both are single path segments the backend validates again
/// before touching the disk.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidateDto {
    id: String,
    display_name: String,
}

impl ImportCandidateDto {
    fn from_candidate(candidate: &ImportCandidate) -> Self {
        Self {
            id: candidate.version_id.clone(),
            display_name: candidate.display_name.clone(),
        }
    }
}

/// A scanned source the frontend can preview and then import from.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceDto {
    /// Opaque handle for [`import_versions`]; never a path.
    token: String,
    /// `pcl2`, `hmcl`, `bakaxl`, `prism` or `folder`.
    source_kind: &'static str,
    /// The folder the user picked, shown back to them for confirmation.
    source_path: String,
    candidates: Vec<ImportCandidateDto>,
}

/// One detected launcher and the sources it contributes.
///
/// A game-directory launcher contributes one source per game folder; Prism and
/// MultiMC contribute a single source whose candidates are their instances.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherInstallationDto {
    /// `pcl2`, `hmcl`, `bakaxl`, `prism` or `folder`.
    kind: &'static str,
    launcher_root: String,
    sources: Vec<ImportSourceDto>,
}

/// One entry that was skipped during an import.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSkippedDto {
    id: String,
    reason: &'static str,
}

/// Result of one import call.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResultDto {
    imported: Vec<InstanceSummary>,
    skipped: Vec<ImportSkippedDto>,
}

/// Resolves the application-owned data directory and prepares the instance
/// store. The path comes from Tauri, never from IPC.
fn app_data_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("application data directory is unavailable: {error}"))?;
    fs::create_dir_all(base.join("instances"))
        .map_err(|error| format!("cannot prepare the instance store: {error}"))?;
    Ok(base)
}

/// Maps a scan failure onto a stable, path-free message.
fn describe_scan_error(error: &ImportError) -> String {
    match error {
        ImportError::InvalidRoot => "the selected folder is not a readable directory".to_owned(),
        ImportError::TooManyEntries => {
            "the selected folder has too many entries to scan safely".to_owned()
        }
        ImportError::Io(_) => "the selected folder could not be scanned".to_owned(),
    }
}

/// Scans `path` and plans one candidate per version it contains.
///
/// `forced_kind` labels the source when the caller already knows where it came
/// from (a PCL2 game folder has no marker of its own). Without it the launcher is
/// detected from the listing, and a plain game directory falls back to
/// [`ImportSourceKind::Folder`].
fn plan_source(
    path: &Path,
    forced_kind: Option<ImportSourceKind>,
) -> Result<(ImportSourceKind, Vec<ImportCandidate>), String> {
    let files = scan_game_directory(path).map_err(|error| describe_scan_error(&error))?;
    let kind = forced_kind
        .or_else(|| aether_storage::detect(&files))
        .unwrap_or(ImportSourceKind::Folder);
    Ok((kind, plan_candidates(kind, &files)))
}

/// Scans `path` as a game directory and registers it as an importable source.
fn preview_game_directory(
    state: &ImportState,
    path: &Path,
    forced_kind: Option<ImportSourceKind>,
) -> Result<ImportSourceDto, String> {
    let (kind, candidates) = plan_source(path, forced_kind)?;
    let token = state.remember(PendingSource::GameDirectory {
        root: path.to_path_buf(),
    })?;
    Ok(ImportSourceDto {
        token,
        source_kind: kind.as_str(),
        source_path: path.to_string_lossy().into_owned(),
        candidates: candidates
            .iter()
            .map(ImportCandidateDto::from_candidate)
            .collect(),
    })
}

/// Lists the instances of a Prism/MultiMC launcher and registers them as one
/// source, so several instances can be imported in a single call.
fn build_prism_installation(state: &ImportState, root: &Path) -> LauncherInstallationDto {
    let instances = prism::list_instances(root);
    let mut sources = Vec::new();

    // Prism/MultiMC 每个实例自带一个游戏目录（`instances/<id>/.minecraft`），
    // 加载器、mods、saves 都在里面。启动和扫描都读这个目录，不复制。
    let game_directory = prism::locate_instances_root(root);
    if !instances.is_empty()
        && let Some(game_directory) = game_directory
        && let Ok(token) = state.remember(PendingSource::PrismLauncher {
            root: root.to_path_buf(),
            game_directory,
        })
    {
        sources.push(ImportSourceDto {
            token,
            source_kind: ImportSourceKind::Prism.as_str(),
            source_path: root.to_string_lossy().into_owned(),
            candidates: instances
                .iter()
                .map(|instance| ImportCandidateDto {
                    id: instance.id.clone(),
                    display_name: instance.display_name(),
                })
                .collect(),
        });
    }

    LauncherInstallationDto {
        kind: ImportSourceKind::Prism.as_str(),
        launcher_root: root.to_string_lossy().into_owned(),
        sources,
    }
}

/// Opens the native folder picker and previews whatever the user chose.
///
/// Returns `Ok(None)` when the picker was dismissed.
#[tauri::command]
pub(crate) async fn pick_import_folder(
    app: tauri::AppHandle,
) -> Result<Option<ImportSourceDto>, String> {
    let Some(path) = pick_folder(app.clone(), "选择 .minecraft 或游戏文件夹").await? else {
        return Ok(None);
    };

    let state = app.state::<ImportState>().inner().clone();
    let source = preview_game_directory(&state, &path, None)?;
    if source.candidates.is_empty() {
        return Err(
            "this folder has no versions/<id>/<id>.json, so there is nothing to import".to_owned(),
        );
    }
    Ok(Some(source))
}

/// Opens the native folder picker and returns the chosen directory.
async fn pick_folder(
    app: tauri::AppHandle,
    title: &'static str,
) -> Result<Option<PathBuf>, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().set_title(title).blocking_pick_folder()
    })
    .await
    .map_err(|error| format!("folder picker failed: {error}"))?;

    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked
        .into_path()
        .map_err(|_| "the selected folder path is not usable".to_owned())?;
    Ok(Some(path))
}

/// Game folders a PCL2 installation can import from.
///
/// The configured folder comes first, then the launcher folder itself and a
/// `.minecraft` beside it — PCL2 treats both as game folders when they contain
/// `versions/`. Only directories that actually hold versions survive.
fn pcl2_game_directories(root: &Path, config: &[(String, String)]) -> Vec<PathBuf> {
    let mut directories = pcl2::configured_game_directories(config, root);
    for extra in [root.to_path_buf(), root.join(".minecraft")] {
        if !directories.iter().any(|existing| existing == &extra) {
            directories.push(extra);
        }
    }
    directories.retain(|directory| directory.join("versions").is_dir());
    directories.truncate(MAX_LAUNCHER_GAME_DIRECTORIES);
    directories
}

/// Builds the DTO for one PCL2 installation, skipping folders with no versions.
fn build_pcl2_installation(
    state: &ImportState,
    config: pcl2::Pcl2Config,
) -> LauncherInstallationDto {
    let mut sources = Vec::new();
    for directory in pcl2_game_directories(&config.launcher_root, &config.pairs) {
        let Ok(source) = preview_game_directory(state, &directory, Some(ImportSourceKind::Pcl2))
        else {
            continue;
        };
        if source.candidates.is_empty() {
            continue;
        }
        sources.push(source);
    }

    LauncherInstallationDto {
        kind: ImportSourceKind::Pcl2.as_str(),
        launcher_root: config.launcher_root.to_string_lossy().into_owned(),
        sources,
    }
}

/// Builds the DTO for one BakaXL installation, skipping folders with no versions.
fn build_bakaxl_installation(state: &ImportState, root: &Path) -> LauncherInstallationDto {
    let mut sources = Vec::new();
    for directory in bakaxl::game_directories(root) {
        let Ok(source) = preview_game_directory(state, &directory, Some(ImportSourceKind::BakaXl))
        else {
            continue;
        };
        if source.candidates.is_empty() {
            continue;
        }
        sources.push(source);
    }

    LauncherInstallationDto {
        kind: ImportSourceKind::BakaXl.as_str(),
        launcher_root: root.to_string_lossy().into_owned(),
        sources,
    }
}

/// Builds the DTO for a launcher whose whole game lives in one directory.
///
/// HMCL keeps `.hmcl.json` inside the game folder rather than beside it, so the
/// launcher root and the game directory are the same path.
fn build_game_directory_installation(
    state: &ImportState,
    root: &Path,
    kind: ImportSourceKind,
) -> LauncherInstallationDto {
    let sources = preview_game_directory(state, root, Some(kind))
        .ok()
        .filter(|source| !source.candidates.is_empty())
        .into_iter()
        .collect();

    LauncherInstallationDto {
        kind: kind.as_str(),
        launcher_root: root.to_string_lossy().into_owned(),
        sources,
    }
}

/// Snapshot of one directory's immediate entries, for the marker checks.
fn listing_of(directory: &Path) -> ExistingFiles {
    let mut entries = Vec::new();
    if let Ok(read) = fs::read_dir(directory) {
        for entry in read.flatten().take(MAX_LAUNCHER_PROBE_DIRECTORIES) {
            if let Ok(name) = entry.file_name().into_string() {
                entries.push(name);
            }
        }
    }
    ExistingFiles::new(entries)
}

/// Recognises the launcher `directory` belongs to, if any.
///
/// The order is PCL2 → Prism → HMCL → BakaXL: a launcher folder marker wins over
/// a game folder marker, because a launcher folder can contain a `.minecraft`
/// that another launcher once wrote into.
///
/// Returns `None` for a directory that is not a launcher, and for a launcher
/// whose configuration points at nothing importable — the caller then keeps
/// descending, which is how a `.minecraft` below a launcher folder is found.
fn installation_at(
    state: &ImportState,
    directory: &Path,
    listing: &ExistingFiles,
) -> Option<LauncherInstallationDto> {
    if let Some(config) = pcl2::locate_config(directory) {
        let installation = build_pcl2_installation(state, config);
        return (!installation.sources.is_empty()).then_some(installation);
    }
    if prism::has_config(directory) {
        let installation = build_prism_installation(state, directory);
        return (!installation.sources.is_empty()).then_some(installation);
    }
    if hmcl::matches(listing) {
        let installation =
            build_game_directory_installation(state, directory, ImportSourceKind::Hmcl);
        return (!installation.sources.is_empty()).then_some(installation);
    }
    if bakaxl::matches(listing) {
        let installation = build_bakaxl_installation(state, directory);
        return (!installation.sources.is_empty()).then_some(installation);
    }
    None
}

/// Directories a launcher is likely to live in, with how deep to look.
///
/// None of the supported launchers register themselves, so detection is a
/// bounded search: the user's real Desktop, Downloads and Documents folders —
/// which Windows may have redirected to OneDrive or another drive — the home
/// folder itself, and the top level of every fixed drive.
fn launcher_search_roots(app: &tauri::AppHandle) -> Vec<(PathBuf, usize)> {
    let resolver = app.path();
    let mut roots: Vec<(PathBuf, usize)> = Vec::new();

    for (directory, depth) in [
        (resolver.desktop_dir().ok(), 2),
        (resolver.download_dir().ok(), 2),
        (resolver.document_dir().ok(), 2),
        (resolver.home_dir().ok(), 1),
    ] {
        let Some(directory) = directory else {
            continue;
        };
        if !roots.iter().any(|(existing, _)| existing == &directory) {
            roots.push((directory, depth));
        }
    }

    for drive in fixed_drive_roots() {
        roots.push((drive, 1));
    }

    roots
}

/// Records the launcher of `directory`, then descends `depth` more levels while
/// the budget lasts.
fn probe(
    state: &ImportState,
    directory: &Path,
    depth: usize,
    found: &mut Vec<LauncherInstallationDto>,
    budget: &mut usize,
) {
    if *budget == 0 || found.len() >= MAX_LAUNCHER_INSTALLATIONS {
        return;
    }
    *budget -= 1;

    if let Some(installation) = installation_at(state, directory, &listing_of(directory)) {
        // Two search roots can reach the same launcher; keep the first one.
        if !found
            .iter()
            .any(|existing| existing.launcher_root == installation.launcher_root)
        {
            found.push(installation);
        }
        return;
    }
    if depth == 0 {
        return;
    }

    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if *budget == 0 || found.len() >= MAX_LAUNCHER_INSTALLATIONS {
            return;
        }
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        probe(state, &entry.path(), depth - 1, found, budget);
    }
}

/// Roots of the fixed drives, or nothing on platforms without drive letters.
fn fixed_drive_roots() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    ('A'..='Z')
        .map(|letter| PathBuf::from(format!("{letter}:\\")))
        .filter(|root| root.is_dir())
        .collect()
}

/// Looks for every supported launcher and previews what it can import.
#[tauri::command]
pub(crate) async fn detect_launcher_installations(
    app: tauri::AppHandle,
) -> Result<Vec<LauncherInstallationDto>, String> {
    let state = app.state::<ImportState>().inner().clone();
    let handle = app.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let mut found = Vec::new();
        let mut budget = MAX_LAUNCHER_PROBE_DIRECTORIES;
        for (root, depth) in launcher_search_roots(&handle) {
            probe(&state, &root, depth, &mut found, &mut budget);
        }
        found
    })
    .await
    .map_err(|error| format!("launcher detection task failed: {error}"))
}

/// Reads a launcher or game folder the user picked by hand.
///
/// Returns `Ok(None)` when the picker was dismissed, and an error when the
/// folder is neither a recognised launcher nor a game directory with versions.
#[tauri::command]
pub(crate) async fn pick_launcher_folder(
    app: tauri::AppHandle,
) -> Result<Option<LauncherInstallationDto>, String> {
    let Some(root) = pick_folder(app.clone(), "选择启动器文件夹或 .minecraft 文件夹").await?
    else {
        return Ok(None);
    };

    let state = app.state::<ImportState>().inner().clone();
    let listing = listing_of(&root);
    let installation = match installation_at(&state, &root, &listing) {
        Some(installation) => installation,
        None => {
            // A plain game directory: label it by what its listing looks like.
            let kind = aether_storage::detect(&listing).unwrap_or(ImportSourceKind::Folder);
            build_game_directory_installation(&state, &root, kind)
        }
    };

    if installation.sources.is_empty() {
        return Err(
            "this folder has no versions/<id>/<id>.json and no launcher instances, so there is nothing to import"
                .to_owned(),
        );
    }
    Ok(Some(installation))
}

/// Copies the selected versions out of a previewed source and creates one
/// managed instance per version.
///
/// `token` must come from a preview returned by this session; `ids` are the
/// identifiers the user ticked. Everything else — the source folder, the game
/// tree and the instance store — is resolved by the backend.
#[tauri::command]
pub(crate) async fn import_versions(
    app: tauri::AppHandle,
    token: String,
    ids: Vec<String>,
) -> Result<ImportResultDto, String> {
    if ids.is_empty() {
        return Err("select at least one version or instance to import".to_owned());
    }
    if ids.len() > MAX_IMPORT_VERSIONS {
        return Err(format!(
            "at most {MAX_IMPORT_VERSIONS} versions can be imported at once"
        ));
    }

    let base = app_data_root(&app)?;
    let pending = app.state::<ImportState>().take(&token)?;

    tauri::async_runtime::spawn_blocking(move || {
        let outcome = match &pending {
            PendingSource::GameDirectory { root } => {
                // 导入的目标就是这个 `.minecraft` 本身：版本元数据写回它的
                // `versions/`，实例记录也指向它，一个字节都不复制。
                aether_storage::import_versions(root, root, &base, &ids)
            }
            PendingSource::PrismLauncher {
                root,
                game_directory,
            } => aether_storage::import_prism_instances(root, game_directory, &base, &ids),
        }
        .map_err(|error| format!("cannot import instances: {error}"))?;

        Ok(ImportResultDto {
            imported: outcome
                .imported
                .iter()
                .map(InstanceSummary::from_instance)
                .collect(),
            skipped: outcome
                .skipped
                .into_iter()
                .map(|(id, reason)| ImportSkippedDto {
                    id,
                    reason: reason.as_str(),
                })
                .collect(),
        })
    })
    .await
    .map_err(|error| format!("import task failed: {error}"))?
}

/// 给一个刚导入的实例补齐运行环境。
///
/// 导入只复制版本元数据，客户端 jar、libraries、natives 和 assets 由这里补齐。
/// 走的是与 [`super::install::install_vanilla_version`] 相同的两段流程：
/// [`super::vanilla::prepare_version`] 从官方目录取回并校验版本 JSON，
/// [`super::install::install_version`] 再按该 JSON 的 URL 与哈希下载文件。
/// 前端只传实例 ID，目标目录来自实例自己的 `effective_game_directory()`，
/// 因此导入带来的 `game_directory` 决定文件落在哪个 `.minecraft`。
///
/// 下载期间与正式安装共用 [`super::install::InstallState`] 的进度事件与取消
/// 语义；已存在的文件校验通过即跳过，重复调用可以续传。
#[tauri::command]
pub(crate) async fn install_imported_instance(
    app: tauri::AppHandle,
    state: tauri::State<'_, super::install::InstallState>,
    id: String,
    needs_manifest: bool,
) -> Result<usize, String> {
    let base = app_data_root(&app)?;
    let token = state.begin()?;
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let instance_id = InstanceId::parse(&id).map_err(|_| "invalid instance ID".to_owned())?;
        let instance = aether_storage::InstanceStore::open(&base)
            .map_err(|error| error.to_string())?
            .load(instance_id)
            .map_err(|error| error.to_string())?;
        let game_root = instance.effective_game_directory();
        if !game_root.is_dir() {
            return Err(format!(
                "the instance's game directory is missing: {}",
                game_root.display()
            ));
        }
        let transport = KernelHttpTransport::new(UreqKernel::new());
        let version = instance.version.as_str().to_owned();
        if needs_manifest && game_root.join("versions").join(&version).is_dir() {
            super::vanilla::prepare_version(&transport, &game_root, &version)?;
        }
        let reporter = EventReporter { app };
        super::install::install_version(&transport, &game_root, &version, &token, &reporter)
    })
    .await
    .map_err(|error| format!("client installation task failed: {error}"));
    state.finish();
    outcome?
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
