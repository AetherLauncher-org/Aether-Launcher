//! 给已有实例补上游戏目录。
//!
//! `game_directory` 字段加入之前创建的实例没有游戏目录，只留下一个空的
//! `minecraft/`。Aether 不从别的启动器读配置，也不猜路径，所以唯一的补救方式
//! 是用户明确选一次 `.minecraft`，然后把这些记录补上。
//!
//! 这个模块负责那次批量写回：
//!
//! 1. 校验目录确实是游戏目录（有 `versions/`）；
//! 2. 把**所有要改的记录**先备份到 `<store>/instances/.backup-<时间戳>/`；
//! 3. 逐条改用 [`crate::update::set_instance_game_directory`]，它走和重命名、改
//!    版本相同的原子写入路径；
//! 4. 已经设置过游戏目录的实例不动，损坏的记录记为失败而不是中断整批。
//!
//! 备份是刻意的：这一步会改用户已有的记录，出错时必须能退回去。

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::instances::InstanceStore;
use crate::update::{self, UpdateError};

/// 一次批量设置的结果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GameDirectoryOutcome {
    /// 被改写的记录数。
    pub updated: usize,
    /// 本来就有游戏目录、因此没动的实例数。
    pub skipped: usize,
    /// 改不动的实例，附带稳定原因的字符串。
    pub failed: Vec<String>,
    /// 备份目录；没有需要改的记录时为 `None`。
    pub backup: Option<PathBuf>,
}

/// 批量设置失败的原因（单条记录失败不算，记进 [`GameDirectoryOutcome::failed`]）。
#[derive(Debug, Error)]
pub enum GameDirectoryError {
    /// 选中的目录不是游戏目录，或者存储根不可用。
    #[error("the chosen directory is not a usable game directory")]
    InvalidGameDirectory,
    /// 读取实例列表失败。
    #[error("cannot read the instance store")]
    InvalidStore,
    /// 备份失败：不继续改写任何记录。
    #[error("cannot back up the instance records")]
    BackupFailed,
    /// 目录操作失败。
    #[error("cannot rewrite the instance records")]
    Io(#[from] std::io::Error),
}

/// 把 `game_directory` 写进所有还没有游戏目录的实例。
///
/// # Errors
///
/// 见 [`GameDirectoryError`]。单条记录失败会被收集进结果，不影响其余记录。
pub fn apply_game_directory(
    store_root: &Path,
    game_directory: &Path,
) -> Result<GameDirectoryOutcome, GameDirectoryError> {
    if !crate::import::fallback::looks_like_game_directory(game_directory) {
        return Err(GameDirectoryError::InvalidGameDirectory);
    }
    let game_directory = fs::canonicalize(game_directory)?;

    let store = InstanceStore::open(store_root).map_err(|_| GameDirectoryError::InvalidStore)?;
    let instances =
        crate::list_instances(store_root).map_err(|_| GameDirectoryError::InvalidStore)?;

    let pending: Vec<_> = instances
        .instances
        .iter()
        .filter(|instance| instance.game_directory.is_none())
        .cloned()
        .collect();

    let mut outcome = GameDirectoryOutcome {
        skipped: instances.instances.len() - pending.len(),
        ..GameDirectoryOutcome::default()
    };
    if pending.is_empty() {
        return Ok(outcome);
    }

    // 先全部备份成功，才开始改；备份失败就整体放弃。
    let backup = backup_records(store_root, &pending)?;
    outcome.backup = Some(backup);

    for instance in &pending {
        // 重新读一次：备份和改写之间如果记录变了，以磁盘上的为准。
        let Ok(current) = store.load(instance.id) else {
            outcome
                .failed
                .push(format!("{}: unreadable record", instance.id));
            continue;
        };
        if current.game_directory.is_some() {
            outcome.skipped += 1;
            continue;
        }
        match update::set_instance_game_directory(&current.root, &current, &game_directory) {
            Ok(_) => outcome.updated += 1,
            Err(error) => outcome
                .failed
                .push(format!("{}: {}", instance.id, describe(&error))),
        }
    }

    Ok(outcome)
}

/// 把每条记录的 `instance.json` 复制进一个带时间戳的备份目录。
fn backup_records(
    store_root: &Path,
    instances: &[aether_core::GameInstance],
) -> Result<PathBuf, GameDirectoryError> {
    let instances_directory = store_root.join(crate::create::INSTANCES_DIR);
    let backup = instances_directory.join(format!(".backup-{}", timestamp()));
    fs::create_dir_all(&backup)?;

    for instance in instances {
        let source = instance.root.join(update::INSTANCE_RECORD_FILE);
        let Some(name) = instance.root.file_name() else {
            continue;
        };
        let destination = backup.join(name);
        fs::create_dir_all(&destination)?;
        match fs::copy(&source, destination.join(update::INSTANCE_RECORD_FILE)) {
            Ok(_) => {}
            // 读不到记录时仍然留下目录，让「备份了什么」一目了然。
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(GameDirectoryError::BackupFailed),
        }
    }

    Ok(backup)
}

fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn describe(error: &UpdateError) -> String {
    match error {
        UpdateError::UnsafeDirectory => "unsafe-directory".to_owned(),
        UpdateError::InvalidName(_) => "invalid-name".to_owned(),
        UpdateError::TooLarge => "record-too-large".to_owned(),
        UpdateError::Io(_) | UpdateError::Serialize(_) => "io".to_owned(),
    }
}

#[cfg(test)]
#[path = "game_directory_tests.rs"]
mod tests;
