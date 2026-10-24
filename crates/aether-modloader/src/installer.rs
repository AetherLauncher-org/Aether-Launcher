//! 模组加载器安装「计划」。
//!
//! 本模块只描述**要做什么**，不执行任何动作：不访问网络、不读写文件、不启动 Java 进程。
//! 真正的下载与落盘由调用方（安装引擎）按 [`InstallStep`] 的顺序逐条完成。
//!
//! [`ModLoaderInstallPlan::validate`] 做的是**静态安全检查**：目标版本 id 会成为
//! `versions/<id>/` 的目录名，因此必须满足 Windows 的文件/目录名限制（不能有 `/` `\` 这类分隔符、
//! 不能是 `CON` / `NUL` 这类设备名、不能以点或空格结尾……）；
//! 同时每个会写盘的路径都必须是安全的**相对**路径，避免路径逃逸。
//!
//! 校验逻辑参照 `aether-download` 中的 `is_safe_relative_path` / `is_safe_segment`；
//! 单个路径段的规则集中在 [`crate::json::safe_version_segment`]，本模块只额外挡掉
//! `.` / `..` 这类结构性的段，避免两处规则漂移。

use crate::ModLoaderKind;
use crate::transport::MetaError;

/// 版本 id 的最大长度（按字节计）。Windows 单个文件名上限 255，但版本 id 会同时出现在
/// `versions/<id>/<id>.json` 与临时目录里，留足余量后取 63。
pub const MAX_VERSION_ID_LEN: usize = 63;

/// 校验失败时使用的统一原因文案，保证调用方与测试可以按字符串断言。
const REASON_EMPTY: &str = "version identifier is empty";
const REASON_TOO_LONG: &str = "version identifier is too long";
const REASON_SEPARATOR: &str = "version identifier contains a path separator";
const REASON_CONTROL: &str = "version identifier contains a control character";
const REASON_TRAILING: &str = "version identifier ends with a dot or space";
const REASON_RELATIVE: &str = "version identifier is a relative path component";
const REASON_RESERVED: &str = "version identifier is a reserved device name";
const REASON_UNSAFE_DESTINATION: &str = "destination path is not a safe relative path";

/// 安装计划中的单个步骤。
///
/// 每个变体只描述意图与参数，本身不执行任何 I/O。因为
/// [`InstallStep::WriteVersionJson`] 携带 `serde_json::Value`（不是 `Eq`），
/// 这里只派生 `PartialEq`，便于测试与 UI 做差异比较。
#[derive(Debug, Clone, PartialEq)]
pub enum InstallStep {
    /// 下载一个文件到 `destination`（相对游戏目录的路径）。`sha1` / `size` 可用时用于校验。
    Download {
        url: String,
        destination: String,
        sha1: Option<String>,
        size: Option<u64>,
        label: String,
    },
    /// 运行安装器 jar（例如 Forge / NeoForge 的 `-installer.jar`）。
    RunInstaller {
        jar_path: String,
        args: Vec<String>,
        output_dir: Option<String>,
    },
    /// 写入 `<id>.json` 版本文件。
    WriteVersionJson {
        path: String,
        content: serde_json::Value,
    },
    /// 从 maven 仓库复制库文件（例如把已缓存的构件复制进实例的 `libraries/`）。
    CopyLibraries {
        repository: String,
        coordinates: Vec<String>,
    },
    /// 复制单个文件。
    CopyFile { from: String, to: String },
    /// 写入一个文本文件（例如启动参数片段）。
    WriteFile { path: String, content: String },
}

impl InstallStep {
    /// 返回给 UI 展示的一行中文说明；保留关键路径 / 仓库地址，方便用户核对。
    pub fn describe(&self) -> String {
        match self {
            Self::Download {
                destination, label, ..
            } => format!("下载 {label} 到 {destination}"),
            Self::RunInstaller { jar_path, .. } => format!("运行安装器 {jar_path}"),
            Self::WriteVersionJson { path, .. } => format!("写入版本文件 {path}"),
            Self::CopyLibraries {
                repository,
                coordinates,
            } => format!("从 {repository} 复制 {} 个库", coordinates.len()),
            Self::CopyFile { from, to } => format!("复制 {from} 到 {to}"),
            Self::WriteFile { path, .. } => format!("写入 {path}"),
        }
    }

    /// 该步骤是否需要从网络拉取文件。
    pub fn is_download(&self) -> bool {
        matches!(self, Self::Download { .. })
    }

    /// 该步骤会写盘的目标路径（相对路径）；不写盘的步骤返回 `None`。
    ///
    /// 注意：[`InstallStep::CopyFile`] 的源 `from` 不在此列，只有目标 `to` 会被写入。
    pub fn destination(&self) -> Option<&str> {
        match self {
            Self::Download { destination, .. } => Some(destination),
            Self::WriteVersionJson { path, .. } | Self::WriteFile { path, .. } => Some(path),
            Self::CopyFile { to, .. } => Some(to),
            Self::RunInstaller { .. } | Self::CopyLibraries { .. } => None,
        }
    }
}

/// 一份模组加载器安装计划。
///
/// 计划本身是纯数据：构造它不产生任何副作用；只有通过 [`ModLoaderInstallPlan::validate`]
/// 之后才应交给安装执行器。
#[derive(Debug, Clone, PartialEq)]
pub struct ModLoaderInstallPlan {
    /// 加载器类型。
    pub kind: ModLoaderKind,
    /// 加载器版本（例如 `0.16.0`、`47.4.28`）。
    pub loader_version: String,
    /// 对应的原版游戏版本（例如 `1.21.1`）。
    pub game_version: String,
    /// 目标版本 id，会成为 `versions/<id>/` 的目录名；空字符串表示尚未设置。
    pub target_version_id: String,
    /// 版本 json 的 `mainClass`。
    pub main_class: String,
    /// 版本 json 的 `inheritsFrom`，默认等于游戏版本。
    pub inherits_from: String,
    /// 按顺序执行的安装步骤。
    pub steps: Vec<InstallStep>,
}

impl ModLoaderInstallPlan {
    /// 创建一个空计划：`target_version_id` 与 `main_class` 为空，
    /// `inherits_from = game_version`，步骤列表为空。
    /// 调用方随后用 [`ModLoaderInstallPlan::with_target`] 等方法补齐。
    pub fn new(
        kind: ModLoaderKind,
        loader_version: impl Into<String>,
        game_version: impl Into<String>,
    ) -> Self {
        let game_version = game_version.into();
        Self {
            kind,
            loader_version: loader_version.into(),
            inherits_from: game_version.clone(),
            game_version,
            target_version_id: String::new(),
            main_class: String::new(),
            steps: Vec::new(),
        }
    }

    /// 设置目标版本 id（即 `versions/<id>/` 的目录名）。
    pub fn with_target(mut self, target_version_id: impl Into<String>) -> Self {
        self.target_version_id = target_version_id.into();
        self
    }

    /// 设置版本 json 的 `mainClass`。
    pub fn with_main_class(mut self, main_class: impl Into<String>) -> Self {
        self.main_class = main_class.into();
        self
    }

    /// 追加一个安装步骤（链式调用）。
    pub fn push(mut self, step: InstallStep) -> Self {
        self.steps.push(step);
        self
    }

    /// 静态安全检查，**不触碰文件系统、不执行任何东西**。
    ///
    /// 依次检查：
    /// 1. `target_version_id` 是安全的版本 id（见 [`is_safe_version_id`]）；
    /// 2. `existing` 中没有同名版本（避免覆盖已安装的实例）；
    /// 3. 步骤列表非空；
    /// 4. 每个会写盘的目标路径都是安全的相对路径：包括 [`InstallStep::destination`] 给出的路径，
    ///    以及 [`InstallStep::CopyFile`] 的 `to`、[`InstallStep::RunInstaller`] 的 `output_dir`。
    pub fn validate(&self, existing: &[&str]) -> Result<(), MetaError> {
        is_safe_version_id(&self.target_version_id)?;

        if existing.iter().any(|name| *name == self.target_version_id) {
            return Err(MetaError::VersionAlreadyInstalled {
                id: self.target_version_id.clone(),
            });
        }

        if self.steps.is_empty() {
            return Err(MetaError::EmptyPlan);
        }

        for step in &self.steps {
            if let Some(path) = step.destination() {
                ensure_relative_destination(path)?;
            }
            match step {
                InstallStep::WriteVersionJson { content, .. } => {
                    // 写出去的版本 json 必须自带 `id`，否则上层拿它合并继承链时会没有名字。
                    crate::json::string_field(content, "id", "version json id")?;
                }
                InstallStep::RunInstaller { output_dir, .. } => {
                    if let Some(dir) = output_dir {
                        ensure_relative_destination(dir)?;
                    }
                }
                // `CopyFile.to` 已经由 `destination()` 覆盖
                InstallStep::Download { .. }
                | InstallStep::WriteFile { .. }
                | InstallStep::CopyFile { .. }
                | InstallStep::CopyLibraries { .. } => {}
            }
        }

        Ok(())
    }
}

/// 校验目标版本 id 能否安全地作为 `versions/<id>/` 的目录名。
///
/// 拒绝：空串、超过 [`MAX_VERSION_ID_LEN`] 字节、含 `/` 或 `\`、含 ASCII 控制字符、
/// 以 `.` 或空格结尾、等于 `.` 或 `..`、以及 Windows 保留设备名
/// （`CON` / `PRN` / `AUX` / `NUL` / `COM1`-`COM9` / `LPT1`-`LPT9`，大小写不敏感，
/// 按第一个 `.` 之前的部分判断，所以 `con.txt`、`COM5.sys` 同样被拒绝）。
pub fn is_safe_version_id(id: &str) -> Result<(), MetaError> {
    let reject = |reason: &'static str| {
        Err(MetaError::UnsafeVersionId {
            id: id.to_owned(),
            reason,
        })
    };

    if id.is_empty() {
        return reject(REASON_EMPTY);
    }
    if id.len() > MAX_VERSION_ID_LEN {
        return reject(REASON_TOO_LONG);
    }
    if id.contains(['/', '\\']) {
        return reject(REASON_SEPARATOR);
    }
    if id.chars().any(char::is_control) {
        return reject(REASON_CONTROL);
    }
    // 先判 `.` / `..`：它们是**结构性**问题（路径穿越），比「结尾是点」这种格式问题严重得多。
    // 如果先判结尾点，`.` 永远只会报 "ends with a dot or space"，真正的危险反而被掩盖。
    if id == "." || id == ".." {
        return reject(REASON_RELATIVE);
    }
    // Windows 会静默去掉结尾的点和空格，落盘后的目录名会和 id 不一致
    if id.ends_with(['.', ' ']) {
        return reject(REASON_TRAILING);
    }
    if id
        .split(['/', '\\'])
        .any(|segment| !is_safe_segment(segment))
    {
        return reject(REASON_RESERVED);
    }

    Ok(())
}

/// 确保某条写盘目标是一个安全的相对路径；否则包装成 [`MetaError::UnsafeVersionId`]，
/// 其中 `id` 字段放的是这条路径本身（`MetaError` 不新增变体，路径问题沿用此错误）。
fn ensure_relative_destination(path: &str) -> Result<(), MetaError> {
    if is_safe_relative_path(path) {
        Ok(())
    } else {
        Err(MetaError::UnsafeVersionId {
            id: path.to_owned(),
            reason: REASON_UNSAFE_DESTINATION,
        })
    }
}

/// 与平台无关的相对路径检查：同时把 `/` 与 `\` 当作分隔符，
/// 这样在 Linux 上通过的路径换到 Windows 上也不会越出目录。
fn is_safe_relative_path(path: &str) -> bool {
    // 以分隔符开头即绝对路径，必须先拦掉；空串没有段，`split` 会给出一个空段，同样被 `is_safe_segment` 拒绝
    if path.starts_with(['/', '\\']) {
        return false;
    }
    !path.is_empty() && path.split(['/', '\\']).all(is_safe_segment)
}

/// 单个路径段的检查：非空、不是 `.` / `..`、不含 Windows 非法字符、不以点或空格结尾、
/// 不含控制字符、不是保留设备名。
///
/// 具体规则由 [`crate::json::safe_version_segment`] 统一维护（版本 id 与路径段共用同一套
/// Windows 文件/目录名限制），这里只额外挡掉 `.` / `..` 这两个纯结构性的段。
fn is_safe_segment(segment: &str) -> bool {
    if segment == "." || segment == ".." {
        return false;
    }
    crate::json::safe_version_segment(segment)
}

#[cfg(test)]
#[path = "installer/installer_tests.rs"]
mod tests;
