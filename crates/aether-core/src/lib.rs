use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InstanceId(Uuid);

impl InstanceId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(value).map(Self)
    }
}

impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionId(String);

impl VersionId {
    pub fn parse(value: impl Into<String>) -> Result<Self, CoreError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(CoreError::EmptyVersionId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceState {
    Ready,
    Incomplete,
    Running,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameInstance {
    pub id: InstanceId,
    pub name: String,
    pub version: VersionId,
    /// 实例记录自己的目录：`<base>/instances/<uuid>`。只放 `instance.json` 这类
    /// 小型配置文件，和 PCL 的 `PCL/` 文件夹同一定位。
    pub root: PathBuf,
    /// 这个实例使用的**游戏目录**（真正的 `.minecraft`）。原地读、原地下载，
    /// 不复制、不搬家：`versions/`、`libraries/`、`assets/`、`mods/`、`saves/`、
    /// `schematics/` 全部在这个目录下。多个实例可以指向同一个目录。
    ///
    /// `None` 是旧记录（本字段加入之前创建的实例），读取方会退回
    /// `root/minecraft`；写入方应当尽快补上真实路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_directory: Option<PathBuf>,
    pub state: InstanceState,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("version identifier cannot be empty")]
    EmptyVersionId,
}

impl GameInstance {
    /// 实例实际使用的游戏目录。旧记录没有这个字段时退回 `root/minecraft`。
    ///
    /// 这是**读取**用的回退，不是推荐做法：新建的实例一律写入真实 `.minecraft`。
    #[must_use]
    pub fn effective_game_directory(&self) -> PathBuf {
        match &self.game_directory {
            Some(directory) => directory.clone(),
            None => self.root.join("minecraft"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::VersionId;

    #[test]
    fn rejects_blank_version_identifier() {
        assert!(VersionId::parse("  ").is_err());
    }
}

pub mod config;
pub use config::{ConfigError, InstanceConfig};
