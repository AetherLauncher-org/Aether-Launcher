//! NeoForge 加载器元数据：`maven-metadata.xml` 解析、游戏版本推导、installer 下载与安装计划。
//!
//! NeoForge 只有 Maven 一种来源，没有 Fabric 那样的 JSON API：
//!
//! - 现代坐标 `net/neoforged/neoforge`，版本形如 `20.4.237`、`21.1.80`、`26.3.0.97`；
//! - 1.20.1 及更早的兼容坐标 `net/neoforged/forge`，版本形如 `1.20.1-47.1.106`。
//!
//! 两个坐标都只提供一个 `maven-metadata.xml`，因此这里**不引入 XML crate**，
//! 只用 [`regex`] 抽取 `<version>` 列表；这种做法的边界见 [`parse_maven_metadata`] 的文档。
//!
//! 本模块只生成「要下载什么、要执行什么」的描述，不访问网络、不执行安装器：
//! 下载与执行由 launcher 按 [`install_plan`](NeoForgeVersion::install_plan) 的步骤自行完成。

use std::sync::OnceLock;

use regex::Regex;

use crate::ModLoaderKind;
use crate::installer::{InstallStep, ModLoaderInstallPlan};
use crate::transport::{MetaError, MetaSource};

/// 默认的 NeoForge Maven 仓库（`/releases` 段之后就是正常的 Maven 相对路径）。
pub const DEFAULT_MAVEN_BASE: &str = "https://maven.neoforged.net/releases";

/// 现代 NeoForge 坐标在 Maven 仓库里的路径。
pub const GROUP_PATH: &str = "net/neoforged/neoforge";

/// 1.20.1 及更早版本的兼容坐标（`net.neoforged:forge`）路径。
pub const LEGACY_GROUP_PATH: &str = "net/neoforged/forge";

/// maven-metadata.xml 里 `<version>` 元素的正文。
///
/// - `(?s)`：允许 `<version>` 与正文之间出现换行、制表符；
/// - `\b` 保证不会把 `<versions>` 容器当成版本；
/// - `[^>]*` 容忍任意属性及属性顺序（`<version foo="1">`）；
/// - 正文 `[^<]+?` + 末尾 `</version>` 保证只取完整元素，`\s*` 吃掉正文两侧空白。
const VERSION_PATTERN: &str = r"(?s)<version\b[^>]*>\s*([^<]+?)\s*</version>";
/// `<groupId>` 正文，`first_capture` 只取第一个。
const GROUP_ID_PATTERN: &str = r"(?s)<groupId\s*[^>]*>\s*([^<]+?)\s*</groupId>";
/// `<artifactId>` 正文，`first_capture` 只取第一个。
const ARTIFACT_ID_PATTERN: &str = r"(?s)<artifactId\s*[^>]*>\s*([^<]+?)\s*</artifactId>";
/// `<latest>` 正文，`first_capture` 只取第一个。
const LATEST_PATTERN: &str = r"(?s)<latest\s*[^>]*>\s*([^<]+?)\s*</latest>";
/// `<release>` 正文，`first_capture` 只取第一个。
const RELEASE_PATTERN: &str = r"(?s)<release\s*[^>]*>\s*([^<]+?)\s*</release>";

/// 编译并缓存一条正则。
///
/// 模式都是本文件里的字面量常量，编译失败属于编程错误而不是输入问题，
/// 但这里仍然**不** `unwrap()` / `expect()`：失败时返回 `None`，由调用方转成
/// [`MetaError::InvalidXml`]（reason 说明是内部正则错误）。这样热路径上不会 panic。
fn compiled(pattern: &'static str) -> Option<&'static Regex> {
    static CELLS: OnceLock<Vec<(&'static str, Option<Regex>)>> = OnceLock::new();
    let cells = CELLS.get_or_init(|| {
        // 固定顺序：版本、groupId、artifactId、latest、release。
        [
            VERSION_PATTERN,
            GROUP_ID_PATTERN,
            ARTIFACT_ID_PATTERN,
            LATEST_PATTERN,
            RELEASE_PATTERN,
        ]
        .into_iter()
        .map(|pattern| (pattern, Regex::new(pattern).ok()))
        .collect()
    });
    cells
        .iter()
        .find(|(key, _)| *key == pattern)
        .and_then(|(_, compiled)| compiled.as_ref())
}

/// 取某条内置正则，编译失败时返回内部错误。
fn regex_for(pattern: &'static str) -> Result<&'static Regex, MetaError> {
    compiled(pattern).ok_or_else(|| MetaError::InvalidXml {
        url: String::new(),
        reason: "internal error: built-in maven-metadata pattern failed to compile".to_string(),
    })
}

/// 取第一个捕获组（已去掉两侧空白）作为字符串。
fn first_capture(regex: &Regex, input: &str) -> Option<String> {
    regex
        .captures(input)
        .and_then(|captures| captures.get(1))
        .map(|group| group.as_str().trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Maven `maven-metadata.xml` 里本模块关心的字段。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MavenMetadata {
    /// `<groupId>`，缺失时为 `None`（不报错）。
    pub group_id: Option<String>,
    /// `<artifactId>`，缺失时为 `None`（不报错）。
    pub artifact_id: Option<String>,
    /// `<latest>`，缺失时为 `None`（不报错）。
    pub latest: Option<String>,
    /// `<release>`，缺失时为 `None`（不报错）。
    pub release: Option<String>,
    /// 全部 `<version>` 正文，按文档顺序；不做去重与排序。
    pub versions: Vec<String>,
}

/// 解析 `maven-metadata.xml`。
///
/// 只用正则抽取，不依赖 XML crate：`<version>` 元素允许带属性、允许属性顺序任意、
/// 允许标签与正文之间有空白或换行，正文两侧空白会被裁掉；`<versions>` 容器本身
/// 不会被当成一个版本（正则要求以 `</version>` 收尾）。
///
/// 失败时返回 [`MetaError::InvalidXml`]。由于本函数的入参**没有 URL 上下文**，
/// 错误里的 `url` 字段统一是空字符串，原因写进 `reason`；需要带 URL 的诊断请用
/// [`fetch_versions`] / [`fetch_legacy_versions`]，它们在拿到响应体后会重写 `url`。
///
/// 输入里完全没有 `<version>` 元素（不是 metadata 文档，或是别的 XML）属于错误，
/// 而不是「空列表」——空的版本列表通常意味着请求到了一个错误页面。
pub fn parse_maven_metadata(input: &str) -> Result<MavenMetadata, MetaError> {
    let version_regex = regex_for(VERSION_PATTERN)?;
    let versions: Vec<String> = version_regex
        .captures_iter(input)
        .filter_map(|captures| captures.get(1))
        .map(|group| group.as_str().trim().to_string())
        .filter(|version| !version.is_empty())
        .collect();
    if versions.is_empty() {
        return Err(MetaError::InvalidXml {
            url: String::new(),
            reason: "no <version> element found; the response is not a maven-metadata.xml document"
                .to_string(),
        });
    }

    let first_matching = |pattern: &'static str| -> Result<Option<String>, MetaError> {
        Ok(first_capture(regex_for(pattern)?, input))
    };

    Ok(MavenMetadata {
        group_id: first_matching(GROUP_ID_PATTERN)?,
        artifact_id: first_matching(ARTIFACT_ID_PATTERN)?,
        latest: first_matching(LATEST_PATTERN)?,
        release: first_matching(RELEASE_PATTERN)?,
        versions,
    })
}

/// NeoForge 版本属于哪个 Maven 坐标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeoForgeArtifact {
    /// `net.neoforged:neoforge`（1.20.2 起的独立发行线）。
    Modern,
    /// `net.neoforged:forge`（1.20.1 及更早的兼容坐标）。
    Legacy,
}

impl NeoForgeArtifact {
    /// 该坐标对应的 Maven 路径前缀。
    pub fn group_path(self) -> &'static str {
        match self {
            Self::Modern => GROUP_PATH,
            Self::Legacy => LEGACY_GROUP_PATH,
        }
    }

    /// 该坐标下 installer jar 的 artifactId（`neoforge` / `forge`）。
    pub fn artifact_id(self) -> &'static str {
        match self {
            Self::Modern => "neoforge",
            Self::Legacy => "forge",
        }
    }
}

/// 一个可安装的 NeoForge 版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeoForgeVersion {
    /// Maven 版本号，例如 `21.1.80`、`1.20.1-47.1.106`。
    pub version: String,
    /// 推导出的 Minecraft 版本，例如 `1.21.1`、`1.20.1`。
    pub game_version: String,
    /// 版本所属坐标。
    pub artifact: NeoForgeArtifact,
    /// 预发布标记（`alpha` / `beta` / `pre` / `snapshot` / `rc`，见 [`is_pre_release`]）。
    pub pre_release: bool,
}

impl NeoForgeVersion {
    /// installer jar 的文件名：现代 `neoforge-<ver>-installer.jar`，legacy `forge-<ver>-installer.jar`。
    ///
    /// 名字来自真实目录列表（`20.2.86` / `20.4.237` / `20.6.119` / `21.1.80` 目录里
    /// 只有 `neoforge-<ver>-installer.jar`，没有额外后缀）。
    pub fn installer_file_name(&self) -> String {
        format!(
            "{}-{}-installer.jar",
            self.artifact.artifact_id(),
            self.version
        )
    }

    /// installer jar 的绝对下载地址。
    ///
    /// `maven_base` 末尾的 `/` 会被裁掉，不会拼出 `//`。
    pub fn installer_url(&self, maven_base: &str) -> String {
        format!(
            "{}/{}/{}/{}",
            maven_base.trim_end_matches('/'),
            self.artifact.group_path(),
            self.version,
            self.installer_file_name()
        )
    }

    /// launcher 里该版本使用的版本 id：两种坐标都是 `neoforge-<ver>`。
    pub fn target_version_id(&self) -> String {
        format!("neoforge-{}", self.version)
    }

    /// 安装计划：先下载 installer jar，再执行 `java -jar ... --installClient`。
    ///
    /// 说明：`RunInstaller` 的参数里游戏目录是一个**占位符**，真正的目录由 launcher 在
    /// 执行前替换（安装计划本身不感知实例路径）；`output_dir` 留空表示使用默认位置。
    /// 这里不编造任何绝对路径。
    pub fn install_plan(&self, maven_base: &str) -> ModLoaderInstallPlan {
        let file_name = self.installer_file_name();
        let destination = format!("installers/{file_name}");
        ModLoaderInstallPlan::new(
            ModLoaderKind::NeoForge,
            self.version.clone(),
            self.game_version.clone(),
        )
        .with_target(self.target_version_id())
        .push(InstallStep::Download {
            url: self.installer_url(maven_base),
            destination: destination.clone(),
            sha1: None,
            size: None,
            label: format!("NeoForge {} installer", self.version),
        })
        .push(InstallStep::RunInstaller {
            jar_path: destination,
            args: vec![
                "--installClient".to_string(),
                GAME_DIR_PLACEHOLDER.to_string(),
            ],
            output_dir: None,
        })
    }
}

/// `RunInstaller` 参数里的游戏目录占位符，由 launcher 在执行前替换成真实路径。
pub const GAME_DIR_PLACEHOLDER: &str = "<game_dir>";

/// 从 NeoForge 版本号推导对应的 Minecraft 版本。
///
/// 规则（对着真实列表核对过）：
///
/// - 主版本 `20..=21`（NeoForge 自己的版本号体系）：`1.<major>.<minor>`，
///   例如 `20.4.237` → `1.20.4`、`20.2.86` → `1.20.2`、`21.1.80` → `1.21.1`；
/// - 主版本 `>= 22`（与 Minecraft 同步的新版本号体系）：`<major>.<minor>`，
///   例如 `26.3.0.97` → `26.3`、`26.1.0.0-alpha.1+snapshot-1` → `26.1`；
/// - 预发布后缀（`-beta`、`+snapshot-1`、`-rc.1`）不影响推导：`20.4.0-beta` → `1.20.4`；
/// - 前导数字段少于 2 个（`20`、`20.4`）或首段不是纯数字（`0.25w14craftmine.3-beta`、`abc.1`）→ `None`。
///
/// 也就是说只有版本号开头连续的 `<数字>.<数字>` 会被使用，后面的段与后缀一概忽略。
pub fn game_version_for(version: &str) -> Option<String> {
    let trimmed = version.trim();
    let mut segments = trimmed.split('.').map(str::trim);
    let major = segments.next()?.parse::<u32>().ok()?;
    // 第二段必须是**完整的十进制数字**：`20.4` 只有两段，按规格判为「段数不足」，
    // 不能推出 `1.20.4`；`0.25w14craftmine.3-beta` 的首段不是数字，已在上一行被拒。
    let minor = segments.next()?.parse::<u32>().ok()?;
    // 至少还要有第三段且非空，`20.4` / `21.1` 这类才能被挡掉。
    if segments.next().is_none_or(str::is_empty) {
        return None;
    }
    match major {
        20..=21 => Some(format!("1.{major}.{minor}")),
        22..=u32::MAX => Some(format!("{major}.{minor}")),
        _ => None,
    }
}

/// 版本号是否是预发布版：不区分大小写地包含 `alpha` / `beta` / `pre` / `snapshot` / `rc`。
///
/// 这是启发式判断（NeoForge 的 maven-metadata 里没有稳定的通道字段）。
pub fn is_pre_release(version: &str) -> bool {
    let lower = version.to_ascii_lowercase();
    ["alpha", "beta", "pre", "snapshot", "rc"]
        .iter()
        .any(|marker| lower.contains(marker))
}

/// 把现代坐标的 `maven-metadata.xml` 变成版本列表。
///
/// 推导不出游戏版本的条目（例如 `0.25w14craftmine.3-beta`）会被**丢弃**而不是报错；
/// 条目的 `artifact` 一律是 [`NeoForgeArtifact::Modern`]。
pub fn parse_versions(input: &str) -> Result<Vec<NeoForgeVersion>, MetaError> {
    let metadata = parse_maven_metadata(input)?;
    Ok(modern_versions(&metadata))
}

/// 现代坐标的版本列表（内部实现，便于 `fetch_*` 重写错误里的 URL）。
fn modern_versions(metadata: &MavenMetadata) -> Vec<NeoForgeVersion> {
    metadata
        .versions
        .iter()
        .filter_map(|version| {
            game_version_for(version).map(|game_version| NeoForgeVersion {
                version: version.clone(),
                game_version,
                artifact: NeoForgeArtifact::Modern,
                pre_release: is_pre_release(version),
            })
        })
        .collect()
}

/// 拆分 legacy 坐标的版本号 `"1.20.1-47.1.106"` → `("1.20.1", "47.1.106")`。
///
/// 只在**第一个** `-` 处切分；左侧必须「以数字开头且包含 `.`」才认为像游戏版本
/// （真实列表里混过裸 `47.1.82` 这种没有游戏版本前缀的条目，不能要）。
pub(crate) fn split_legacy_version(version: &str) -> Option<(&str, &str)> {
    let (game, loader) = version.trim().split_once('-')?;
    let game = game.trim();
    let loader = loader.trim();
    let looks_like_game = game.starts_with(|c: char| c.is_ascii_digit()) && game.contains('.');
    if !looks_like_game || loader.is_empty() {
        return None;
    }
    Some((game, loader))
}

/// 把 legacy 坐标（`net/neoforged/forge`）的 `maven-metadata.xml` 变成版本列表。
///
/// 版本形如 `1.20.1-47.1.106`：`-` 左边是游戏版本，右边是 NeoForge 版本；
/// 左侧不像游戏版本（见 [`split_legacy_version`]）的条目会被丢弃。
/// 条目的 `artifact` 一律是 [`NeoForgeArtifact::Legacy`]，`version` 字段保留完整坐标字符串
/// （installer 的文件名与目录都用完整字符串）。
pub fn parse_legacy_versions(input: &str) -> Result<Vec<NeoForgeVersion>, MetaError> {
    let metadata = parse_maven_metadata(input)?;
    Ok(legacy_versions(&metadata))
}

/// legacy 坐标的版本列表（内部实现，便于 `fetch_*` 重写错误里的 URL）。
fn legacy_versions(metadata: &MavenMetadata) -> Vec<NeoForgeVersion> {
    metadata
        .versions
        .iter()
        .filter_map(|version| {
            let (game_version, _) = split_legacy_version(version)?;
            Some(NeoForgeVersion {
                version: version.clone(),
                game_version: game_version.to_string(),
                artifact: NeoForgeArtifact::Legacy,
                pre_release: is_pre_release(version),
            })
        })
        .collect()
}

/// 只要某个游戏版本的现代 NeoForge 版本（精确匹配 `game_version`，保持原顺序）。
pub fn versions_for_game(
    input: &str,
    game_version: &str,
) -> Result<Vec<NeoForgeVersion>, MetaError> {
    Ok(parse_versions(input)?
        .into_iter()
        .filter(|version| version.game_version == game_version)
        .collect())
}

/// 拉取并解析现代坐标的 `maven-metadata.xml`。
///
/// `source.fetch(&format!("{GROUP_PATH}/maven-metadata.xml"))`；解析失败时把真实的
/// 请求 URL 写进 [`MetaError::InvalidXml::url`]，方便诊断。
pub fn fetch_versions(source: &MetaSource) -> Result<Vec<NeoForgeVersion>, MetaError> {
    fetch_metadata(source, GROUP_PATH).map(|metadata| modern_versions(&metadata))
}

/// 拉取并解析 legacy 坐标（`net/neoforged/forge`）的 `maven-metadata.xml`。
pub fn fetch_legacy_versions(source: &MetaSource) -> Result<Vec<NeoForgeVersion>, MetaError> {
    fetch_metadata(source, LEGACY_GROUP_PATH).map(|metadata| legacy_versions(&metadata))
}

/// 取某个 Maven 坐标目录下的 `maven-metadata.xml` 并解析。
fn fetch_metadata(source: &MetaSource, group_path: &str) -> Result<MavenMetadata, MetaError> {
    let path = format!("{group_path}/maven-metadata.xml");
    let url = source.url(&path);
    let body = source.fetch(&path)?;
    parse_maven_metadata(&body).map_err(|error| with_url(error, &url))
}

/// 给缺少 URL 上下文的解析错误补上真实请求地址（`InvalidXml` 的 `url` 由本模块填空字符串）。
fn with_url(error: MetaError, url: &str) -> MetaError {
    match error {
        MetaError::InvalidXml { reason, .. } => MetaError::InvalidXml {
            url: url.to_string(),
            reason,
        },
        other => other,
    }
}

#[cfg(test)]
#[path = "neoforge/neoforge_tests.rs"]
mod tests;
