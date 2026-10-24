mod commands;

use serde::Serialize;
use tauri::ipc::Invoke;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationInfo {
    name: &'static str,
    version: &'static str,
    core_status: &'static str,
}

#[tauri::command]
fn application_info() -> ApplicationInfo {
    ApplicationInfo {
        name: "Aether Launcher",
        version: env!("CARGO_PKG_VERSION"),
        core_status: "foundation",
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![application_info])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Compose the command handlers contributed by the narrow command modules.
///
/// `src-tauri/src/lib.rs` is intentionally kept small so that parallel feature
/// branches only conflict on the `generate_handler!` list. Instances are the
/// first module; as more trusted backends are exposed, their handlers are
/// chained here.
#[allow(dead_code)]
fn command_handlers() -> impl Fn(Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        application_info,
        commands::instances::list_instances,
        commands::instances::create_instance,
    ]
}
