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
    ArtifactResult, AttemptError, CancellationToken, DownloadArtifact, DownloadEngine,
    DownloadError, DownloadPlan, ProgressSink, expand_plan_with_mirrors, is_safe_relative_path,
    with_mirrors,
};
use aether_launch::build_install_plan;
use aether_manifest::load_resolved_version_manifest;
use aether_storage::{InstanceStore, create_instance, list_instances, validate_instance_name};
use serde::Serialize;
use tauri::{Emitter, Manager};

use super::instances::InstanceSummary;
use super::launch::{activate_instance, current_environment};
use super::vanilla::prepare_version;

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

pub(super) struct EventReporter {
    pub(super) app: tauri::AppHandle,
}

impl Reporter for EventReporter {
    fn report(&self, progress: &InstallProgress) {
        let _ = self.app.emit(PROGRESS_EVENT, progress);
    }
}

/// At most one installation runs at a time; this holds its cancellation token.
#[derive(Default)]
pub struct InstallState {
    active: Mutex<Option<CancellationToken>>,
}

impl InstallState {
    pub(super) fn begin(&self) -> Result<CancellationToken, String> {
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

    pub(super) fn finish(&self) {
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

/// Download one plan, skipping files that already verify.
fn run_stage(
    engine: &DownloadEngine<'_>,
    game_root: &Path,
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
        if verify_local_artifact(game_root, artifact).is_ok() {
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
pub(super) fn install_version(
    transport: &dyn HttpTransport,
    game_root: &Path,
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
    // 官方源在前、BMCLAPI 镜像在后；传输失败后的回退与按源熔断都由引擎处理。
    let plan = expand_plan_with_mirrors(
        build_install_plan(&resolved.manifest, &client_version, &current_environment())
            .map_err(|error| format!("cannot plan installation: {error}"))?,
    );

    let engine = DownloadEngine::new(transport, game_root);
    let mut total = run_stage(
        &engine,
        game_root,
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
                sources: with_mirrors("mojang", &index.url.clone()),
            }],
        };
        total += run_stage(
            &engine,
            game_root,
            &index_plan,
            Stage::Assets,
            version,
            cancel,
            reporter,
        )?;
        let objects = build_asset_object_plan(game_root, &index.id, &index.sha1, index.size)
            .map_err(|error| format!("cannot plan asset download: {error}"))?;
        let objects = expand_plan_with_mirrors(objects);
        total += run_stage(
            &engine,
            game_root,
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
fn check_instance_name(base: &Path, name: &str) -> Result<(), String> {
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

/// Install a Vanilla version and create a ready-to-launch instance for it.
/// `base` is the application data directory; the game tree is `base/game`.
pub(super) fn install_and_create(
    transport: &dyn HttpTransport,
    base: &Path,
    version: &str,
    name: &str,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
) -> Result<InstanceSummary, String> {
    check_instance_name(base, name)?;
    let game_root = base.join("game");

    reporter.report(&InstallProgress {
        version: version.to_owned(),
        stage: Stage::Metadata,
        files_done: 0,
        files_total: 1,
        bytes_done: 0,
        bytes_total: 0,
    });
    prepare_version(transport, &game_root, version)?;
    if cancel.is_cancelled() {
        return Err(CANCELLED.to_owned());
    }

    install_version(transport, &game_root, version, cancel, reporter)?;

    reporter.report(&InstallProgress {
        version: version.to_owned(),
        stage: Stage::Finalize,
        files_done: 0,
        files_total: 1,
        bytes_done: 0,
        bytes_total: 0,
    });
    let instance = create_instance(base, name, version, &game_root)
        .map_err(|error| format!("cannot create instance: {error}"))?;
    activate_instance(base, &game_root, instance.id)?;
    let ready = InstanceStore::open(base)
        .and_then(|store| store.load(instance.id))
        .map_err(|error| format!("cannot reload instance: {error}"))?;
    Ok(InstanceSummary::from_instance(&ready))
}

/// Installs a Vanilla version from Mojang's official servers and creates an
/// instance for it. Progress arrives as `install://progress` events.
#[tauri::command]
pub async fn install_vanilla_version(
    app: tauri::AppHandle,
    state: tauri::State<'_, InstallState>,
    version: String,
    name: String,
) -> Result<InstanceSummary, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|_| "application data directory is unavailable".to_owned())?;
    let token = state.begin()?;
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        fs::create_dir_all(&base)
            .map_err(|_| "cannot prepare application data directory".to_owned())?;
        let transport = KernelHttpTransport::new(UreqKernel::new());
        let reporter = EventReporter { app };
        install_and_create(&transport, &base, &version, &name, &token, &reporter)
    })
    .await
    .map_err(|_| "installation task failed".to_owned());
    state.finish();
    outcome?
}

/// Cancel the running installation, if any. Downloaded files are kept so the
/// next attempt resumes.
#[tauri::command]
pub fn cancel_vanilla_install(state: tauri::State<'_, InstallState>) -> bool {
    state.cancel()
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
