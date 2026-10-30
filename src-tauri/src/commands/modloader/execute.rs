//! 按 [`InstallStep`] 的顺序执行一份加载器安装计划。
//!
//! `aether-modloader` 只算「要做什么」，这里负责真正联网、写盘、起进程：
//!
//! 1. 先按原版流程装好游戏本体并建实例 —— 加载器的版本 json 都 `inheritsFrom`
//!    原版，没有原版那一层就启动不了；
//! 2. 再逐条执行加载器计划的步骤（下载库 / 跑 installer / 写版本 json）；
//! 3. 把实例的版本改成加载器的版本 id，再激活。
//!
//! 所有写入都限制在 `base/game` 里：`plan.validate` 已经把目标路径约束成安全的
//! 相对路径，这里拼完再检查一次没有跳出根目录。

use std::fs;
use std::path::{Path, PathBuf};

use aether_download::CancellationToken;
use aether_download::transport::KernelHttpTransport;
use aether_download::transport::http::UreqKernel;
use aether_modloader::transport::{MetaError, MetaSource};
use aether_modloader::{InstallStep, ModLoaderInstallPlan, ModLoaderKind, fabric, forge, neoforge};
use aether_storage::{InstanceStore, list_instances, set_instance_version};
use serde::Serialize;

use super::HttpMetaFetcher;
use super::fetch::download_into;
use crate::commands::install::{
    InstallOptions, InstallProgress, Reporter, Stage, install_and_create_with,
};
use crate::commands::instances::InstanceSummary;
use crate::commands::launch::activate_instance;

/// 跑 installer 时传给它的 `--launcherDir` 参数名。
const LAUNCHER_DIR_FLAG: &str = "--launcherDir";

/// 一个加载器版本装好之后返回给前端的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLoaderInstallResult {
    /// 新建的实例（版本已经指向加载器）。
    pub instance: InstanceSummary,
    /// 加载器的版本 id，同时是 `versions/<id>/` 的目录名。
    pub version_id: String,
}

/// 把加载器安装的进度发到前端，复用原版安装的 `install://progress` 事件。
pub(super) struct AppReporter {
    pub app: tauri::AppHandle,
}

impl Reporter for AppReporter {
    fn report(&self, progress: &InstallProgress) {
        use tauri::Emitter;
        let _ = self
            .app
            .emit(crate::commands::install::PROGRESS_EVENT, progress);
    }
}

/// 一次加载器安装的输入。
pub(super) struct InstallRequest<'a> {
    /// 应用数据目录；游戏树是 `<base>/game`。
    pub base: &'a Path,
    pub kind: ModLoaderKind,
    pub game_version: &'a str,
    pub loader_version: &'a str,
    /// 实例名；空字符串时用加载器版本 id。
    pub name: &'a str,
}

/// 安装主流程，**不依赖 Tauri**，便于测试。
///
/// 参数都由命令层给出字符串，URL 一律由安装计划按官方 maven 规则拼出来。
pub(super) fn install_with(
    transport: &KernelHttpTransport<UreqKernel>,
    request: InstallRequest<'_>,
    cancel: &CancellationToken,
    reporter: &dyn Reporter,
) -> Result<ModLoaderInstallResult, String> {
    let base = request.base;
    let kind = request.kind;
    let game_version = request.game_version;
    let loader_version = request.loader_version;
    let name = request.name;
    // 计划先算出来：版本 id 不合法、元数据里没这个版本，都应该在写盘之前就失败。
    let plan = build_plan(kind, game_version, loader_version)?;
    let version_id = plan.target_version_id.clone();
    let instance_name = match name.trim() {
        "" => version_id.clone(),
        trimmed => trimmed.to_owned(),
    };

    let game_root = base.join("game");

    // 原版那一层。已经装过时，`prepare_version` 会命中校验通过的缓存直接复用，
    // 所以这里不需要额外判断目录是否存在。
    install_and_create_with(
        transport,
        base,
        game_version,
        &instance_name,
        InstallOptions {
            seed: None,
            catalog: None,
        },
        cancel,
        reporter,
    )?;

    let instance_id = find_instance_id(base, &instance_name)?;

    // 加载器自己的步骤。
    let total = plan.steps.len();
    for (index, step) in plan.steps.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err("installation cancelled".to_owned());
        }
        step_execute(transport, &game_root, step)
            .map_err(|error| format!("{} 失败：{error}", step.describe()))?;
        reporter.report(&InstallProgress::mod_loader(
            &plan.loader_version,
            Stage::ModLoader,
            index + 1,
            total,
        ));
    }

    reporter.report(&InstallProgress::mod_loader(
        &plan.loader_version,
        Stage::Finalize,
        1,
        1,
    ));

    // 让实例从原版切到加载器版本，再按新版本重建启动信息。
    let store = InstanceStore::open(base).map_err(|error| error.to_string())?;
    let instance = store.load(instance_id).map_err(|error| error.to_string())?;
    let record_dir = base.join("instances").join(instance_id.to_string());
    let updated = set_instance_version(&record_dir, &instance, &version_id)
        .map_err(|error| format!("cannot point instance at {version_id}: {error}"))?;
    activate_instance(base, &game_root, updated.id)
        .map_err(|error| format!("cannot activate instance: {error}"))?;

    let ready = store.load(instance_id).map_err(|error| error.to_string())?;
    Ok(ModLoaderInstallResult {
        instance: InstanceSummary::from_instance(&ready),
        version_id,
    })
}

/// 按加载器种类 + 游戏版本 + 加载器版本构造安装计划，并做静态校验。
pub(super) fn build_plan(
    kind: ModLoaderKind,
    game_version: &str,
    loader_version: &str,
) -> Result<ModLoaderInstallPlan, String> {
    let plan = match kind {
        ModLoaderKind::Fabric => {
            let source = MetaSource::new(fabric::DEFAULT_META_BASE, HttpMetaFetcher::new());
            let entry = fabric::fetch_loader_versions(&source, game_version)
                .map_err(display_meta_error)?
                .into_iter()
                .find(|entry| entry.loader_version == loader_version)
                .ok_or_else(|| format!("Fabric {loader_version} 不支持 {game_version}"))?;
            entry.install_plan(game_version, fabric::DEFAULT_MAVEN_BASE)
        }
        ModLoaderKind::Forge => {
            let source = MetaSource::new(forge::DEFAULT_MAVEN_BASE, HttpMetaFetcher::new());
            let entry = forge::fetch_maven_versions(&source)
                .map_err(display_meta_error)?
                .into_iter()
                .find(|entry| entry.version == loader_version && entry.game_version == game_version)
                .ok_or_else(|| format!("Forge {loader_version} 不支持 {game_version}"))?;
            entry.install_plan(forge::DEFAULT_MAVEN_BASE)
        }
        ModLoaderKind::NeoForge => {
            let source = MetaSource::new(neoforge::DEFAULT_MAVEN_BASE, HttpMetaFetcher::new());
            let entry = neoforge::fetch_versions(&source)
                .map_err(display_meta_error)?
                .into_iter()
                .find(|entry| entry.version == loader_version && entry.game_version == game_version)
                .ok_or_else(|| format!("NeoForge {loader_version} 不支持 {game_version}"))?;
            entry.install_plan(neoforge::DEFAULT_MAVEN_BASE)
        }
        ModLoaderKind::Quilt => return Err("Quilt 还不支持".to_owned()),
    };
    plan.validate(&[])
        .map_err(|error| format!("安装计划校验失败：{error}"))?;
    Ok(plan)
}

fn display_meta_error(error: MetaError) -> String {
    format!("读取官方元数据失败：{error}")
}

/// 执行单条步骤。所有路径都相对 `game_root`。
fn step_execute(
    transport: &KernelHttpTransport<UreqKernel>,
    game_root: &Path,
    step: &InstallStep,
) -> Result<(), String> {
    match step {
        InstallStep::Download {
            url, destination, ..
        } => download_into(transport, game_root, url, destination).map(|_| ()),
        InstallStep::WriteVersionJson { path, content } => {
            let text = serde_json::to_string_pretty(content)
                .map_err(|error| format!("cannot serialise version json: {error}"))?;
            write_file(game_root, path, text.as_bytes())
        }
        InstallStep::WriteFile { path, content } => write_file(game_root, path, content.as_bytes()),
        InstallStep::CopyFile { from, to } => {
            let source = safe_target(game_root, from)?;
            let target = safe_target(game_root, to)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
            }
            fs::copy(&source, &target).map(|_| ()).map_err(|error| {
                format!(
                    "cannot copy {} to {}: {error}",
                    source.display(),
                    target.display()
                )
            })
        }
        InstallStep::CopyLibraries {
            repository,
            coordinates,
        } => copy_libraries(game_root, repository, coordinates),
        InstallStep::RunInstaller {
            jar_path,
            args,
            output_dir,
        } => run_installer(game_root, jar_path, args, output_dir.as_deref()),
    }
}

fn write_file(game_root: &Path, relative: &str, bytes: &[u8]) -> Result<(), String> {
    let target = safe_target(game_root, relative)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(&target, bytes).map_err(|error| format!("cannot write {}: {error}", target.display()))
}

/// 拼路径并确认没有跳出 `game_root`。
pub(super) fn safe_target(game_root: &Path, relative: &str) -> Result<PathBuf, String> {
    // 只按 `starts_with` 判断不够：`"C:/game/../../evil.json"` 的组件以 `game` 开头，
    // `..` 只是普通组件，比较会通过，但落盘时已经跳出根目录。
    // 所以先把 `..` 与 `.` 解析掉，再要求结果仍在根目录之下。
    let mut depth = 0usize;
    let mut push = |segment: &str| -> bool {
        match segment {
            "" | "." => true,
            ".." => {
                if depth == 0 {
                    return false;
                }
                depth -= 1;
                true
            }
            _ => {
                depth += 1;
                true
            }
        }
    };

    // 绝对路径直接拒绝：计划里的目标永远是相对路径。
    if relative.starts_with('/') || relative.starts_with('\\') {
        return Err(format!(
            "refusing to write outside the game directory: {relative}"
        ));
    }
    // Windows 盘符（`C:`）同样算绝对路径。
    let bytes = relative.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Err(format!(
            "refusing to write outside the game directory: {relative}"
        ));
    }

    for segment in relative.split(['/', '\\']) {
        if !push(segment) {
            return Err(format!(
                "refusing to write outside the game directory: {relative}"
            ));
        }
    }

    Ok(game_root.join(relative.replace('\\', "/")))
}

/// 把 maven 缓存里已有的构件复制进 `libraries/`。
///
/// `CopyLibraries` 的语义是搬运**已经在本地 maven 仓库里**的构件；需要联网的
/// 构件由 `Download` 步骤负责，这里不静默联网。
///
/// 缓存位置按 `AETHER_MAVEN_CACHE` → `~/.m2/repository` 的顺序找。
fn copy_libraries(
    game_root: &Path,
    repository: &str,
    coordinates: &[String],
) -> Result<(), String> {
    let cache = maven_cache_root();
    for coordinate in coordinates {
        let Some(relative) = maven_relative_path(coordinate) else {
            return Err(format!("cannot parse maven coordinate: {coordinate}"));
        };
        let target = safe_target(game_root, &format!("libraries/{relative}"))?;
        if target.is_file() {
            continue;
        }
        let Some(source) = cache.as_ref().map(|root| root.join(&relative)) else {
            return Err(format!(
                "{coordinate} 需要从 {repository} 下载，但本机没有 maven 缓存"
            ));
        };
        if !source.is_file() {
            return Err(format!(
                "{coordinate} 不在本机 maven 缓存里（仓库 {repository}）"
            ));
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        fs::copy(&source, &target)
            .map_err(|error| format!("cannot copy {}: {error}", source.display()))?;
    }
    Ok(())
}

fn maven_cache_root() -> Option<PathBuf> {
    if let Some(configured) = std::env::var_os("AETHER_MAVEN_CACHE") {
        return Some(PathBuf::from(configured));
    }
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|home| PathBuf::from(home).join(".m2").join("repository"))
}

/// `group:artifact:version[:classifier]` → `group/path/artifact/version/artifact-version[-classifier].jar`
fn maven_relative_path(coordinate: &str) -> Option<String> {
    let parts: Vec<&str> = coordinate.split(':').collect();
    if parts.len() < 3 || parts.len() > 4 {
        return None;
    }
    let (group, artifact, version) = (parts[0], parts[1], parts[2]);
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    if parts
        .iter()
        .any(|part| part.contains(['/', '\\']) || part.contains(".."))
    {
        return None;
    }
    let classifier = parts
        .get(3)
        .map(|value| format!("-{value}"))
        .unwrap_or_default();
    Some(format!(
        "{}/{artifact}/{version}/{artifact}-{version}{classifier}.jar",
        group.replace('.', "/")
    ))
}

/// 用 Java 跑加载器自带的 installer jar。
///
/// Forge / NeoForge 的 installer 需要 `--launcherDir` 才知道往哪写版本文件，
/// 不给就会写进当前工作目录，所以这里总是把 `game_root` 传进去。
fn run_installer(
    game_root: &Path,
    jar_path: &str,
    args: &[String],
    output_dir: Option<&str>,
) -> Result<(), String> {
    let jar = safe_target(game_root, jar_path)?;
    if !jar.is_file() {
        return Err(format!("{} 不存在，安装器没有下载成功", jar.display()));
    }

    let java = find_java()?;
    let launcher_dir = match output_dir {
        Some(dir) => safe_target(game_root, dir)?,
        None => game_root.to_path_buf(),
    };
    fs::create_dir_all(&launcher_dir)
        .map_err(|error| format!("cannot create {}: {error}", launcher_dir.display()))?;

    // 计划的 args 已经带了 `--installClient`；只在没有时才补 `--launcherDir`。
    let mut command = aether_runtime::hidden_command(&java);
    command.arg("-jar").arg(&jar).args(args);
    if !args.iter().any(|arg| arg == LAUNCHER_DIR_FLAG) {
        command.arg(LAUNCHER_DIR_FLAG).arg(&launcher_dir);
    }
    command.current_dir(game_root);

    let output = command
        .output()
        .map_err(|error| format!("cannot run {}: {error}", java.display()))?;
    if !output.status.success() {
        // installer 的 stderr 是唯一线索，只留最后一段，别把整份日志塞进对话框。
        let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
        if text.trim().is_empty() {
            text = String::from_utf8_lossy(&output.stdout).into_owned();
        }
        return Err(format!(
            "安装器退出码 {:?}：{}",
            output.status.code(),
            tail(&text, 600)
        ));
    }
    Ok(())
}

fn tail(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= limit {
        return trimmed.to_owned();
    }
    trimmed.chars().skip(count - limit).collect()
}

/// 找一个能跑 installer 的 Java：installer 是普通程序，挑主版本最高的那个。
fn find_java() -> Result<PathBuf, String> {
    let mut runtimes = aether_runtime::discover_java_runtimes();
    runtimes.sort_by_key(|runtime| std::cmp::Reverse(runtime.major_version));
    runtimes
        .into_iter()
        .map(|runtime| runtime.executable)
        .next()
        .ok_or_else(|| "找不到可用的 Java，无法运行加载器安装器".to_owned())
}

fn find_instance_id(base: &Path, name: &str) -> Result<aether_core::InstanceId, String> {
    let listing =
        list_instances(base).map_err(|error| format!("cannot list instances: {error}"))?;
    listing
        .instances
        .iter()
        .find(|instance| instance.name == name)
        .map(|instance| instance.id)
        .ok_or_else(|| format!("找不到刚创建的实例：{name}"))
}
#[cfg(test)]
#[path = "execute_tests.rs"]
mod tests;
