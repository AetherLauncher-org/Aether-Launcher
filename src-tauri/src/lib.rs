mod commands;

use std::path::PathBuf;

use aether_runtime::JavaRuntime;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplicationInfo {
    name: &'static str,
    version: &'static str,
    core_status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JavaRuntimeInfo {
    executable: PathBuf,
    major_version: u16,
    architecture: String,
    vendor: Option<String>,
}

impl From<JavaRuntime> for JavaRuntimeInfo {
    fn from(runtime: JavaRuntime) -> Self {
        Self {
            executable: runtime.executable,
            major_version: runtime.major_version,
            architecture: runtime.architecture,
            vendor: runtime.vendor,
        }
    }
}

#[tauri::command]
fn application_info() -> ApplicationInfo {
    ApplicationInfo {
        name: "Aether Launcher",
        version: env!("CARGO_PKG_VERSION"),
        core_status: "foundation",
    }
}

/// A read-only IPC command: enumerate detected Java installations without
/// blocking the Tauri main thread.
///
/// `refresh` 为 false 时先读上次的结果，只有用户主动点「重新扫描」才真的去
/// 扫。扫描要启动一堆 `java -version`，每次切页面都做一遍会让界面一直卡一下，
/// 还会在任务栏闪命令窗口。
#[tauri::command]
async fn list_java_runtimes(
    app: tauri::AppHandle,
    refresh: Option<bool>,
) -> Result<Vec<JavaRuntimeInfo>, String> {
    let runtimes = commands::java::list_java_runtimes(app, refresh.unwrap_or(false)).await?;
    Ok(runtimes.into_iter().map(JavaRuntimeInfo::from).collect())
}

/// 让用户手动挑一个 `java`，挑完立刻探测并写进缓存。
#[tauri::command]
async fn pick_java_runtime(app: tauri::AppHandle) -> Result<Option<JavaRuntimeInfo>, String> {
    let picked = commands::java::pick_java_runtime(app).await?;
    Ok(picked.map(JavaRuntimeInfo::from))
}

/// A read-only IPC command: installed and available physical memory, so the
/// settings page can size the game's heap against the real machine. It takes
/// no input; `None` means the platform did not report memory.
#[tauri::command]
async fn system_memory() -> Option<aether_runtime::memory::SystemMemory> {
    tauri::async_runtime::spawn_blocking(aether_runtime::memory::system_memory)
        .await
        .ok()
        .flatten()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::launch::LaunchState::default())
        .manage(commands::install::InstallState::default())
        .manage(commands::import::ImportState::default())
        .manage(commands::microsoft::MicrosoftLoginState::default())
        .invoke_handler(tauri::generate_handler![
            application_info,
            list_java_runtimes,
            pick_java_runtime,
            system_memory,
            commands::catalog::list_minecraft_versions,
            commands::modloader::commands::list_modloader_versions,
            commands::modloader::commands::install_modloader_version,
            commands::install::install_vanilla_version,
            commands::install::cancel_vanilla_install,
            commands::instances::list_instances,
            commands::launch::start_installed_instance,
            commands::launch::stop_installed_instance,
            commands::launch::game_status,
            commands::import::pick_import_folder,
            commands::import::detect_launcher_installations,
            commands::import::pick_launcher_folder,
            commands::import::import_versions,
            commands::microsoft::microsoft_login_available,
            commands::microsoft::begin_microsoft_login,
            commands::microsoft::cancel_microsoft_login,
            commands::microsoft::open_microsoft_verification_page,
            commands::microsoft::list_microsoft_accounts,
            commands::microsoft::remove_microsoft_account,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_runtime_response_uses_camel_case() {
        let runtime = JavaRuntime {
            executable: PathBuf::from("java"),
            major_version: 21,
            architecture: "arm64".to_string(),
            vendor: Some("Adoptium".to_string()),
        };
        let value = serde_json::to_value(JavaRuntimeInfo::from(runtime)).unwrap();

        assert_eq!(value["executable"], "java");
        assert_eq!(value["majorVersion"], 21);
        assert_eq!(value["architecture"], "arm64");
        assert_eq!(value["vendor"], "Adoptium");
        assert!(value.get("major_version").is_none());
    }

    #[test]
    fn java_runtime_response_preserves_missing_vendor() {
        let runtime = JavaRuntime {
            executable: PathBuf::from("java"),
            major_version: 17,
            architecture: "unknown".to_string(),
            vendor: None,
        };
        let value = serde_json::to_value(JavaRuntimeInfo::from(runtime)).unwrap();
        assert!(value["vendor"].is_null());
    }
}
