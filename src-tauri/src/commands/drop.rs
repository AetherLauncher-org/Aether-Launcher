//! Private native drop bridge. No command accepts a filesystem path.
use super::host::run;
use aether_app::drop::{self, DropState, NativeDropListener, NoNativeDropListener, Report};
use std::path::PathBuf;
use tauri::{Emitter, Manager};

fn stage_and_emit(app: &tauri::AppHandle, state: &DropState, paths: Vec<PathBuf>) {
    match drop::stage(state, paths) {
        Ok(preview) => {
            let _ = app.emit("drop://staged", preview);
        }
        Err(message) => {
            let _ = app.emit("drop://rejected", message);
        }
    }
}

/// Replace this listener when a platform hook can deliver paths exclusively to Rust.
pub fn start(app: tauri::AppHandle) {
    let listener = NoNativeDropListener;
    listener.start(Box::new(move |paths: Vec<PathBuf>| {
        let state = app.state::<DropState>().inner().clone();
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || stage_and_emit(&app, &state, paths));
    }));
}

/// Dev-only stand-in for the native listener: stage the plain files of the
/// `AETHER_DEV_DROP` folder so the whole pipeline (staging, dialog, install)
/// can be exercised without a platform hook. No path crosses IPC — the backend
/// resolves the folder from the developer's own environment. Release builds refuse.
#[tauri::command]
pub async fn simulate_dev_drop(
    app: tauri::AppHandle,
    state: tauri::State<'_, DropState>,
) -> Result<(), String> {
    #[cfg(not(debug_assertions))]
    {
        let _ = (&app, &state);
        return Err("simulated drops are available in dev builds only".to_owned());
    }
    #[cfg(debug_assertions)]
    {
        let folder = std::env::var("AETHER_DEV_DROP")
            .map_err(|_| "set AETHER_DEV_DROP to a folder to simulate a drop".to_owned())?;
        let state = state.inner().clone();
        let sender = app.clone();
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            let paths = std::fs::read_dir(&folder)
                .map_err(|_| "cannot read the AETHER_DEV_DROP folder".to_owned())?
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect::<Vec<_>>();
            stage_and_emit(&sender, &state, paths);
            Ok(())
        })
        .await
        .map_err(|_| "the task failed".to_owned())?
    }
}

#[tauri::command]
pub async fn install_drop(
    app: tauri::AppHandle,
    state: tauri::State<'_, DropState>,
    token: String,
    id: String,
    worlds: Vec<String>,
) -> Result<Report, String> {
    let state = state.inner().clone();
    run(app, move |host| {
        drop::install(host, &state, &token, &id, &worlds)
    })
    .await
}
