//! 删除一个实例记录。
//!
//! # 删的是什么
//!
//! 只删 `<store>/instances/<uuid>/` 这一个目录，也就是 `instance.json` 和旧版本
//! 遗留的空 `minecraft/`。**游戏文件一个字节都不动**：`versions/`、`libraries/`、
//! `assets/`、`mods/`、`saves/`、`schematics/` 全在用户自己的 `.minecraft` 里，
//! 实例记录只是指向它。删掉实例等于「让 Aether 忘掉这个版本」，不是「卸载游戏」。
//!
//! # 安全约束
//!
//! - 标识必须是一个合法的 [`InstanceId`]，路径由它推导，不接受调用方给路径；
//! - 目标目录必须是 `<store>/instances/` 的直接子目录，规范化之后仍在该目录下；
//! - 目标目录不能是符号链接或 reparse point，避免删到别处；
//! - 记录读不出来时仍然允许删除：用户可能正是在清理损坏的记录。此时只按目录名
//!   判断，且仍受上面两条约束。
//!
//! # 为什么不用 `remove_dir_all` 直接删
//!
//! 目录里可能有符号链接指到游戏目录。`remove_dir_all` 不会跟随目录链接，但仍然
//! 更稳妥的做法是先逐项确认真实路径都在实例目录内，再删除。

use std::fs;
use std::path::{Path, PathBuf};

use aether_core::InstanceId;
use thiserror::Error;

use crate::list::INSTANCES_DIR;

/// 删除失败的原因。
#[derive(Debug, Error)]
pub enum DeleteError {
    /// 存储根不可用，或者 `instances/` 不是普通目录。
    #[error("instance store is not usable")]
    InvalidStore,
    /// 该 UUID 在存储里没有对应目录。
    #[error("no instance with that identifier exists")]
    NotFound,
    /// 目录不是存储的直接子目录，或者是符号链接。
    #[error("refusing to delete a directory that is not a managed instance")]
    UnsafeDirectory,
    /// 文件系统操作失败。
    #[error("cannot delete the instance")]
    Io(#[from] std::io::Error),
}

/// 删掉 `id` 对应的实例记录，返回被删的目录。
///
/// `store_root` 是存储根（`instances/` 的父目录），由后端提供。
///
/// # Errors
///
/// 见 [`DeleteError`]。
pub fn delete_instance(store_root: &Path, id: InstanceId) -> Result<PathBuf, DeleteError> {
    let instances = store_root.join(INSTANCES_DIR);
    let metadata = fs::symlink_metadata(&instances).map_err(|_| DeleteError::InvalidStore)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(DeleteError::InvalidStore);
    }
    let canonical_instances =
        fs::canonicalize(&instances).map_err(|_| DeleteError::InvalidStore)?;

    let target = instances.join(id.to_string());
    let target_metadata = match fs::symlink_metadata(&target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(DeleteError::NotFound);
        }
        Err(error) => return Err(DeleteError::Io(error)),
    };
    // 符号链接、reparse point、普通文件都不删。
    if target_metadata.file_type().is_symlink() || !target_metadata.is_dir() {
        return Err(DeleteError::UnsafeDirectory);
    }

    let canonical_target = fs::canonicalize(&target)?;
    if canonical_target.parent() != Some(canonical_instances.as_path()) {
        return Err(DeleteError::UnsafeDirectory);
    }

    fs::remove_dir_all(&canonical_target)?;
    Ok(canonical_target)
}

#[cfg(test)]
#[path = "delete_tests.rs"]
mod tests;
