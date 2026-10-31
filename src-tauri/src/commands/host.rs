//! The Tauri shell's answer to what `aether-app` needs from its host.

use std::sync::Arc;

use aether_app::Host;
use aether_app::host::KnownFolders;
use aether_app::import::{ITEM_EVENT, ImportItemEvent};
use aether_app::install::{self, InstallProgress};
use aether_app::microsoft::{LoginEvent, STATUS_EVENT};
use aether_core::InstanceId;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

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
    fn emit_import_item(&self, event: &ImportItemEvent) {
        let _ = self.app.emit(ITEM_EVENT, event);
    }

    fn pick_folder(&self, title: &str) -> Result<Option<std::path::PathBuf>, String> {
        let picked = self
            .app
            .dialog()
            .file()
            .set_title(title)
            .blocking_pick_folder();
        let Some(picked) = picked else {
            return Ok(None);
        };
        let path = picked
            .into_path()
            .map_err(|_| "the selected folder path is not usable".to_owned())?;
        Ok(Some(path))
    }

    fn known_folders(&self) -> KnownFolders {
        let resolver = self.app.path();
        KnownFolders {
            desktop: resolver.desktop_dir().ok(),
            downloads: resolver.download_dir().ok(),
            documents: resolver.document_dir().ok(),
            home: resolver.home_dir().ok(),
        }
    }

    fn emit_install(&self, progress: &InstallProgress) {
        let _ = self.app.emit(install::PROGRESS_EVENT, progress);
    }

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
