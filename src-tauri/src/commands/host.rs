//! The Tauri shell's answer to what `aether-app` needs from its host.

use std::sync::Arc;

use aether_app::Host;
use aether_app::microsoft::{LoginEvent, STATUS_EVENT};
use aether_core::InstanceId;
use tauri::{Emitter, Manager};

/// Wraps the running app so the UI-independent layer can ask it for the data directory and
/// whether an instance's game is running.
#[derive(Clone)]
pub(super) struct TauriHost {
    app: tauri::AppHandle,
}

impl TauriHost {
    pub(super) fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl Host for TauriHost {
    fn data_dir(&self) -> Result<std::path::PathBuf, String> {
        self.app
            .path()
            .app_data_dir()
            .map_err(|_| "application data directory is unavailable".to_owned())
    }

    fn is_instance_running(&self, id: InstanceId) -> Result<bool, String> {
        super::launch::is_instance_running(&self.app, id)
    }

    fn emit_login(&self, event: &LoginEvent) {
        // A closed window has nobody to tell.
        let _ = self.app.emit(STATUS_EVENT, event);
    }
}

/// Runs an `aether-app` operation on a blocking thread (file system work must not stall the
/// interface) with this app as its host.
pub(super) async fn run<T, F>(app: tauri::AppHandle, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&TauriHost) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || work(&TauriHost::new(app)))
        .await
        .map_err(|_| "the task failed".to_owned())?
}

/// As [`run`], for operations that start a background thread and so keep the host: they get
/// it behind an `Arc`.
pub(super) async fn run_shared<T, F>(app: tauri::AppHandle, work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Arc<dyn Host>) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let host: Arc<dyn Host> = Arc::new(TauriHost::new(app));
        work(&host)
    })
    .await
    .map_err(|_| "the task failed".to_owned())?
}
