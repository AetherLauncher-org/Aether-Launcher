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

use aether_download::transport::http::UreqKernel;
use aether_download::transport::{HttpTransport, KernelHttpTransport, TransportRequest};
use aether_modloader::transport::{MetaError, MetaFetcher};

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

/// 执行一条 `InstallStep::Download`：下载到目标相对路径。
///
/// `destination` 由 `ModLoaderInstallPlan::validate` 保证是安全的相对路径，
/// 这里仍然再拼一次 `root` 并检查没有跳出根目录 —— 校验和写入之间不该有假设。
pub fn download_into(
    transport: &KernelHttpTransport<UreqKernel>,
    root: &Path,
    url: &str,
    destination: &str,
) -> Result<PathBuf, String> {
    if !url.starts_with("https://") {
        return Err(format!(
            "refusing to download over anything but https: {url}"
        ));
    }
    let target = root.join(destination.replace('\\', "/"));
    if !target.starts_with(root) {
        return Err(format!(
            "refusing to write outside the game directory: {destination}"
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }

    let request = TransportRequest {
        url: url.to_owned(),
        range: None,
        timeout_secs: 60,
    };
    let mut response = transport
        .get(&request)
        .map_err(|error| format!("cannot download {url}: {error}"))?;

    let mut body = Vec::new();
    response
        .body
        .read_to_end(&mut body)
        .map_err(|error| format!("cannot read {url}: {error}"))?;
    std::fs::write(&target, &body)
        .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
    Ok(target)
}
