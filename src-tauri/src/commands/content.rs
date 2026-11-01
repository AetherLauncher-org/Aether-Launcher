//! Tauri commands for finding content on Modrinth and installing it into an instance.
//!
//! These only turn arguments and results into IPC. What is contacted, where a file may land,
//! and how a file is checked before it is kept are in `aether_app::content_install`: no paths
//! or addresses come from the page, a file is checked against the hashes Modrinth lists, and
//! an existing file is never overwritten.

use aether_app::content_install::{self, InstallReport, SearchRequest, SearchResult, Target};

use aether_app::content_operation::{ContentInstallResult, ContentInstallState};
use tauri::Manager;

use super::host::{TauriHost, run};

/// What content fits an instance (its game version and mod loader), for default filters.
#[tauri::command]
pub async fn get_content_target(app: tauri::AppHandle, id: String) -> Result<Target, String> {
    run(app, move |host| content_install::content_target(host, &id)).await
}

/// One page of Modrinth search results, with icons.
#[tauri::command]
pub async fn search_content(
    app: tauri::AppHandle,
    text: String,
    project_type: String,
    game_version: Option<String>,
    loader: Option<String>,
    categories: Option<Vec<String>>,
    sort: String,
    offset: u32,
) -> Result<SearchResult, String> {
    let categories = categories.unwrap_or_default();
    run(app, move |host| {
        content_install::search_modrinth(
            host,
            &SearchRequest {
                text: &text,
                project_type: &project_type,
                game_version: game_version.as_deref(),
                loader: loader.as_deref(),
                categories: &categories,
                sort: &sort,
                offset,
            },
        )
    })
    .await
}

/// Starts a content install and returns its backend-generated operation ID.
#[tauri::command]
pub fn begin_content_install(
    app: tauri::AppHandle,
    id: String,
    project_id: String,
    project_type: String,
    world: Option<String>,
    with_dependencies: bool,
) -> Result<String, String> {
    let operation_id = app.state::<ContentInstallState>().begin()?;
    let worker_id = operation_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ContentInstallState>();
        let host = TauriHost::new(app.clone());
        aether_app::content_operation::execute(
            &state,
            &worker_id,
            &host,
            &id,
            &project_id,
            &project_type,
            world.as_deref(),
            with_dependencies,
        );
    });
    Ok(operation_id)
}

/// Requests cancellation of only the named content operation.
#[tauri::command]
pub fn cancel_content_install(
    state: tauri::State<'_, ContentInstallState>,
    operation_id: String,
) -> bool {
    state.cancel(&operation_id)
}

/// Returns the retained outcome even when the worker finished before this call.
#[tauri::command]
pub async fn wait_content_install(
    app: tauri::AppHandle,
    operation_id: String,
) -> Result<ContentInstallResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<ContentInstallState>().wait(&operation_id)
    })
    .await
    .map_err(|_| "the content install task failed".to_owned())?
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

/// Starts a folder save and returns its backend-generated operation ID. The user picks the
/// destination natively on the worker; no folder path crosses IPC.
#[tauri::command]
pub fn begin_folder_save(
    app: tauri::AppHandle,
    id: String,
    project_id: String,
    project_type: String,
    with_dependencies: bool,
) -> Result<String, String> {
    let operation_id = app.state::<ContentInstallState>().begin()?;
    let worker_id = operation_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<ContentInstallState>();
        let host = TauriHost::new(app.clone());
        aether_app::content_operation::execute_folder(
            &state,
            &worker_id,
            &host,
            &id,
            &project_id,
            &project_type,
            with_dependencies,
        );
    });
    Ok(operation_id)
}
