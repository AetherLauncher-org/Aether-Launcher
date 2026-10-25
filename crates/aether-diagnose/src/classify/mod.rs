//! Deterministic local crash classification.
//!
//! Rules are ordered by `priority` (lower values win). Log text is untrusted:
//! it is only ever copied into short, control-character-free evidence lines.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Confidence of a local rule result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// An action the launcher understands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SuggestedAction {
    SwitchJava { target: String },
    IncreaseMemory { target: String },
    DisableMod { target: String },
    ReinstallVersion,
    ExportDiagnostics,
    ViewLogs,
}

/// A local rule. Lower `priority` wins when multiple rules match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub id: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub confidence: Confidence,
    pub priority: u8,
}

/// Static local rules, kept in priority order for predictable collisions.
pub const RULES: &[Rule] = &[
    Rule {
        id: "unrecognized_option",
        description: "JVM 拒绝了当前参数",
        category: "java",
        confidence: Confidence::High,
        priority: 10,
    },
    Rule {
        id: "java_version_too_old",
        description: "Java 版本低于程序要求",
        category: "java",
        confidence: Confidence::High,
        priority: 20,
    },
    Rule {
        id: "out_of_memory",
        description: "Java 内存不足",
        category: "runtime",
        confidence: Confidence::High,
        priority: 30,
    },
    Rule {
        id: "mod_resolution_exception",
        description: "模组依赖解析失败",
        category: "mod",
        confidence: Confidence::High,
        priority: 40,
    },
    Rule {
        id: "missing_mod_dependency",
        description: "缺少或不兼容的模组依赖",
        category: "mod",
        confidence: Confidence::High,
        priority: 50,
    },
    Rule {
        id: "mixin_apply_failed",
        description: "模组 Mixin 应用失败",
        category: "mod",
        confidence: Confidence::High,
        priority: 60,
    },
    Rule {
        id: "opengl_or_driver",
        description: "OpenGL 或显卡驱动初始化失败",
        category: "graphics",
        confidence: Confidence::High,
        priority: 70,
    },
    Rule {
        id: "auth_token_expired",
        description: "游戏身份验证失败",
        category: "account",
        confidence: Confidence::High,
        priority: 80,
    },
];

/// The local diagnosis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalDiagnosis {
    pub cause_id: String,
    pub title_zh: String,
    pub confidence: Confidence,
    pub actions: Vec<SuggestedAction>,
    pub evidence_lines: Vec<String>,
}

impl LocalDiagnosis {
    #[must_use]
    pub fn confident(&self) -> bool {
        self.confidence == Confidence::High
    }
}

static UNRECOGNIZED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Unrecognized option:\s*(--\S+)").expect("valid regex"));
static CLASS_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:class file version|class file major version)\s+(\d+)(?:\.\d+)?")
        .expect("valid regex")
});
static MOD_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)Mod ['\"]([^'\"]+)['\"]"#).expect("valid regex"));

/// Classifies a session locally, selecting the lowest-priority matching rule.
#[must_use]
pub fn classify_locally(
    text: &str,
    arguments: &[String],
    java_major: Option<u16>,
) -> Option<LocalDiagnosis> {
    let id = if UNRECOGNIZED.is_match(text)
        || text.contains("Could not create the Java Virtual Machine")
        || text.contains("A fatal exception has occurred")
    {
        Some("unrecognized_option")
    } else if CLASS_VERSION.is_match(text)
        || text.contains("UnsupportedClassVersionError")
        || text.contains("Unsupported class file major version")
    {
        Some("java_version_too_old")
    } else if text.contains("OutOfMemoryError")
        || text.contains("Java heap space")
        || text.contains("Metaspace")
        || text.contains("GC overhead limit exceeded")
    {
        Some("out_of_memory")
    } else if text.contains("ModLoadingException")
        || text.contains("ModResolutionException")
        || text.contains("Missing or unsupported mandatory dependencies")
        || text.contains("neoforge") && text.contains("dependency")
    {
        Some("mod_resolution_exception")
    } else if text.contains("Incompatible mod set!")
        || text.contains("net.fabricmc.loader.impl.FormattedException")
        || text.contains("mods.toml missing")
        || (text.contains("requires '") && text.contains("of version"))
    {
        Some("missing_mod_dependency")
    } else if text.contains("Mixin apply failed")
        || text.contains("MixinApplyError")
        || text.contains("org.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError")
    {
        Some("mixin_apply_failed")
    } else if text.contains("GLFW error 65542")
        || text.contains("Failed to create window")
        || text.contains("OpenGL 3.2 or later")
        || text.contains("Pixel format not accelerated")
        || text.contains("WGL: The driver does not appear to support OpenGL")
    {
        Some("opengl_or_driver")
    } else if text.contains("Invalid credentials")
        || text.contains("token expired")
        || text.contains("Failed to authenticate")
        || text.contains("401 Unauthorized")
        || text.contains("com.mojang.authlib.exceptions.AuthenticationException")
        || text.contains("Invalid session (Try restarting your game)")
    {
        Some("auth_token_expired")
    } else {
        None
    };
    let id = id?;
    let lines = text
        .lines()
        .filter(|line| rule_matches(id, line))
        .take(5)
        .map(redact_log_line)
        .collect();
    let (title, confidence, actions) = diagnosis_details(id, text, arguments, java_major);
    Some(LocalDiagnosis {
        cause_id: id.to_string(),
        title_zh: title,
        confidence,
        actions,
        evidence_lines: lines,
    })
}

fn rule_matches(id: &str, line: &str) -> bool {
    match id {
        "unrecognized_option" => {
            UNRECOGNIZED.is_match(line)
                || line.contains("Could not create the Java Virtual Machine")
                || line.contains("A fatal exception has occurred")
        }
        "java_version_too_old" => {
            CLASS_VERSION.is_match(line)
                || line.contains("UnsupportedClassVersionError")
                || line.contains("Unsupported class file major version")
        }
        "out_of_memory" => {
            line.contains("OutOfMemoryError")
                || line.contains("Java heap space")
                || line.contains("Metaspace")
                || line.contains("GC overhead limit exceeded")
        }
        "mod_resolution_exception" => {
            line.contains("ModLoadingException")
                || line.contains("ModResolutionException")
                || line.contains("Missing or unsupported mandatory dependencies")
                || line.to_ascii_lowercase().contains("neoforge")
                    && line.to_ascii_lowercase().contains("dependency")
        }
        "missing_mod_dependency" => {
            line.contains("Incompatible mod set!")
                || line.contains("FormattedException")
                || line.contains("mods.toml missing")
                || line.contains("requires '") && line.contains("of version")
        }
        "mixin_apply_failed" => {
            line.contains("Mixin apply failed")
                || line.contains("MixinApplyError")
                || line.contains("MixinTransformerError")
        }
        "opengl_or_driver" => {
            line.contains("GLFW error 65542")
                || line.contains("Failed to create window")
                || line.contains("OpenGL 3.2 or later")
                || line.contains("Pixel format not accelerated")
                || line.contains("WGL: The driver does not appear to support OpenGL")
        }
        "auth_token_expired" => {
            line.contains("Invalid credentials")
                || line.contains("token expired")
                || line.contains("Failed to authenticate")
                || line.contains("401 Unauthorized")
                || line.contains("AuthenticationException")
                || line.contains("Invalid session (Try restarting your game)")
        }
        _ => false,
    }
}

fn diagnosis_details(
    id: &str,
    text: &str,
    arguments: &[String],
    _java_major: Option<u16>,
) -> (String, Confidence, Vec<SuggestedAction>) {
    match id {
        "unrecognized_option" => {
            let flag = UNRECOGNIZED
                .captures(text)
                .and_then(|c| c.get(1))
                .map_or("", |m| m.as_str());
            let min = crate::javatab::min_java_for_argument(flag).or_else(|| {
                flag.strip_prefix("--sun-misc-unsafe-memory-access")
                    .map(|_| 23)
            });
            let confidence = if min.is_some() && arguments.iter().any(|a| a == flag) {
                Confidence::High
            } else {
                Confidence::Medium
            };
            let title = min.map_or_else(
                || "Java 不支持当前启动参数".to_string(),
                |v| format!("需要 Java {v} 或更高版本"),
            );
            let actions = min.map_or_else(
                || vec![SuggestedAction::ViewLogs],
                |v| {
                    vec![SuggestedAction::SwitchJava {
                        target: format!("Java {v}"),
                    }]
                },
            );
            (title, confidence, actions)
        }
        "java_version_too_old" => {
            let version = CLASS_VERSION
                .captures(text)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<u16>().ok())
                .and_then(class_major_to_java);
            let title = version.map_or_else(
                || "Java 版本过低，无法加载程序".to_string(),
                |v| format!("需要 Java {v} 或更高版本"),
            );
            let actions = version.map_or_else(
                || vec![SuggestedAction::ViewLogs],
                |v| {
                    vec![SuggestedAction::SwitchJava {
                        target: format!("Java {v}"),
                    }]
                },
            );
            (title, Confidence::High, actions)
        }
        "out_of_memory" => (
            "Java 内存不足".into(),
            Confidence::High,
            vec![SuggestedAction::IncreaseMemory {
                target: "增加分配给游戏的内存".into(),
            }],
        ),
        "mod_resolution_exception" => (
            "模组依赖解析失败".into(),
            Confidence::High,
            vec![
                SuggestedAction::DisableMod {
                    target: mod_target(text),
                },
                SuggestedAction::ExportDiagnostics,
            ],
        ),
        "missing_mod_dependency" => (
            "缺少或不兼容的模组依赖".into(),
            Confidence::High,
            vec![
                SuggestedAction::DisableMod {
                    target: mod_target(text),
                },
                SuggestedAction::ExportDiagnostics,
            ],
        ),
        "mixin_apply_failed" => (
            "模组代码兼容性冲突".into(),
            Confidence::High,
            vec![SuggestedAction::DisableMod {
                target: mod_target(text),
            }],
        ),
        "opengl_or_driver" => (
            "OpenGL 或显卡驱动初始化失败".into(),
            Confidence::High,
            vec![
                SuggestedAction::ExportDiagnostics,
                SuggestedAction::ViewLogs,
            ],
        ),
        "auth_token_expired" => (
            "游戏身份验证失败，请重新登录".into(),
            Confidence::High,
            vec![
                SuggestedAction::ViewLogs,
                SuggestedAction::ExportDiagnostics,
            ],
        ),
        _ => ("无法判断崩溃原因".into(), Confidence::Low, vec![]),
    }
}

fn mod_target(text: &str) -> String {
    MOD_NAME
        .captures(text)
        .and_then(|c| c.get(1))
        .map_or_else(|| "最近安装或更新的模组".into(), |m| m.as_str().to_string())
}

fn class_major_to_java(major: u16) -> Option<u16> {
    (52..=69).contains(&major).then_some(major - 44)
}

/// Redacts one log line for inclusion as evidence.
#[must_use]
pub fn redact_log_line(line: &str) -> String {
    let cleaned: String = line
        .chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .collect();
    cleaned.chars().take(300).collect()
}
