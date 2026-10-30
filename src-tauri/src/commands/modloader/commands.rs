//! 模组加载器的 IPC 命令。

use serde::Serialize;

use super::list_modloader_choices;

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

impl From<super::ModLoaderChoice> for ModLoaderChoiceInfo {
    fn from(choice: super::ModLoaderChoice) -> Self {
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
    game_version: String,
) -> Result<Vec<ModLoaderChoiceInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        list_modloader_choices(&game_version)
            .into_iter()
            .map(ModLoaderChoiceInfo::from)
            .collect()
    })
    .await
    .map_err(|error| format!("mod loader metadata task failed: {error}"))
}

/// 装好一个模组加载器版本并为它建实例。
///
/// 前端只传「哪个加载器 / 哪个游戏版本 / 哪个加载器版本 / 实例叫什么」，
/// URL 全部由后端的安装计划按官方源拼出来。
#[tauri::command]
pub async fn install_modloader_version(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::commands::install::InstallState>,
    kind: String,
    game_version: String,
    loader_version: String,
    name: String,
) -> Result<super::execute::ModLoaderInstallResult, String> {
    let parsed = super::parse_kind(&kind).ok_or_else(|| format!("unknown mod loader: {kind}"))?;
    let base = {
        use tauri::Manager;
        app.path()
            .app_data_dir()
            .map_err(|_| "application data directory is unavailable".to_owned())?
    };
    let token = state.begin()?;
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&base)
            .map_err(|_| "cannot prepare application data directory".to_owned())?;
        let transport = aether_download::transport::KernelHttpTransport::new(
            aether_download::transport::http::UreqKernel::new(),
        );
        let reporter = super::execute::AppReporter { app };
        super::execute::install_with(
            &transport,
            super::execute::InstallRequest {
                base: &base,
                kind: parsed,
                game_version: &game_version,
                loader_version: &loader_version,
                name: &name,
            },
            &token,
            &reporter,
        )
    })
    .await
    .map_err(|_| "mod loader installation task failed".to_owned());
    state.finish();
    outcome?
}
