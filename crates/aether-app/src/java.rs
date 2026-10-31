//! Java 运行时的列表：缓存、重新扫描、手动导入。
//!
//! 扫描要真正启动每个候选 `java -version`，一台机器上十几个候选意味着一两秒
//! 的等待，以及（在 Windows 上）一串一闪而过的控制台窗口。所以结果落在
//! `java-runtimes.json` 里，平时只读缓存；只有用户点「重新扫描」或导入一个新的
//! Java 时才会重新探测。
//!
//! 手动导入的路径单独存在同一个文件的 `imported` 里：这些位置扫描器未必找得到
//! （IDE 自带的 JBR、解压到奇怪目录的 JDK），不能因为一次重新扫描就丢掉。

use std::path::PathBuf;

use aether_runtime::JavaRuntime;
use serde::{Deserialize, Serialize};

use crate::host::Host;

/// 缓存文件里的内容。
#[derive(Debug, Default, Serialize, Deserialize)]
struct JavaCache {
    /// 上次扫描/导入得到的结果，直接返回给前端。
    #[serde(default)]
    runtimes: Vec<JavaRuntime>,
    /// 用户手动导入的可执行文件。重新扫描时一起带上，排在前面。
    #[serde(default)]
    imported: Vec<PathBuf>,
}

/// 缓存文件路径。放在应用数据目录，跟着这个启动器走。
fn cache_path(host: &dyn Host) -> Result<PathBuf, String> {
    host.data_dir()
        .map(|dir| dir.join("java-runtimes.json"))
        .map_err(|error| format!("cannot locate the app data directory: {error}"))
}

fn read_cache(host: &dyn Host) -> JavaCache {
    let Ok(path) = cache_path(host) else {
        return JavaCache::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return JavaCache::default();
    };
    // 缓存是本地文件，可能被手改或者写到一半；解析不了就当没有，重新扫一次。
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_cache(host: &dyn Host, cache: &JavaCache) {
    let Ok(path) = cache_path(host) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(cache) {
        let _ = std::fs::write(&path, text);
    }
}

/// 列出 Java。`refresh` 为 false 且有缓存时直接返回缓存。
pub fn list_runtimes(
    host: &dyn Host,
    refresh: bool,
) -> Result<Vec<aether_runtime::JavaRuntime>, String> {
    let cached = read_cache(host);
    // 缓存里的 Java 可能已经被卸载了：只返回文件还在的。全都没了就当没有缓存，重新扫。
    let still_there: Vec<JavaRuntime> = cached
        .runtimes
        .iter()
        .filter(|runtime| runtime.executable.is_file())
        .cloned()
        .collect();
    if !refresh && !still_there.is_empty() {
        return Ok(still_there);
    }

    let imported = cached.imported.clone();
    let runtimes = aether_runtime::scan_java_runtimes(&imported);

    write_cache(
        host,
        &JavaCache {
            runtimes: runtimes.clone(),
            imported: cached.imported,
        },
    );
    Ok(runtimes)
}

/// 让用户挑一个 `java`（或它所在的目录），探测成功后记进缓存。
///
/// 返回 `None` 表示用户取消了文件选择框。
pub fn pick_runtime(host: &dyn Host) -> Result<Option<aether_runtime::JavaRuntime>, String> {
    let Some(picked) = host.pick_file("Java", &["exe"])? else {
        return Ok(None);
    };
    let Some(executable) = aether_runtime::normalize_java_path(&picked) else {
        return Err(format!(
            "{} 不是一个 Java 可执行文件：请选 bin\\java.exe，或 Java 的安装目录",
            picked.display()
        ));
    };

    let probe = executable.clone();
    let runtime =
        JavaRuntime::probe(probe).map_err(|error| format!("这个 Java 用不了：{error}"))?;

    // 记进 imported，下次重新扫描也不会丢；已经有的就先去掉旧的那条。
    let mut cache = read_cache(host);
    cache.imported.retain(|path| *path != executable);
    cache.imported.push(executable);
    let imported = cache.imported.clone();
    let runtimes = aether_runtime::scan_java_runtimes(&imported);
    cache.runtimes = runtimes;
    write_cache(host, &cache);

    Ok(Some(runtime))
}
