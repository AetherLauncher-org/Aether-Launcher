//! Backend-owned content operation IDs, cancellation, and retained terminal results.

use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::Host;
use crate::content_install::{InstallControl, InstallReport, InstalledFile, SkippedProject};

/// Content download progress forwarded by the desktop host.
pub const PROGRESS_EVENT: &str = "content-install://progress";

const MAX_ACTIVE: usize = 2;
const MAX_FINISHED: usize = 128;
const EVENT_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContentInstallProgress {
    pub operation_id: String,
    pub phase: &'static str,
    pub project_id: String,
    pub file_name: Option<String>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: usize,
    /// Unique projects discovered so far; a lower bound until traversal finishes.
    pub files_discovered: usize,
    pub resolving: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContentInstallResult {
    pub operation_id: String,
    pub status: &'static str,
    pub installed: Vec<InstalledFile>,
    pub skipped: Vec<SkippedProject>,
    pub error: Option<String>,
}

#[derive(Default)]
struct Registry {
    entries: BTreeMap<String, Arc<Operation>>,
    finished: VecDeque<String>,
}

#[derive(Default)]
pub struct ContentInstallState {
    registry: Mutex<Registry>,
}

#[derive(Default)]
struct Operation {
    cancelled: AtomicBool,
    data: Mutex<OperationData>,
    ready: Condvar,
}

#[derive(Default)]
struct OperationData {
    started: bool,
    result: Option<ContentInstallResult>,
}

impl ContentInstallState {
    /// Reserves one of two active slots and returns a backend-generated UUID v4.
    ///
    /// # Errors
    /// When two operations are already reserved or running.
    pub fn begin(&self) -> Result<String, String> {
        let mut registry = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        if registry.entries.len() - registry.finished.len() >= MAX_ACTIVE {
            return Err("two content installs are already active".to_owned());
        }
        let id = aether_core::InstanceId::new().to_string();
        registry
            .entries
            .insert(id.clone(), Arc::new(Operation::default()));
        Ok(id)
    }

    fn operation(&self, id: &str) -> Result<Arc<Operation>, String> {
        self.registry
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .get(id)
            .cloned()
            .ok_or_else(|| "unknown content operation".to_owned())
    }

    /// Returns false for unknown or finished operations. Cancellation is cooperative.
    #[must_use]
    pub fn cancel(&self, id: &str) -> bool {
        let Ok(operation) = self.operation(id) else {
            return false;
        };
        let data = operation.data.lock().unwrap_or_else(|e| e.into_inner());
        if data.result.is_some() {
            return false;
        }
        operation.cancelled.store(true, Ordering::Release);
        true
    }

    /// Waits on a worker thread. An attached waiter retains its result even after eviction.
    /// The latest 128 terminal results remain available for later callers.
    ///
    /// # Errors
    /// For an unknown or evicted operation.
    pub fn wait(&self, id: &str) -> Result<ContentInstallResult, String> {
        let operation = self.operation(id)?;
        let mut data = operation.data.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(result) = &data.result {
                return Ok(result.clone());
            }
            data = operation
                .ready
                .wait(data)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    #[must_use]
    pub fn result(&self, id: &str) -> Option<ContentInstallResult> {
        self.operation(id)
            .ok()?
            .data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .result
            .clone()
    }

    pub(crate) fn start(&self, id: &str) -> Result<Arc<AtomicCancel>, String> {
        let operation = self.operation(id)?;
        let mut data = operation.data.lock().unwrap_or_else(|e| e.into_inner());
        if data.started || data.result.is_some() {
            return Err("content operation already started".to_owned());
        }
        data.started = true;
        drop(data);
        Ok(Arc::new(AtomicCancel(operation)))
    }

    pub(crate) fn finish(&self, result: ContentInstallResult) -> ContentInstallResult {
        let mut registry = self.registry.lock().unwrap_or_else(|e| e.into_inner());
        let Some(operation) = registry.entries.get(&result.operation_id).cloned() else {
            return result;
        };
        let mut data = operation.data.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(existing) = &data.result {
            return existing.clone();
        }
        data.result = Some(result.clone());
        operation.ready.notify_all();
        registry.finished.push_back(result.operation_id.clone());
        while registry.finished.len() > MAX_FINISHED {
            if let Some(id) = registry.finished.pop_front() {
                registry.entries.remove(&id);
            }
        }
        result
    }

    /// Completes a reservation when its worker could not be launched. Running workers catch
    /// panics themselves and preserve their exact report; do not fail a live worker externally.
    ///
    /// # Errors
    /// For an unknown operation or one whose worker has already started.
    pub fn fail(&self, id: &str, error: String) -> Result<ContentInstallResult, String> {
        let operation = self.operation(id)?;
        let mut data = operation.data.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(result) = &data.result {
            return Ok(result.clone());
        }
        if data.started {
            return Err("content operation already started".to_owned());
        }
        data.started = true;
        drop(data);
        Ok(self.finish(ContentInstallResult {
            operation_id: id.to_owned(),
            status: "failed",
            installed: Vec::new(),
            skipped: Vec::new(),
            error: Some(error),
        }))
    }
}

/// Runs a reserved content operation on a blocking worker thread.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    state: &ContentInstallState,
    operation_id: &str,
    host: &dyn Host,
    instance_id: &str,
    project_id: &str,
    project_type: &str,
    world: Option<&str>,
    with_dependencies: bool,
) {
    let Ok(cancel) = state.start(operation_id) else {
        return;
    };
    let control = OperationControl::new(host, operation_id, project_id, cancel);
    let outcome = crate::content_install::install_with_control(
        &crate::identify::UreqNet,
        host,
        instance_id,
        project_id,
        project_type,
        world,
        with_dependencies,
        &control,
    );
    let (status, report, error) = match outcome {
        Ok(report) => ("done", report, None),
        Err((reason, installed)) => {
            let status = if reason == "cancelled" {
                "cancelled"
            } else {
                "failed"
            };
            (
                status,
                InstallReport {
                    installed,
                    skipped: Vec::new(),
                },
                Some(reason),
            )
        }
    };
    // Retain the outcome before emitting: even a failing host cannot strand waiters.
    state.finish(ContentInstallResult {
        operation_id: operation_id.to_owned(),
        status,
        installed: report.installed.clone(),
        skipped: report.skipped.clone(),
        error,
    });
    control.terminal(status, &report);
}

pub(crate) struct AtomicCancel(Arc<Operation>);

pub(crate) struct OperationControl<'a> {
    host: &'a dyn Host,
    cancel: Arc<AtomicCancel>,
    progress: Mutex<(ContentInstallProgress, Option<Instant>)>,
}

impl<'a> OperationControl<'a> {
    pub(crate) fn new(
        host: &'a dyn Host,
        id: &str,
        project: &str,
        cancel: Arc<AtomicCancel>,
    ) -> Self {
        Self {
            host,
            cancel,
            progress: Mutex::new((
                ContentInstallProgress {
                    operation_id: id.to_owned(),
                    phase: "resolving",
                    project_id: project.to_owned(),
                    file_name: None,
                    bytes_done: 0,
                    bytes_total: 0,
                    files_done: 0,
                    files_discovered: 1,
                    resolving: true,
                },
                None,
            )),
        }
    }

    fn update(&self, terminal: bool, change: impl FnOnce(&mut ContentInstallProgress)) {
        let event = {
            let mut data = self.progress.lock().unwrap_or_else(|e| e.into_inner());
            change(&mut data.0);
            let now = Instant::now();
            if !terminal
                && data
                    .1
                    .is_some_and(|last| now.duration_since(last) < EVENT_INTERVAL)
            {
                return;
            }
            data.1 = Some(now);
            data.0.clone()
        };
        self.host.emit_content_install(&event);
    }

    pub(crate) fn terminal(&self, status: &'static str, report: &InstallReport) {
        self.update(true, |p| {
            p.phase = status;
            p.resolving = false;
            p.files_done = report.installed.len();
        });
    }
}

impl InstallControl for OperationControl<'_> {
    fn is_cancelled(&self) -> bool {
        self.cancel.0.cancelled.load(Ordering::Acquire)
    }
    fn progress(&self, done: u64, total: u64) {
        self.update(false, |p| {
            p.bytes_done = done;
            p.bytes_total = total;
        });
    }
    fn resolving(&self, project: &str) {
        self.update(false, |p| {
            p.phase = "resolving";
            p.project_id = project.to_owned();
            p.file_name = None;
            p.bytes_done = 0;
            p.bytes_total = 0;
        });
    }
    fn discovered(&self, count: usize) {
        self.update(false, |p| p.files_discovered = count);
    }
    fn downloading(&self, name: &str, size: u64) {
        self.update(false, |p| {
            p.phase = "downloading";
            p.file_name = Some(name.to_owned());
            p.bytes_done = 0;
            p.bytes_total = size;
        });
    }
    fn verifying(&self) {
        self.update(false, |p| p.phase = "verifying");
    }
    fn committed(&self) {
        self.update(false, |p| p.files_done += 1);
    }
}
