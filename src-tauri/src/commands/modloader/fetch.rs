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
use aether_download::transport::{
    HttpTransport, KernelHttpTransport, TransportError, TransportRequest, TransportResponse,
};
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

/// 一次请求最多试几次。一个加载器安装要下几十到上百个文件，网络偶尔抖一下很常见，
/// 不该因为一次连接失败就把整个安装作废。
const REQUEST_ATTEMPTS: u32 = 3;

/// 这个错误值得重试：连不上、超时、服务器 5xx。404、哈希不对这类不会因为再试一次变好。
fn is_transient(error: &TransportError) -> bool {
    match error {
        TransportError::Connect { .. } | TransportError::Timeout { .. } => true,
        TransportError::HttpStatus { status, .. } => *status >= 500,
        _ => false,
    }
}

/// `transport.get`，对瞬时错误最多重试 [`REQUEST_ATTEMPTS`] 次，每次多等一会儿。
fn get_with_retries(
    transport: &dyn HttpTransport,
    request: &TransportRequest,
) -> Result<TransportResponse, TransportError> {
    let mut attempt = 1;
    loop {
        match transport.get(request) {
            Err(error) if attempt < REQUEST_ATTEMPTS && is_transient(&error) => {
                std::thread::sleep(std::time::Duration::from_millis(400 * u64::from(attempt)));
                attempt += 1;
            }
            other => return other,
        }
    }
}

/// `.sha1` 旁注文件的大小上限。里面只有 40 个十六进制字符，可能带文件名。
const MAX_SIDECAR_BYTES: u64 = 1024;

/// 一个合法的 SHA-1：40 个十六进制字符。
fn is_sha1(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 从官方 maven 的 `<文件>.sha1` 旁注文件读摘要。
///
/// Fabric、Forge、NeoForge 的 maven 都为每个构件发布 `.sha1`。这些加载器没有像
/// Mojang 那样的「带哈希的官方清单」，旁注文件是官方仓库自己给出的唯一校验值：
/// 它和构件在同一个 https 主机上，防的是传输损坏和落盘错误，防不了仓库本身被
/// 攻破。是否接受这个信任模型是设计问题，见 `docs/modules/modloader.md`。
///
/// 读不到、格式不对一律失败：宁可装不上，也不装一个没有校验值的文件。
fn fetch_sidecar_sha1(transport: &dyn HttpTransport, url: &str) -> Result<String, String> {
    let sidecar = format!("{url}.sha1");
    let response = get_with_retries(
        transport,
        &TransportRequest {
            url: sidecar.clone(),
            range: None,
            timeout_secs: META_TIMEOUT_SECS,
        },
    )
    .map_err(|error| format!("cannot read the checksum {sidecar}: {error}"))?;
    let mut body = Vec::new();
    response
        .body
        .take(MAX_SIDECAR_BYTES)
        .read_to_end(&mut body)
        .map_err(|error| format!("cannot read the checksum {sidecar}: {error}"))?;
    let text = String::from_utf8_lossy(&body);
    // 常见写法是 `<sha1>` 或 `<sha1>  <文件名>`，取第一个词。
    let digest = text.split_whitespace().next().unwrap_or_default();
    if !is_sha1(digest) {
        return Err(format!(
            "{sidecar} does not contain a SHA-1; refusing to install a file with no checksum"
        ));
    }
    Ok(digest.to_ascii_lowercase())
}

/// 执行一条 `InstallStep::Download`：下载到目标相对路径。
///
/// - `destination` 由 `ModLoaderInstallPlan::validate` 保证是安全的相对路径，但这里是
///   落盘的最后一道关口，所以仍然走 [`safe_target`](super::execute::safe_target)。
/// - **每个文件都必须有校验值。** `sha1` 是计划里官方清单给的摘要；计划没有给（Forge、
///   NeoForge 的 installer，Fabric 的核心库）时，从官方 maven 的 `<文件>.sha1` 读，
///   读不到就失败。大小 `size` 同理，给了就必须一致。
/// - 先写到 `<目标>.part`，大小和哈希都通过后才改名到位：中断或校验失败不会在最终
///   路径上留下一个残缺或被替换的文件。
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

    let expected_sha1 = match sha1.map(str::trim) {
        Some(given) if is_sha1(given) => given.to_ascii_lowercase(),
        Some(given) => {
            return Err(format!(
                "the manifest gives an invalid SHA-1 for {url}: {given:?}"
            ));
        }
        None => fetch_sidecar_sha1(transport, url)?,
    };

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut part_name = target
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    part_name.push(".part");
    let part = target.with_file_name(part_name);

    let request = TransportRequest {
        url: url.to_owned(),
        range: None,
        timeout_secs: 60,
    };
    let response = get_with_retries(transport, &request)
        .map_err(|error| format!("cannot download {url}: {error}"))?;

    // 流式落盘：边读边写边算摘要，内存里只留一个缓冲区。
    let limit = size.unwrap_or(MAX_DOWNLOAD_BYTES).min(MAX_DOWNLOAD_BYTES);
    let fail = |message: String| {
        let _ = std::fs::remove_file(&part);
        message
    };
    let file = std::fs::File::create(&part)
        .map_err(|error| format!("cannot create {}: {error}", part.display()))?;
    let mut writer = HashingWriter::new(file);
    let copied = std::io::copy(
        &mut response.body.take(limit.saturating_add(1)),
        &mut writer,
    )
    .map_err(|error| fail(format!("cannot read {url}: {error}")))?;
    let (file, actual_size, actual_sha1) = writer.finish();
    drop(file);

    if copied > limit {
        return Err(fail(format!(
            "{url} is larger than the {limit} byte limit; refusing to keep it"
        )));
    }
    if let Some(expected) = size.filter(|expected| actual_size != *expected) {
        return Err(fail(format!(
            "{url} is {actual_size} bytes but the manifest says {expected}"
        )));
    }
    if !actual_sha1.eq_ignore_ascii_case(&expected_sha1) {
        return Err(fail(format!(
            "{url} has SHA-1 {actual_sha1} but the checksum says {expected_sha1}"
        )));
    }
    std::fs::rename(&part, &target).map_err(|error| {
        fail(format!(
            "cannot move the verified file into place at {}: {error}",
            target.display()
        ))
    })?;
    Ok(target)
}

#[cfg(test)]
#[path = "fetch_tests.rs"]
mod tests;
