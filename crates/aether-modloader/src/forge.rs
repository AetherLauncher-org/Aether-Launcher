//! Forge 加载器：`promotions_slim.json` 与 maven-metadata.xml 的解析、`--game-version`
//! 过滤，以及 installer 的定位与安装计划。
//!
//! Forge 没有统一的「全部版本 + 每版本元数据」JSON API，只有两处数据：
//!
//! - `promotions_slim.json`：每个游戏版本推荐哪个 Forge 版本（`latest` / `recommended` 两个渠道）；
//! - `forge/maven-metadata.xml`：完整的版本坐标列表，坐标形如 `1.19.2-43.5.1`。
//!
//! maven-metadata 只用 `regex` 抽取 `<version>`，不引入 XML 依赖；边界见
//! [`parse_maven_versions`] 的文档。本模块不联网、不执行安装器。

use std::collections::BTreeMap;

use regex::Regex;

use crate::ModLoaderKind;
use crate::installer::{InstallStep, ModLoaderInstallPlan};
use crate::json::parse_json;
use crate::transport::{MetaError, MetaSource};

/// Forge maven 仓库根地址（**不带**结尾斜杠）。
pub const DEFAULT_MAVEN_BASE: &str = "https://maven.minecraftforge.net";

/// `promotions_slim.json` 的默认地址。
pub const DEFAULT_PROMOTIONS_URL: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";

/// maven-metadata.xml 里 `<version>` 元素的正文。
///
/// - `(?s)`：允许元素内部出现换行；
/// - `\b` 保证不会把 `<versions>` 容器当成版本元素；
/// - `[^>]*` 容忍任意属性及其顺序（`<version foo="1">`）；
/// - 正文 `[^<]+?` + 结尾 `</version>` 保证只取完整元素。
const VERSION_PATTERN: &str = r"(?s)<version\b[^>]*>\s*([^<]+?)\s*</version>";

/// 推广渠道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionChannel {
    /// 该游戏版本最新的 Forge 版本
    Latest,
    /// 该游戏版本被标记为推荐的 Forge 版本
    Recommended,
}

impl PromotionChannel {
    /// 渠道在 `promotions_slim.json` 键名里的后缀。
    pub fn key_suffix(self) -> &'static str {
        match self {
            Self::Latest => "latest",
            Self::Recommended => "recommended",
        }
    }

    fn from_suffix(suffix: &str) -> Option<Self> {
        match suffix {
            "latest" => Some(Self::Latest),
            "recommended" => Some(Self::Recommended),
            _ => None,
        }
    }
}

/// `promotions_slim.json` 里的一条推广记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgePromotion {
    /// 游戏版本，例如 `1.20.1`
    pub game_version: String,
    /// 渠道
    pub channel: PromotionChannel,
    /// Forge 版本，例如 `47.4.28`
    pub version: String,
}

/// 解析后的 `promotions_slim.json`。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForgePromotions {
    /// 上游给的 `homepage`，仅用于展示
    pub homepage: Option<String>,
    /// 按游戏版本收集的推广记录
    pub promos: Vec<ForgePromotion>,
}

impl ForgePromotions {
    /// 取某个游戏版本推荐的 Forge 版本：`latest` 优先，没有就退到 `recommended`。
    pub fn for_game(&self, game_version: &str) -> Option<&ForgePromotion> {
        self.promoted(game_version, PromotionChannel::Latest)
            .or_else(|| self.promoted(game_version, PromotionChannel::Recommended))
    }

    /// 取某个游戏版本在指定渠道上的记录。
    pub fn promoted(
        &self,
        game_version: &str,
        channel: PromotionChannel,
    ) -> Option<&ForgePromotion> {
        self.promos.iter().find(|promotion| {
            promotion.game_version == game_version && promotion.channel == channel
        })
    }
}

/// Forge 的两条版本分支。1.13 及以后是 [`ForgeLine::Modern`]。
///
/// 两条分支的差别主要在安装器行为上：现代版用 `--installClient` 生成
/// `versions/<id>/<id>.json`；1.12.2 及更早还需要额外复制 `universal.jar`。
/// 本 crate 只描述步骤，具体路径由上层安装引擎确认。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeLine {
    /// 1.12.2 及更早
    Legacy,
    /// 1.13 及以后（含 `26.3` 这类新版本号体系）
    Modern,
}

/// maven-metadata 里的一个 Forge 坐标，拆成游戏版本与 Forge 版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeVersion {
    /// Forge 版本，例如 `43.5.1`
    pub version: String,
    /// 游戏版本，例如 `1.19.2`
    pub game_version: String,
    /// 版本分支
    pub line: ForgeLine,
}

impl ForgeVersion {
    /// installer jar 的文件名：`forge-<完整坐标>-installer.jar`。
    pub fn installer_file_name(&self) -> String {
        format!("forge-{}-{}-installer.jar", self.game_version, self.version)
    }

    /// installer jar 的下载地址。
    ///
    /// `maven_base` 末尾有没有 `/` 都行，不会拼出双斜杠。
    pub fn installer_url(&self, maven_base: &str) -> String {
        let coordinate = format!("{}-{}", self.game_version, self.version);
        format!(
            "{}/{}/forge/{coordinate}/{}",
            maven_base.trim_end_matches('/'),
            "net/minecraftforge",
            self.installer_file_name()
        )
    }

    /// 安装后写进 `versions/<id>/` 的版本 id：`<game>-forge-<forge 版本>`。
    pub fn target_version_id(&self) -> String {
        format!("{}-forge-{}", self.game_version, self.version)
    }

    /// 安装计划：下载 installer jar，然后运行它。
    ///
    /// 现代版传 `--installClient`；旧版（1.12.2 及更早）的 universal jar 路径依赖
    /// 安装器自身的行为，这里只描述运行 installer，具体补齐由上层负责。
    pub fn install_plan(&self, maven_base: &str) -> ModLoaderInstallPlan {
        let destination = format!("installers/{}", self.installer_file_name());
        let mut plan = ModLoaderInstallPlan::new(
            ModLoaderKind::Forge,
            self.version.clone(),
            self.game_version.clone(),
        )
        .with_target(self.target_version_id());

        plan = plan.push(InstallStep::Download {
            url: self.installer_url(maven_base),
            destination: destination.clone(),
            sha1: None,
            size: None,
            label: format!("Forge {} installer", self.version),
        });

        plan.push(InstallStep::RunInstaller {
            jar_path: destination,
            args: vec!["--installClient".to_owned()],
            output_dir: None,
        })
    }
}

/// 解析 `promotions_slim.json`。
///
/// 结构是 `{"homepage": "...", "promos": {"<game>-<channel>": "<version>"}}`；
/// 键里渠道后缀不是 `latest` / `recommended` 的条目会被跳过，不会报错。
pub fn parse_promotions(input: &str) -> Result<ForgePromotions, MetaError> {
    let value = parse_json(DEFAULT_PROMOTIONS_URL, input)?;
    let homepage = value
        .get("homepage")
        .and_then(|homepage| homepage.as_str())
        .map(str::to_owned);

    let mut promos = Vec::new();
    if let Some(entries) = value.get("promos").and_then(|promos| promos.as_object()) {
        // BTreeMap 让输出顺序稳定，便于测试与快照。
        let ordered: BTreeMap<&String, &serde_json::Value> = entries.iter().collect();
        for (key, version) in ordered {
            let Some((game_version, suffix)) = key.rsplit_once('-') else {
                continue;
            };
            let Some(channel) = PromotionChannel::from_suffix(suffix) else {
                continue;
            };
            let Some(version) = version
                .as_str()
                .filter(|version| !version.trim().is_empty())
            else {
                continue;
            };
            if game_version.trim().is_empty() {
                continue;
            }
            promos.push(ForgePromotion {
                game_version: game_version.to_owned(),
                channel,
                version: version.to_owned(),
            });
        }
    }

    Ok(ForgePromotions { homepage, promos })
}

/// 解析 maven-metadata.xml 里的 Forge 版本列表。
///
/// 用 `regex` 抽取 `<version>`：不依赖属性顺序、换行与标签内空白，也不会把
/// `<versions>` 容器误当成版本。**局限**：这是文本抽取而不是真正的 XML 解析，
/// 如果标签出现在 XML 注释里会被一并取出（上游 metadata 不含注释）。
///
/// 无法拆成 `<游戏版本>-<Forge 版本>` 的条目会被跳过；一条都没有时视为不是
/// maven-metadata，返回 [`MetaError::InvalidXml`]。
pub fn parse_maven_versions(input: &str) -> Result<Vec<ForgeVersion>, MetaError> {
    let pattern = Regex::new(VERSION_PATTERN).map_err(|_| MetaError::InvalidXml {
        url: String::new(),
        reason: "internal error: built-in maven-metadata pattern failed to compile".to_owned(),
    })?;

    let mut versions = Vec::new();
    for capture in pattern.captures_iter(input) {
        let Some(raw) = capture.get(1).map(|found| found.as_str().trim()) else {
            continue;
        };
        let Some((game_version, forge_version)) = split_coordinate(raw) else {
            continue;
        };
        let Some(line) = line_for_game(&game_version) else {
            continue;
        };
        versions.push(ForgeVersion {
            version: forge_version,
            game_version,
            line,
        });
    }

    if versions.is_empty() {
        return Err(MetaError::InvalidXml {
            url: String::new(),
            reason: "no <version> element found in maven metadata".to_owned(),
        });
    }

    Ok(versions)
}

/// 按游戏版本过滤（`--game-version` 的实现），保持原始顺序。
pub fn filter_by_game<'a>(
    versions: &'a [ForgeVersion],
    game_version: &str,
) -> Vec<&'a ForgeVersion> {
    versions
        .iter()
        .filter(|version| version.game_version == game_version)
        .collect()
}

/// 把 maven 坐标 `1.19.2-43.5.1` 拆成 `(游戏版本, Forge 版本)`。
///
/// - `1.19.2-43.5.1` → `("1.19.2", "43.5.1")`
/// - `1.7.10-10.13.4.1614-1.7.10` → `("1.7.10", "10.13.4.1614")`（末尾重复的游戏版本要丢掉）
/// - `1.21-51.0.33` → `("1.21", "51.0.33")`
/// - `26.3-66.1.0` → `("26.3", "66.1.0")`
///
/// 没有 `-`、游戏版本不是「以数字开头且带点」、Forge 版本为空、或游戏版本段里
/// 出现路径分隔符 / 空白时返回 `None`。
pub fn split_coordinate(version: &str) -> Option<(String, String)> {
    let (game_version, rest) = version.split_once('-')?;
    let game_version = game_version.trim();
    let rest = rest.trim();

    if game_version.is_empty() || rest.is_empty() {
        return None;
    }
    if game_version.contains(['/', '\\']) || game_version.chars().any(char::is_whitespace) {
        return None;
    }
    if !game_version.starts_with(|character: char| character.is_ascii_digit())
        || !game_version.contains('.')
    {
        return None;
    }

    // 老坐标会把游戏版本重复在末尾：`1.7.10-10.13.4.1614-1.7.10`。
    let forge_version = match rest.strip_suffix(game_version) {
        Some(head) => head.strip_suffix('-').unwrap_or(head),
        None => rest,
    };
    let forge_version = forge_version.trim();

    if forge_version.is_empty() || forge_version.starts_with('-') {
        return None;
    }
    if forge_version.contains(['/', '\\']) || forge_version.chars().any(char::is_whitespace) {
        return None;
    }

    Some((game_version.to_owned(), forge_version.to_owned()))
}

/// 由游戏版本判断分支；无法判断（不是 `x.y` 形式）时返回 `None`。
fn line_for_game(game_version: &str) -> Option<ForgeLine> {
    let mut parts = game_version.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;

    if major == 1 {
        // 1.13 起是「现代」分支。
        return Some(if minor >= 13 {
            ForgeLine::Modern
        } else {
            ForgeLine::Legacy
        });
    }
    if major >= 22 {
        // `26.3` 这类新版本号体系只有现代版。
        return Some(ForgeLine::Modern);
    }
    None
}

/// 拉取并解析 `promotions_slim.json`。
pub fn fetch_promotions(source: &MetaSource) -> Result<ForgePromotions, MetaError> {
    let body = source.fetch("promotions_slim.json")?;
    let _ = source.url("promotions_slim.json");
    parse_promotions(&body)
}

/// 拉取并解析 Forge 的 `maven-metadata.xml`。
pub fn fetch_maven_versions(source: &MetaSource) -> Result<Vec<ForgeVersion>, MetaError> {
    let path = "net/minecraftforge/forge/maven-metadata.xml";
    let body = source.fetch(path)?;
    parse_maven_versions(&body)
}

#[cfg(test)]
#[path = "forge/forge_tests.rs"]
mod tests;
