//! Importing instances from an existing Minecraft installation.

use aether_app::import::{
    self, ImportResultDto, ImportSourceDto, ImportState, LauncherInstallationDto,
};
use aether_app::install::InstallState;
use tauri::Manager;

use super::host::run;

#[tauri::command]
pub(crate) async fn pick_import_folder(
    app: tauri::AppHandle,
) -> Result<Option<ImportSourceDto>, String> {
    let handle = app.clone();
    run(app, move |host| {
        import::pick_import_folder(host, &handle.state::<ImportState>())
    })
    .await
}

#[tauri::command]
pub(crate) async fn detect_launcher_installations(
    app: tauri::AppHandle,
) -> Result<Vec<LauncherInstallationDto>, String> {
    let handle = app.clone();
    run(app, move |host| {
        import::detect_launcher_installations(host, &handle.state::<ImportState>())
    })
    .await
}

#[tauri::command]
pub(crate) async fn pick_launcher_folder(
    app: tauri::AppHandle,
) -> Result<Option<LauncherInstallationDto>, String> {
    let handle = app.clone();
    run(app, move |host| {
        import::pick_launcher_folder(host, &handle.state::<ImportState>())
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_versions(
    app: tauri::AppHandle,
    token: String,
    ids: Vec<String>,
) -> Result<ImportResultDto, String> {
    let handle = app.clone();
    run(app, move |host| {
        import::import_versions(
            host,
            &handle.state::<ImportState>(),
            &handle.state::<InstallState>(),
            &token,
            &ids,
        )
    })
    .await
}
