//! Fabric 加载器：解析 `meta.fabricmc.net` 的版本列表，并据此组装版本 JSON 与安装计划。
//!
//! 本模块自身不联网：要么解析调用方给的字符串（`parse_*`），要么通过注入的
//! [`MetaSource`] 取文本（`fetch_*`），测试里用 `StaticFetcher` 提供内存响应。

use std::collections::HashSet;

use serde_json::Value;

use crate::ModLoaderKind;
use crate::installer::{InstallStep, ModLoaderInstallPlan};
use crate::json::{
    inherited_version_with_arguments, loader_library, opt_bool, opt_string, parse_json,
};
use crate::transport::{MetaError, MetaSource};

/// Fabric meta 的默认根地址（**不带**结尾斜杠，配 [`MetaSource`] 使用）。
pub const DEFAULT_META_BASE: &str = "https://meta.fabricmc.net";

/// Fabric 官方 maven 仓库（**带**结尾斜杠，可以直接拼相对路径）。
pub const DEFAULT_MAVEN_BASE: &str = "https://maven.fabricmc.net/";

/// 客户端主类。`launcherMeta.mainClass.client` 缺失时用它兜底。
pub const MAIN_CLASS_CLIENT: &str = "net.fabricmc.loader.impl.launch.knot.KnotClient";

/// 原版客户端主类。26.x 起 Fabric 没有 intermediary 映射，需要靠
/// -DFabricMcEmu 指定它，否则加载器找不到入口。
pub const MAIN_CLASS_VANILLA: &str = "net.minecraft.client.main.Main";

const GAME_VERSIONS_PATH: &str = "v2/versions/game";
const LOADER_VERSIONS_PATH: &str = "v2/versions/loader";

/// `GET /v2/versions/game` 里的一项。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FabricGameVersion {
    /// 游戏版本号，例如 `1.21.1`、`26.4-snapshot-5`
    pub version: String,
    /// Fabric 官方标为正式版；快照是 `false`
    pub stable: bool,
}

/// 加载器在 `launcherMeta.libraries` 里声明的一个额外库。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FabricLibrary {
    /// maven 坐标，例如 `org.ow2.asm:asm:9.10.1`
    pub name: String,
    /// 仓库地址；0.1.x 这类老加载器的条目可能没有
    pub url: Option<String>,
    /// 文件大小（字节），缺失为 `None`
    pub size: Option<u64>,
    /// 官方给的文件摘要。Fabric 的 `launcherMeta` 里带 `sha1`，
    /// 有就必须校验 —— 这是唯一可信的哈希来源。
    pub sha1: Option<String>,
}

/// `GET /v2/versions/loader/<game>` 里的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FabricLoaderVersion {
    /// 加载器版本，例如 `0.20.1`
    pub loader_version: String,
    /// intermediary 版本，Fabric 官方数据里通常与游戏版本相同
    pub intermediary_version: String,
    /// 加载器自身标记的正式版；字段缺失按 `false` 处理
    pub stable: bool,
    /// `launcherMeta.libraries` 的 `client`、`common`、`server` 三个数组里的坐标
    /// （按 `client` → `common` → `server`、数组内原顺序，按 `name` 去重保留第一次出现），
    /// `development` 里的开发期库不收集。
    pub libraries: Vec<FabricLibrary>,
    /// 客户端主类；`launcherMeta.mainClass` 缺失时是 [`MAIN_CLASS_CLIENT`]
    pub main_class: String,
}

impl Default for FabricLoaderVersion {
    fn default() -> Self {
        Self {
            loader_version: String::new(),
            intermediary_version: String::new(),
            stable: false,
            libraries: Vec::new(),
            main_class: MAIN_CLASS_CLIENT.to_owned(),
        }
    }
}

impl FabricLoaderVersion {
    /// 安装后写进 `versions/<id>/` 的版本 id：`fabric-loader-<loader>-<game>`。
    pub fn target_version_id(&self, game_version: &str) -> String {
        format!("fabric-loader-{}-{}", self.loader_version, game_version)
    }

    /// 生成子版本 JSON（`inheritsFrom` 指向原版游戏版本）。
    ///
    /// 库的顺序：`intermediary` 和 `fabric-loader` 两件核心库在最前（都指向
    /// [`DEFAULT_MAVEN_BASE`]），随后是 [`FabricLoaderVersion::libraries`]，按 `name` 去重；
    /// 不写 `arguments`（参数由被继承的原版版本提供）。
    pub fn to_version_json(&self, game_version: &str) -> serde_json::Value {
        let id = self.target_version_id(game_version);
        let mut seen: HashSet<String> = HashSet::new();
        let mut libraries = Vec::new();

        // 库的顺序照官方 profile：`launcherMeta` 里的库在前，`fabric-loader` 在
        // **最后**；有 intermediary 时它排在最前面。
        //
        // 顺序不是无关紧要的：`fabric-loader` 自己要在 classpath 上晚于它依赖的
        // asm / mixin 出现，位置换掉会让 loader 在 26.x 上找不到原版主类
        // （`no such method: ...Main.main(String[])`）。
        if self.has_real_intermediary() {
            let coordinate = format!("net.fabricmc:intermediary:{}", self.intermediary_version);
            if seen.insert(coordinate.clone()) {
                libraries.push(loader_library(&coordinate, Some(DEFAULT_MAVEN_BASE)));
            }
        }
        for library in &self.libraries {
            if seen.insert(library.name.clone()) {
                libraries.push(loader_library(&library.name, library.url.as_deref()));
            }
        }
        if !self.loader_version.trim().is_empty() {
            let coordinate = format!("net.fabricmc:fabric-loader:{}", self.loader_version);
            if seen.insert(coordinate.clone()) {
                libraries.push(loader_library(&coordinate, Some(DEFAULT_MAVEN_BASE)));
            }
        }

        // 版本 JSON 必须带上官方 profile 的 JVM 参数。26.x 起 Fabric 没有
        // intermediary 映射，`net.minecraft.client.main.Main` 只能靠
        // `FabricMcEmu` 定位；缺了它启动会报
        // `no such method: ...Main.main(String[])void/invokeStatic`。
        inherited_version_with_arguments(
            &id,
            game_version,
            &self.main_class,
            &libraries,
            self.jvm_arguments(),
        )
    }

    /// 官方 profile 里的 `arguments.jvm`。
    ///
    /// 目前只有 `-DFabricMcEmu= net.minecraft.client.main.Main` 这一条，
    /// 取自 `GET /v2/versions/loader/<game>/<loader>/profile/json`。
    fn jvm_arguments(&self) -> serde_json::Value {
        if self.has_real_intermediary() {
            // 有 intermediary 映射时加载器能自己找到主类，官方 profile 也不带参数。
            return serde_json::Value::Null;
        }
        serde_json::json!({
            "game": [],
            "jvm": [format!("-DFabricMcEmu= {}", MAIN_CLASS_VANILLA)],
        })
    }

    /// 安装计划：每个能定位到的库一条下载步骤，最后写入版本 JSON。
    ///
    /// 核心两件（`intermediary`、`fabric-loader`）与 `launcherMeta` 里的库都会被写进版本
    /// JSON，所以**两者都必须生成下载步骤** —— 只写 JSON 不下载，启动时就会报
    /// 「required local file does not exist」。
    ///
    /// - 下载目标：`libraries/<maven 相对路径>`，URL 用库自己的仓库地址；仓库地址是官方
    ///   [`DEFAULT_MAVEN_BASE`] 时换成调用方给的 `maven_base`，方便走镜像。
    /// - 没有仓库地址的库（例如老加载器里的 `net.minecraft:launchwrapper`）来自被继承的
    ///   原版版本，这里不生成下载步骤。
    /// - 最后一步 `WriteVersionJson` 写 `versions/<id>/<id>.json`。
    pub fn install_plan(&self, game_version: &str, maven_base: &str) -> ModLoaderInstallPlan {
        let target = self.target_version_id(game_version);
        let mut plan = ModLoaderInstallPlan::new(
            ModLoaderKind::Fabric,
            self.loader_version.clone(),
            game_version.to_owned(),
        )
        .with_target(target.clone())
        .with_main_class(self.main_class.clone());

        // 核心两件（intermediary、fabric-loader）与 launcherMeta 里的库都会被写进版本
        // JSON，所以两者都必须生成下载步骤 —— 只写 JSON 不下载，启动时就会报
        // 「required local file does not exist」。
        let core = self.core_libraries();
        for coordinate in &core {
            let Some(relative) = maven_relative_path(coordinate) else {
                continue;
            };
            plan = plan.push(InstallStep::Download {
                url: join_repository(maven_base, &relative),
                destination: format!("libraries/{relative}"),
                sha1: None,
                size: None,
                label: coordinate.clone(),
            });
        }

        for library in &self.libraries {
            // 核心两件已经在上面按 maven_base 加过了，避免重复下载。
            if core.contains(&library.name) {
                continue;
            }
            let Some(relative) = maven_relative_path(&library.name) else {
                continue;
            };
            let Some(repository) = repository_base(library.url.as_deref(), maven_base) else {
                continue;
            };
            plan = plan.push(InstallStep::Download {
                url: join_repository(&repository, &relative),
                destination: format!("libraries/{relative}"),
                sha1: library.sha1.clone(),
                size: library.size,
                label: library.name.clone(),
            });
        }

        plan.push(InstallStep::WriteVersionJson {
            path: format!("versions/{target}/{target}.json"),
            content: self.to_version_json(game_version),
        })
    }

    /// 这个加载器版本有没有无法安装的已知问题。
    ///
    /// 目前没有：`intermediary` 是占位版本 `0.0.0` 只说明这个游戏版本不需要它
    /// （26.x 起 Fabric 改用官方映射），官方 profile 里也同样没有这一件，
    /// 照常安装即可 —— 见 [`Self::has_real_intermediary`]。
    pub fn unsupported_reason(&self) -> Option<String> {
        None
    }

    /// 核心库坐标：`fabric-loader` 一定需要，`intermediary` 只在有真实映射时才加。
    ///
    /// 26.x 起 Fabric 改用官方映射，接口仍会在 `intermediary` 字段里填占位版本
    /// `0.0.0`（那是 572 字节的空壳），但官方 profile 已经不再包含它。这里跟官方
    /// profile 保持一致：占位版本直接跳过，而不是把它写进版本 JSON。
    fn core_libraries(&self) -> Vec<String> {
        let mut core = Vec::new();
        if self.has_real_intermediary() {
            core.push(format!(
                "net.fabricmc:intermediary:{}",
                self.intermediary_version
            ));
        }
        if !self.loader_version.trim().is_empty() {
            core.push(format!(
                "net.fabricmc:fabric-loader:{}",
                self.loader_version
            ));
        }
        core
    }

    /// intermediary 是否是一个真实存在的映射。
    ///
    /// `0.0.0` 是 Fabric 对「这个游戏版本不再需要 intermediary」给出的占位值；
    /// 空串表示接口没给。两种情况都不该去下载它。
    fn has_real_intermediary(&self) -> bool {
        let version = self.intermediary_version.trim();
        !version.is_empty() && version != "0.0.0"
    }
}

/// 解析 `GET /v2/versions/game` 的响应；空数组是合法结果，不是数组则报 [`MetaError::InvalidJson`]。
///
/// 缺 `stable` 的条目标成 `false`，缺 `version` 的条目直接跳过。
pub fn parse_game_versions(input: &str) -> Result<Vec<FabricGameVersion>, MetaError> {
    parse_game_versions_at(&format!("{DEFAULT_META_BASE}/{GAME_VERSIONS_PATH}"), input)
}

/// 解析 `GET /v2/versions/loader/<game>` 的响应。
///
/// 容错：缺 `launcherMeta`、缺 `launcherMeta.mainClass`、缺 `loader.stable` /
/// `intermediary.stable`、库缺 `url` / `size` 都能解析；缺 `loader.version` 的条目跳过，
/// 缺 `intermediary.version` 时退回 `game_version`（Fabric 官方数据里两者相同）。
pub fn parse_loader_versions(
    game_version: &str,
    input: &str,
) -> Result<Vec<FabricLoaderVersion>, MetaError> {
    let url = format!("{DEFAULT_META_BASE}/{LOADER_VERSIONS_PATH}/{game_version}");
    parse_loader_versions_at(&url, game_version, input)
}

/// 拉取 `GET /v2/versions/game`。
pub fn fetch_game_versions(source: &MetaSource) -> Result<Vec<FabricGameVersion>, MetaError> {
    let body = source.fetch(GAME_VERSIONS_PATH)?;
    parse_game_versions_at(&source.url(GAME_VERSIONS_PATH), &body)
}

/// 拉取 `GET /v2/versions/loader/<game>`。
pub fn fetch_loader_versions(
    source: &MetaSource,
    game: &str,
) -> Result<Vec<FabricLoaderVersion>, MetaError> {
    let path = format!("{LOADER_VERSIONS_PATH}/{game}");
    let body = source.fetch(&path)?;
    parse_loader_versions_at(&source.url(&path), game, &body)
}

/// 把 maven 坐标换成仓库内的相对路径。
///
/// `group:artifact:version[:classifier][@ext]` →
/// `<group 的 . 换成 />/<artifact>/<version>/<artifact>-<version>[-<classifier>].jar`，
/// 例如 `org.ow2.asm:asm:9.6` → `org/ow2/asm/asm/9.6/asm-9.6.jar`。
///
/// 段数超过 4、有空段、或某段含 `/`、`\`、控制字符、`.`
/// [`classifier`] 之类会越出目录的内容时返回 `None`。`@ext` 后缀会被去掉并按 `.jar` 处理
/// （Fabric 的库里没有非 jar 坐标）。
pub(crate) fn maven_relative_path(coordinate: &str) -> Option<String> {
    let coordinate = match coordinate.split_once('@') {
        Some((head, _extension)) => head,
        None => coordinate,
    };

    let mut parts = coordinate.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    let classifier = parts.next().filter(|part| !part.is_empty());
    if parts.next().is_some() {
        return None;
    }
    if !safe_maven_segment(artifact) || !safe_maven_segment(version) {
        return None;
    }
    if !group.split('.').all(safe_maven_segment) {
        return None;
    }
    if classifier.is_some_and(|part| !safe_maven_segment(part)) {
        return None;
    }

    let mut file_name = format!("{artifact}-{version}");
    if let Some(classifier) = classifier {
        file_name.push('-');
        file_name.push_str(classifier);
    }
    file_name.push_str(".jar");

    Some(format!(
        "{}/{artifact}/{version}/{file_name}",
        group.replace('.', "/")
    ))
}

fn safe_maven_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.contains(['/', '\\'])
        && !segment.chars().any(char::is_control)
}

/// 下载仓库地址：库自带地址优先；官方 maven 换成调用方给的 `maven_base`（可指向镜像）；
/// 没有地址的库返回 `None`（它的 jar 由被继承的原版版本提供）。
fn repository_base(library_url: Option<&str>, maven_base: &str) -> Option<String> {
    let library_url = library_url.map(str::trim).filter(|url| !url.is_empty())?;
    if same_repository(library_url, DEFAULT_MAVEN_BASE) {
        let maven_base = maven_base.trim();
        if maven_base.is_empty() {
            Some(DEFAULT_MAVEN_BASE.to_owned())
        } else {
            Some(maven_base.to_owned())
        }
    } else {
        Some(library_url.to_owned())
    }
}

fn same_repository(left: &str, right: &str) -> bool {
    left.trim_end_matches('/') == right.trim_end_matches('/')
}

fn join_repository(repository: &str, relative: &str) -> String {
    let repository = repository.trim_end_matches('/');
    format!("{repository}/{relative}")
}

fn parse_game_versions_at(url: &str, input: &str) -> Result<Vec<FabricGameVersion>, MetaError> {
    let value = parse_json(url, input)?;
    let entries = as_array(url, &value)?;

    let mut versions = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(version) = opt_string(entry, "version").filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        versions.push(FabricGameVersion {
            version,
            stable: opt_bool(entry, "stable").unwrap_or(false),
        });
    }
    Ok(versions)
}

fn parse_loader_versions_at(
    url: &str,
    game_version: &str,
    input: &str,
) -> Result<Vec<FabricLoaderVersion>, MetaError> {
    let value = parse_json(url, input)?;
    let entries = as_array(url, &value)?;

    let mut versions = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(loader) = entry.get("loader") else {
            continue;
        };
        let Some(loader_version) =
            opt_string(loader, "version").filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let intermediary_version = entry
            .get("intermediary")
            .and_then(|intermediary| opt_string(intermediary, "version"))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| game_version.to_owned());

        versions.push(FabricLoaderVersion {
            loader_version,
            intermediary_version,
            stable: opt_bool(loader, "stable").unwrap_or(false),
            libraries: parse_libraries(entry.get("launcherMeta")),
            main_class: parse_main_class(entry.get("launcherMeta")),
        });
    }
    Ok(versions)
}

fn as_array<'a>(url: &str, value: &'a Value) -> Result<&'a [Value], MetaError> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| MetaError::InvalidJson {
            url: url.to_owned(),
            reason: "expected a JSON array".to_owned(),
        })
}

/// 收集 `launcherMeta.libraries` 里的库，顺序 `client` → `common` → `server`，按 `name` 去重。
fn parse_libraries(launcher_meta: Option<&Value>) -> Vec<FabricLibrary> {
    const GROUPS: [&str; 3] = ["client", "common", "server"];

    let Some(groups) = launcher_meta.and_then(|meta| meta.get("libraries")) else {
        return Vec::new();
    };

    let mut seen: HashSet<String> = HashSet::new();
    let mut libraries = Vec::new();
    for group in GROUPS {
        let Some(entries) = groups.get(group).and_then(Value::as_array) else {
            continue;
        };
        for entry in entries {
            let Some(name) = opt_string(entry, "name").filter(|name| !name.trim().is_empty())
            else {
                continue;
            };
            if !seen.insert(name.clone()) {
                continue;
            }
            libraries.push(FabricLibrary {
                url: opt_string(entry, "url").filter(|url| !url.trim().is_empty()),
                size: entry.get("size").and_then(Value::as_u64),
                sha1: opt_string(entry, "sha1").filter(|sha1| !sha1.trim().is_empty()),
                name,
            });
        }
    }
    libraries
}

/// `mainClass` 既可能是 `{"client": "...", "server": "..."}`，也可能是老版本的纯字符串。
fn parse_main_class(launcher_meta: Option<&Value>) -> String {
    let Some(main_class) = launcher_meta.and_then(|meta| meta.get("mainClass")) else {
        return MAIN_CLASS_CLIENT.to_owned();
    };
    match main_class {
        Value::String(class) if !class.trim().is_empty() => class.clone(),
        Value::Object(_) => opt_string(main_class, "client")
            .filter(|class| !class.trim().is_empty())
            .unwrap_or_else(|| MAIN_CLASS_CLIENT.to_owned()),
        _ => MAIN_CLASS_CLIENT.to_owned(),
    }
}

#[cfg(test)]
#[path = "fabric/fabric_tests.rs"]
mod tests;
