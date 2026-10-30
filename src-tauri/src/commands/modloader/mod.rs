//! 模组加载器的列表与安装命令。
//!
//! 只走官方源：Fabric 的 `meta.fabricmc.net`、Forge 的 `files.minecraftforge.net`
//! 与 `maven.minecraftforge.net`、NeoForge 的 `maven.neoforged.net`。不做镜像切换，
//! 也不接受前端传入的地址。

pub mod commands;
pub(super) mod execute;
mod fetch;

use aether_modloader::transport::{MetaFetcher, MetaSource};
use aether_modloader::{ModLoaderKind, fabric, forge, neoforge};
use serde::Serialize;

pub use fetch::HttpMetaFetcher;

/// Fabric API 在 Modrinth 上的 project slug。
const FABRIC_API_PROJECT: &str = "fabric-api";

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
    /// 会连带装上的附属组件。目前只有 Fabric 的 Fabric API。
    ///
    /// 有了这个字段，前端在选择 Fabric 时就能直接看到 Fabric API 的版本，
    /// 不用等用户自己想起来还要另装一个。
    pub companions: Vec<ModLoaderCompanion>,
}

/// 跟着加载器一起装的附属组件。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLoaderCompanion {
    /// 给人看的名字，例如 `Fabric API`。
    pub label: String,
    /// 匹配当前游戏版本的版本号，最新的在前。
    pub versions: Vec<String>,
    /// 读不到版本时的原因；`None` 表示列表正常。
    pub reason: Option<String>,
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
                    // Fabric API 是绝大多数 Fabric 模组的硬依赖，装上加载器就该一起装。
                    companions: vec![fabric_api_companion(game_version)],
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
        companions: Vec::new(),
    });

    // Forge：官方 maven 的 metadata 列出全部版本，按游戏版本前缀筛。
    choices.push({
        let source = MetaSource::new(forge::DEFAULT_MAVEN_BASE, HttpMetaFetcher::new());
        match forge::fetch_maven_versions(&source) {
            Ok(list) => {
                let versions: Vec<String> = list
                    .into_iter()
                    .filter(|entry| entry.game_version == game_version)
                    .map(|entry| entry.version)
                    .collect();
                ModLoaderChoice {
                    kind: "forge".to_owned(),
                    label: "Forge".to_owned(),
                    available: !versions.is_empty(),
                    reason: versions
                        .is_empty()
                        .then(|| format!("Forge 没有支持 {game_version} 的版本")),
                    versions,
                    companions: Vec::new(),
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
                let versions: Vec<String> = list
                    .into_iter()
                    .filter(|entry| entry.game_version == game_version)
                    .map(|entry| entry.version)
                    .collect();
                ModLoaderChoice {
                    kind: "neoforge".to_owned(),
                    label: "NeoForge".to_owned(),
                    available: !versions.is_empty(),
                    reason: versions
                        .is_empty()
                        .then(|| format!("NeoForge 没有支持 {game_version} 的版本")),
                    versions,
                    companions: Vec::new(),
                }
            }
            Err(error) => unavailable("neoforge", "NeoForge", error),
        }
    });

    mark_incompatible(&mut choices);
    choices
}

/// 能在同一个版本上共存的加载器分组：Forge 和 NeoForge 同族（NeoForge 从 Forge
/// fork 出来），Fabric 和 Quilt 同族（Quilt 是 Fabric 的分支）。族内二选一，
/// 跨族互斥。
///
/// 这里是**静态规则**，不依赖网络，所以每个加载器各自失败也不影响判断。
fn loader_family(kind: &str) -> &'static str {
    match kind {
        "forge" | "neoforge" => "forge",
        "fabric" | "quilt" => "fabric",
        _ => "other",
    }
}

/// 给已经在列表里的加载器补上「与 X 不兼容」的说明。
///
/// 只标**需要标的那几个**，否则四行全红等于什么都没说：
///
/// - 族内（Forge ↔ NeoForge、Fabric ↔ Quilt）二选一，不算不兼容，不标。
/// - 跨族才冲突，但只标「不是首选族」的那一族，并且每族只标一个代表。
///
/// 首选族是 Fabric：社区里用得最多，也是这个启动器默认推荐的。所以 Forge 一行
/// 会写「与 Fabric 不兼容」，Fabric 一行保持干净；NeoForge 是 Forge 的同族兄弟，
/// 已经被 Forge 那行代表，不再重复；Quilt 同理跟在 Fabric 后面。
///
/// 对方**确实可装**才会标：对方读不到版本时写「与它不兼容」没有意义，用户也点不了。
fn mark_incompatible(choices: &mut [ModLoaderChoice]) {
    // 每个族里第一个可装的加载器就是这个族的代表。
    let mut representatives: Vec<(String, String)> = Vec::new();
    for choice in choices.iter() {
        if !choice.available || choice.versions.is_empty() {
            continue;
        }
        let family = loader_family(&choice.kind);
        if representatives
            .iter()
            .any(|(kind, _)| loader_family(kind) == family)
        {
            continue;
        }
        representatives.push((choice.kind.clone(), choice.label.clone()));
    }

    // 首选族是 Fabric；它不可装时退回到第一个可装的族。
    let preferred = representatives
        .iter()
        .find(|(kind, _)| loader_family(kind) == "fabric")
        .or_else(|| representatives.first());
    let Some((preferred_kind, preferred_label)) = preferred else {
        return;
    };
    let preferred_family = loader_family(preferred_kind);

    for choice in choices.iter_mut() {
        if !choice.available || choice.versions.is_empty() {
            continue;
        }
        let family = loader_family(&choice.kind);
        // 首选族自己不用标；同族兄弟跟在首选族后面，也不标。
        if family == preferred_family {
            continue;
        }
        // 每个非首选族只标代表的那一个，别把 NeoForge / Quilt 也拉进来。
        let is_representative = representatives.iter().any(|(kind, _)| kind == &choice.kind);
        if !is_representative {
            continue;
        }
        choice.reason = Some(format!("与 {preferred_label} 不兼容"));
    }
}

/// 读 Fabric API 对某个游戏版本的版本列表。
///
/// 走 Modrinth 的公开接口（`https://api.modrinth.com/v2/...`），按游戏版本和
/// `fabric` 加载器筛选。读不到不算错误：`reason` 里写原因，界面照常显示，用户
/// 自己去下载就行。
fn fabric_api_companion(game_version: &str) -> ModLoaderCompanion {
    let label = "Fabric API".to_owned();
    match fetch_modrinth_versions(FABRIC_API_PROJECT, game_version, "fabric") {
        Ok(versions) if !versions.is_empty() => ModLoaderCompanion {
            label,
            versions,
            reason: None,
        },
        Ok(_) => ModLoaderCompanion {
            label,
            versions: Vec::new(),
            reason: Some(format!("Fabric API 没有支持 {game_version} 的版本")),
        },
        Err(reason) => ModLoaderCompanion {
            label,
            versions: Vec::new(),
            reason: Some(reason),
        },
    }
}

/// Modrinth 的公开 v2 接口。
const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";

/// 从 Modrinth 读某个 project 对 `game_version` + `loader` 的版本号，最新的在前。
fn fetch_modrinth_versions(
    project: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<String>, String> {
    let url = format!("{MODRINTH_API_BASE}/project/{project}/version");
    let body = HttpMetaFetcher::new()
        .fetch_text(&url)
        .map_err(|error| format!("读取 {project} 版本失败：{error}"))?;

    let parsed: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("{project} 的版本列表不是合法 JSON：{error}"))?;
    let entries = parsed
        .as_array()
        .ok_or_else(|| format!("{project} 的版本列表不是数组"))?;

    let mut versions = Vec::new();
    for entry in entries {
        let matches_game = entry
            .get("game_versions")
            .and_then(|value| value.as_array())
            .is_some_and(|list| list.iter().any(|item| item.as_str() == Some(game_version)));
        let matches_loader = entry
            .get("loaders")
            .and_then(|value| value.as_array())
            .is_some_and(|list| list.iter().any(|item| item.as_str() == Some(loader)));
        if !matches_game || !matches_loader {
            continue;
        }
        if let Some(version) = entry.get("version_number").and_then(|value| value.as_str()) {
            versions.push(version.to_owned());
        }
    }

    // Modrinth 默认按「最近发布」排，不是按版本号；这里只保证最新的排在最前，
    // 与其它加载器的列表语义一致，不做严格的 semver 比较。
    versions.reverse();
    Ok(versions)
}

fn unavailable(kind: &str, label: &str, error: impl std::fmt::Display) -> ModLoaderChoice {
    ModLoaderChoice {
        kind: kind.to_owned(),
        label: label.to_owned(),
        available: false,
        reason: Some(format!("读取官方元数据失败：{error}")),
        versions: Vec::new(),
        companions: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn choice(kind: &str, label: &str, available: bool) -> ModLoaderChoice {
        ModLoaderChoice {
            kind: kind.to_owned(),
            label: label.to_owned(),
            available,
            reason: None,
            versions: if available {
                vec!["1.0".to_owned()]
            } else {
                Vec::new()
            },
            companions: Vec::new(),
        }
    }

    #[test]
    fn only_the_non_preferred_family_is_flagged() {
        let mut choices = vec![
            choice("forge", "Forge", true),
            choice("neoforge", "NeoForge", true),
            choice("fabric", "Fabric", true),
            choice("quilt", "Quilt", true),
        ];
        mark_incompatible(&mut choices);

        // Fabric 是首选族：自己这一族两个都不标。
        assert_eq!(choices[2].reason, None, "Fabric should stay clean");
        assert_eq!(choices[3].reason, None, "Quilt is Fabric's sibling");
        // Forge 一族：只有代表 Forge 被标，NeoForge 不重复。
        assert_eq!(choices[0].reason.as_deref(), Some("与 Fabric 不兼容"));
        assert_eq!(choices[1].reason, None, "NeoForge follows Forge");
    }

    #[test]
    fn a_lone_family_is_never_flagged() {
        // 只有一个族能装时不存在冲突。
        let mut choices = vec![
            choice("forge", "Forge", true),
            choice("neoforge", "NeoForge", true),
            choice("fabric", "Fabric", false),
            choice("quilt", "Quilt", false),
        ];
        mark_incompatible(&mut choices);

        assert_eq!(choices[0].reason, None);
        assert_eq!(choices[1].reason, None);
    }

    #[test]
    fn an_unavailable_loader_is_not_blamed_for_a_conflict() {
        // Fabric 只是读取失败（不可装），不该让 Forge 背上「与 Fabric 不兼容」。
        let mut choices = vec![
            choice("forge", "Forge", true),
            choice("fabric", "Fabric", false),
        ];
        mark_incompatible(&mut choices);

        assert_eq!(choices[0].reason, None);
        assert_eq!(choices[1].reason, None);
    }
}
