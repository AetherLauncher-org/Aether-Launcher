//! 模组加载器的列表与安装命令。
//!
//! 只走官方源：Fabric 的 `meta.fabricmc.net` 与 `maven.fabricmc.net`、Forge 的
//! `maven.minecraftforge.net`、NeoForge 的 `maven.neoforged.net`。不做镜像切换，
//! 也不接受前端传入的地址。

pub(super) mod execute;
mod fetch;
mod profile_install;

use aether_modloader::transport::MetaSource;
use aether_modloader::{ModLoaderKind, fabric, forge, neoforge};
use serde::Serialize;

use crate::Host;
use crate::install::{ActiveInstall, HostReporter, InstallState};
use aether_download::transport::KernelHttpTransport;
use aether_download::transport::http::UreqKernel;

pub use execute::ModLoaderInstallResult;

pub use fetch::HttpMetaFetcher;

/// 一个加载器在某个游戏版本下的可用情况。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLoaderChoice {
    /// `fabric` / `forge` / `neoforge` / `quilt`
    pub kind: String,
    /// 列表里给人看的名字。
    pub label: String,
    /// 这个游戏版本能不能装；不能时装的原因。
    pub available: bool,
    pub reason: Option<String>,
    /// 可选的加载器版本，最新的排在前面。
    pub versions: Vec<String>,
}

/// 拉取四个加载器对某个游戏版本的可用版本。
///
/// 每个加载器各自独立失败：Fabric 的接口挂了不该让整个列表消失，所以单个加载器
/// 出错只把那一行标成不可用并带上原因。
pub fn list_modloader_choices(game_version: &str) -> Vec<ModLoaderChoice> {
    let mut choices = Vec::new();

    // Fabric：官方元数据直接给出「这个游戏版本有哪些加载器版本」。
    choices.push({
        let source = MetaSource::new(fabric::DEFAULT_META_BASE, HttpMetaFetcher::new());
        match fabric::fetch_loader_versions(&source, game_version) {
            Ok(list) => {
                let versions: Vec<String> =
                    list.into_iter().map(|entry| entry.loader_version).collect();
                ModLoaderChoice {
                    kind: "fabric".to_owned(),
                    label: "Fabric".to_owned(),
                    available: !versions.is_empty(),
                    reason: versions
                        .is_empty()
                        .then(|| format!("Fabric 没有支持 {game_version} 的加载器版本")),
                    versions,
                }
            }
            Err(error) => unavailable("fabric", "Fabric", error),
        }
    });

    // Quilt：接口独立，但本项目还没有 quilt 的版本列表接口，先按不可用列出。
    choices.push(ModLoaderChoice {
        kind: "quilt".to_owned(),
        label: "Quilt".to_owned(),
        available: false,
        reason: Some("Quilt 的元数据接口还没接入".to_owned()),
        versions: Vec::new(),
    });

    // Forge：官方 maven 的 metadata 列出全部版本，按游戏版本前缀筛。
    choices.push({
        let source = MetaSource::new(forge::DEFAULT_MAVEN_BASE, HttpMetaFetcher::new());
        match forge::fetch_maven_versions(&source) {
            Ok(list) => {
                let mut versions: Vec<String> = list
                    .into_iter()
                    .filter(|entry| entry.game_version == game_version)
                    .map(|entry| entry.version)
                    .collect();
                newest_first(&mut versions);
                ModLoaderChoice {
                    kind: "forge".to_owned(),
                    label: "Forge".to_owned(),
                    available: !versions.is_empty(),
                    reason: versions
                        .is_empty()
                        .then(|| format!("Forge 没有支持 {game_version} 的版本")),
                    versions,
                }
            }
            Err(error) => unavailable("forge", "Forge", error),
        }
    });

    // NeoForge：同样的 maven 思路，但它自己的版本号编码里带游戏版本。
    choices.push({
        let source = MetaSource::new(neoforge::DEFAULT_MAVEN_BASE, HttpMetaFetcher::new());
        match neoforge::fetch_versions(&source) {
            Ok(list) => {
                let mut versions: Vec<String> = list
                    .into_iter()
                    .filter(|entry| entry.game_version == game_version)
                    .map(|entry| entry.version)
                    .collect();
                newest_first(&mut versions);
                ModLoaderChoice {
                    kind: "neoforge".to_owned(),
                    label: "NeoForge".to_owned(),
                    available: !versions.is_empty(),
                    reason: versions
                        .is_empty()
                        .then(|| format!("NeoForge 没有支持 {game_version} 的版本")),
                    versions,
                }
            }
            Err(error) => unavailable("neoforge", "NeoForge", error),
        }
    });

    // 不兼容不再由后端写死：PCL 是拿**已选项**去判（`SelectedFabric IsNot Nothing`），
    // 所以只有前端知道该标哪一行，这里只负责给出每个加载器各自的可用性。
    choices
}

/// Sorts loader versions newest first.
///
/// A maven `maven-metadata.xml` lists versions oldest first, and the picker marks
/// `versions[0]` as the latest and preselects it, so Forge and NeoForge have to be put in
/// the order Fabric's API already returns. Dotted numbers compare numerically (`10` after
/// `9`), a version without a suffix is newer than the same numbers with one
/// (`21.0.0` over `21.0.0-beta`), and anything else falls back to text order so the result
/// is always deterministic.
fn newest_first(versions: &mut [String]) {
    versions.sort_by(|a, b| compare_versions(b, a));
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let (a_numbers, a_suffix) = split_version(a);
    let (b_numbers, b_suffix) = split_version(b);
    for index in 0..a_numbers.len().max(b_numbers.len()) {
        let left = a_numbers.get(index).copied().unwrap_or(0);
        let right = b_numbers.get(index).copied().unwrap_or(0);
        if left != right {
            return left.cmp(&right);
        }
    }
    // Same numbers: a release is newer than a pre-release.
    match (a_suffix.is_empty(), b_suffix.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => a_suffix.cmp(b_suffix).then_with(|| a.cmp(b)),
    }
}

/// `"21.0.0-beta"` → `([21, 0, 0], "-beta")`. Stops at the first part that is not a number.
fn split_version(version: &str) -> (Vec<u64>, &str) {
    let mut numbers = Vec::new();
    let mut rest = version;
    loop {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        // `parse` fails on an absurdly long run of digits; treat that as "not a number".
        let Some(number) = rest[..digits].parse::<u64>().ok().filter(|_| digits > 0) else {
            break;
        };
        numbers.push(number);
        rest = &rest[digits..];
        match rest.strip_prefix('.') {
            Some(after) => rest = after,
            None => break,
        }
    }
    (numbers, rest)
}

fn unavailable(kind: &str, label: &str, error: impl std::fmt::Display) -> ModLoaderChoice {
    ModLoaderChoice {
        kind: kind.to_owned(),
        label: label.to_owned(),
        available: false,
        reason: Some(format!("读取官方元数据失败：{error}")),
        versions: Vec::new(),
    }
}

/// 把前端的字符串映射回枚举；只认这四个名字。
#[must_use]
pub fn parse_kind(kind: &str) -> Option<ModLoaderKind> {
    match kind {
        "fabric" => Some(ModLoaderKind::Fabric),
        "forge" => Some(ModLoaderKind::Forge),
        "neoforge" => Some(ModLoaderKind::NeoForge),
        "quilt" => Some(ModLoaderKind::Quilt),
        _ => None,
    }
}

/// 装好一个模组加载器版本（连同它依赖的原版），再为它建实例。
///
/// 调用方只给「哪个加载器 / 哪个游戏版本 / 哪个加载器版本 / 实例叫什么」，URL 全部由
/// 安装计划按官方源拼出来。同一时间只跑一个安装，进度经 `Host::emit_install` 发出。
pub fn install(
    host: &dyn Host,
    state: &InstallState,
    kind: &str,
    game_version: &str,
    loader_version: &str,
    name: &str,
) -> Result<ModLoaderInstallResult, String> {
    let parsed = parse_kind(kind).ok_or_else(|| format!("unknown mod loader: {kind}"))?;
    let base = host.data_dir()?;
    let token = state.begin()?;
    let _active = ActiveInstall(state);
    std::fs::create_dir_all(&base)
        .map_err(|_| "cannot prepare application data directory".to_owned())?;
    let transport = KernelHttpTransport::new(UreqKernel::new());
    let reporter = HostReporter(host);
    execute::install_with(
        &transport,
        execute::InstallRequest {
            base: &base,
            kind: parsed,
            game_version,
            loader_version,
            name,
        },
        &token,
        &reporter,
    )
}

#[cfg(test)]
#[path = "order_tests.rs"]
mod order_tests;
