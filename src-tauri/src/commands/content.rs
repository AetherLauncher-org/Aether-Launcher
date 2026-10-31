//! Tauri commands for finding content on Modrinth and installing it into an instance.
//!
//! These only turn arguments and results into IPC. What is contacted, where a file may land,
//! and how a file is checked before it is kept are in `aether_app::content_install`: no paths
//! or addresses come from the page, a file is checked against the hashes Modrinth lists, and
//! an existing file is never overwritten.

use aether_app::content_install::{self, InstallReport, SearchResult, Target};

use super::host::run;

/// What content fits an instance (its game version and mod loader), for default filters.
#[tauri::command]
pub async fn get_content_target(app: tauri::AppHandle, id: String) -> Result<Target, String> {
    run(app, move |host| content_install::content_target(host, &id)).await
}

/// One page of Modrinth search results.
#[tauri::command]
pub async fn search_content(
    text: String,
    project_type: String,
    game_version: Option<String>,
    loader: Option<String>,
    sort: String,
    offset: u32,
) -> Result<SearchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        content_install::search_modrinth(
            &text,
            &project_type,
            game_version.as_deref(),
            loader.as_deref(),
            &sort,
            offset,
        )
    })
    .await
    .map_err(|_| "the search was interrupted".to_owned())?
}

/// Installs the newest suitable version of a Modrinth project into an instance (or, for a data
/// pack, one of its worlds). A running instance cannot be changed.
#[tauri::command]
pub async fn install_content(
    app: tauri::AppHandle,
    id: String,
    project_id: String,
    project_type: String,
    world: Option<String>,
    with_dependencies: bool,
) -> Result<InstallReport, String> {
    run(app, move |host| {
        content_install::install(
            host,
            &id,
            &project_id,
            &project_type,
            world.as_deref(),
            with_dependencies,
        )
    })
    .await
}
