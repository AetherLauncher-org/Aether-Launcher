//! 模组加载器的 IPC 命令。

use aether_modloader::transport::MetaSource;
use aether_modloader::{ModLoaderInstallPlan, ModLoaderKind};
use serde::Serialize;

use super::fetch::{HttpMetaFetcher, download_into};
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
    pub companions: Vec<ModLoaderCompanionInfo>,
}

/// 跟着加载器一起装的附属组件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLoaderCompanionInfo {
    pub label: String,
    pub versions: Vec<String>,
    pub reason: Option<String>,
}

impl From<super::ModLoaderCompanion> for ModLoaderCompanionInfo {
    fn from(companion: super::ModLoaderCompanion) -> Self {
        Self {
            label: companion.label,
            versions: companion.versions,
            reason: companion.reason,
        }
    }
}

impl From<super::ModLoaderChoice> for ModLoaderChoiceInfo {
    fn from(choice: super::ModLoaderChoice) -> Self {
        Self {
            kind: choice.kind,
            label: choice.label,
            available: choice.available,
            reason: choice.reason,
            versions: choice.versions,
            companions: choice
                .companions
                .into_iter()
                .map(ModLoaderCompanionInfo::from)
                .collect(),
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

/// 按加载器种类构造安装计划。计划只是数据，不联网也不写盘。
fn plan_for(
    kind: ModLoaderKind,
    game_version: &str,
    loader_version: &str,
) -> Result<ModLoaderInstallPlan, String> {
    match kind {
        ModLoaderKind::Fabric => {
            let source = MetaSource::new(
                aether_modloader::fabric::DEFAULT_META_BASE,
                HttpMetaFetcher::new(),
            );
            let entry = aether_modloader::fabric::fetch_loader_versions(&source, game_version)
                .map_err(|error| error.to_string())?
                .into_iter()
                .find(|entry| entry.loader_version == loader_version)
                .ok_or_else(|| format!("Fabric {loader_version} 不支持 {game_version}"))?;
            Ok(entry.install_plan(game_version, aether_modloader::fabric::DEFAULT_MAVEN_BASE))
        }
        ModLoaderKind::Forge => {
            let source = MetaSource::new(
                aether_modloader::forge::DEFAULT_MAVEN_BASE,
                HttpMetaFetcher::new(),
            );
            let entry = aether_modloader::forge::fetch_maven_versions(&source)
                .map_err(|error| error.to_string())?
                .into_iter()
                .find(|entry| entry.version == loader_version && entry.game_version == game_version)
                .ok_or_else(|| format!("Forge {loader_version} 不支持 {game_version}"))?;
            Ok(entry.install_plan(aether_modloader::forge::DEFAULT_MAVEN_BASE))
        }
        ModLoaderKind::NeoForge => {
            let source = MetaSource::new(
                aether_modloader::neoforge::DEFAULT_MAVEN_BASE,
                HttpMetaFetcher::new(),
            );
            let entry = aether_modloader::neoforge::fetch_versions(&source)
                .map_err(|error| error.to_string())?
                .into_iter()
                .find(|entry| entry.version == loader_version && entry.game_version == game_version)
                .ok_or_else(|| format!("NeoForge {loader_version} 不支持 {game_version}"))?;
            Ok(entry.install_plan(aether_modloader::neoforge::DEFAULT_MAVEN_BASE))
        }
        ModLoaderKind::Quilt => Err("Quilt 还不支持".to_owned()),
    }
}

/// 下载某个加载器版本的安装器 jar。
///
/// 前端只传加载器种类、游戏版本和加载器版本，URL 由后端按官方 maven 规则从安装
/// 计划里取 —— 前端无法让这里请求任意地址。
#[tauri::command]
pub async fn download_modloader_installer(
    app: tauri::AppHandle,
    kind: String,
    game_version: String,
    loader_version: String,
) -> Result<Vec<String>, String> {
    use tauri::Manager;

    let parsed = super::parse_kind(&kind).ok_or_else(|| format!("unknown mod loader: {kind}"))?;
    let base = app
        .path()
        .app_data_dir()
        .map_err(|_| "application data directory is unavailable".to_owned())?;

    tauri::async_runtime::spawn_blocking(move || {
        let plan = plan_for(parsed, &game_version, &loader_version)?;
        // 计划里同一个版本可能已存在；这里只需要下载步骤能成功。
        plan.validate(&[])
            .map_err(|error| format!("安装计划校验失败：{error}"))?;

        let transport = aether_download::transport::KernelHttpTransport::new(
            aether_download::transport::http::UreqKernel::new(),
        );
        let mut downloaded = Vec::new();
        for step in &plan.steps {
            if let aether_modloader::InstallStep::Download {
                url,
                destination,
                sha1,
                size,
                ..
            } = step
            {
                let path =
                    download_into(&transport, &base, url, destination, sha1.as_deref(), *size)?;
                downloaded.push(path.display().to_string());
            }
        }
        if downloaded.is_empty() {
            return Err("这个加载器版本没有可下载的安装器".to_owned());
        }
        Ok(downloaded)
    })
    .await
    .map_err(|error| format!("mod loader download task failed: {error}"))?
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
