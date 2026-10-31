//! Read-only official Mojang catalog lookup. Never accept an IPC-supplied URL.

use aether_app::catalog;
use aether_manifest::catalog::VersionCatalog;

use super::host::run;

/// No user-provided URL, file path or credentials; blocking network I/O stays off the UI thread.
#[tauri::command]
pub async fn list_minecraft_versions(app: tauri::AppHandle) -> Result<VersionCatalog, String> {
    run(app, |_| catalog::list_minecraft_versions()).await
}
