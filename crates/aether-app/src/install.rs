//! One-click Vanilla installation with progress events and cancellation.
//!
//! The frontend supplies only a version ID and an instance name. The version
//! JSON is fetched through the official catalog and verified by its SHA-1
//! (`vanilla::prepare_version`); every URL, hash and destination afterwards
//! comes from that JSON. Files that already verify are skipped, so re-running
//! an interrupted installation continues where it stopped.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use aether_download::assets::build_asset_object_plan;
use aether_download::transport::http::UreqKernel;
use aether_download::transport::{HttpTransport, KernelHttpTransport, TransportError};
use aether_download::verify::verify_local_artifact;
use aether_download::{
    ArtifactResult, ArtifactSource, AttemptError, CancellationToken, DownloadArtifact,
    DownloadEngine, DownloadError, DownloadPlan, ProgressSink, is_safe_relative_path,
};
use aether_launch::build_install_plan;
use aether_manifest::catalog::VersionCatalog;
use aether_manifest::load_resolved_version_manifest;
use aether_storage::{InstanceStore, create_instance, list_instances, validate_instance_name};
use serde::Serialize;

use crate::host::Host;
use crate::instances::InstanceSummary;
use crate::launch::{activate_instance, current_environment};
use crate::vanilla::{prepare_version, prepare_version_from_catalog};

/// Error text returned when the user cancels; the frontend matches on it.
pub const CANCELLED: &str = "cancelled";

/// Event name carrying [`InstallProgress`] payloads.
pub const PROGRESS_EVENT: &str = "install://progress";

const DOWNLOAD_CONCURRENCY: usize = 4;
const EMIT_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Metadata,
    Files,
    Assets,
    Finalize,
    /// 模组加载器安装：下载 installer / 运行 installer / 写版本文件。
    ///
    /// 加载器安装没有可以提前算出的字节数，所以这一阶段的进度按**步骤**计
    /// （`files_done` / `files_total` 表示第几个步骤），`bytes_*` 恒为 0。
    ModLoader,
}

/// 当前阶段是模组加载器安装时，`version` 字段里放的加载器版本 id
/// （例如 `fabric-loader-0.16.0-1.21.1`），而不是原版版本号。
impl InstallProgress {
    /// 构造一条模组加载器安装的进度：按步骤计数，没有字节数。
    pub(crate) fn mod_loader(
        version: impl Into<String>,
        stage: Stage,
        steps_done: usize,
        steps_total: usize,
    ) -> Self {
        Self {
            version: version.into(),
            stage,
            files_done: steps_done.min(steps_total),
            files_total: steps_total,
            bytes_done: 0,
            bytes_total: 0,
        }
    }
}

/// Progress of the current stage. Byte and file counts include files that were
/// already present and verified, so the bar never starts "behind".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub version: String,
    pub stage: Stage,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

pub trait Reporter: Send + Sync {
    fn report(&self, progress: &InstallProgress);
}

pub(crate) struct HostReporter<'a>(pub(crate) &'a dyn Host);

impl Reporter for HostReporter<'_> {
    fn report(&self, progress: &InstallProgress) {
        self.0.emit_install(progress);
    }
}

/// At most one installation runs at a time; this holds its cancellation token.
#[derive(Default)]
pub struct InstallState {
    active: Mutex<Option<CancellationToken>>,
}

impl InstallState {
    pub fn begin(&self) -> Result<CancellationToken, String> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| "installer state is unavailable".to_owned())?;
        if active.is_some() {
            return Err("another installation is already running".to_owned());
        }
        let token = CancellationToken::new();
        *active = Some(token.clone());
        Ok(token)
    }

    pub fn finish(&self) {
        if let Ok(mut active) = self.active.lock() {
            *active = None;
        }
    }

    fn cancel(&self) -> bool {
        match self.active.lock() {
            Ok(active) => active.as_ref().map(CancellationToken::cancel).is_some(),
            Err(_) => false,
        }
    }
}

/// Aggregates per-file progress from the download threads into stage totals
/// and forwards them to the reporter at a bounded rate.
struct AggregateSink<'a> {
    version: &'a str,
    stage: Stage,
    reporter: &'a dyn Reporter,
    files_total: usize,
    bytes_total: u64,
    base_files: usize,
    base_bytes: u64,
    per_file: Mutex<HashMap<PathBuf, (u64, u64)>>,
    last_emit: Mutex<Instant>,
}

impl AggregateSink<'_> {
    fn snapshot(&self) -> InstallProgress {
        let (mut files, mut bytes) = (self.base_files, self.base_bytes);
        if let Ok(map) = self.per_file.lock() {
            for (downloaded, total) in map.values() {
                bytes += (*downloaded).min(*total);
                if downloaded >= total {
                    files += 1;
                }
            }
        }
        InstallProgress {
            version: self.version.to_owned(),
            stage: self.stage,
            files_done: files.min(self.files_total),
            files_total: self.files_total,
            bytes_done: bytes.min(self.bytes_total),
            bytes_total: self.bytes_total,
        }
    }
}

impl ProgressSink for AggregateSink<'_> {
    fn progress(&self, destination: &Path, _provider: &str, downloaded: u64, total: u64) {
        if let Ok(mut map) = self.per_file.lock() {
            map.insert(destination.to_path_buf(), (downloaded, total));
        }
        let Ok(mut last) = self.last_emit.lock() else {
            return;
        };
        if last.elapsed() < EMIT_INTERVAL {
            return;
        }
        *last = Instant::now();
        drop(last);
        self.reporter.report(&self.snapshot());
    }
}

/// Copies a file the user already has (typically in another launcher's folder)
/// into the launcher-owned game tree, but only when its size and SHA-1 match the
/// official plan. Returns whether the file is now in place.
///
/// `seed` is read-only and never a write destination. A file that fails the
/// check, or any filesystem problem along the way, simply returns `false` and the
/// caller downloads the file as usual — a bad seed can slow an import down but
/// cannot change what ends up in the game tree.
fn borrow_from_seed(seed: &Path, game_root: &Path, artifact: &DownloadArtifact) -> bool {
    let Ok(source) = verify_local_artifact(seed, artifact) else {
        return false;
    };
    let destination = game_root.join(&artifact.destination);
    let (Some(parent), Some(name)) = (destination.parent(), destination.file_name()) else {
        return false;
    };
    if fs::create_dir_all(parent).is_err() {
        return false;
    }
    // The tree is launcher-owned, but refuse to write through a link that points
    // somewhere else all the same.
    let inside = match (fs::canonicalize(parent), fs::canonicalize(game_root)) {
        (Ok(parent), Ok(root)) => parent.starts_with(root),
        _ => false,
    };
    if !inside {
        return false;
    }

    let mut staging_name = name.to_os_string();
    staging_name.push(".seed");
    let staging = parent.join(staging_name);
    let staged = DownloadArtifact {
        destination: artifact
            .destination
            .with_file_name(staging.file_name().unwrap_or(name)),
        ..artifact.clone()
    };
    let placed = fs::copy(&source, &staging).is_ok()
        // Hash what was actually copied: the seed could change between checks.
        && verify_local_artifact(game_root, &staged).is_ok()
        && fs::rename(&staging, &destination).is_ok();
    if !placed {
        let _ = fs::remove_file(&staging);
    }
    placed
}

/// Download one plan, skipping files that already verify and borrowing files
/// from `seed` when it has verified copies.
#[allow(clippy::too_many_arguments)]
fn run_stage(
    engine: &DownloadEngine<'_>,
    game_root: &Path,
    seed: Option<&Path>,
    plan: &DownloadPlan,
    stage: Stage,
    version: &str,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
) -> Result<usize, String> {
    let files_total = plan.artifacts.len();
    let bytes_total: u64 = plan.artifacts.iter().map(|item| item.size).sum();
    let (mut base_files, mut base_bytes) = (0_usize, 0_u64);
    let mut pending = Vec::new();
    for artifact in &plan.artifacts {
        if cancel.is_cancelled() {
            return Err(CANCELLED.to_owned());
        }
        if verify_local_artifact(game_root, artifact).is_ok()
            || seed.is_some_and(|seed| borrow_from_seed(seed, game_root, artifact))
        {
            base_files += 1;
            base_bytes += artifact.size;
        } else {
            pending.push(artifact.clone());
        }
    }

    let sink = AggregateSink {
        version,
        stage,
        reporter,
        files_total,
        bytes_total,
        base_files,
        base_bytes,
        per_file: Mutex::new(HashMap::new()),
        last_emit: Mutex::new(Instant::now() - EMIT_INTERVAL),
    };
    reporter.report(&sink.snapshot());

    if !pending.is_empty() {
        let results = engine
            .download_plan(
                &DownloadPlan { artifacts: pending },
                DOWNLOAD_CONCURRENCY,
                cancel,
                &sink,
            )
            .map_err(|error| format!("cannot start download: {error}"))?;
        if results
            .iter()
            .any(|item| matches!(item.outcome, Err(DownloadError::Cancelled(_))))
        {
            return Err(CANCELLED.to_owned());
        }
        summarize(&results)?;
    }
    reporter.report(&InstallProgress {
        version: version.to_owned(),
        stage,
        files_done: files_total,
        files_total,
        bytes_done: bytes_total,
        bytes_total,
    });
    Ok(files_total)
}

/// Download the client JAR, libraries, natives and assets of a version whose
/// JSON is already cached under `game_root`. Returns the number of files.
#[cfg(test)]
fn install_version(
    transport: &dyn HttpTransport,
    game_root: &Path,
    version: &str,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
) -> Result<usize, String> {
    install_version_with(transport, game_root, None, version, cancel, reporter)
}

/// [`install_version`] that may borrow already-downloaded files from `seed`
/// (see [`borrow_from_seed`]).
fn install_version_with(
    transport: &dyn HttpTransport,
    game_root: &Path,
    seed: Option<&Path>,
    version: &str,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
) -> Result<usize, String> {
    if !is_safe_relative_path(Path::new(version)) {
        return Err("invalid version identifier".to_owned());
    }
    let resolved = load_resolved_version_manifest(game_root, version)
        .map_err(|_| "version JSON is not cached; cache it first".to_owned())?;
    let client_version = resolved
        .client_jar_version
        .ok_or_else(|| "version JSON has no official client download".to_owned())?;
    let plan = build_install_plan(&resolved.manifest, &client_version, &current_environment())
        .map_err(|error| format!("cannot plan installation: {error}"))?;

    let engine = DownloadEngine::new(transport, game_root);
    let mut total = run_stage(
        &engine,
        game_root,
        seed,
        &plan,
        Stage::Files,
        version,
        cancel,
        reporter,
    )?;

    if let Some(index) = &resolved.manifest.asset_index {
        if !index.url.starts_with("https://") {
            return Err("asset index URL is not HTTPS".to_owned());
        }
        let index_plan = DownloadPlan {
            artifacts: vec![DownloadArtifact {
                destination: PathBuf::from("assets")
                    .join("indexes")
                    .join(format!("{}.json", index.id)),
                size: index.size,
                sha1: index.sha1.to_ascii_lowercase(),
                sources: vec![ArtifactSource {
                    provider: "mojang".to_owned(),
                    url: index.url.clone(),
                }],
            }],
        };
        total += run_stage(
            &engine,
            game_root,
            seed,
            &index_plan,
            Stage::Assets,
            version,
            cancel,
            reporter,
        )?;
        let objects = build_asset_object_plan(game_root, &index.id, &index.sha1, index.size)
            .map_err(|error| format!("cannot plan asset download: {error}"))?;
        total += run_stage(
            &engine,
            game_root,
            seed,
            &objects,
            Stage::Assets,
            version,
            cancel,
            reporter,
        )?;
    }
    Ok(total)
}

/// Report how many files failed, grouped by cause, without echoing URLs.
fn summarize(results: &[ArtifactResult]) -> Result<usize, String> {
    let mut causes = std::collections::BTreeMap::<String, usize>::new();
    for item in results {
        if let Err(error) = &item.outcome {
            *causes.entry(describe_failure(error)).or_default() += 1;
        }
    }
    if causes.is_empty() {
        return Ok(results.len());
    }
    let failed: usize = causes.values().sum();
    let detail: Vec<String> = causes
        .iter()
        .map(|(cause, count)| format!("{cause} x{count}"))
        .collect();
    Err(format!(
        "{failed} of {} files failed to download or verify ({})",
        results.len(),
        detail.join(", ")
    ))
}

fn describe_failure(error: &DownloadError) -> String {
    let DownloadError::AllSourcesFailed { sources, .. } = error else {
        return "cancelled or invalid".to_owned();
    };
    match sources.last().and_then(|source| source.attempts.last()) {
        Some(AttemptError::Transport(TransportError::HttpStatus { status, .. })) => {
            format!("HTTP {status}")
        }
        Some(AttemptError::Transport(TransportError::Timeout { .. })) => "timeout".to_owned(),
        Some(AttemptError::Transport(_)) => "network error".to_owned(),
        Some(AttemptError::Hash(_)) => "hash mismatch".to_owned(),
        Some(AttemptError::Io(_)) => "local file error".to_owned(),
        Some(AttemptError::Cancelled) | None => "cancelled or invalid".to_owned(),
    }
}

/// Fail fast on an unusable instance name, before a long download.
pub(crate) fn check_instance_name(base: &Path, name: &str) -> Result<(), String> {
    validate_instance_name(name).map_err(|error| format!("invalid instance name: {error}"))?;
    fs::create_dir_all(base.join("instances"))
        .map_err(|_| "cannot prepare the instance store".to_owned())?;
    let listing =
        list_instances(base).map_err(|error| format!("cannot list instances: {error}"))?;
    if listing
        .instances
        .iter()
        .any(|existing| existing.name.eq_ignore_ascii_case(name))
    {
        return Err("an instance with this name already exists".to_owned());
    }
    Ok(())
}

/// Optional inputs to [`install_and_create_with`].
#[derive(Default, Clone, Copy)]
// still used by src-tauri's import until it moves
pub struct InstallOptions<'a> {
    /// A read-only folder laid out like a `.minecraft` whose verified files may
    /// be copied instead of downloaded.
    pub seed: Option<&'a Path>,
    /// An already-read official catalog, so a batch reads it once.
    pub catalog: Option<&'a VersionCatalog>,
}

/// Install a Vanilla version and create a ready-to-launch instance for it.
/// `base` is the application data directory; the game tree is `base/game`.
fn install_and_create(
    transport: &dyn HttpTransport,
    base: &Path,
    version: &str,
    name: &str,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
) -> Result<InstanceSummary, String> {
    install_and_create_with(
        transport,
        base,
        version,
        name,
        InstallOptions::default(),
        cancel,
        reporter,
    )
}

/// [`install_and_create`] with a seed folder and/or a reused catalog.
// still used by src-tauri's import until it moves
pub fn install_and_create_with(
    transport: &dyn HttpTransport,
    base: &Path,
    version: &str,
    name: &str,
    options: InstallOptions<'_>,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
) -> Result<InstanceSummary, String> {
    check_instance_name(base, name)?;
    install_game_files(transport, base, version, options, cancel, reporter)?;
    create_ready_instance(base, name, version, reporter)
}

/// Fetches and verifies the version JSON, then the client JAR, libraries, natives
/// and assets of `version` into `base/game`. It creates no instance: a caller that
/// still has more to install (a mod loader) creates the instance only once all of it
/// succeeded, so a failure never leaves a half-installed instance behind.
pub(crate) fn install_game_files(
    transport: &dyn HttpTransport,
    base: &Path,
    version: &str,
    options: InstallOptions<'_>,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
) -> Result<(), String> {
    let game_root = base.join("game");

    reporter.report(&InstallProgress {
        version: version.to_owned(),
        stage: Stage::Metadata,
        files_done: 0,
        files_total: 1,
        bytes_done: 0,
        bytes_total: 0,
    });
    match options.catalog {
        Some(catalog) => prepare_version_from_catalog(transport, catalog, &game_root, version)?,
        None => prepare_version(transport, &game_root, version)?,
    };
    if cancel.is_cancelled() {
        return Err(CANCELLED.to_owned());
    }

    install_version_with(
        transport,
        &game_root,
        options.seed,
        version,
        cancel,
        reporter,
    )?;
    Ok(())
}

/// Creates the instance for an installed `version` (already verified on disk) and
/// marks it ready. `version` may be a mod loader's version ID that inherits from a
/// vanilla one.
pub(crate) fn create_ready_instance(
    base: &Path,
    name: &str,
    version: &str,
    reporter: &dyn Reporter,
) -> Result<InstanceSummary, String> {
    let game_root = base.join("game");
    reporter.report(&InstallProgress {
        version: version.to_owned(),
        stage: Stage::Finalize,
        files_done: 0,
        files_total: 1,
        bytes_done: 0,
        bytes_total: 0,
    });
    let instance = create_instance(base, name, version)
        .map_err(|error| format!("cannot create instance: {error}"))?;
    activate_instance(base, &game_root, instance.id)?;
    let ready = InstanceStore::open(base)
        .and_then(|store| store.load(instance.id))
        .map_err(|error| format!("cannot reload instance: {error}"))?;
    Ok(InstanceSummary::from_instance(&ready))
}

/// Installs a Vanilla version from Mojang's official servers and creates an
/// instance for it. Progress arrives as `install://progress` events.
pub fn install_vanilla(
    host: &dyn Host,
    state: &InstallState,
    version: &str,
    name: &str,
) -> Result<InstanceSummary, String> {
    let base = host.data_dir()?;
    let token = state.begin()?;
    let _active = ActiveInstall(state);
    fs::create_dir_all(&base)
        .map_err(|_| "cannot prepare application data directory".to_owned())?;
    let transport = KernelHttpTransport::new(UreqKernel::new());
    let reporter = HostReporter(host);
    install_and_create(&transport, &base, version, name, &token, &reporter)
}

/// Gives the install slot back when the installation ends, however it ends.
pub(crate) struct ActiveInstall<'a>(pub(crate) &'a InstallState);

impl Drop for ActiveInstall<'_> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// Cancel the running installation, if any. Downloaded files are kept so the
/// next attempt resumes.
pub fn cancel(state: &InstallState) -> bool {
    state.cancel()
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
