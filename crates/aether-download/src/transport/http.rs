//! HTTP 传输实现（策略层），网络内核由外部注入。
//!
//! 真实网络只出现在 [`UreqKernel`]（基于 ureq）这一个类型里；连接、TLS、DNS
//! 之外的策略（状态分类、重定向、Range 协商、大小上限、错误脱敏）都在本文件的
//! 纯逻辑层完成，测试全部使用内存内核，零真实外网。
//!
//! 与共享契约（[`super::HttpTransport`] / [`super::TransportError`]）的取舍：
//! - 重定向后的最终 URL：`TransportResponse` 没有对应字段，共享 trait 不在本阶段
//!   修改范围内，因此不随响应返回，仅在此处注释记录（见 `get` 实现）。
//! - 「超出大小上限」与「Content-Range 格式错误」：共享错误枚举没有对应变体，
//!   不改共享 enum，分别用 io 错误（`LimitedReader`）和
//!   [`TransportError::ReadInterrupted`] 携带明确的消息表达，消息中写明原因。
//!
//! 分层说明：本层的大小上限（`max_bytes`）是传输层防线，在字节流交给调用方之前
//! 就截断超限响应（包括无 Content-Length 的 chunked 流），防的是恶意镜像虚报
//! 长度耗尽磁盘/内存；engine 层之后还会按官方清单做 SHA-1 与最终大小校验，防的
//! 是内容被替换。两层互补，任何一层都不能替代另一层。

use std::io::{self, Read};

use super::{
    ContentRange, HttpTransport, Range, TransportError, TransportRequest, TransportResponse,
};

/// 默认响应体大小上限：512 MiB。
pub const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// 最多跟随的重定向跳数；超过即报错。
const MAX_REDIRECTS: usize = 3;

/// 真实网络内核的抽象。
///
/// 内核只负责一次 GET 的原始收发，不做状态分类、重定向或限流；
/// 生产实现是 [`UreqKernel`]，测试用内存替身，不经真实网络。
pub trait HttpKernel: Send + Sync {
    /// 执行一次 GET，返回原始状态码、响应头与响应体。
    fn get(&self, request: &KernelRequest) -> Result<KernelResponse, KernelError>;
}

/// 内核请求参数。
pub struct KernelRequest {
    /// 目标 URL（已通过合法性校验）。
    pub url: String,
    /// `Range` 请求头的值，例如 `bytes=10-99`；未请求区间时为 `None`。
    pub range_header: Option<String>,
    /// 连接与读取超时秒数。
    pub timeout_secs: u64,
}

/// 内核的原始响应。
///
/// 响应头以名值对列表原样回传（重定向策略需要读 `Location` 等头），
/// 因此不单独设 `accept_ranges`/`content_range` 字段，统一用 [`KernelResponse::header`]
/// 按名（大小写不敏感）取值；`content_length` 因为是纯数值，单独解析后提供。
pub struct KernelResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// `Content-Length` 的值；服务器未告知时为 `None`。
    pub content_length: Option<u64>,
    /// 原样回传的响应头列表。
    pub headers: Vec<(String, String)>,
    /// 响应体。
    pub body: Box<dyn Read + Send + 'static>,
}

impl KernelResponse {
    /// 按名（大小写不敏感）取响应头，首个匹配生效。
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// 内核错误。
pub enum KernelError {
    /// 建立连接失败（DNS/TCP/TLS）。
    Connection(String),
    /// 在超时时间内未完成请求。
    Timeout,
    /// 读取中断。
    Io(String),
}

/// 基于注入内核的 [`HttpTransport`] 实现：状态分类、重定向策略、大小上限。
pub struct KernelHttpTransport<K> {
    kernel: K,
    max_bytes: u64,
}

impl<K: HttpKernel> KernelHttpTransport<K> {
    /// 用给定内核创建传输，大小上限取 [`DEFAULT_MAX_BYTES`]。
    pub fn new(kernel: K) -> Self {
        Self {
            kernel,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    /// 覆写响应体大小上限。
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// 把内核的原始响应分类成 [`TransportResponse`] 或 [`TransportError`]。
    fn finish(
        &self,
        response: KernelResponse,
        url: &str,
        range_requested: bool,
    ) -> Result<TransportResponse, TransportError> {
        let _ = range_requested;
        match response.status {
            200 => {
                // 请求了 Range 但得到 200：如实返回 200，supports_range 置 false，
                // 上层据此走「不支持续传」路径（契约见 TransportResponse 文档）。
                Ok(TransportResponse {
                    status: 200,
                    content_length: response.content_length,
                    supports_range: false,
                    content_range: None,
                    body: Box::new(LimitedReader::new(response.body, self.max_bytes)),
                })
            }
            206 => {
                let header = response.header("Content-Range").ok_or_else(|| {
                    TransportError::ReadInterrupted {
                        url: url.to_owned(),
                        source: io::Error::other("206 response without Content-Range header"),
                    }
                })?;
                // 共享 enum 没有「格式错误」变体：按约定不改 enum，
                // 用 ReadInterrupted 携带明确消息。
                let content_range =
                    parse_content_range(header).ok_or_else(|| TransportError::ReadInterrupted {
                        url: url.to_owned(),
                        source: io::Error::other(format!(
                            "malformed Content-Range header: {header:?}"
                        )),
                    })?;
                Ok(TransportResponse {
                    status: 206,
                    content_length: response.content_length,
                    supports_range: true,
                    content_range: Some(content_range),
                    body: Box::new(LimitedReader::new(response.body, self.max_bytes)),
                })
            }
            status => Err(TransportError::HttpStatus {
                url: url.to_owned(),
                status,
            }),
        }
    }
}

impl<K: HttpKernel> HttpTransport for KernelHttpTransport<K> {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let mut url = request.url.clone();
        validate_url(&url)?;
        // 循环体最多执行 MAX_REDIRECTS + 1 次：前 MAX_REDIRECTS 次允许消费重定向，
        // 最后一轮仍是重定向则视为超限。重定向后的最终 URL 不随响应返回
        // （TransportResponse 无该字段，共享契约本阶段冻结），如需暴露应后续加字段。
        for _ in 0..=MAX_REDIRECTS {
            let kernel_response = self
                .kernel
                .get(&KernelRequest {
                    url: url.clone(),
                    range_header: request.range.map(Range::to_header_value),
                    timeout_secs: request.timeout_secs,
                })
                .map_err(|error| map_kernel_error(error, &url, request.timeout_secs))?;

            if matches!(kernel_response.status, 301 | 302 | 303 | 307 | 308) {
                let location = kernel_response
                    .header("Location")
                    .ok_or_else(|| TransportError::Connect {
                        url: url.clone(),
                        source: io::Error::other("redirect response without Location header"),
                    })?
                    .to_owned();
                validate_url(&location)?;
                if is_https(&url) && !is_https(&location) {
                    // https → http 降级一律拒绝。
                    return Err(TransportError::Connect {
                        url: url.clone(),
                        source: io::Error::other(format!(
                            "refusing downgrade redirect to {location}"
                        )),
                    });
                }
                url = location;
                continue;
            }

            return self.finish(kernel_response, &url, request.range.is_some());
        }
        Err(TransportError::Connect {
            url: url.clone(),
            source: io::Error::other(format!("exceeded {MAX_REDIRECTS} redirects")),
        })
    }
}

/// 把内核错误映射到共享错误枚举。
fn map_kernel_error(error: KernelError, url: &str, timeout_secs: u64) -> TransportError {
    match error {
        KernelError::Connection(message) => TransportError::Connect {
            url: url.to_owned(),
            source: io::Error::other(message),
        },
        KernelError::Timeout => TransportError::Timeout {
            url: url.to_owned(),
            timeout_secs,
        },
        KernelError::Io(message) => TransportError::ReadInterrupted {
            url: url.to_owned(),
            source: io::Error::other(message),
        },
    }
}

/// URL 必须是 http/https 且 host 非空。
fn validate_url(url: &str) -> Result<(), TransportError> {
    let invalid = || TransportError::Connect {
        url: url.to_owned(),
        source: io::Error::other("invalid URL: expected http/https with a non-empty host"),
    };
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(invalid());
    };
    if !matches!(scheme, "http" | "https") {
        return Err(invalid());
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return Err(invalid());
    }
    Ok(())
}

fn is_https(url: &str) -> bool {
    url.starts_with("https://")
}

/// 解析 `Content-Range: bytes start-end/total`，`total` 为 `*` 时是 `None`。
fn parse_content_range(value: &str) -> Option<ContentRange> {
    let rest = value.strip_prefix("bytes")?.trim_start();
    let (range_part, total_part) = rest.split_once('/')?;
    let (start, end) = range_part.split_once('-')?;
    let start = start.trim().parse::<u64>().ok()?;
    let end = end.trim().parse::<u64>().ok()?;
    if end < start {
        return None;
    }
    let total = if total_part.trim() == "*" {
        None
    } else {
        Some(total_part.trim().parse::<u64>().ok()?)
    };
    Some(ContentRange { start, end, total })
}

/// 超过大小上限即报错的读取包装。
///
/// 共享错误枚举没有「超出大小上限」变体，且响应体在返回后只能以 io::Error
/// 报错（body 是 `dyn Read`），因此这里用 io 错误、消息中写明 `exceeds size cap`。
struct LimitedReader {
    inner: Box<dyn Read + Send>,
    remaining: u64,
}

impl LimitedReader {
    fn new(inner: Box<dyn Read + Send>, max_bytes: u64) -> Self {
        Self {
            inner,
            remaining: max_bytes,
        }
    }
}

impl Read for LimitedReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other("response body exceeds size cap"));
        }
        let limit = buf.len().min(self.remaining as usize);
        let read = self.inner.read(&mut buf[..limit])?;
        self.remaining -= read as u64;
        Ok(read)
    }
}

/// 生产内核：真实网络只出现在这一个类型里（内部是 ureq）。
///
/// 状态码与重定向不由内核裁决（`http_status_as_error(false)`、
/// `max_redirects(0)`），统一交给 [`KernelHttpTransport`]，保证策略只有一份。
/// 超时是每次请求的配置，因此每次调用构建独立 agent；连接复用交给 engine 层
/// 的大块 Range 请求来摊薄，内核保持无状态。
pub struct UreqKernel {}

impl UreqKernel {
    /// 创建内核。
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for UreqKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpKernel for UreqKernel {
    fn get(&self, request: &KernelRequest) -> Result<KernelResponse, KernelError> {
        let agent = ureq::config::Config::builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .user_agent(concat!("AetherLauncher/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(std::time::Duration::from_secs(request.timeout_secs)))
            .build();
        let agent = ureq::Agent::new_with_config(agent);
        let mut call = agent.get(&request.url);
        if let Some(range) = &request.range_header {
            call = call.header("Range", range);
        }
        let response = call.call().map_err(map_ureq_error)?;
        let status = response.status().as_u16();
        let mut headers = Vec::new();
        for (name, value) in response.headers().iter() {
            if let Ok(value) = value.to_str() {
                headers.push((name.to_string(), value.to_owned()));
            }
        }
        let content_length = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
            .and_then(|(_, value)| value.parse().ok());
        let body = response.into_parts().1.into_reader();
        Ok(KernelResponse {
            status,
            content_length,
            headers,
            body: Box::new(body),
        })
    }
}

/// 把 ureq 错误映射为 [`KernelError`]。
///
/// ureq 错误的字符串可能携带原始 URL（含查询串），所以这里只透出类别
/// 描述，不透出错误原文；URL 由 `TransportError` 各变体以脱敏形式补回。
fn map_ureq_error(error: ureq::Error) -> KernelError {
    match error {
        ureq::Error::Timeout(_) => KernelError::Timeout,
        ureq::Error::Io(ref io)
            if matches!(
                io.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            KernelError::Timeout
        }
        ureq::Error::HostNotFound => KernelError::Connection("host not found".to_owned()),
        ureq::Error::ConnectionFailed => KernelError::Connection("connection failed".to_owned()),
        ureq::Error::Tls(_) => KernelError::Connection("tls error".to_owned()),
        // 状态码错误只会在 http_status_as_error(true) 时出现，这里配置为
        // false，状态码走 KernelResponse 原样返回；兜底归为连接错误。
        _ => KernelError::Connection("network error".to_owned()),
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
