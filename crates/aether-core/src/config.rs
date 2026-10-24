//! 实例可编辑配置。
//!
//! 提供 [`InstanceConfig`] 及其校验逻辑，用于表示单个游戏实例的可编辑启动配置。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 内存下限：512 MiB。
pub const MIN_MEMORY_MIB: u32 = 512;

/// 内存上限：128 GiB（131_072 MiB）。
pub const MAX_MEMORY_MIB: u32 = 131_072;

/// 单条参数的最大长度（按字节计）。
pub const MAX_ARGUMENT_LEN: usize = 8192;

/// 单个参数字段允许的最大条目数。
pub const MAX_ARGUMENTS_PER_LIST: usize = 512;

/// 窗口宽高的最小值。
pub const MIN_WINDOW_SIZE: u32 = 1;

/// 窗口宽高的最大值。
pub const MAX_WINDOW_SIZE: u32 = 16384;

/// 严格模式下视为 shell 元字符的字符集：
/// `;` `&` `|` `<` `>` `$` `` ` `` `(` `)` `{` `}` `[` `]` `^` `!` `%` `"` `'`，
/// 外加 NUL、换行（`\n`、`\r`）。
///
/// 该集合刻意**不**包含 `=`、`:`、`/`、`\`、`.`、`,`、`-`、`@`、`+`、`_`
/// 等参数中的常见字符（例如 Windows 路径 `C:\Games\Minecraft`、JVM 参数
/// `-Dpath=C:\Games\Minecraft`），也不包含空格与 `*`/`?` —— 因为参数始终作为
/// 单个 argv 传递、不经过 shell 分词，这些字符不会构成注入风险。
pub const SHELL_METACHARACTERS: &str = ";&|<>$`(){}[]^!%\"'\0\n\r";

/// 单个游戏实例的可编辑配置。
///
/// # 安全说明
/// - 所有参数字段（`jvm_args`、`game_args`、`extra_jvm_args`、`extra_game_args`）
///   最终会作为**单个** `std::process::Command` 参数原样传入启动进程，
///   **永远不经过 shell**，因此参数中即使含有 `;`、`&`、`|` 等元字符
///   也不会被解释；
/// - 本类型**不做任何 OS 级分词**（no OS-level splitting），也不会把参数
///   拼接成命令行字符串；
/// - [`Self::validate`] 是默认/标准校验，**不**检查 shell 元字符；
/// - [`Self::reject_shell_metacharacters`] 是可选的额外严格检查，用于
///   「防御性拒绝」以及后续可能出现的 shell 场景（例如经 `cmd.exe`/`sh`
///   间接启动时）。
///
/// 字段采用 snake_case 序列化命名（不启用 camelCase），与 crate 内
/// [`crate::InstanceState`]、[`crate::GameInstance`] 保持一致；
/// `#[serde(default)]` 使得缺省字段回落到 [`Default`] 实现。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstanceConfig {
    /// 分配的内存大小（MiB），合法范围 `MIN_MEMORY_MIB..=MAX_MEMORY_MIB`（512 MiB .. 128 GiB）。
    pub memory_mib: u32,
    /// 用户覆盖的 Java 可执行文件路径；`None` 表示使用默认 Java。
    /// 若为 `Some`，必须是非空绝对路径（见 [`Self::validate`]）。
    pub java_executable: Option<PathBuf>,
    /// 基础 JVM 参数。
    pub jvm_args: Vec<String>,
    /// 基础游戏参数。
    pub game_args: Vec<String>,
    /// 用户追加的 JVM 参数（与基础参数分开，便于 UI 表达「追加」语义）。
    pub extra_jvm_args: Vec<String>,
    /// 用户追加的游戏参数。
    pub extra_game_args: Vec<String>,
    /// 窗口宽度；`None` 表示使用游戏/清单默认值。
    pub window_width: Option<u32>,
    /// 窗口高度；`None` 表示使用游戏/清单默认值。
    pub window_height: Option<u32>,
    /// 是否全屏启动。
    pub fullscreen: bool,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            memory_mib: 4096,
            java_executable: None,
            jvm_args: Vec::new(),
            game_args: Vec::new(),
            extra_jvm_args: Vec::new(),
            extra_game_args: Vec::new(),
            window_width: None,
            window_height: None,
            fullscreen: false,
        }
    }
}

/// 配置校验错误。
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// 内存大小超出 `512..=131072` MiB 范围。
    #[error("memory_mib {memory_mib} is out of range 512..=131072 MiB")]
    MemoryOutOfRange { memory_mib: u32 },
    /// 单条参数长度超过 [`MAX_ARGUMENT_LEN`]。
    #[error(
        "argument at index {index} in `{field}` exceeds the maximum length of 8192 bytes (actual: {length})"
    )]
    ArgumentTooLong {
        field: &'static str,
        index: usize,
        length: usize,
    },
    /// 参数包含 NUL 或换行符（`\n`/`\r`）。
    #[error(
        "argument at index {index} in `{field}` contains a forbidden character (NUL, newline or carriage return)"
    )]
    ArgumentContainsForbiddenCharacter { field: &'static str, index: usize },
    /// 参数条目数超过 [`MAX_ARGUMENTS_PER_LIST`]。
    #[error("`{field}` has too many arguments ({count} > 512)")]
    TooManyArguments { field: &'static str, count: usize },
    /// 窗口宽高异常：宽高必须在 `1..=16384` 内，且必须同时设置或同时不设置；
    /// 只有一个设置视为异常；`fullscreen == true` 时允许两者均不设置。
    #[error(
        "window size is invalid: width and height must both be set or both be unset, and each must be within 1..=16384 (width: {width:?}, height: {height:?})"
    )]
    WindowSizeOutOfRange {
        width: Option<u32>,
        height: Option<u32>,
    },
    /// 严格模式下参数包含 shell 元字符。
    #[error("argument at index {index} in `{field}` contains a shell metacharacter")]
    ShellMetacharacter { field: &'static str, index: usize },
    /// `java_executable` 已设置但非绝对路径。
    #[error("java_executable must be an absolute path when set")]
    JavaExecutableNotAbsolute,
    /// `java_executable` 已设置但为空。
    #[error("java_executable must not be empty when set")]
    JavaExecutableEmpty,
}

impl InstanceConfig {
    /// 默认/标准模式校验，按以下规则检查：
    /// 1. `memory_mib` 必须在 `MIN_MEMORY_MIB..=MAX_MEMORY_MIB` 内；
    /// 2. 每个参数字段的条目数不得超过 [`MAX_ARGUMENTS_PER_LIST`]，
    ///    单条参数长度不得超过 [`MAX_ARGUMENT_LEN`]，且不得包含 NUL、`\n`、`\r`；
    /// 3. 窗口宽高必须同时设置或同时不设置，且取值在
    ///    `MIN_WINDOW_SIZE..=MAX_WINDOW_SIZE` 内；
    /// 4. `java_executable` 若为 `Some`，必须是非空绝对路径。
    ///
    /// # 与 [`Self::reject_shell_metacharacters`] 的关系
    ///
    /// 本方法是**默认/标准模式**，**不**检查 shell 元字符：参数最终作为单个
    /// `std::process::Command` 参数传入、从不经过 shell，因此 `;`、`&`、`|`
    /// 这类字符不会构成注入风险。若调用方需要「防御性拒绝」（例如将来经
    /// `cmd.exe` / `sh` 间接启动），必须**额外显式调用**
    /// [`Self::reject_shell_metacharacters`] —— `validate()` 不会替你调用它。
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.check_memory()?;
        self.check_argument_list("jvm_args", &self.jvm_args)?;
        self.check_argument_list("game_args", &self.game_args)?;
        self.check_argument_list("extra_jvm_args", &self.extra_jvm_args)?;
        self.check_argument_list("extra_game_args", &self.extra_game_args)?;
        self.check_window_size()?;
        self.check_java_executable()?;
        Ok(())
    }

    /// 可选的严格检查：扫描四个参数字段，若任一参数包含
    /// [`SHELL_METACHARACTERS`] 中的字符，则返回
    /// [`ConfigError::ShellMetacharacter`]。
    ///
    /// 正常启动路径（`std::process::Command`，不经 shell）不需要调用本方法；
    /// 它用于「防御性拒绝」以及后续可能出现的 shell 场景。
    pub fn reject_shell_metacharacters(&self) -> Result<(), ConfigError> {
        self.check_shell_metacharacters("jvm_args", &self.jvm_args)?;
        self.check_shell_metacharacters("game_args", &self.game_args)?;
        self.check_shell_metacharacters("extra_jvm_args", &self.extra_jvm_args)?;
        self.check_shell_metacharacters("extra_game_args", &self.extra_game_args)?;
        Ok(())
    }

    /// 合并后的有效 JVM 参数：基础参数在前、追加参数在后。
    ///
    /// 结果可直接按单个 argv 依次传给 [`std::process::Command`]，无需分词。
    pub fn effective_jvm_args(&self) -> Vec<&str> {
        let mut args = Vec::with_capacity(self.jvm_args.len() + self.extra_jvm_args.len());
        args.extend(self.jvm_args.iter().map(String::as_str));
        args.extend(self.extra_jvm_args.iter().map(String::as_str));
        args
    }

    /// 合并后的有效游戏参数：基础参数在前、追加参数在后。
    ///
    /// 结果可直接按单个 argv 依次传给 [`std::process::Command`]，无需分词。
    pub fn effective_game_args(&self) -> Vec<&str> {
        let mut args = Vec::with_capacity(self.game_args.len() + self.extra_game_args.len());
        args.extend(self.game_args.iter().map(String::as_str));
        args.extend(self.extra_game_args.iter().map(String::as_str));
        args
    }

    fn check_memory(&self) -> Result<(), ConfigError> {
        if !(MIN_MEMORY_MIB..=MAX_MEMORY_MIB).contains(&self.memory_mib) {
            return Err(ConfigError::MemoryOutOfRange {
                memory_mib: self.memory_mib,
            });
        }
        Ok(())
    }

    fn check_argument_list(&self, field: &'static str, args: &[String]) -> Result<(), ConfigError> {
        if args.len() > MAX_ARGUMENTS_PER_LIST {
            return Err(ConfigError::TooManyArguments {
                field,
                count: args.len(),
            });
        }
        for (index, arg) in args.iter().enumerate() {
            if arg.len() > MAX_ARGUMENT_LEN {
                return Err(ConfigError::ArgumentTooLong {
                    field,
                    index,
                    length: arg.len(),
                });
            }
            if arg.contains('\0') || arg.contains('\n') || arg.contains('\r') {
                return Err(ConfigError::ArgumentContainsForbiddenCharacter { field, index });
            }
        }
        Ok(())
    }

    fn check_window_size(&self) -> Result<(), ConfigError> {
        match (self.window_width, self.window_height) {
            (Some(width), Some(height)) => {
                if !(MIN_WINDOW_SIZE..=MAX_WINDOW_SIZE).contains(&width)
                    || !(MIN_WINDOW_SIZE..=MAX_WINDOW_SIZE).contains(&height)
                {
                    return Err(ConfigError::WindowSizeOutOfRange {
                        width: Some(width),
                        height: Some(height),
                    });
                }
                Ok(())
            }
            (None, None) => Ok(()),
            (width, height) => Err(ConfigError::WindowSizeOutOfRange { width, height }),
        }
    }

    fn check_java_executable(&self) -> Result<(), ConfigError> {
        match &self.java_executable {
            None => Ok(()),
            Some(path) => {
                if path.as_os_str().is_empty() {
                    return Err(ConfigError::JavaExecutableEmpty);
                }
                if !path.is_absolute() {
                    return Err(ConfigError::JavaExecutableNotAbsolute);
                }
                Ok(())
            }
        }
    }

    fn check_shell_metacharacters(
        &self,
        field: &'static str,
        args: &[String],
    ) -> Result<(), ConfigError> {
        for (index, arg) in args.iter().enumerate() {
            if has_shell_metacharacters(arg) {
                return Err(ConfigError::ShellMetacharacter { field, index });
            }
        }
        Ok(())
    }
}

/// 判断字符是否属于严格模式拒绝的 shell 元字符集合（见 [`SHELL_METACHARACTERS`]）。
#[must_use]
pub fn is_shell_metacharacter(c: char) -> bool {
    SHELL_METACHARACTERS.contains(c)
}

/// 判断字符串是否包含任一 shell 元字符。
fn has_shell_metacharacters(value: &str) -> bool {
    value.chars().any(is_shell_metacharacter)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// 构造仅指定一个参数字段的配置（其余取默认值）。
    fn config_with_args(field: &'static str, args: Vec<String>) -> InstanceConfig {
        match field {
            "jvm_args" => InstanceConfig {
                jvm_args: args,
                ..InstanceConfig::default()
            },
            "game_args" => InstanceConfig {
                game_args: args,
                ..InstanceConfig::default()
            },
            "extra_jvm_args" => InstanceConfig {
                extra_jvm_args: args,
                ..InstanceConfig::default()
            },
            "extra_game_args" => InstanceConfig {
                extra_game_args: args,
                ..InstanceConfig::default()
            },
            _ => unreachable!("unknown argument field: {field}"),
        }
    }

    #[test]
    fn default_values_are_used() {
        let config = InstanceConfig::default();
        assert_eq!(config.memory_mib, 4096);
        assert_eq!(config.java_executable, None);
        assert!(config.jvm_args.is_empty());
        assert!(config.game_args.is_empty());
        assert!(config.extra_jvm_args.is_empty());
        assert!(config.extra_game_args.is_empty());
        assert_eq!(config.window_width, None);
        assert_eq!(config.window_height, None);
        assert!(!config.fullscreen);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn memory_out_of_range_is_rejected() {
        for memory_mib in [0, 511, 131_073] {
            let config = InstanceConfig {
                memory_mib,
                ..InstanceConfig::default()
            };
            assert_eq!(
                config.validate().unwrap_err(),
                ConfigError::MemoryOutOfRange { memory_mib }
            );
        }
    }

    #[test]
    fn memory_boundaries_are_accepted() {
        for memory_mib in [512, 131_072] {
            let config = InstanceConfig {
                memory_mib,
                ..InstanceConfig::default()
            };
            assert!(
                config.validate().is_ok(),
                "memory_mib {memory_mib} should be accepted"
            );
        }
    }

    #[test]
    fn argument_over_max_length_is_rejected() {
        let config = config_with_args("extra_game_args", vec!["a".repeat(MAX_ARGUMENT_LEN + 1)]);
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigError::ArgumentTooLong {
                field: "extra_game_args",
                index: 0,
                length: MAX_ARGUMENT_LEN + 1,
            }
        );
    }

    #[test]
    fn argument_at_max_length_is_accepted() {
        let config = config_with_args("jvm_args", vec!["a".repeat(MAX_ARGUMENT_LEN)]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn nul_in_argument_is_rejected() {
        for field in ["jvm_args", "game_args", "extra_jvm_args", "extra_game_args"] {
            let config = config_with_args(field, vec!["a\0b".to_string()]);
            assert_eq!(
                config.validate().unwrap_err(),
                ConfigError::ArgumentContainsForbiddenCharacter { field, index: 0 }
            );
        }
    }

    #[test]
    fn newline_in_argument_is_rejected() {
        for field in ["jvm_args", "game_args", "extra_jvm_args", "extra_game_args"] {
            let config = config_with_args(field, vec!["a\nb".to_string()]);
            assert_eq!(
                config.validate().unwrap_err(),
                ConfigError::ArgumentContainsForbiddenCharacter { field, index: 0 }
            );
        }
    }

    #[test]
    fn carriage_return_in_argument_is_rejected() {
        for field in ["jvm_args", "game_args", "extra_jvm_args", "extra_game_args"] {
            let config = config_with_args(field, vec!["a\rb".to_string()]);
            assert_eq!(
                config.validate().unwrap_err(),
                ConfigError::ArgumentContainsForbiddenCharacter { field, index: 0 }
            );
        }
    }

    #[test]
    fn too_many_arguments_are_rejected() {
        let args = (0..MAX_ARGUMENTS_PER_LIST + 1)
            .map(|i| format!("arg{i}"))
            .collect::<Vec<_>>();
        let config = config_with_args("jvm_args", args);
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigError::TooManyArguments {
                field: "jvm_args",
                count: MAX_ARGUMENTS_PER_LIST + 1,
            }
        );
    }

    #[test]
    fn max_argument_count_is_accepted() {
        let args = (0..MAX_ARGUMENTS_PER_LIST)
            .map(|i| format!("arg{i}"))
            .collect::<Vec<_>>();
        let config = config_with_args("game_args", args);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn zero_window_width_is_rejected() {
        let config = InstanceConfig {
            window_width: Some(0),
            window_height: Some(480),
            ..InstanceConfig::default()
        };
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigError::WindowSizeOutOfRange {
                width: Some(0),
                height: Some(480),
            }
        );
    }

    #[test]
    fn oversized_window_is_rejected() {
        let config = InstanceConfig {
            window_width: Some(16_385),
            window_height: Some(480),
            ..InstanceConfig::default()
        };
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigError::WindowSizeOutOfRange {
                width: Some(16_385),
                height: Some(480),
            }
        );
    }

    #[test]
    fn width_without_height_is_rejected() {
        let config = InstanceConfig {
            window_width: Some(854),
            ..InstanceConfig::default()
        };
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::WindowSizeOutOfRange { .. }
        ));
    }

    #[test]
    fn height_without_width_is_rejected() {
        let config = InstanceConfig {
            window_height: Some(480),
            ..InstanceConfig::default()
        };
        assert!(matches!(
            config.validate().unwrap_err(),
            ConfigError::WindowSizeOutOfRange { .. }
        ));
    }

    #[test]
    fn valid_window_size_is_accepted() {
        let config = InstanceConfig {
            window_width: Some(854),
            window_height: Some(480),
            ..InstanceConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn fullscreen_without_window_size_is_accepted() {
        let config = InstanceConfig {
            fullscreen: true,
            ..InstanceConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn relative_java_executable_is_rejected() {
        let config = InstanceConfig {
            java_executable: Some(PathBuf::from("java")),
            ..InstanceConfig::default()
        };
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigError::JavaExecutableNotAbsolute
        );
    }

    #[test]
    fn empty_java_executable_is_rejected() {
        let config = InstanceConfig {
            java_executable: Some(PathBuf::new()),
            ..InstanceConfig::default()
        };
        assert_eq!(
            config.validate().unwrap_err(),
            ConfigError::JavaExecutableEmpty
        );
    }

    #[test]
    fn absolute_java_executable_is_accepted() {
        let config = InstanceConfig {
            java_executable: Some(std::env::current_exe().expect("current executable has an absolute path")),
            ..InstanceConfig::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn shell_metacharacter_is_rejected_in_strict_mode() {
        let config = config_with_args("extra_jvm_args", vec!["-Dx=a&b".to_string()]);
        assert_eq!(
            config.reject_shell_metacharacters().unwrap_err(),
            ConfigError::ShellMetacharacter {
                field: "extra_jvm_args",
                index: 0,
            }
        );
    }

    #[test]
    fn shell_safe_arguments_pass_strict_mode() {
        for arg in ["-Xmx2G", "--username=Steve", r"-Dpath=C:\Games\Minecraft"] {
            let config = config_with_args("extra_game_args", vec![arg.to_string()]);
            assert!(
                config.reject_shell_metacharacters().is_ok(),
                "argument {arg:?} must not contain shell metacharacters"
            );
        }
    }

    #[test]
    fn default_validation_does_not_reject_shell_metacharacters() {
        let config = config_with_args("extra_jvm_args", vec!["-Dx=a&b".to_string()]);
        assert!(
            config.validate().is_ok(),
            "default validation must not reject shell metacharacters"
        );
    }

    #[test]
    fn effective_arguments_combine_base_and_extra() {
        let config = InstanceConfig {
            jvm_args: vec!["-Xmx4G".to_string(), "-XX:+UseG1GC".to_string()],
            extra_jvm_args: vec!["-Dfml.ignoreInvalidMinecraftCertificates=true".to_string()],
            game_args: vec!["--username".to_string(), "Steve".to_string()],
            extra_game_args: vec!["--demo".to_string()],
            ..InstanceConfig::default()
        };
        assert_eq!(
            config.effective_jvm_args(),
            vec![
                "-Xmx4G",
                "-XX:+UseG1GC",
                "-Dfml.ignoreInvalidMinecraftCertificates=true",
            ]
        );
        assert_eq!(
            config.effective_game_args(),
            vec!["--username", "Steve", "--demo"]
        );
    }
}
