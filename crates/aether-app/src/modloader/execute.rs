//! 按 [`InstallStep`] 的顺序执行一份加载器安装计划。
//!
//! `aether-modloader` 只算「要做什么」，这里负责真正联网、写盘、起进程：
//!
//! 1. 先检查实例名，再按原版流程装好游戏本体（不建实例）—— 加载器的版本 json 都
//!    `inheritsFrom` 原版，没有原版那一层就启动不了；
//! 2. 再逐条执行加载器计划的步骤（下载库 / 跑 installer / 写版本 json）；
//! 3. **全部成功之后**才建实例，版本直接指向加载器的版本 id，再激活。
//!
//! 实例放在最后建：中途失败（网络、installer 报错、取消）不会留下一个指向原版、
//! 名字却写着加载器的实例。
//!
//! 所有写入都限制在 `base/game` 里：`plan.validate` 已经把目标路径约束成安全的
//! 相对路径，这里拼完再检查一次没有跳出根目录。

use std::fs;
use std::path::{Path, PathBuf};

use aether_download::transport::KernelHttpTransport;
use aether_download::transport::http::UreqKernel;
use aether_download::{CancellationToken, is_safe_relative_path};
use aether_modloader::transport::{MetaError, MetaSource};
use aether_modloader::{InstallStep, ModLoaderInstallPlan, ModLoaderKind, fabric, forge, neoforge};
use serde::Serialize;

use super::HttpMetaFetcher;
use super::fetch::download_into;
use super::profile_install::{ClientInstall, ProcessorRunner};
use crate::install::{
    InstallOptions, InstallProgress, Reporter, Stage, check_instance_name, create_ready_instance,
    install_game_files,
};
use crate::instances::InstanceSummary;

/// 一个加载器版本装好之后返回给前端的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLoaderInstallResult {
    /// 新建的实例（版本已经指向加载器）。
    pub instance: InstanceSummary,
    /// 加载器的版本 id，同时是 `versions/<id>/` 的目录名。
    pub version_id: String,
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

    // 先查名字：重名或不合法在下载之前就该失败，而不是装完才发现。
    check_instance_name(base, &instance_name)?;

    // 原版那一层。已经装过时，`prepare_version` 会命中校验通过的缓存直接复用，
    // 所以这里不需要额外判断目录是否存在。这一步不建实例。
    install_game_files(
        transport,
        base,
        game_version,
        InstallOptions::default(),
        cancel,
        reporter,
    )?;

    // 加载器自己的步骤。Forge / NeoForge 的安装步骤会报告 installer 里写的真实版本 id，
    // 以它为准建实例。
    let total = plan.steps.len();
    let mut installed_id: Option<String> = None;
    for (index, step) in plan.steps.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err("installation cancelled".to_owned());
        }
        if let Some(id) = step_execute(transport, &game_root, step, cancel)
            .map_err(|error| format!("{} 失败：{error}", step.describe()))?
        {
            installed_id = Some(id);
        }
        reporter.report(&InstallProgress::mod_loader(
            &plan.loader_version,
            Stage::ModLoader,
            index + 1,
            total,
        ));
    }

    // 全部装好了才建实例，版本直接是加载器的版本 id。
    let version_id = installed_id.unwrap_or(version_id);
    let instance = create_ready_instance(base, &instance_name, &version_id, reporter)?;
    Ok(ModLoaderInstallResult {
        instance,
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
///
/// 返回 `Some(id)` 表示这一步写下了一个版本，`id` 是它真实的版本 id。
fn step_execute(
    transport: &KernelHttpTransport<UreqKernel>,
    game_root: &Path,
    step: &InstallStep,
    cancel: &CancellationToken,
) -> Result<Option<String>, String> {
    match step {
        InstallStep::Download {
            url,
            destination,
            sha1,
            size,
            ..
        } => download_into(
            transport,
            game_root,
            url,
            destination,
            sha1.as_deref(),
            *size,
        )
        .map(|_| None),
        InstallStep::WriteVersionJson { path, content } => {
            let text = serde_json::to_string_pretty(content)
                .map_err(|error| format!("cannot serialise version json: {error}"))?;
            write_file(game_root, path, text.as_bytes()).map(|()| None)
        }
        InstallStep::WriteFile { path, content } => {
            write_file(game_root, path, content.as_bytes()).map(|()| None)
        }
        InstallStep::CopyFile { from, to } => {
            let source = safe_target(game_root, from)?;
            let target = safe_target(game_root, to)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
            }
            fs::copy(&source, &target).map(|_| None).map_err(|error| {
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
        } => copy_libraries(repository, coordinates).map(|()| None),
        // 不再把 installer 当黑盒运行：读它的安装描述，自己下载、校验、跑处理器。
        // `--installClient` 之类的参数对官方 installer 才有意义，这里用不到。
        InstallStep::RunInstaller { jar_path, .. } => {
            let installer = safe_target(game_root, jar_path)?;
            let runner = JavaProcessRunner { java: find_java()? };
            ClientInstall {
                transport,
                game_root,
                installer: &installer,
                runner: &runner,
                cancel,
            }
            .run()
            .map(Some)
        }
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
///
/// 规则和原版安装共用 [`is_safe_relative_path`]：拒绝绝对路径、`..`、盘符和数据流
/// （`:`）、Windows 保留名、结尾的点和空格，以及控制字符。只有一份规则，原版和加载器
/// 的写入才不会对「什么算安全」有两种说法。
pub(super) fn safe_target(game_root: &Path, relative: &str) -> Result<PathBuf, String> {
    if !is_safe_relative_path(Path::new(relative)) {
        return Err(format!(
            "refusing to write outside the game directory: {relative}"
        ));
    }
    Ok(game_root.join(relative.replace('\\', "/")))
}

/// `CopyLibraries`：不支持，直接拒绝。
///
/// 这一步原本会从 `AETHER_MAVEN_CACHE` 或 `~/.m2` 把构件复制进 `libraries/`，既没有
/// 哈希校验，来源又是环境变量或用户目录，任何能写那个目录的进程都能往游戏的
/// classpath 里塞代码。现在没有任何安装计划会产出这一步；万一有，宁可失败也不静默
/// 复制未经校验的文件。需要的构件应当走 `Download`，由官方源和校验值把关。
fn copy_libraries(repository: &str, coordinates: &[String]) -> Result<(), String> {
    Err(format!(
        "不支持从本机缓存复制库（{} 个来自 {repository}）：库必须从官方源下载并校验",
        coordinates.len()
    ))
}

/// 用 Java 运行一个处理器：`java -cp <classpath> <main> <args>`。
///
/// 处理器的输出可能很多，所以读输出放在单独的线程里（不然管道写满会把它卡死），
/// 只留最后一小段，出错时拿来当线索。主线程每 100 毫秒看一次进程有没有结束，
/// 有没有被取消；取消就杀掉进程。
struct JavaProcessRunner {
    java: PathBuf,
}

impl ProcessorRunner for JavaProcessRunner {
    fn run(
        &self,
        classpath: &[PathBuf],
        main_class: &str,
        args: &[String],
        cwd: &Path,
        cancel: &CancellationToken,
    ) -> Result<(), String> {
        use std::io::Read;
        use std::process::Stdio;
        use std::thread;
        use std::time::Duration;

        let joined = std::env::join_paths(classpath)
            .map_err(|error| format!("cannot build the classpath: {error}"))?;
        let mut command = aether_runtime::hidden_command(&self.java);
        command
            .arg("-cp")
            .arg(joined)
            .arg(main_class)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("cannot run {}: {error}", self.java.display()))?;

        fn drain(mut pipe: impl Read + Send + 'static) -> thread::JoinHandle<String> {
            thread::spawn(move || {
                let mut kept: Vec<u8> = Vec::new();
                let mut buffer = [0u8; 8192];
                while let Ok(read) = pipe.read(&mut buffer) {
                    if read == 0 {
                        break;
                    }
                    kept.extend_from_slice(&buffer[..read]);
                    if kept.len() > 64 * 1024 {
                        kept.drain(..kept.len() - 16 * 1024);
                    }
                }
                String::from_utf8_lossy(&kept).into_owned()
            })
        }
        let stdout = child.stdout.take().map(drain);
        let stderr = child.stderr.take().map(drain);

        let status = loop {
            if cancel.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err("installation cancelled".to_owned());
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(error) => return Err(format!("cannot wait for the process: {error}")),
            }
        };
        let stdout = stdout
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        let stderr = stderr
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        if status.success() {
            return Ok(());
        }
        let text = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        Err(format!(
            "exit code {:?}: {}",
            status.code(),
            tail(&text, 600)
        ))
    }
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
    // Only Javas that are safe to run: the known install locations (and anything the user
    // imported is deliberately not used here). The Settings rescan also lists folders found
    // on drive roots, but those are never executed, so they must not be picked for this.
    let mut runtimes = aether_runtime::discover_java_runtimes(&[]);
    runtimes.sort_by_key(|runtime| std::cmp::Reverse(runtime.major_version));
    runtimes
        .into_iter()
        .map(|runtime| runtime.executable)
        .next()
        .ok_or_else(|| "找不到可用的 Java，无法运行加载器安装器".to_owned())
}

#[cfg(test)]
#[path = "execute_tests.rs"]
mod tests;
