//! 下载源的选择与排序。
//!
//! 一个文件通常有多个下载源。挑选顺序决定了两件互相拉扯的事：
//!
//! * 快的源应该先被尝试；
//! * 坏掉的源不该在每次下载时都先浪费一轮超时。
//!
//! 这里的做法是给每个 provider 记一份统计（[`ProviderStat`]），
//! 排序键依次为「有效权重降序 → 连续失败次数升序 → 原始下标升序」。
//! 连续失败达到阈值（熔断）的源权重被压成 [`degraded_weight`]，
//! 于是它会排在所有未熔断的源之后，即使它原来的权重高得多。
//!
//! 模块内只有纯函数：不读写文件、不发请求、不碰时间，
//! 输入相同则输出必然相同，可以直接在测试里断言顺序。

use std::collections::HashMap;

use crate::ArtifactSource;

/// 熔断状态下使用的权重。取值 `0`，保证它不超过任何真实权重。
const DEGRADED_WEIGHT: u32 = 0;

/// 单个 provider 的会话内统计。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderStat {
    /// 该 provider 的权重，越大越优先。未知 provider 视为 `0`。
    pub weight: u32,
    /// 自最近一次成功以来连续失败的次数。
    pub consecutive_failures: u32,
}

impl ProviderStat {
    /// 记录一次成功：连续失败计数清零。
    pub fn record_success(&mut self) -> &mut Self {
        self.consecutive_failures = 0;
        self
    }

    /// 记录一次失败：连续失败计数加一，饱和以免溢出回绕。
    pub fn record_failure(&mut self) -> &mut Self {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self
    }

    /// 是否已经熔断（连续失败达到阈值）。
    ///
    /// 阈值为 `0` 表示不启用熔断，永远返回 `false`。
    #[must_use]
    pub fn is_tripped(&self, failure_threshold: u32) -> bool {
        failure_threshold > 0 && self.consecutive_failures >= failure_threshold
    }
}

/// 熔断后的权重。目前就是 `0`，单独成函数是为了让调用点自解释。
#[must_use]
pub const fn degraded_weight(_weight: u32) -> u32 {
    DEGRADED_WEIGHT
}

/// 排序时真正使用的权重：熔断的源降权，其余原样返回。
#[must_use]
pub fn effective_weight(stat: &ProviderStat, failure_threshold: u32) -> u32 {
    if stat.is_tripped(failure_threshold) {
        degraded_weight(stat.weight)
    } else {
        stat.weight
    }
}

/// 按优先级排列下载源。
///
/// * 有效权重高的在前；
/// * 权重相同时连续失败少的在前；
/// * 仍然相同则保持原始顺序（稳定、可预测）。
///
/// 不在 `stats` 里的 provider 视为「权重 0、零次失败」。
#[must_use]
pub fn order_sources<'a>(
    sources: &'a [ArtifactSource],
    stats: &HashMap<String, ProviderStat>,
    failure_threshold: u32,
) -> Vec<&'a ArtifactSource> {
    let fallback = ProviderStat::default();
    let mut ordered: Vec<(u32, u32, usize, &'a ArtifactSource)> = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let stat = stats.get(&source.provider).unwrap_or(&fallback);
            (
                effective_weight(stat, failure_threshold),
                stat.consecutive_failures,
                index,
                source,
            )
        })
        .collect();
    ordered.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    ordered
        .into_iter()
        .map(|(_, _, _, source)| source)
        .collect()
}

#[cfg(test)]
#[path = "selector_tests.rs"]
mod tests;
