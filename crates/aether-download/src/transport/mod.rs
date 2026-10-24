//! 下载传输层的接口定义。
//!
//! 本模块只定义接口与数据结构，不做真实网络请求；真实实现由后续 PR 提供。
//!
//! TODO: 提供基于真实 HTTP 客户端的 `HttpTransport` 实现（连接池、代理、TLS、重定向策略）。

use std::io::Read;

use thiserror::Error;

/// `Content-Range: bytes start-end/total`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    /// 本次响应体第一个字节在整个文件中的偏移。
    pub start: u64,
    /// 本次响应体最后一个字节的偏移（含）。
    pub end: u64,
    /// 整个文件的总长度；服务器用 `*` 表示未知时为 `None`。
    pub total: Option<u64>,
}

/// 字节区间，用于 HTTP Range 请求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    /// 起始字节偏移（含）。
    pub start: u64,
    /// 结束字节偏移（含）；`None` 表示到文件末尾。
    pub end: Option<u64>,
}

impl Range {
    /// 生成 Range 请求头的值，例如 `bytes=0-`、`bytes=10-99`。
    #[must_use]
    pub fn to_header_value(self) -> String {
        match self.end {
            Some(end) => format!("bytes={}-{}", self.start, end),
            None => format!("bytes={}-", self.start),
        }
    }
}

/// 一次传输请求的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportRequest {
    /// 目标 URL。
    pub url: String,
    /// 可选的字节区间；`None` 表示请求整个文件。
    pub range: Option<Range>,
    /// 连接与读取的超时秒数。
    pub timeout_secs: u64,
}

/// 传输层的响应。
///
/// 错误消息只含 URL 与状态信息，不含凭据或本地路径。
pub struct TransportResponse {
    /// HTTP 状态码；只有 2xx 才会作为响应返回，其余状态走
    /// [`TransportError::HttpStatus`]。
    pub status: u16,
    /// 响应体长度；服务器未告知时为 `None`。
    pub content_length: Option<u64>,
    /// 是否支持断点续传：对带 Range 的请求，206 为 `true`；
    /// 服务器忽略 Range 返回 200 时为 `false`。
    pub supports_range: bool,
    /// 服务器实际返回的区间（`Content-Range` 头），仅对 206 有意义。
    pub content_range: Option<ContentRange>,
    /// 响应体。
    pub body: Box<dyn Read + Send>,
}

/// 去掉 URL 里的凭据与查询串，只用于错误消息。
fn redact(url: &str) -> String {
    let without_query = url.split_once('?').map_or(url, |(base, _)| base);
    if let Some((scheme, rest)) = without_query.split_once("://") {
        let host_path = rest.rsplit_once('@').map_or(rest, |(_, value)| value);
        format!("{scheme}://{host_path}")
    } else {
        without_query.to_owned()
    }
}

struct Redacted<'a>(&'a str);

impl std::fmt::Display for Redacted<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&redact(self.0))
    }
}

/// 传输层的错误。
///
/// 错误消息只含 URL 与状态信息，不含凭据或本地路径。
#[derive(Debug, Error)]
pub enum TransportError {
    /// 建立连接失败。
    #[error("failed to connect to {url}: {source}", url = Redacted(url))]
    Connect {
        /// 目标 URL。
        url: String,
        /// 底层连接错误。
        #[source]
        source: std::io::Error,
    },
    /// 在超时时间内未完成请求。
    #[error("request to {url} timed out after {timeout_secs}s", url = Redacted(url))]
    Timeout {
        /// 目标 URL。
        url: String,
        /// 配置的超时秒数。
        timeout_secs: u64,
    },
    /// 服务器返回了非 2xx 状态码。
    #[error("server returned HTTP {status} for {url}", url = Redacted(url))]
    HttpStatus {
        /// 目标 URL。
        url: String,
        /// 服务器返回的状态码。
        status: u16,
    },
    /// 读取响应体时连接中断。
    #[error("connection to {url} was interrupted while reading: {source}", url = Redacted(url))]
    ReadInterrupted {
        /// 目标 URL。
        url: String,
        /// 底层读取错误。
        #[source]
        source: std::io::Error,
    },
}

/// HTTP 传输抽象。
///
/// 真实实现尚未提供——需接入实际 HTTP 客户端（连接池、代理、TLS、重定向策略）；
/// 测试全部用内存替身，不触网。
pub trait HttpTransport: Send + Sync {
    /// 按请求参数执行一次 GET，返回 2xx 响应。
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError>;
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
