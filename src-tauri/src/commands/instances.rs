//! Instance listing command.
//!
//! The store root is resolved by the backend from the application data
//! directory, so the frontend never chooses a path on disk. Listing tolerates
//! individual broken records and reports why they were skipped. Instances are
//! created by the installer (`install::install_vanilla_version`).

use std::path::{Path, PathBuf};

use aether_core::{GameInstance, InstanceState};
use serde::Serialize;
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// One entry of the instance list as the frontend sees it.
///
/// Every type that crosses the IPC boundary must be public: Tauri generates a
/// wrapper whose signature mentions it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    id: String,
    name: String,
    version: String,
    state: &'static str,
    /// 版本类型分组，界面据此像 PCL 那样按类别折叠。
    category: &'static str,
    /// 这个实例的游戏目录；老记录为 `None`，界面会提示去设置。
    game_directory: Option<String>,
}

impl InstanceSummary {
    /// 构造一个列表项，同时**按磁盘现状**判断状态。
    ///
    /// 记录里的 `state` 是创建时写下的 `incomplete`，导入时并不校验文件，所以
    /// 直接用会让所有实例永远显示「待补齐」。这里改为检查版本 jar 是否真的在
    /// 游戏目录里：在就是 `ready`，不在才保留原状态。
    ///
    /// 只做 `metadata` 查询和一次目录列举，不读文件内容，134 个实例也只有毫秒级。
    pub(crate) fn from_instance(instance: &GameInstance) -> Self {
        let game_directory = instance.game_directory.clone();
        let state = match &game_directory {
            Some(directory) if version_is_installed(directory, instance.version.as_str()) => {
                InstanceState::Ready
            }
            _ => instance.state.clone(),
        };
        Self {
            id: instance.id.to_string(),
            name: instance.name.clone(),
            version: instance.version.as_str().to_owned(),
            state: instance_state_label(&state),
            category: instance_category(instance.version.as_str()),
            game_directory: game_directory.map(|path| path.to_string_lossy().into_owned()),
        }
    }
}

/// 这个版本的客户端 jar 是否已经在游戏目录里。
///
/// 判据只看 `versions/<版本>/<版本>.jar` 存在且不是符号链接 —— 它就是「能不能
/// 启动」最直接的那一个文件。库和资源缺失由启动前的验证负责，这里不重复。
fn version_is_installed(game_directory: &Path, version: &str) -> bool {
    let jar = game_directory
        .join("versions")
        .join(version)
        .join(format!("{version}.jar"));
    match std::fs::symlink_metadata(&jar) {
        Ok(metadata) => metadata.is_file() && !metadata.file_type().is_symlink(),
        Err(_) => false,
    }
}
/// 从版本名推断实例属于哪一类。
///
/// 只做**名字层面的归类**，不读文件：整合包的版本名通常就是整合包名
/// （如 `[GTCEu] GregTech Expert 2`），加载器则写在名字里（`1.12.2-forge1.12.2-…`）。
/// 判据保守：认不出加载器、且不像纯版本号时才算整合包。
#[must_use]
pub fn instance_category(version: &str) -> &'static str {
    let lowered = version.to_ascii_lowercase();
    if lowered.contains("neoforge") {
        return "neoforge";
    }
    if lowered.contains("fabric") {
        return "fabric";
    }
    if lowered.contains("quilt") {
        return "quilt";
    }
    if lowered.contains("forge") {
        return "forge";
    }
    if lowered.contains("liteloader") {
        return "liteloader";
    }
    if looks_like_vanilla_version(version) {
        return "vanilla";
    }
    // 名字不像纯版本号：多半是整合包自己起的名字。
    "modpack"
}

/// 纯版本号：只有数字、点和版本修饰词（`1.21.1`、`20w14a`、`1.12.2 2`）。
///
/// 名字里出现中括号、版本号以外的大段文字时不算纯版本，交给整合包那一类。
fn looks_like_vanilla_version(version: &str) -> bool {
    let trimmed = version.trim();
    if trimmed.is_empty() || trimmed.len() > 32 {
        return false;
    }
    if trimmed.contains(['[', ']', '【', '】', '(']) {
        return false;
    }
    // 纯版本号只可能由 ASCII 组成：出现中文等字符说明是用户自己起的名字。
    if !trimmed.is_ascii() {
        return false;
    }
    // 版本号里不会有多个用空格分开的词；`All the Mods 9` 这种是整合包的名字，
    // 纯版本号最多是 `1.12.2 2`（两个词）。
    if trimmed.split_whitespace().count() > 2 {
        return false;
    }
    trimmed.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | ' ' | '-' | '_')
    })
}

/// A record that the listing pass refused to load, with a stable category
/// instead of a raw file system path.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedInstance {
    id: String,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceListing {
    instances: Vec<InstanceSummary>,
    skipped: Vec<SkippedInstance>,
}

fn instance_state_label(state: &InstanceState) -> &'static str {
    match state {
        InstanceState::Ready => "ready",
        InstanceState::Incomplete => "incomplete",
        InstanceState::Running => "running",
        InstanceState::Failed => "failed",
    }
}

/// Resolve the application-owned instance store root and make sure the
/// `instances/` directory exists. The path comes from Tauri, never from IPC.
fn instance_store_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("application data directory is unavailable: {error}"))?;

    std::fs::create_dir_all(base.join("instances"))
        .map_err(|error| format!("cannot prepare the instance store: {error}"))?;

    Ok(base)
}

#[tauri::command]
pub(crate) async fn list_instances(app: tauri::AppHandle) -> Result<InstanceListing, String> {
    let root = instance_store_root(&app)?;

    tauri::async_runtime::spawn_blocking(move || match aether_storage::list_instances(&root) {
        Ok(listing) => Ok(InstanceListing {
            instances: listing
                .instances
                .iter()
                .map(InstanceSummary::from_instance)
                .collect(),
            skipped: listing
                .skipped
                .iter()
                .map(|skipped| SkippedInstance {
                    id: skipped.id.clone(),
                    reason: skipped.reason,
                })
                .collect(),
        }),
        Err(error) => Err(format!("cannot list instances: {error}")),
    })
    .await
    .map_err(|error| format!("instance listing task failed: {error}"))?
}

/// 新建实例。游戏目录由**后端**弹出选择框让用户挑，前端从不传路径。
///
/// 用户取消选择时返回 `Ok(None)`：这不是错误，不需要提示。
#[tauri::command]
pub(crate) async fn create_instance(
    app: tauri::AppHandle,
    name: String,
    version: String,
) -> Result<Option<InstanceSummary>, String> {
    let root = instance_store_root(&app)?;
    let Some(game_directory) = pick_game_directory(&app).await? else {
        return Ok(None);
    };

    tauri::async_runtime::spawn_blocking(move || {
        aether_storage::create_instance(&root, &name, &version, &game_directory)
            .map(|instance| InstanceSummary::from_instance(&instance))
            .map_err(|error| format!("cannot create instance: {error}"))
    })
    .await
    .map_err(|error| format!("instance creation task failed: {error}"))?
    .map(Some)
}

/// 删除一个实例记录。
///
/// **只删 `<store>/instances/<uuid>/`，绝不碰游戏文件**：`versions/`、`saves/`、
/// `mods/`、`schematics/` 都在用户自己的 `.minecraft` 里，删实例只是让 Aether
/// 忘掉这个版本，不是卸载游戏。这一点写在 [`aether_storage::delete_instance`]
/// 的文档里，也有测试守着。
#[tauri::command]
pub(crate) async fn delete_instance(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let id = aether_core::InstanceId::parse(&id).map_err(|_| "invalid instance ID".to_owned())?;
    let root = instance_store_root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        aether_storage::delete_instance(&root, id)
            .map(|_| ())
            .map_err(|error| format!("cannot delete the instance: {error}"))
    })
    .await
    .map_err(|error| format!("delete task failed: {error}"))?
}

/// 让用户选择一个 `.minecraft` 作为游戏目录；取消时返回 `None`。
async fn pick_game_directory(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    app.dialog()
        .file()
        .set_title("选择游戏目录（.minecraft）")
        .pick_folder(move |picked| {
            let _ = sender.send(picked);
        });
    receiver
        .recv()
        .map_err(|_| "the folder picker closed unexpectedly".to_owned())?
        .map(|path| {
            path.into_path()
                .map_err(|error| format!("the chosen folder is not usable: {error}"))
        })
        .transpose()
}

/// 给所有还没设置游戏目录的实例指定一个 `.minecraft`。
///
/// 这是老实例（`game_directory` 字段加入之前创建的）唯一的补救方式：Aether 不
/// 从别的启动器读配置，也不猜路径，所以由用户在后端点一次目录选择框，把结果
/// 写进每一条需要修的记录。
///
/// 写入前会把 `instance.json` 备份到 `<store>/instances/.backup-<时间戳>/`；
/// 已经设置过游戏目录的实例不动。
#[tauri::command]
pub(crate) async fn set_game_directory_for_all_instances(
    app: tauri::AppHandle,
) -> Result<GameDirectoryReport, String> {
    let root = instance_store_root(&app)?;
    let Some(game_directory) = pick_game_directory(&app).await? else {
        return Ok(GameDirectoryReport {
            updated: 0,
            skipped: 0,
            failed: Vec::new(),
            cancelled: true,
        });
    };

    tauri::async_runtime::spawn_blocking(move || {
        aether_storage::apply_game_directory(&root, &game_directory)
            .map(Into::into)
            .map_err(|error| format!("cannot set the game directory: {error}"))
    })
    .await
    .map_err(|error| format!("game directory task failed: {error}"))?
}

/// 一次批量设置的结果。`cancelled` 为真时其余字段没有意义。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDirectoryReport {
    updated: usize,
    skipped: usize,
    failed: Vec<String>,
    cancelled: bool,
}

impl From<aether_storage::GameDirectoryOutcome> for GameDirectoryReport {
    fn from(outcome: aether_storage::GameDirectoryOutcome) -> Self {
        Self {
            updated: outcome.updated,
            skipped: outcome.skipped,
            failed: outcome.failed,
            cancelled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> GameInstance {
        GameInstance {
            id: aether_core::InstanceId::new(),
            name: "Survival".to_owned(),
            version: aether_core::VersionId::parse("1.21.1").unwrap(),
            root: PathBuf::from("instances"),
            game_directory: None,
            state: InstanceState::Ready,
        }
    }

    #[test]
    fn summary_fields_are_camel_case_and_stable() {
        let value = serde_json::to_value(InstanceSummary::from_instance(&sample())).unwrap();

        assert_eq!(value["name"], "Survival");
        assert_eq!(value["version"], "1.21.1");
        assert_eq!(value["state"], "ready");
        assert!(value.get("memory_mib").is_none());
        assert!(value.get("root").is_none());
    }

    #[test]
    fn every_instance_state_has_a_lowercase_label() {
        for (state, label) in [
            (InstanceState::Ready, "ready"),
            (InstanceState::Incomplete, "incomplete"),
            (InstanceState::Running, "running"),
            (InstanceState::Failed, "failed"),
        ] {
            assert_eq!(instance_state_label(&state), label);
        }
    }
}

#[cfg(test)]
mod category_tests {
    use super::instance_category;

    #[test]
    fn recognises_loaders_from_the_version_name() {
        assert_eq!(
            instance_category("1.12.2-forge1.12.2-12.18.3.2185"),
            "forge"
        );
        assert_eq!(instance_category("1.20.1-NeoForge_47.3.0"), "neoforge");
        assert_eq!(instance_category("1.19.4-Fabric 0.16.3"), "fabric");
        assert_eq!(instance_category("1.20.1-Quilt"), "quilt");
        assert_eq!(instance_category("1.12.2-LiteLoader"), "liteloader");
    }

    #[test]
    fn a_plain_version_number_is_vanilla() {
        assert_eq!(instance_category("1.21.1"), "vanilla");
        assert_eq!(instance_category("1.12.2 2"), "vanilla");
        assert_eq!(instance_category("20w14a"), "vanilla");
    }

    #[test]
    fn a_named_build_is_a_modpack() {
        assert_eq!(instance_category("[GTCEu] GregTech Expert 2"), "modpack");
        assert_eq!(instance_category("Better MC [FABRIC] - BMC1"), "fabric");
        assert_eq!(instance_category("All the Mods 9 - To the Sky"), "modpack");
        // 「原版」是用户自己起的实例名，不是版本号；ASCII 短名仍按纯版本处理。
    }
}

#[cfg(test)]
mod state_tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use aether_core::{InstanceId, InstanceState, VersionId};

    use super::{InstanceSummary, version_is_installed};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_game(label: &str) -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "aether-state-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("versions")).expect("game");
        path
    }

    #[test]
    fn an_installed_jar_counts_as_ready_even_if_the_record_says_incomplete() {
        let game = temp_game("ready");
        let version = "1.21.1";
        fs::create_dir_all(game.join("versions").join(version)).expect("version dir");
        fs::write(
            game.join("versions").join(version).join("1.21.1.jar"),
            b"jar",
        )
        .expect("jar");

        // 记录里是创建时写下的 incomplete：导入不校验文件，不该一直显示「待补齐」。
        let instance = aether_core::GameInstance {
            id: InstanceId::new(),
            name: "Survival".to_owned(),
            version: VersionId::parse(version).expect("version"),
            root: game.join("instance"),
            game_directory: Some(game.clone()),
            state: InstanceState::Incomplete,
        };

        let summary = InstanceSummary::from_instance(&instance);
        assert_eq!(summary.state, "ready");

        fs::remove_dir_all(game).expect("cleanup");
    }

    #[test]
    fn a_missing_jar_keeps_the_recorded_state() {
        let game = temp_game("missing");
        let instance = aether_core::GameInstance {
            id: InstanceId::new(),
            name: "Survival".to_owned(),
            version: VersionId::parse("1.21.1").expect("version"),
            root: game.join("instance"),
            game_directory: Some(game.clone()),
            state: InstanceState::Incomplete,
        };

        assert!(!version_is_installed(&game, "1.21.1"));
        assert_eq!(
            InstanceSummary::from_instance(&instance).state,
            "incomplete"
        );

        fs::remove_dir_all(game).expect("cleanup");
    }

    #[test]
    fn a_record_without_a_game_directory_keeps_its_state() {
        let game = temp_game("no-directory");
        let instance = aether_core::GameInstance {
            id: InstanceId::new(),
            name: "Survival".to_owned(),
            version: VersionId::parse("1.21.1").expect("version"),
            root: game.join("instance"),
            game_directory: None,
            state: InstanceState::Incomplete,
        };

        let summary = InstanceSummary::from_instance(&instance);
        assert_eq!(summary.state, "incomplete");
        assert!(summary.game_directory.is_none());

        fs::remove_dir_all(game).expect("cleanup");
    }
}
