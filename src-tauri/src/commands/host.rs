//! The Tauri shell's answer to what `aether-app` needs from its host.

use aether_app::Host;
use aether_core::InstanceId;
use tauri::Manager;

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
