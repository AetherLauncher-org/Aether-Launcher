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
    ///
    /// Spawning a console program from a GUI app opens a console window on
    /// Windows unless the child is created detached, so every run goes through
    /// [`hidden_command`].
    pub fn probe(executable: PathBuf) -> Result<Self, RuntimeError> {
        let mut output = hidden_command(&executable)
            .args(["-XshowSettings:properties", "-version"])
            .output()
            .map_err(|error| RuntimeError::CommandFailed(executable.clone(), error))?;

        // Some older VMs do not support -XshowSettings:properties.
        if !output.status.success() {
            output = hidden_command(&executable)
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

impl JavaRuntime {
    /// Describe a Java installation **without running it**, from the `release`
    /// file that JDKs and JREs keep in their home directory (`<home>/release`,
    /// next to `bin/`).
    ///
    /// Folders found by scanning arbitrary places on disk may contain anything, so
    /// nothing in there is executed to learn what it is: running an unknown
    /// `java.exe` found in `E:\something` would run whatever someone put there.
    /// The `release` file is plain text, read with a size limit and never through
    /// a symbolic link. Returns `None` when there is no usable one.
    #[must_use]
    pub fn from_release_file(executable: &Path) -> Option<Self> {
        const MAX_RELEASE_BYTES: u64 = 64 * 1024;

        let is_java = executable.file_name().is_some_and(|name| {
            name.eq_ignore_ascii_case("java.exe")
                || name.eq_ignore_ascii_case("javaw.exe")
                || name.eq_ignore_ascii_case("java")
        });
        if !is_java {
            return None;
        }
        let home = executable.parent()?.parent()?;
        let release = home.join("release");

        let metadata = std::fs::symlink_metadata(&release).ok()?;
        if !metadata.is_file() || metadata.len() > MAX_RELEASE_BYTES {
            return None;
        }
        let text = std::fs::read_to_string(&release).ok()?;

        let value_of = |key: &str| -> Option<String> {
            text.lines().find_map(|line| {
                let (name, value) = line.trim().split_once('=')?;
                (name.trim() == key)
                    .then(|| value.trim().trim_matches('"').to_owned())
                    .filter(|value| !value.is_empty())
            })
        };

        let major_version = major_from_version(&value_of("JAVA_VERSION")?)?;
        let architecture = value_of("OS_ARCH")
            .map(|arch| normalize_architecture(&arch))
            .unwrap_or_else(|| "unknown".to_owned());
        let vendor = extract_vendor(&text).or_else(|| value_of("IMPLEMENTOR"));

        Some(Self {
            executable: executable.to_path_buf(),
            major_version,
            architecture,
            vendor,
        })
    }
}

/// A [Command] that never flashes a console window.
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
        if let Some(major) = major_from_version(version) {
            return Some(major);
        }
    }
    None
}

/// `1.8.0_391` → 8，`21.0.4` → 21，`25-ea` → 25。
fn major_from_version(version: &str) -> Option<u16> {
    let mut parts = version.trim().split('.');
    let first = parts.next()?;
    if first == "1" {
        // Java 8 and earlier used versions such as 1.8.0_391.
        let second: String = parts
            .next()?
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        return second.parse::<u16>().ok();
    }

    // Early-access JVMs may advertise versions such as "25-ea".
    let digits: String = first.chars().take_while(char::is_ascii_digit).collect();
    digits.parse::<u16>().ok()
}

fn extract_property<'a>(output: &'a str, property: &str) -> Option<&'a str> {
    output.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        (name.trim() == property)
            .then_some(value.trim())
            .filter(|value| !value.is_empty())
    })
}

fn normalize_architecture(arch: &str) -> String {
    match arch.to_ascii_lowercase().as_str() {
        "amd64" | "x86_64" | "x64" => "x64",
        "aarch64" | "arm64" => "arm64",
        "x86" | "i386" | "i486" | "i586" | "i686" => "x86",
        "arm" | "arm32" => "arm",
        _ => arch,
    }
    .to_string()
}

fn extract_architecture(output: &str) -> String {
    if let Some(arch) = extract_property(output, "os.arch") {
        return normalize_architecture(arch);
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

/// 找出**可以运行**的 Java：已知的安装位置（PATH、JAVA_HOME、各厂商的标准安装目录、
/// Minecraft 自带的 runtime）加上用户手动导入的。启动游戏用的就是这个。
///
/// extra 是用户手动导入的 java 可执行文件（或它所在的 bin 目录），排在
/// 自动扫描结果前面：手动指定的东西不该被扫描漏掉。用户自己挑的，才算信任。
pub fn discover_java_runtimes(extra: &[PathBuf]) -> Vec<JavaRuntime> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for path in extra {
        if let Some(executable) = normalize_java_path(path) {
            candidates.push(executable);
        }
    }
    candidates.extend(discover_java_candidates());
    probe_candidates(candidates)
}

/// 设置页里「重新扫描」用的列表：在 [discover_java_runtimes] 的基础上，再翻一遍每个盘符
/// 根目录下的文件夹（解压版 JDK 常被随手放在 E:\jdk17 这种地方）。
///
/// 这些随便翻到的文件夹里什么都可能有，所以**不运行**它们的 java，只读旁边的 release
/// 文件来描述（见 [JavaRuntime::from_release_file]）。启动游戏不会用到它们；想用的话，
/// 在设置里「导入」，用户亲自选过才算信任。
pub fn scan_java_runtimes(extra: &[PathBuf]) -> Vec<JavaRuntime> {
    let mut runtimes = discover_java_runtimes(extra);
    let mut seen: HashSet<PathBuf> = runtimes
        .iter()
        .map(|runtime| {
            std::fs::canonicalize(&runtime.executable)
                .unwrap_or_else(|_| runtime.executable.clone())
        })
        .collect();
    runtimes.extend(described_candidates(broad_java_candidates(), &mut seen));
    runtimes.sort_by_key(|runtime| std::cmp::Reverse(runtime.major_version));
    runtimes
}

/// 用 release 文件描述一组候选，跳过已经见过的路径。不运行任何东西。
fn described_candidates<I>(candidates: I, seen: &mut HashSet<PathBuf>) -> Vec<JavaRuntime>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut runtimes = Vec::new();
    for candidate in candidates {
        let identity = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if !seen.insert(identity) {
            continue;
        }
        if let Some(runtime) = JavaRuntime::from_release_file(&candidate) {
            runtimes.push(runtime);
        }
    }
    runtimes
}

/// 把用户挑的路径规整成 java 可执行文件。
///
/// 允许三种输入：`java.exe` 本身、包含它的 `bin` 目录、以及 JDK 根目录。
/// 挑错的情况很常见（有人直接选了 `jdk-21` 那一层），这里替用户补一层。
#[must_use]
pub fn normalize_java_path(path: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    if path.is_file() {
        let looks_like_java = path.file_name().is_some_and(|file| {
            file.eq_ignore_ascii_case(name)
                || file.eq_ignore_ascii_case("javaw.exe")
                || file.eq_ignore_ascii_case("java")
        });
        return looks_like_java.then(|| path.to_path_buf());
    }
    [path.join("bin").join(name), path.join(name)]
        .into_iter()
        .find(|candidate| candidate.is_file())
}

/// 探测一组可执行文件，去掉重复路径并按版本从高到低排。
fn probe_candidates<I>(candidates: I) -> Vec<JavaRuntime>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut runtimes = Vec::new();
    let mut seen = HashSet::new();

    for candidate in candidates {
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

/// 只探测用户手动挑的这些路径，用于「导入」时立刻给出反馈。
pub fn probe_java_paths(paths: &[PathBuf]) -> Vec<JavaRuntime> {
    let candidates: Vec<PathBuf> = paths
        .iter()
        .filter_map(|path| normalize_java_path(path))
        .collect();
    probe_candidates(candidates)
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

    // 常见安装位置：各厂商的标准安装目录。
    //
    // 这些目录里就是一堆 <产品名>/bin，往下看几层；它们在 Program Files 下，普通
    // 进程写不进去，所以可以运行里面的 java。随便放在盘根目录的那种见
    // [broad_java_candidates]。
    let bases: Vec<PathBuf> = [
        r"C:\Program Files\Java",
        r"C:\Program Files (x86)\Java",
        r"C:\Program Files\Eclipse Adoptium",
        r"C:\Program Files\Eclipse Foundation",
        r"C:\Program Files\Microsoft",
        r"C:\Program Files\Zulu",
        r"C:\Program Files\Amazon Corretto",
        r"C:\Program Files\BellSoft",
        r"C:\Program Files\Semeru",
        r"C:\Program Files\JetBrains",
        r"C:\Program Files\Android\Android Studio",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    for base_dir in &bases {
        collect_java_in(base_dir, &mut paths, 0);
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

/// Every top-level folder on every drive: `<dir>/bin/java.exe` and the IDE layout
/// `<dir>/jbr/bin/java.exe`.
///
/// Unpacked JDKs often end up in places like `E:\jdk17` with names nobody could list in
/// advance. Only one level is looked at, and the usual big system folders are skipped:
/// a drive root holds hundreds of folders, and going deeper turns a scan of a few seconds
/// into one of a minute and a half.
///
/// These are only **candidates and must never be run**: a USB stick, a network drive or a
/// freshly extracted archive can contain anything. [`scan_java_runtimes`] describes them
/// from their `release` file instead.
#[cfg(target_os = "windows")]
fn broad_java_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for letter in 'A'..='Z' {
        let Ok(entries) = std::fs::read_dir(format!("{letter}:\\")) else {
            continue;
        };
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let name = entry.file_name();
            if is_skipped_directory(&name.to_string_lossy()) {
                continue;
            }
            collect_java_in(&entry.path(), &mut paths, 2);
        }
    }
    paths
}

/// Other platforms have no drive letters; their usual locations are already in
/// `discover_java_candidates`.
#[cfg(not(target_os = "windows"))]
fn broad_java_candidates() -> Vec<PathBuf> {
    Vec::new()
}

/// 在一个基础目录里找 `<base>/bin/java.exe` 和 `<base>/<子目录>/bin/java.exe`。
///
/// 只走三层，且跳过 `node_modules`、`.git` 这类明显不会有 JRE 的目录：整盘
/// 递归会把启动拖到几十秒，而 Java 的目录结构就那么几种。
#[cfg(target_os = "windows")]
fn collect_java_in(base: &Path, paths: &mut Vec<PathBuf>, depth: u8) {
    if depth > 2 {
        return;
    }
    let direct = base.join("bin").join("java.exe");
    if direct.is_file() {
        paths.push(direct);
    }
    // 有些 IDE 把 JBR 放在 `bin` 的兄弟层级，例如 `jbr/bin`。
    let jbr = base.join("jbr").join("bin").join("java.exe");
    if jbr.is_file() {
        paths.push(jbr);
    }

    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_skipped_directory(&name) {
            continue;
        }
        collect_java_in(&entry.path(), paths, depth + 1);
    }
}

/// 明显不会藏着 JRE、但目录特别大的那些名字。
///
/// 盘根下一层的目录动辄几十个、里面几千个文件，逐个往下翻会把扫描拖成几十秒，
/// 所以这里先按名字挡掉一批。挡错了的代价只是那个目录里的 Java 扫不到，用户还
/// 可以手动导入。
#[cfg(target_os = "windows")]
fn is_skipped_directory(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "windows"
            | "winsxs"
            | "system32"
            | "syswow64"
            | "node_modules"
            | ".git"
            | "$recycle.bin"
            | "system volume information"
            | "recovery"
            | "perflogs"
            | "programdata"
    )
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
        let runtimes = discover_java_runtimes(&[]);
        // 只检查排序，不假设系统一定有 Java
        for i in 1..runtimes.len() {
            assert!(
                runtimes[i - 1].major_version >= runtimes[i].major_version,
                "runtimes should be sorted in descending order"
            );
        }
    }
}

#[cfg(test)]
mod release_file_tests {
    //! `from_release_file` describes a Java **without running it**.

    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct TempHome(PathBuf);

    impl TempHome {
        /// `<home>/bin/<exe>` with a deliberately *unrunnable* "java", plus an optional
        /// `release` file. If anything tried to execute it, it would fail loudly.
        fn new(release: Option<&str>, exe: &str) -> Self {
            let id = COUNTER.fetch_add(1, Ordering::Relaxed);
            let home =
                std::env::temp_dir().join(format!("aether-release-{}-{id}", std::process::id()));
            let _ = fs::remove_dir_all(&home);
            fs::create_dir_all(home.join("bin")).expect("create bin");
            fs::write(home.join("bin").join(exe), b"this is not a program").expect("write exe");
            if let Some(release) = release {
                fs::write(home.join("release"), release).expect("write release");
            }
            Self(home)
        }

        fn java(&self, exe: &str) -> PathBuf {
            self.0.join("bin").join(exe)
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const JDK21: &str = "IMPLEMENTOR=\"Eclipse Adoptium\"\nJAVA_VERSION=\"21.0.4\"\nOS_ARCH=\"x86_64\"\nOS_NAME=\"Windows\"\n";

    #[test]
    fn describes_a_jdk_from_its_release_file_without_running_it() {
        let home = TempHome::new(Some(JDK21), "java.exe");

        let runtime = JavaRuntime::from_release_file(&home.java("java.exe"))
            .expect("a readable release file is enough");

        assert_eq!(runtime.major_version, 21);
        assert_eq!(runtime.architecture, "x64");
        assert_eq!(runtime.vendor.as_deref(), Some("Adoptium"));
        assert_eq!(runtime.executable, home.java("java.exe"));
    }

    #[test]
    fn reads_the_old_and_the_early_access_version_styles() {
        for (version, major) in [("1.8.0_391", 8), ("17.0.11", 17), ("25-ea", 25), ("21", 21)] {
            let home = TempHome::new(
                Some(&format!(
                    "JAVA_VERSION=\"{version}\"\nOS_ARCH=\"aarch64\"\n"
                )),
                "java.exe",
            );
            let runtime = JavaRuntime::from_release_file(&home.java("java.exe")).expect(version);
            assert_eq!(runtime.major_version, major, "{version}");
            assert_eq!(runtime.architecture, "arm64");
        }
    }

    #[test]
    fn falls_back_to_the_implementor_for_an_unknown_vendor() {
        let home = TempHome::new(
            Some("IMPLEMENTOR=\"JetBrains s.r.o.\"\nJAVA_VERSION=\"21.0.3\"\n"),
            "java.exe",
        );

        let runtime = JavaRuntime::from_release_file(&home.java("java.exe")).expect("runtime");

        assert_eq!(runtime.vendor.as_deref(), Some("JetBrains s.r.o."));
        assert_eq!(runtime.architecture, "unknown");
    }

    #[test]
    fn a_folder_with_no_release_file_is_not_listed() {
        let home = TempHome::new(None, "java.exe");
        assert!(JavaRuntime::from_release_file(&home.java("java.exe")).is_none());
    }

    #[test]
    fn a_release_file_without_a_usable_version_is_not_listed() {
        for text in [
            "",
            "OS_ARCH=\"x64\"\n",
            "JAVA_VERSION=\"\"\n",
            "JAVA_VERSION=\"banana\"\n",
        ] {
            let home = TempHome::new(Some(text), "java.exe");
            assert!(
                JavaRuntime::from_release_file(&home.java("java.exe")).is_none(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn an_oversized_release_file_is_ignored() {
        let big = format!("JAVA_VERSION=\"21\"\n{}", "x".repeat(70 * 1024));
        let home = TempHome::new(Some(&big), "java.exe");
        assert!(JavaRuntime::from_release_file(&home.java("java.exe")).is_none());
    }

    #[test]
    fn only_files_named_like_java_count() {
        let home = TempHome::new(Some(JDK21), "evil.exe");
        assert!(JavaRuntime::from_release_file(&home.java("evil.exe")).is_none());
    }

    #[test]
    fn scanning_lists_folders_it_never_executes() {
        // The decisive property: a candidate whose "java" cannot run at all is still
        // described, which is only possible if nothing runs it.
        let home = TempHome::new(Some(JDK21), "java.exe");
        let mut seen = HashSet::new();

        let described = described_candidates([home.java("java.exe")], &mut seen);

        assert_eq!(described.len(), 1);
        assert_eq!(described[0].major_version, 21);
        // ...whereas probing (which runs it) rejects the very same file.
        assert!(JavaRuntime::probe(home.java("java.exe")).is_err());
    }

    #[test]
    fn a_candidate_seen_twice_is_listed_once() {
        let home = TempHome::new(Some(JDK21), "java.exe");
        let mut seen = HashSet::new();

        let first = described_candidates([home.java("java.exe")], &mut seen);
        let second = described_candidates([home.java("java.exe")], &mut seen);

        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
    }

    #[test]
    fn version_strings_map_to_majors() {
        assert_eq!(major_from_version("1.8.0_391"), Some(8));
        assert_eq!(major_from_version("1.7"), Some(7));
        assert_eq!(major_from_version("21.0.4"), Some(21));
        assert_eq!(major_from_version(" 25-ea "), Some(25));
        assert_eq!(major_from_version(""), None);
        assert_eq!(major_from_version("x"), None);
    }

    /// 在这台机器上真扫一遍，看数量和耗时。默认跳过：
    ///
    /// ```text
    /// cargo test -p aether-runtime -- --ignored live_scan --nocapture
    /// ```
    #[test]
    #[ignore = "scans this machine's disks"]
    fn live_scan_lists_this_machine() {
        let started = std::time::Instant::now();
        let runtimes = scan_java_runtimes(&[]);
        let trusted = discover_java_runtimes(&[]).len();
        println!(
            "scan: {} runtimes ({} of them runnable, the rest described from release files) in {:?}",
            runtimes.len(),
            trusted,
            started.elapsed()
        );
        for runtime in &runtimes {
            println!(
                "  Java {} {} {:?}",
                runtime.major_version, runtime.architecture, runtime.executable
            );
        }
        assert!(runtimes.len() >= trusted);
    }
}
