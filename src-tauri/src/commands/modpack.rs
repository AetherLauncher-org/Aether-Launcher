//! Modrinth 整合包安装的 IPC 命令。

use aether_app::install::InstallState;
use aether_app::instances::InstanceSummary;
use tauri::Manager;

use super::host::run;

/// 安装一个 Modrinth 整合包，并为它建一个新实例。
///
/// `project_id` 为 `None` 时打开本地文件选择框（路径不经过 IPC，后端自己挑）。
/// 进度经 `install://progress` 事件发出，取消复用 `cancel_vanilla_install`。
#[tauri::command]
pub async fn install_modpack(
    app: tauri::AppHandle,
    project_id: Option<String>,
    name: String,
    optional: bool,
) -> Result<InstanceSummary, String> {
    let handle = app.clone();
    run(app, move |host| {
        aether_app::modpack::install(
            host,
            &handle.state::<InstallState>(),
            project_id.as_deref(),
            &name,
            optional,
        )
    })
    .await
}
