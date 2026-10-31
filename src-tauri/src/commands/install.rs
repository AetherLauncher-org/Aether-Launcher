//! One-click Vanilla installation with progress events and cancellation.

use aether_app::install::{self, InstallState};
use aether_app::instances::InstanceSummary;
use tauri::Manager;

use super::host::run;

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
