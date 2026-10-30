//! 模组加载器元数据的网络拉取与安装执行。
//!
//! `aether-modloader` 只负责**算出要做什么**（构造 [`ModLoaderInstallPlan`]），
//! 它刻意不访问网络、不写文件、不启动进程 —— 网络通过 `MetaFetcher` trait 注入，
//! 落盘由这里的执行器按 `InstallStep` 逐条完成。
//!
//! 下载源只用官方：Fabric 的 `meta.fabricmc.net`、Forge 的
//! `maven.minecraftforge.net`、NeoForge 的 `maven.neoforged.net`。不做镜像切换，
//! 也不接受用户传入的地址 —— 清单和 jar 都从官方域名取。

use std::io::Read;
use std::path::{Path, PathBuf};

use aether_download::hash::HashingWriter;
use aether_download::transport::http::UreqKernel;
use aether_download::transport::{HttpTransport, KernelHttpTransport, TransportRequest};
use aether_modloader::transport::{MetaError, MetaFetcher};

use super::execute::safe_target;

/// 元数据请求的超时（秒）。这些接口都返回小 JSON，慢就是网络问题。
const META_TIMEOUT_SECS: u64 = 20;

/// 用启动器现成的 HTTP 传输实现 `MetaFetcher`。
///
/// 复用 `aether-download` 的 `KernelHttpTransport`，而不是再引一个 HTTP 客户端：
/// 这样超时、体积上限和错误文案都和游戏文件下载保持一致。
pub struct HttpMetaFetcher {
    transport: KernelHttpTransport<UreqKernel>,
}

impl HttpMetaFetcher {
    #[must_use]
    pub fn new() -> Self {
        Self {
            transport: KernelHttpTransport::new(UreqKernel::new()),
        }
    }
}

impl Default for HttpMetaFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaFetcher for HttpMetaFetcher {
    fn fetch_text(&self, url: &str) -> Result<String, MetaError> {
        // 只认 https：清单是信任起点，明文传输会被中间人换掉。
        if !url.starts_with("https://") {
            return Err(MetaError::Transport {
                url: url.to_owned(),
                reason: "metadata must be fetched over https".to_owned(),
            });
        }

        let request = TransportRequest {
            url: url.to_owned(),
            range: None,
            timeout_secs: META_TIMEOUT_SECS,
        };
        let response = self
            .transport
            .get(&request)
            .map_err(|error| MetaError::Transport {
                url: url.to_owned(),
                reason: error.to_string(),
            })?;

        // 元数据都是几十 KB 的 JSON；给一个上限，避免对端无限吐数据。
        const MAX_META_BYTES: u64 = 16 * 1024 * 1024;
        let mut body = Vec::new();
        response
            .body
            .take(MAX_META_BYTES)
            .read_to_end(&mut body)
            .map_err(|error| MetaError::Transport {
                url: url.to_owned(),
                reason: format!("cannot read response: {error}"),
            })?;

        String::from_utf8(body).map_err(|error| MetaError::Transport {
            url: url.to_owned(),
            reason: format!("response was not UTF-8: {error}"),
        })
    }
}

/// 单个下载文件的大小上限。jar 最大也就几十 MB，超过这个数一定是出了问题：
/// 对端被劫持、或者 URL 指错了地方。上限存在的意义是不让响应体撑爆内存。
const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

/// 执行一条 `InstallStep::Download`：下载到目标相对路径。
///
/// `destination` 由 `ModLoaderInstallPlan::validate` 保证是安全的相对路径，
/// 但这里是落盘的最后一道关口，所以仍然走
/// [`safe_target`](super::execute::safe_target) 重新解析一遍 —— 校验和写入之间
/// 不该有假设。`starts_with(root)` 挡不住 `..`，必须按路径组件解析。
///
/// `sha1` 是**官方清单给出的摘要**，不是从同一个响应里现算的。给了就必须匹配，
/// 不匹配直接删除落盘文件并报错。`size` 同理。两者都缺时只做大小上限检查，
/// 调用方应当在清单里尽量把哈希填上。
pub fn download_into(
    transport: &dyn HttpTransport,
    root: &Path,
    url: &str,
    destination: &str,
    sha1: Option<&str>,
    size: Option<u64>,
) -> Result<PathBuf, String> {
    if !url.starts_with("https://") {
        return Err(format!(
            "refusing to download over anything but https: {url}"
        ));
    }
    let target = safe_target(root, destination)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }

    let request = TransportRequest {
        url: url.to_owned(),
        range: None,
        timeout_secs: 60,
    };
    let response = transport
        .get(&request)
        .map_err(|error| format!("cannot download {url}: {error}"))?;

    // 流式落盘：边读边写边算摘要，内存里只留一个缓冲区。
    let limit = size.unwrap_or(MAX_DOWNLOAD_BYTES).min(MAX_DOWNLOAD_BYTES);
    let file = std::fs::File::create(&target)
        .map_err(|error| format!("cannot create {}: {error}", target.display()))?;
    let mut writer = HashingWriter::new(file);
    let copied = std::io::copy(
        &mut response.body.take(limit.saturating_add(1)),
        &mut writer,
    )
    .map_err(|error| {
        let _ = std::fs::remove_file(&target);
        format!("cannot read {url}: {error}")
    })?;
    let (file, actual_size, actual_sha1) = writer.finish();
    drop(file);

    if copied > limit {
        let _ = std::fs::remove_file(&target);
        return Err(format!(
            "{url} is larger than the {limit} byte limit; refusing to keep it"
        ));
    }
    if size.is_some_and(|expected| actual_size != expected) {
        let expected = size.unwrap_or_default();
        let _ = std::fs::remove_file(&target);
        return Err(format!(
            "{url} is {actual_size} bytes but the manifest says {expected}"
        ));
    }
    if sha1.is_some_and(|expected| !actual_sha1.eq_ignore_ascii_case(expected.trim())) {
        let expected = sha1.unwrap_or_default();
        let _ = std::fs::remove_file(&target);
        return Err(format!(
            "{url} has SHA-1 {actual_sha1} but the manifest says {expected}"
        ));
    }
    Ok(target)
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
