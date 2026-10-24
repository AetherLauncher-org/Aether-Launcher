//! Quilt 加载器：解析 `meta.quiltmc.org` 的版本列表，并据此组装版本 JSON 与安装计划。
//!
//! 结构上与 [`crate::fabric`] 同源，但有三处实质差异：
//!
//! 1. Quilt 的 API 是 `/v3/`，Fabric 是 `/v2/`；
//! 2. Quilt 多一个 `hashed` 段（替代 Fabric 的 `intermediary` 搭配方式），坐标是
//!    `org.quiltmc:hashed`，仓库在 Quilt maven；
//! 3. **Quilt 的 `loader` 段没有 `stable` 字段**，因此 [`QuiltLoaderVersion::stable`]
//!    是按版本号里的预发布标记推断的启发式结果，不是上游给的布尔值。
//!
//! 本模块自身不联网：要么解析调用方给的字符串（`parse_*`），要么通过注入的
//! [`MetaSource`] 取文本（`fetch_*`），测试里用 `StaticFetcher` 提供内存响应。

use std::collections::HashSet;

use serde_json::Value;

use crate::ModLoaderKind;
use crate::fabric::maven_relative_path;
use crate::installer::{InstallStep, ModLoaderInstallPlan};
use crate::json::{inherited_version, loader_library, opt_string, parse_json};
use crate::transport::{MetaError, MetaSource};

/// Quilt meta 的默认根地址（**不带**结尾斜杠，配 [`MetaSource`] 使用）。
pub const DEFAULT_META_BASE: &str = "https://meta.quiltmc.org";

/// Quilt 官方 maven 仓库（**带**结尾斜杠，可以直接拼相对路径）。
pub const QUILT_MAVEN_BASE: &str = "https://maven.quiltmc.org/repository/release/";

/// Fabric 官方 maven 仓库。Quilt 的 `intermediary` 与部分公共库仍托管在这里。
pub const FABRIC_MAVEN_BASE: &str = "https://maven.fabricmc.net/";

/// 客户端主类。`launcherMeta.mainClass.client` 缺失时用它兜底。
pub const MAIN_CLASS_CLIENT: &str = "org.quiltmc.loader.impl.launch.knot.KnotClient";

const GAME_VERSIONS_PATH: &str = "v3/versions/game";
const LOADER_VERSIONS_PATH: &str = "v3/versions/loader";

/// 预发布标记：版本号里出现这些片段时，[`QuiltLoaderVersion::stable`] 视为 `false`。
const PRE_RELEASE_MARKERS: [&str; 4] = ["-alpha", "-beta", "-pre", "-rc"];

/// `GET /v3/versions/game` 里的一项。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuiltGameVersion {
    /// 游戏版本号，例如 `1.21.1`
    pub version: String,
    /// Quilt 官方标为正式版；快照是 `false`
    pub stable: bool,
}

/// 加载器在 `launcherMeta.libraries` 里声明的一个额外库。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QuiltLibrary {
    /// maven 坐标，例如 `org.quiltmc:quilt-json5:1.0.4+final`
    pub name: String,
    /// 仓库地址；老条目可能没有
    pub url: Option<String>,
}

/// `GET /v3/versions/loader/<game>` 里的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuiltLoaderVersion {
    /// 加载器版本，例如 `0.20.0-beta.9`
    pub loader_version: String,
    /// intermediary 版本，Quilt 官方数据里通常与游戏版本相同
    pub intermediary_version: String,
    /// `hashed` 段的版本（Quilt 用它替代 Fabric 的 intermediary 搭配方式）
    pub hashed_version: String,
    /// 是否为正式版。
    ///
    /// **这是启发式推断**：Quilt 的 `loader` 段没有 `stable` 字段，这里按版本号里是否
    /// 含 `-alpha` / `-beta` / `-pre` / `-rc` 判断，含则为 `false`。
    pub stable: bool,
    /// `launcherMeta.libraries` 的 `client`、`common`、`server` 三个数组里的坐标
    /// （顺序 `client` → `common` → `server`，按 `name` 去重保留第一次出现），
    /// `development` 里的开发期库不收集。
    pub libraries: Vec<QuiltLibrary>,
    /// 客户端主类；`launcherMeta.mainClass` 缺失时是 [`MAIN_CLASS_CLIENT`]
    pub main_class: String,
}

impl Default for QuiltLoaderVersion {
    fn default() -> Self {
        Self {
            loader_version: String::new(),
            intermediary_version: String::new(),
            hashed_version: String::new(),
            stable: false,
            libraries: Vec::new(),
            main_class: MAIN_CLASS_CLIENT.to_owned(),
        }
    }
}

impl QuiltLoaderVersion {
    /// 安装后写进 `versions/<id>/` 的版本 id：`quilt-loader-<loader>-<game>`。
    pub fn target_version_id(&self, game_version: &str) -> String {
        format!("quilt-loader-{}-{}", self.loader_version, game_version)
    }

    /// 生成子版本 JSON（`inheritsFrom` 指向原版游戏版本）。
    ///
    /// 库的顺序：`quilt-loader`、`hashed`、`intermediary` 三件核心库在最前，
    /// 随后是 [`QuiltLoaderVersion::libraries`]，按 `name` 去重；不写 `arguments`
    /// （参数由被继承的原版版本提供）。
    pub fn to_version_json(&self, game_version: &str) -> serde_json::Value {
        let id = self.target_version_id(game_version);
        let mut seen: HashSet<String> = HashSet::new();
        let mut libraries = Vec::new();

        for (coordinate, repository) in self.core_libraries() {
            if seen.insert(coordinate.clone()) {
                libraries.push(loader_library(&coordinate, Some(repository)));
            }
        }
        for library in &self.libraries {
            if seen.insert(library.name.clone()) {
                libraries.push(loader_library(&library.name, library.url.as_deref()));
            }
        }

        inherited_version(&id, game_version, &self.main_class, &libraries)
    }

    /// 安装计划：每个能定位到的库一条下载步骤，最后写入版本 JSON。
    ///
    /// - 下载目标：`libraries/<maven 相对路径>`，URL 用库自己的仓库地址；仓库地址是
    ///   [`QUILT_MAVEN_BASE`] 或 [`FABRIC_MAVEN_BASE`] 时换成调用方给的 `maven_base`，
    ///   方便走镜像。
    /// - 没有仓库地址的库不生成下载步骤（它们的 jar 由被继承的原版版本提供）。
    /// - 最后一步 `WriteVersionJson` 写 `versions/<id>/<id>.json`。
    pub fn install_plan(&self, game_version: &str, maven_base: &str) -> ModLoaderInstallPlan {
        let target = self.target_version_id(game_version);
        let mut plan = ModLoaderInstallPlan::new(
            ModLoaderKind::Quilt,
            self.loader_version.clone(),
            game_version.to_owned(),
        )
        .with_target(target.clone())
        .with_main_class(self.main_class.clone());

        for library in &self.libraries {
            let Some(relative) = maven_relative_path(&library.name) else {
                continue;
            };
            let Some(repository) = repository_base(library.url.as_deref(), maven_base) else {
                continue;
            };
            plan = plan.push(InstallStep::Download {
                url: join_repository(&repository, &relative),
                destination: format!("libraries/{relative}"),
                sha1: None,
                size: None,
                label: library.name.clone(),
            });
        }

        plan.push(InstallStep::WriteVersionJson {
            path: format!("versions/{target}/{target}.json"),
            content: self.to_version_json(game_version),
        })
    }

    /// 三件核心库的坐标及其仓库（版本号为空时跳过对应的一件）。
    fn core_libraries(&self) -> Vec<(String, &'static str)> {
        let mut core = Vec::new();
        if !self.loader_version.trim().is_empty() {
            core.push((
                format!("org.quiltmc:quilt-loader:{}", self.loader_version),
                QUILT_MAVEN_BASE,
            ));
        }
        if !self.hashed_version.trim().is_empty() {
            core.push((
                format!("org.quiltmc:hashed:{}", self.hashed_version),
                QUILT_MAVEN_BASE,
            ));
        }
        if !self.intermediary_version.trim().is_empty() {
            core.push((
                format!("net.fabricmc:intermediary:{}", self.intermediary_version),
                FABRIC_MAVEN_BASE,
            ));
        }
        core
    }
}

/// 解析 `GET /v3/versions/game` 的响应；空数组是合法结果，不是数组则报 [`MetaError::InvalidJson`]。
///
/// 缺 `stable` 的条目标成 `false`，缺 `version` 的条目直接跳过。
pub fn parse_game_versions(input: &str) -> Result<Vec<QuiltGameVersion>, MetaError> {
    let url = format!("{DEFAULT_META_BASE}/{GAME_VERSIONS_PATH}");
    let value = parse_json(&url, input)?;
    let entries = as_array(&url, &value)?;

    let mut versions = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(version) = opt_string(entry, "version").filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        versions.push(QuiltGameVersion {
            version,
            stable: entry
                .get("stable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(versions)
}

/// 解析 `GET /v3/versions/loader/<game>` 的响应。
///
/// 容错：缺 `launcherMeta`、缺 `launcherMeta.mainClass`、缺 `hashed`、库缺 `url` 都能解析；
/// 缺 `loader.version` 的条目跳过；缺 `intermediary.version` 时退回 `game_version`。
pub fn parse_loader_versions(
    game_version: &str,
    input: &str,
) -> Result<Vec<QuiltLoaderVersion>, MetaError> {
    let url = format!("{DEFAULT_META_BASE}/{LOADER_VERSIONS_PATH}/{game_version}");
    let value = parse_json(&url, input)?;
    let entries = as_array(&url, &value)?;

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
        let hashed_version = entry
            .get("hashed")
            .and_then(|hashed| opt_string(hashed, "version"))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_default();

        versions.push(QuiltLoaderVersion {
            stable: is_stable(&loader_version),
            loader_version,
            intermediary_version,
            hashed_version,
            libraries: parse_libraries(entry.get("launcherMeta")),
            main_class: parse_main_class(entry.get("launcherMeta")),
        });
    }
    Ok(versions)
}

/// 拉取 `GET /v3/versions/game`。
pub fn fetch_game_versions(source: &MetaSource) -> Result<Vec<QuiltGameVersion>, MetaError> {
    let body = source.fetch(GAME_VERSIONS_PATH)?;
    let url = source.url(GAME_VERSIONS_PATH);
    let value = parse_json(&url, &body)?;
    let entries = as_array(&url, &value)?;

    let mut versions = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(version) = opt_string(entry, "version").filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        versions.push(QuiltGameVersion {
            version,
            stable: entry
                .get("stable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        });
    }
    Ok(versions)
}

/// 拉取 `GET /v3/versions/loader/<game>`。
pub fn fetch_loader_versions(
    source: &MetaSource,
    game: &str,
) -> Result<Vec<QuiltLoaderVersion>, MetaError> {
    let path = format!("{LOADER_VERSIONS_PATH}/{game}");
    let body = source.fetch(&path)?;
    let url = source.url(&path);
    let value = parse_json(&url, &body)?;
    let entries = as_array(&url, &value)?;

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
            .unwrap_or_else(|| game.to_owned());
        let hashed_version = entry
            .get("hashed")
            .and_then(|hashed| opt_string(hashed, "version"))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_default();

        versions.push(QuiltLoaderVersion {
            stable: is_stable(&loader_version),
            loader_version,
            intermediary_version,
            hashed_version,
            libraries: parse_libraries(entry.get("launcherMeta")),
            main_class: parse_main_class(entry.get("launcherMeta")),
        });
    }
    Ok(versions)
}

/// 判断加载器版本是否为正式版。
///
/// Quilt 的上游数据没有 `stable` 字段，这里按版本号里的预发布片段推断：
/// 含 `-alpha`、`-beta`、`-pre`、`-rc` 视为预发布。这是**启发式**，不是权威值。
pub fn is_stable(loader_version: &str) -> bool {
    let lowered = loader_version.to_ascii_lowercase();
    !PRE_RELEASE_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker))
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
fn parse_libraries(launcher_meta: Option<&Value>) -> Vec<QuiltLibrary> {
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
            libraries.push(QuiltLibrary {
                url: opt_string(entry, "url").filter(|url| !url.trim().is_empty()),
                name,
            });
        }
    }
    libraries
}

/// `mainClass` 既可能是 `{"client": "...", "server": "..."}`，也可能是纯字符串。
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

/// 下载仓库地址：库自带地址优先；Quilt / Fabric 官方 maven 换成调用方给的 `maven_base`
/// （可指向镜像）；没有地址的库返回 `None`。
fn repository_base(library_url: Option<&str>, maven_base: &str) -> Option<String> {
    let library_url = library_url.map(str::trim).filter(|url| !url.is_empty())?;
    if same_repository(library_url, QUILT_MAVEN_BASE)
        || same_repository(library_url, FABRIC_MAVEN_BASE)
    {
        let maven_base = maven_base.trim();
        if maven_base.is_empty() {
            Some(library_url.to_owned())
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

#[cfg(test)]
#[path = "quilt/quilt_tests.rs"]
mod tests;
