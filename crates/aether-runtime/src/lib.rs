pub mod memory;

use std::collections::HashSet;
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

    /// Probe a Java executable and use its JVM properties when available.
    pub fn probe(executable: PathBuf) -> Result<Self, RuntimeError> {
        let mut output = Command::new(&executable)
            .args(["-XshowSettings:properties", "-version"])
            .output()
            .map_err(|error| RuntimeError::CommandFailed(executable.clone(), error))?;

        // Some older VMs do not support -XshowSettings:properties.
        if !output.status.success() {
            output = Command::new(&executable)
                .arg("-version")
                .output()
                .map_err(|error| RuntimeError::CommandFailed(executable.clone(), error))?;
        }

        if !output.status.success() {
            return Err(RuntimeError::UnsuccessfulExit(executable, output.status));
        }

        // The version banner normally goes to stderr, but some VMs use stdout.
        let version_output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let major_version = parse_java_major_version(&version_output)
            .ok_or_else(|| RuntimeError::VersionParseFailure(executable.clone()))?;

        Ok(Self {
            executable,
            major_version,
            architecture: extract_architecture(&version_output),
            vendor: extract_vendor(&version_output),
        })
    }
}

/// A [`Command`] that never flashes a console window.
///
/// Launching a delimited console app from a windowed process makes Windows
/// allocate a console for it. `CREATE_NO_WINDOW` asks for one that stays
/// invisible; it has no effect on other platforms.
pub fn hidden_command(program: &Path) -> Command {
    // `mut` 只有 Windows 分支用得到（`creation_flags` 需要 `&mut self`）。
    // 其他平台上这段代码被 cfg 掉，clippy 会报 `unused_mut` —— 不能简单删掉
    // `mut`，那样 Windows 上就编译不过了。
    #[allow(unused_mut)]
    let mut command = Command::new(program);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        /// `CREATE_NO_WINDOW`, from `winbase.h`.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("failed to execute java command at {0:?}: {1}")]
    CommandFailed(PathBuf, std::io::Error),
    #[error("failed to parse java version from {0:?}")]
    VersionParseFailure(PathBuf),
    #[error("java command at {0:?} exited unsuccessfully: {1}")]
    UnsuccessfulExit(PathBuf, std::process::ExitStatus),
}

fn parse_java_major_version(output: &str) -> Option<u16> {
    for line in output.lines() {
        let line = line.trim();
        let Some((prefix, quoted)) = line.split_once('"') else {
            continue;
        };
        if !matches!(
            prefix.trim(),
            "java version" | "openjdk version" | "java full version"
        ) {
            continue;
        }

        let version = quoted.split('"').next()?;
        let mut parts = version.split('.');
        let first = parts.next()?;
        if first == "1" {
            // Java 8 and earlier used versions such as 1.8.0_391.
            return parts.next()?.parse::<u16>().ok();
        }

        // Early-access JVMs may advertise versions such as "25-ea".
        let digits: String = first.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(major) = digits.parse::<u16>() {
            return Some(major);
        }
    }
    None
}

fn extract_property<'a>(output: &'a str, property: &str) -> Option<&'a str> {
    output.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        (name.trim() == property)
            .then_some(value.trim())
            .filter(|value| !value.is_empty())
    })
}

fn extract_architecture(output: &str) -> String {
    if let Some(arch) = extract_property(output, "os.arch") {
        let normalized = match arch.to_ascii_lowercase().as_str() {
            "amd64" | "x86_64" | "x64" => "x64",
            "aarch64" | "arm64" => "arm64",
            "x86" | "i386" | "i486" | "i586" | "i686" => "x86",
            "arm" | "arm32" => "arm",
            _ => arch,
        };
        return normalized.to_string();
    }

    // A generic "64-Bit Server VM" banner does not identify the CPU family.
    let lower = output.to_ascii_lowercase();
    if lower.contains("aarch64") || lower.contains("arm64") {
        "arm64".to_string()
    } else if lower.contains("amd64") || lower.contains("x86_64") {
        "x64".to_string()
    } else if lower.contains("i386") || lower.contains("i686") || lower.contains("x86") {
        "x86".to_string()
    } else {
        "unknown".to_string()
    }
}

fn extract_vendor(output: &str) -> Option<String> {
    let lower = output.to_ascii_lowercase();
    // Most vendor distributions also mention OpenJDK; check specific ones first.
    if lower.contains("adoptium") || lower.contains("temurin") {
        Some("Adoptium".to_string())
    } else if lower.contains("zulu") || lower.contains("azul") {
        Some("Azul Zulu".to_string())
    } else if lower.contains("amazon") || lower.contains("corretto") {
        Some("Amazon Corretto".to_string())
    } else if lower.contains("microsoft") {
        Some("Microsoft".to_string())
    } else if lower.contains("graalvm") {
        Some("GraalVM".to_string())
    } else if lower.contains("oracle") {
        Some("Oracle".to_string())
    } else if lower.contains("openjdk") {
        Some("OpenJDK".to_string())
    } else {
        extract_property(output, "java.vendor").map(str::to_string)
    }
}

/// 扫描系统中常见位置的 Java 安装
pub fn discover_java_runtimes() -> Vec<JavaRuntime> {
    let mut runtimes = Vec::new();
    let mut seen = HashSet::new();

    for candidate in discover_java_candidates() {
        // Different installs with the same vendor/major/architecture must remain
        // selectable. Only de-duplicate paths resolving to the same executable.
        let identity = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if !seen.insert(identity) {
            continue;
        }

        if let Ok(runtime) = JavaRuntime::probe(candidate) {
            runtimes.push(runtime);
        }
    }

    runtimes.sort_by_key(|runtime| std::cmp::Reverse(runtime.major_version));
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
    let common_dirs = ["/usr/lib/jvm", "/usr/java", "/opt/java", "/opt/jdk"];

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
    // Mojang commonly installs:
    // runtime/<component>/<platform>/<component>/bin/java(.exe).
    // Bound the depth and avoid symlink directories to prevent cycles.
    let mut pending = vec![(base.to_path_buf(), 0_u8)];
    while let Some((dir, depth)) = pending.pop() {
        let executable = dir.join("bin").join(executable_name);
        if executable.is_file() {
            paths.push(executable);
        }

        if depth >= 4 {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    pending.push((entry.path(), depth + 1));
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
    fn parses_version_after_unquoted_property_lines() {
        let output = "Property settings:\n    os.arch = amd64\nopenjdk version \"17.0.9\"";
        assert_eq!(parse_java_major_version(output), Some(17));
    }

    #[test]
    fn parses_early_access_version() {
        assert_eq!(
            parse_java_major_version("openjdk version \"25-ea\" 2025-03-18"),
            Some(25)
        );
    }

    #[test]
    fn ignores_unrelated_quoted_lines() {
        assert_eq!(
            parse_java_major_version("setting \"99\"\nnot a java version"),
            None
        );
    }

    #[test]
    fn detects_arm64_from_properties_even_with_64_bit_banner() {
        let output = "Property settings:\n    os.arch = aarch64\nOpenJDK 64-Bit Server VM";
        assert_eq!(extract_architecture(output), "arm64");
    }

    #[test]
    fn does_not_guess_x64_from_generic_64_bit_banner() {
        assert_eq!(extract_architecture("OpenJDK 64-Bit Server VM"), "unknown");
    }

    #[test]
    fn prefers_specific_vendor_to_generic_openjdk() {
        assert_eq!(
            extract_vendor("openjdk version \"21\"\nOpenJDK Runtime Environment Temurin"),
            Some("Adoptium".to_string())
        );
    }

    #[test]
    fn finds_mojang_nested_runtime_layout() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "aether-java-runtime-{}-{unique}",
            std::process::id()
        ));
        let name = if cfg!(windows) { "java.exe" } else { "java" };
        let executable = root
            .join("java-runtime-gamma")
            .join("windows-x64")
            .join("java-runtime-gamma")
            .join("bin")
            .join(name);

        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::File::create(&executable).unwrap();

        let mut paths = Vec::new();
        scan_minecraft_runtime(&root, &mut paths, name);
        let result = paths.contains(&executable);
        std::fs::remove_dir_all(root).unwrap();
        assert!(result, "nested Mojang runtime should be discovered");
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
