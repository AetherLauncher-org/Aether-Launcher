//! mod loader 元数据的传输抽象与共享错误类型。
//!
//! 网络访问**只通过 [`MetaFetcher`] trait 进行**：本模块不实现真实 HTTP 请求，
//! 真实实现由调用方提供（例如 `aether-download` 的传输引擎），在构造
//! [`MetaSource`] 时注入。
//!
//! [`StaticFetcher`] 是内存里的「URL → 响应体」固定映射，供测试注入固定 JSON；
//! 未注册的 URL 一律报错，所以测试绝不会意外触网。
//!
//! [`MetaError`] 是本 crate 所有获取 / 解析 / 校验错误的统一出口：错误信息可以
//! 包含 URL（URL 里没有凭据），但**永远不包含完整响应体**。

use std::collections::BTreeMap;

use thiserror::Error;

/// 文本获取器：给定一个绝对 URL，返回响应体文本。
///
/// 本 crate 只依赖这个 trait，不关心底层是 HTTP、本地缓存还是内存 fixture；
/// 真实实现由调用方提供（例如 `aether-download` 的引擎）。
pub trait MetaFetcher {
    /// 拉取 `url` 的响应体；失败时返回 [`MetaError::Transport`]。
    fn fetch_text(&self, url: &str) -> Result<String, MetaError>;
}

/// 内存中的固定 fixture 获取器，供测试注入固定响应。
///
/// 只认识通过 [`StaticFetcher::with`] 注册过的 URL，其它 URL 一律返回
/// [`MetaError::Transport`]，让「测试里意外触网」变成可见的失败。
#[derive(Debug, Clone, Default)]
pub struct StaticFetcher {
    /// URL → 响应体。
    fixtures: BTreeMap<String, String>,
}

impl StaticFetcher {
    /// 创建一个没有任何 fixture 的获取器。
    pub fn new() -> Self {
        Self {
            fixtures: BTreeMap::new(),
        }
    }

    /// 注册一条 fixture（builder 风格），返回自身。
    pub fn with(mut self, url: impl Into<String>, body: impl Into<String>) -> Self {
        self.fixtures.insert(url.into(), body.into());
        self
    }
}

impl MetaFetcher for StaticFetcher {
    fn fetch_text(&self, url: &str) -> Result<String, MetaError> {
        self.fixtures
            .get(url)
            .cloned()
            .ok_or_else(|| MetaError::Transport {
                url: url.to_owned(),
                reason: "no fixture registered for this URL".to_owned(),
            })
    }
}

/// 元数据来源：一个根地址 + 一个文本获取器。
///
/// [`MetaSource::fetch`] 会把 `base_url` 与相对路径拼成绝对 URL 再交给
/// [`MetaFetcher`]；拼接规则保证结果里不会出现双斜杠。
pub struct MetaSource {
    /// 元数据服务的根地址，例如 `https://meta.fabricmc.net`。
    pub base_url: String,
    /// 实际执行拉取的获取器。
    pub fetcher: Box<dyn MetaFetcher>,
}

impl std::fmt::Debug for MetaSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MetaSource")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl MetaSource {
    /// 用根地址和获取器构造来源。
    pub fn new(base_url: impl Into<String>, fetcher: impl MetaFetcher + 'static) -> Self {
        Self {
            base_url: base_url.into(),
            fetcher: Box::new(fetcher),
        }
    }

    /// 拉取相对于 `base_url` 的 `path`，返回响应体文本。
    pub fn fetch(&self, path: &str) -> Result<String, MetaError> {
        let url = self.url(path);
        self.fetcher.fetch_text(&url)
    }

    /// 把相对路径拼成绝对 URL：剥掉 `path` 开头的 `/`，并避免出现 `//`。
    ///
    /// `base_url` 以 `/` 结尾时不再补分隔符，因此
    /// `MetaSource::new("https://a/", ..).url("/x")` 与
    /// `MetaSource::new("https://a", ..).url("x")` 都得到 `https://a/x`。
    pub fn url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        if self.base_url.ends_with('/') {
            format!("{}{path}", self.base_url)
        } else {
            format!("{}/{path}", self.base_url)
        }
    }
}

/// 元数据获取、解析与安装计划校验的统一错误。
///
/// 变体集合是 crate 内多个模块共享的契约，**不要新增或改名**。错误信息可以带
/// URL，但不含凭据，也不含完整响应体。
///
/// 这里的 `clippy::enum_variant_names` 是误报：`Invalid*` 前缀是跨模块共享的
/// API 契约（其它模块已经在构造这些变体），改名会破坏所有调用方。
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MetaError {
    /// URL 不合法（无法解析，或不符合预期形状）。
    #[error("invalid URL: {url}")]
    InvalidUrl {
        /// 出问题的 URL。
        url: String,
    },
    /// 网络 / 传输层失败。
    #[error("failed to fetch {url}: {reason}")]
    Transport {
        /// 目标 URL。
        url: String,
        /// 失败原因（不含响应体全文）。
        reason: String,
    },
    /// 响应体不是合法 JSON。
    #[error("invalid JSON response from {url}: {reason}")]
    InvalidJson {
        /// 目标 URL。
        url: String,
        /// 解析器给出的简短原因。
        reason: String,
    },
    /// 响应体不是合法（或不符合预期的）XML。
    #[error("invalid XML response from {url}: {reason}")]
    InvalidXml {
        /// 目标 URL。
        url: String,
        /// 解析器给出的简短原因。
        reason: String,
    },
    /// 响应里缺少必需字段。
    #[error("missing required field: {what}")]
    MissingField {
        /// 缺失字段的人类可读名称。
        what: &'static str,
    },
    /// 字段存在，但值不合法。
    #[error("invalid {what}: {value}")]
    InvalidVersion {
        /// 字段名。
        what: &'static str,
        /// 不合法的值。
        value: String,
    },
    /// 该游戏版本不被这个加载器版本支持。
    #[error("unsupported game version: {value}")]
    UnsupportedGameVersion {
        /// 游戏版本号。
        value: String,
    },
    /// 未知的游戏版本。
    #[error("unknown game version: {game}")]
    UnknownGameVersion {
        /// 游戏版本号。
        game: String,
    },
    /// 版本 id 不安全：用作文件 / 目录名时会越界，或撞上 Windows 保留名。
    #[error("unsafe version id {id}: {reason}")]
    UnsafeVersionId {
        /// 被拒绝的版本 id。
        id: String,
        /// 拒绝原因。
        reason: &'static str,
    },
    /// 目标版本已经安装。
    #[error("version {id} is already installed")]
    VersionAlreadyInstalled {
        /// 已安装的版本 id。
        id: String,
    },
    /// 安装计划里没有任何步骤。
    #[error("install plan contains no steps")]
    EmptyPlan,
    /// URL 指向的不是普通文件（例如目录列表或重定向页面）。
    #[error("not a file: {url}")]
    NotAFile {
        /// 目标 URL。
        url: String,
    },
}

impl MetaError {
    /// 返回该错误关联的 URL；不携带 URL 的变体返回 `None`。
    pub fn url(&self) -> Option<&str> {
        match self {
            Self::InvalidUrl { url }
            | Self::Transport { url, .. }
            | Self::InvalidJson { url, .. }
            | Self::InvalidXml { url, .. }
            | Self::NotAFile { url } => Some(url),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "transport/transport_tests.rs"]
mod tests;
