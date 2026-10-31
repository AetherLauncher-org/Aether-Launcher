//! One-click Vanilla installation with progress events and cancellation.

use aether_app::install::{self, InstallProgress, InstallState, Reporter};
use aether_app::instances::InstanceSummary;
use tauri::{Emitter, Manager};

use super::host::run;

pub(super) struct EventReporter {
    pub(super) app: tauri::AppHandle,
}

impl Reporter for EventReporter {
    fn report(&self, progress: &InstallProgress) {
        let _ = self.app.emit(install::PROGRESS_EVENT, progress);
    }
}

/// Installs a Vanilla version from Mojang's official servers and creates an
/// instance for it. Progress arrives as `install://progress` events.
#[tauri::command]
pub async fn install_vanilla_version(
    app: tauri::AppHandle,
    version: String,
    name: String,
) -> Result<InstanceSummary, String> {
    let handle = app.clone();
    run(app, move |host| {
        install::install_vanilla(host, &handle.state::<InstallState>(), &version, &name)
    })
    .await
}

/// Cancel the running installation, if any. Downloaded files are kept so the
/// next attempt resumes.
#[tauri::command]
pub fn cancel_vanilla_install(state: tauri::State<'_, InstallState>) -> bool {
    install::cancel(&state)
}
