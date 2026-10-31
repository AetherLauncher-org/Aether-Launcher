//! 模组加载器的 IPC 命令。

use serde::Serialize;

use aether_app::install::InstallState;
use aether_app::modloader::{self, ModLoaderChoice, ModLoaderInstallResult};
use tauri::Manager;

use super::host::run;

/// 一个加载器对某个游戏版本的可用情况，给前端列表用。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLoaderChoiceInfo {
    pub kind: String,
    pub label: String,
    pub available: bool,
    pub reason: Option<String>,
    pub versions: Vec<String>,
}

impl From<ModLoaderChoice> for ModLoaderChoiceInfo {
    fn from(choice: ModLoaderChoice) -> Self {
        Self {
            kind: choice.kind,
            label: choice.label,
            available: choice.available,
            reason: choice.reason,
            versions: choice.versions,
        }
    }
}

/// 列出 Forge / NeoForge / Fabric / Quilt 对这个游戏版本的可用版本。
///
/// 网络访问走官方的元数据接口，只认 https。单个加载器失败只影响它自己那一行。
#[tauri::command]
pub async fn list_modloader_versions(
    app: tauri::AppHandle,
    game_version: String,
) -> Result<Vec<ModLoaderChoiceInfo>, String> {
    run(app, move |_host| {
        Ok(modloader::list_modloader_choices(&game_version)
            .into_iter()
            .map(ModLoaderChoiceInfo::from)
            .collect())
    })
    .await
}

/// 装好一个模组加载器版本并为它建实例。
///
/// 前端只传「哪个加载器 / 哪个游戏版本 / 哪个加载器版本 / 实例叫什么」，
/// URL 全部由后端的安装计划按官方源拼出来。
#[tauri::command]
pub async fn install_modloader_version(
    app: tauri::AppHandle,
    kind: String,
    game_version: String,
    loader_version: String,
    name: String,
) -> Result<ModLoaderInstallResult, String> {
    let handle = app.clone();
    run(app, move |host| {
        modloader::install(
            host,
            &handle.state::<InstallState>(),
            &kind,
            &game_version,
            &loader_version,
            &name,
        )
    })
    .await
}
