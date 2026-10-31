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
//! # What an import does
//!
//! Importing never reuses the other launcher's folder as a game directory.
//! `aether_storage::import::execute` plans the selection (read-only), and each
//! planned version is then installed into the launcher-owned game tree through
//! the normal install path: the version JSON comes from the official catalog and
//! every file is checked against the SHA-1 in it. The source folder is only a
//! **seed** — a file already there whose size and hash match is copied instead
//! of downloaded; anything else is downloaded as usual. See
//! `docs/modules/import.md`.
//!
//! # Two shapes of source
//!
//! PCL2, HMCL and BakaXL keep the game in a `.minecraft` folder and describe a
//! version with `versions/<id>/<id>.json`. Prism Launcher and MultiMC describe a
//! version with `mmc-pack.json`; [`PendingSource`] has a variant for each so a
//! token can never be replayed against the wrong planner.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aether_core::InstanceId;
use aether_download::CancellationToken;
use aether_download::transport::http::UreqKernel;
use aether_download::transport::{HttpTransport, KernelHttpTransport};
use aether_storage::import::{bakaxl, hmcl, pcl2, prism};
use aether_storage::{
    ExistingFiles, ImportCandidate, ImportError, ImportPlan, ImportSkipReason, ImportSourceKind,
    MAX_IMPORT_VERSIONS, list_instances, plan_candidates, plan_folder_import, plan_prism_import,
    scan_game_directory, validate_instance_name,
};
use serde::Serialize;

use crate::catalog::fetch_catalog;
use crate::host::{Host, KnownFolders};
use crate::instances::InstanceSummary;
use crate::install::{
    ActiveInstall, CANCELLED, HostReporter, InstallOptions, InstallState, Reporter,
    install_and_create_with,
};

/// Event name carrying [`ImportItemEvent`] payloads.
pub const ITEM_EVENT: &str = "import://item";

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
    /// A `.minecraft`-shaped game directory; read-only seed for the install.
    GameDirectory { root: PathBuf },
    /// A Prism/MultiMC launcher folder; instances are read from `instances/`.
    PrismLauncher { root: PathBuf },
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
fn app_data_root(host: &dyn Host) -> Result<PathBuf, String> {
    let base = host.data_dir()?;
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

    if !instances.is_empty()
        && let Ok(token) = state.remember(PendingSource::PrismLauncher {
            root: root.to_path_buf(),
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
pub fn pick_import_folder(
    host: &dyn Host,
    state: &ImportState,
) -> Result<Option<ImportSourceDto>, String> {
    let Some(path) = host.pick_folder("选择 .minecraft 或游戏文件夹")? else {
        return Ok(None);
    };

    let source = preview_game_directory(state, &path, None)?;
    if source.candidates.is_empty() {
        return Err(
            "this folder has no versions/<id>/<id>.json, so there is nothing to import".to_owned(),
        );
    }
    Ok(Some(source))
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
fn launcher_search_roots(folders: &KnownFolders) -> Vec<(PathBuf, usize)> {
    let mut roots: Vec<(PathBuf, usize)> = Vec::new();

    for (directory, depth) in [
        (folders.desktop.clone(), 2),
        (folders.downloads.clone(), 2),
        (folders.documents.clone(), 2),
        (folders.home.clone(), 1),
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
pub fn detect_launcher_installations(
    host: &dyn Host,
    state: &ImportState,
) -> Result<Vec<LauncherInstallationDto>, String> {
    let mut found = Vec::new();
    let mut budget = MAX_LAUNCHER_PROBE_DIRECTORIES;
    for (root, depth) in launcher_search_roots(&host.known_folders()) {
        probe(state, &root, depth, &mut found, &mut budget);
    }
    Ok(drop_repeated_game_directories(found))
}

/// 去掉跨启动器重复的游戏目录。
///
/// 同一份 `.minecraft` 常被 PCL2 和 HMCL 同时记录，不去重的话对话框里会列出
/// 好几行一模一样的路径，用户分不清该点哪个。这里只去掉**指向同一目录**的
/// 重复条目；`E:\a` 和 `E:\a\.minecraft` 是两个不同的目录，各自可能装不同的
/// 版本，都要保留。
///
/// 判据是规范化后的路径：先 `canonicalize`（解符号链接和 `..`），失败时退回
/// 小写比较，因为 Windows 路径不区分大小写。同一个目录只保留最先出现的那条，
/// 顺序不变；一个启动器的来源被全部去掉时，这条安装记录本身也不再列出。
fn drop_repeated_game_directories(
    installations: Vec<LauncherInstallationDto>,
) -> Vec<LauncherInstallationDto> {
    let mut seen = std::collections::HashSet::new();
    let mut kept = Vec::with_capacity(installations.len());

    for mut installation in installations {
        installation.sources.retain(|source| {
            let key = PathBuf::from(&source.source_path);
            let key = key
                .canonicalize()
                .map(|resolved| resolved.to_string_lossy().to_lowercase())
                .unwrap_or_else(|_| source.source_path.to_lowercase());
            seen.insert(key)
        });
        if !installation.sources.is_empty() {
            kept.push(installation);
        }
    }

    kept
}

/// Reads a launcher or game folder the user picked by hand.
///
/// Returns `Ok(None)` when the picker was dismissed, and an error when the
/// folder is neither a recognised launcher nor a game directory with versions.
pub fn pick_launcher_folder(
    host: &dyn Host,
    state: &ImportState,
) -> Result<Option<LauncherInstallationDto>, String> {
    let Some(root) = host.pick_folder("选择启动器文件夹或 .minecraft 文件夹")? else {
        return Ok(None);
    };

    let listing = listing_of(&root);
    let installation = match installation_at(state, &root, &listing) {
        Some(installation) => installation,
        None => {
            // A plain game directory: label it by what its listing looks like.
            let kind = aether_storage::detect(&listing).unwrap_or(ImportSourceKind::Folder);
            build_game_directory_installation(state, &root, kind)
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

/// Progress marker emitted before each version is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemEvent {
    pub id: String,
    pub index: usize,
    pub total: usize,
}

/// Installs every planned version into the private game tree.
///
/// Sequential on purpose: the downloads inside one version are already
/// concurrent, and one version at a time keeps the progress events readable and
/// cancellation prompt. A failure skips that version only.
fn run_import(
    transport: &dyn HttpTransport,
    base: &Path,
    plan: ImportPlan,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
    on_item: &dyn Fn(&ImportItemEvent),
) -> ImportResultDto {
    let mut imported = Vec::new();
    let mut skipped: Vec<ImportSkippedDto> = plan
        .skipped
        .iter()
        .map(|(id, reason)| ImportSkippedDto {
            id: id.clone(),
            reason: reason.as_str(),
        })
        .collect();

    let catalog = fetch_catalog(transport);
    let total = plan.requests.len();
    for (index, request) in plan.requests.iter().enumerate() {
        let skip = |reason: ImportSkipReason| ImportSkippedDto {
            id: request.id.clone(),
            reason: reason.as_str(),
        };
        if cancel.is_cancelled() {
            skipped.push(skip(ImportSkipReason::Cancelled));
            continue;
        }
        let Ok(catalog) = &catalog else {
            skipped.push(skip(ImportSkipReason::InstallFailed));
            continue;
        };
        if !catalog
            .versions
            .iter()
            .any(|item| item.id == request.version)
        {
            skipped.push(skip(ImportSkipReason::NotOfficial));
            continue;
        }
        if validate_instance_name(&request.name).is_err() {
            skipped.push(skip(ImportSkipReason::InvalidName));
            continue;
        }
        let duplicate = list_instances(base).is_ok_and(|listing| {
            listing
                .instances
                .iter()
                .any(|existing| existing.name.eq_ignore_ascii_case(&request.name))
        });
        if duplicate {
            skipped.push(skip(ImportSkipReason::DuplicateName));
            continue;
        }

        on_item(&ImportItemEvent {
            id: request.id.clone(),
            index,
            total,
        });
        let options = InstallOptions {
            seed: Some(plan.seed.as_path()),
            catalog: Some(catalog),
        };
        match install_and_create_with(
            transport,
            base,
            &request.version,
            &request.name,
            options,
            cancel,
            reporter,
        ) {
            Ok(summary) => imported.push(summary),
            Err(error) if error == CANCELLED => skipped.push(skip(ImportSkipReason::Cancelled)),
            Err(_) => skipped.push(skip(ImportSkipReason::InstallFailed)),
        }
    }

    ImportResultDto { imported, skipped }
}

/// Imports the selected versions of a previewed source as managed instances.
///
/// `token` must come from a preview returned by this session; `ids` are the
/// identifiers the user ticked. Everything else — the source folder, the game
/// tree and the instance store — is resolved by the backend. Progress arrives as
/// `import://item` and `install://progress` events, and the run can be cancelled
/// with `cancel_vanilla_install`.
pub fn import_versions(
    host: &dyn Host,
    import: &ImportState,
    install: &InstallState,
    token: &str,
    ids: &[String],
) -> Result<ImportResultDto, String> {
    if ids.is_empty() {
        return Err("select at least one version or instance to import".to_owned());
    }
    if ids.len() > MAX_IMPORT_VERSIONS {
        return Err(format!(
            "at most {MAX_IMPORT_VERSIONS} versions can be imported at once"
        ));
    }

    let base = app_data_root(host)?;
    let pending = import.take(token)?;
    let cancel = install.begin()?;
    let _active = ActiveInstall(install);

    let plan = match &pending {
        PendingSource::GameDirectory { root } => plan_folder_import(root, ids),
        PendingSource::PrismLauncher { root } => plan_prism_import(root, ids),
    }
    .map_err(|error| format!("cannot import instances: {error}"))?;

    let transport = KernelHttpTransport::new(UreqKernel::new());
    let reporter = HostReporter(host);
    let announce = |event: &ImportItemEvent| host.emit_import_item(event);
    Ok(run_import(
        &transport, &base, plan, &cancel, &reporter, &announce,
    ))
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
