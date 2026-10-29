//! 校验一个目录是不是**真的游戏目录**。
//!
//! Aether 是独立启动器：游戏目录只来自它自己的实例记录
//! （[`aether_core::GameInstance::game_directory`]），不从别的启动器读配置，
//! 也不扫描磁盘去猜。这个模块只提供那条唯一的判据：
//!
//! > 一个目录算游戏目录，当且仅当 `versions/<版本>/` 真实存在。
//!
//! 它用于两处：
//!
//! - 用户在前端选了一个目录时，写入记录之前先校验一次，选错立刻报错；
//! - 老记录（`game_directory` 字段加入之前创建的实例）没有游戏目录，
//!   只留一个空的 `minecraft/`，此时按版本名在**用户已经选过的目录**里确认
//!   它确实有这个版本，而不是去猜哪个磁盘上有 `.minecraft`。
//!
//! # 边界
//!
//! - 只读；
//! - `versions/` 只做存在性检查，不遍历内容；
//! - 符号链接一律拒绝。

use std::path::{Path, PathBuf};

/// 一个目录里是否真有这个版本。
///
/// 只看 `versions/<version>/` 是否存在且是真实目录；不读里面的 JSON，
/// 也不跟随符号链接。
#[must_use]
pub fn has_version(game_directory: &Path, version: &str) -> bool {
    if !is_usable_version_name(version) {
        return false;
    }
    let Ok(metadata) = std::fs::symlink_metadata(game_directory) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return false;
    }
    let version_directory = game_directory.join("versions").join(version);
    match std::fs::symlink_metadata(&version_directory) {
        Ok(metadata) => metadata.is_dir() && !metadata.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// 一个目录是否像个游戏目录：有 `versions/` 子目录。
///
/// 用户手动选目录时用它做第一道校验（比 [`has_version`] 宽松一个版本）。
#[must_use]
pub fn looks_like_game_directory(directory: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(directory) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return false;
    }
    match std::fs::symlink_metadata(directory.join("versions")) {
        Ok(metadata) => metadata.is_dir() && !metadata.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// 在几个候选目录里找出真的有这个版本的那个。
///
/// 候选来自**用户已经确认过的目录**（实例记录里的游戏目录）。这里不做任何
/// 磁盘搜索：都不匹配就是 `None`，由调用方决定怎么提示。
#[must_use]
pub fn first_directory_with_version(candidates: &[PathBuf], version: &str) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| has_version(candidate, version))
        .cloned()
}

/// 版本名必须能安全地当作一层目录名使用。
fn is_usable_version_name(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 255
        && !version.contains(['/', '\\'])
        && version != "."
        && version != ".."
}

#[cfg(test)]
#[path = "fallback_tests.rs"]
mod tests;
