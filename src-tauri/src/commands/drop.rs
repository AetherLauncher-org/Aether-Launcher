//! Private native drop bridge. No command accepts a filesystem path.
use std::path::PathBuf;
use aether_app::drop::{self, DropState, NativeDropListener, NoNativeDropListener, Report};
use tauri::{Emitter, Manager};
use super::host::run;

/// Replace this listener when a platform hook can deliver paths exclusively to Rust.
pub fn start(app: tauri::AppHandle) {
    let listener = NoNativeDropListener;
    listener.start(Box::new(move |paths: Vec<PathBuf>| {
        let state = app.state::<DropState>().inner().clone();
        let app = app.clone();
        tauri::async_runtime::spawn_blocking(move || match drop::stage(&state, paths) {
            Ok(preview) => { let _ = app.emit("drop://staged", preview); },
            Err(message) => { let _ = app.emit("drop://rejected", message); },
        });
    }));
}

#[tauri::command]
pub async fn install_drop(app: tauri::AppHandle, state: tauri::State<'_, DropState>, token: String, id: String, worlds: Vec<String>) -> Result<Report, String> {
    let state = state.inner().clone();
    run(app, move |host| drop::install(host, &state, &token, &id, &worlds)).await
}
