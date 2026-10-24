use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaRuntime {
    pub executable: PathBuf,
    pub major_version: u16,
    pub architecture: String,
    pub vendor: Option<String>,
}

impl JavaRuntime {
    #[must_use]
    pub fn supports(&self, required_major_version: u16) -> bool {
        self.major_version == required_major_version
    }

    /// 从 java 可执行文件路径探测版本信息
    pub fn probe(executable: PathBuf) -> Result<Self, RuntimeError> {
        let output = Command::new(&executable)
            .arg("-version")
            .output()
            .map_err(|e| RuntimeError::CommandFailed(executable.clone(), e))?;

        let version_output = String::from_utf8_lossy(&output.stderr);

        let major_version = parse_java_major_version(&version_output)
            .ok_or_else(|| RuntimeError::VersionParseFailure(executable.clone()))?;

        let architecture = if version_output.contains("64-Bit") || version_output.contains("amd64") {
            "x64".to_string()
        } else if version_output.contains("aarch64") || version_output.contains("arm64") {
            "arm64".to_string()
        } else {
            "x86".to_string()
        };

        let vendor = extract_vendor(&version_output);

        Ok(Self {
            executable,
            major_version,
            architecture,
            vendor,
        })
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("failed to execute java command at {0:?}: {1}")]
    CommandFailed(PathBuf, std::io::Error),
    #[error("failed to parse java version from {0:?}")]
    VersionParseFailure(PathBuf),
}

fn parse_java_major_version(output: &str) -> Option<u16> {
    // 适配不同 JVM 版本输出格式
    // Java 8: java version "1.8.0_..."
    // Java 9+: java version "11.0.18" 或 openjdk version "17.0.9"

    for line in output.lines() {
        if let Some(version_part) = line.split('"').nth(1) {
            if let Some(major_str) = version_part.split('.').next() {
                if major_str == "1" {
                    // Java 8 及以下是 1.x.x 格式
                    if let Some(minor) = version_part.split('.').nth(1) {
                        if let Ok(v) = minor.parse::<u16>() {
                            return Some(v);
                        }
                    }
                } else if let Ok(v) = major_str.parse::<u16>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn extract_vendor(output: &str) -> Option<String> {
    let lower = output.to_lowercase();
    if lower.contains("openjdk") {
        Some("OpenJDK".to_string())
    } else if lower.contains("oracle") {
        Some("Oracle".to_string())
    } else if lower.contains("adoptium") || lower.contains("temurin") {
        Some("Adoptium".to_string())
    } else if lower.contains("zulu") {
        Some("Azul Zulu".to_string())
    } else if lower.contains("amazon") || lower.contains("corretto") {
        Some("Amazon Corretto".to_string())
    } else if lower.contains("microsoft") {
        Some("Microsoft".to_string())
    } else if lower.contains("graalvm") {
        Some("GraalVM".to_string())
    } else {
        None
    }
}

/// 扫描系统中常见位置的 Java 安装
pub fn discover_java_runtimes() -> Vec<JavaRuntime> {
    let mut runtimes = Vec::new();
    let candidates = discover_java_candidates();

    for candidate in candidates {
        if let Ok(runtime) = JavaRuntime::probe(candidate) {
            // 去重：同版本、同架构只保留一个
            if !runtimes.iter().any(|r: &JavaRuntime| {
                r.major_version == runtime.major_version
                    && r.architecture == runtime.architecture
                    && r.vendor == runtime.vendor
            }) {
                runtimes.push(runtime);
            }
        }
    }

    runtimes.sort_by_key(|r| std::cmp::Reverse(r.major_version));
    runtimes
}

#[cfg(target_os = "windows")]
fn discover_java_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 检查 PATH 环境变量
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let java_exe = dir.join("java.exe");
            if java_exe.exists() {
                paths.push(java_exe);
            }
        }
    }

    // 检查 JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let java_exe = PathBuf::from(java_home).join("bin").join("java.exe");
        if java_exe.exists() {
            paths.push(java_exe);
        }
    }

    // 常见安装位置
    let common_dirs = [
        r"C:\Program Files\Java",
        r"C:\Program Files (x86)\Java",
        r"C:\Program Files\Eclipse Adoptium",
        r"C:\Program Files\Microsoft",
        r"C:\Program Files\Zulu",
        r"C:\Program Files\Amazon Corretto",
    ];

    for base_dir in &common_dirs {
        if let Ok(entries) = std::fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                let java_exe = entry.path().join("bin").join("java.exe");
                if java_exe.exists() {
                    paths.push(java_exe);
                }
            }
        }
    }

    // Minecraft 自带的 Java runtime（通常在 .minecraft/runtime）
    if let Ok(appdata) = std::env::var("APPDATA") {
        let minecraft_runtime = PathBuf::from(appdata).join(".minecraft").join("runtime");
        if minecraft_runtime.exists() {
            scan_minecraft_runtime(&minecraft_runtime, &mut paths, "java.exe");
        }
    }

    paths
}

#[cfg(target_os = "linux")]
fn discover_java_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 检查 PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let java_bin = dir.join("java");
            if java_bin.exists() {
                paths.push(java_bin);
            }
        }
    }

    // 检查 JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let java_bin = PathBuf::from(java_home).join("bin").join("java");
        if java_bin.exists() {
            paths.push(java_bin);
        }
    }

    // 常见安装位置
    let common_dirs = [
        "/usr/lib/jvm",
        "/usr/java",
        "/opt/java",
        "/opt/jdk",
    ];

    for base_dir in &common_dirs {
        if let Ok(entries) = std::fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                let java_bin = entry.path().join("bin").join("java");
                if java_bin.exists() {
                    paths.push(java_bin);
                }
            }
        }
    }

    // Minecraft runtime
    if let Ok(home) = std::env::var("HOME") {
        let minecraft_runtime = PathBuf::from(home).join(".minecraft").join("runtime");
        if minecraft_runtime.exists() {
            scan_minecraft_runtime(&minecraft_runtime, &mut paths, "java");
        }
    }

    paths
}

#[cfg(target_os = "macos")]
fn discover_java_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 检查 PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            let java_bin = dir.join("java");
            if java_bin.exists() {
                paths.push(java_bin);
            }
        }
    }

    // 检查 JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let java_bin = PathBuf::from(java_home).join("bin").join("java");
        if java_bin.exists() {
            paths.push(java_bin);
        }
    }

    // macOS 特有路径
    let common_dirs = [
        "/Library/Java/JavaVirtualMachines",
        "/System/Library/Java/JavaVirtualMachines",
    ];

    for base_dir in &common_dirs {
        if let Ok(entries) = std::fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                let java_bin = entry
                    .path()
                    .join("Contents")
                    .join("Home")
                    .join("bin")
                    .join("java");
                if java_bin.exists() {
                    paths.push(java_bin);
                }
            }
        }
    }

    // Minecraft runtime
    if let Ok(home) = std::env::var("HOME") {
        let minecraft_runtime = PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("minecraft")
            .join("runtime");
        if minecraft_runtime.exists() {
            scan_minecraft_runtime(&minecraft_runtime, &mut paths, "java");
        }
    }

    paths
}

fn scan_minecraft_runtime(base: &Path, paths: &mut Vec<PathBuf>, executable_name: &str) {
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let runtime_path = entry.path();
            // Minecraft runtime 通常是 jre-legacy, java-runtime-alpha, java-runtime-beta, java-runtime-gamma 等
            if runtime_path.is_dir() {
                let java_exe = runtime_path.join("bin").join(executable_name);
                if java_exe.exists() {
                    paths.push(java_exe);
                }
                // 有些版本可能嵌套更深
                if let Ok(nested) = std::fs::read_dir(&runtime_path) {
                    for nested_entry in nested.flatten() {
                        let nested_java = nested_entry.path().join("bin").join(executable_name);
                        if nested_java.exists() {
                            paths.push(nested_java);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_java_8_version() {
        let output = r#"java version "1.8.0_391""#;
        assert_eq!(parse_java_major_version(output), Some(8));
    }

    #[test]
    fn parses_java_17_version() {
        let output = r#"openjdk version "17.0.9" 2023-10-17"#;
        assert_eq!(parse_java_major_version(output), Some(17));
    }

    #[test]
    fn parses_java_21_version() {
        let output = r#"java version "21.0.1" 2023-10-17 LTS"#;
        assert_eq!(parse_java_major_version(output), Some(21));
    }

    #[test]
    fn extracts_openjdk_vendor() {
        let output = "openjdk version \"17.0.9\"";
        assert_eq!(extract_vendor(output), Some("OpenJDK".to_string()));
    }

    #[test]
    fn extracts_adoptium_vendor() {
        let output = "Eclipse Temurin Runtime";
        assert_eq!(extract_vendor(output), Some("Adoptium".to_string()));
    }

    #[test]
    fn discover_returns_sorted_by_version() {
        let runtimes = discover_java_runtimes();
        // 只检查排序，不假设系统一定有 Java
        for i in 1..runtimes.len() {
            assert!(
                runtimes[i - 1].major_version >= runtimes[i].major_version,
                "runtimes should be sorted in descending order"
            );
        }
    }
}
