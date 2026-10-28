//! 官方源与镜像源。
//!
//! Mojang 官方地址是内容的权威出处，但不是唯一出处。本模块把官方 URL 展开为
//! 「官方在前、镜像在后」的多源列表；引擎既有的回退与熔断机制（见 `engine`）
//! 负责在官方源失败后改用镜像，本模块不引入任何新的下载状态。
//!
//! 镜像只改地址，不改内容身份：期望的大小与 SHA-1 始终来自官方元数据，
//! 与 `DownloadPlan::validate` 和引擎校验的契约一致。

use crate::transport::{HttpTransport, TransportError, TransportRequest, TransportResponse};
use crate::{ArtifactSource, DownloadPlan};

/// BMCLAPI 的服务基址（社区维护的 Minecraft 资源镜像）。
pub const BMCLAPI_BASE: &str = "https://bmclapi2.bangbang93.com";

/// 镜像源在 `ArtifactSource.provider` 里的名字；熔断统计按 provider 记账，
/// 镜像与官方源各自独立计数。
pub const BMCLAPI_PROVIDER: &str = "bmclapi";

/// 把一个 Mojang 官方 URL 推导出对应的 BMCLAPI 地址。
///
/// 路径规则与 BMCLAPI 的公开端点一致：
/// - 元数据与数据主机（`piston-meta` / `piston-data` / `launchermeta` /
///   `launcher.mojang.com`）：路径原样拼接；
/// - 库主机 `libraries.minecraft.net`：拼到 `/maven/` 之后；
/// - 资源主机 `resources.download.minecraft.net`：拼到 `/assets/` 之后。
///
/// 返回 `None` 表示该主机没有已配置的镜像（调用方保持单源，不猜测地址）。
#[must_use]
pub fn bmclapi_mirror(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    // 镜像固定走 https；非 http(s) 的官方地址不推导镜像。
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let path_start = rest.find('/').unwrap_or(rest.len());
    let host = rest[..path_start].to_ascii_lowercase();
    let path = &rest[path_start..];
    let mirrored = match host.as_str() {
        "piston-meta.mojang.com"
        | "piston-data.mojang.com"
        | "launchermeta.mojang.com"
        | "launcher.mojang.com" => path.to_owned(),
        "libraries.minecraft.net" => format!("/maven{path}"),
        "resources.download.minecraft.net" => format!("/assets{path}"),
        _ => return None,
    };
    Some(format!("{BMCLAPI_BASE}{mirrored}"))
}

/// 把单个官方源展开为「官方在前、镜像在后」的多源列表。
///
/// 没有镜像规则的地址保持单源；镜像地址与官方地址相同时不追加。
#[must_use]
pub fn with_mirrors(provider: &str, url: &str) -> Vec<ArtifactSource> {
    let mut sources = vec![ArtifactSource {
        provider: provider.to_owned(),
        url: url.to_owned(),
    }];
    if let Some(mirror) = bmclapi_mirror(url)
        && mirror != url
    {
        sources.push(ArtifactSource {
            provider: BMCLAPI_PROVIDER.to_owned(),
            url: mirror,
        });
    }
    sources
}

/// 给计划里的每个 artifact 追加镜像源。
///
/// 官方源的相对顺序保持不变，镜像按官方地址逐条推导、按 URL 去重后追加；
/// 目标路径、大小与 SHA-1 一律不动。
#[must_use]
pub fn expand_plan_with_mirrors(mut plan: DownloadPlan) -> DownloadPlan {
    for artifact in &mut plan.artifacts {
        let mut additions = Vec::new();
        for source in &artifact.sources {
            let Some(mirror) = bmclapi_mirror(&source.url) else {
                continue;
            };
            let known = artifact
                .sources
                .iter()
                .chain(additions.iter())
                .any(|existing| existing.url == mirror);
            if !known {
                additions.push(ArtifactSource {
                    provider: BMCLAPI_PROVIDER.to_owned(),
                    url: mirror,
                });
            }
        }
        artifact.sources.extend(additions);
    }
    plan
}

/// 依次尝试每个地址，返回第一个传输层成功的响应。
///
/// 只在传输层失败（连接、超时、非 2xx 状态）时换下一个地址；内容校验由
/// 调用方在拿到响应后完成——校验失败是内容身份问题，不是可达性问题，
/// 不在本函数的回退范围内。`urls` 为空时返回连接错误。
pub fn fetch_first_available(
    transport: &dyn HttpTransport,
    urls: &[String],
    timeout_secs: u64,
) -> Result<TransportResponse, TransportError> {
    let mut last = None;
    for url in urls {
        match transport.get(&TransportRequest {
            url: url.clone(),
            range: None,
            timeout_secs,
        }) {
            Ok(response) => return Ok(response),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| TransportError::Connect {
        url: String::new(),
        source: std::io::Error::other("no download source URL was provided"),
    }))
}

#[cfg(test)]
#[path = "mirrors_tests.rs"]
mod tests;
