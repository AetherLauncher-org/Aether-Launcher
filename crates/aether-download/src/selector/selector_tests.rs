use super::*;

const THRESHOLD: u32 = 2;

fn source(provider: &str) -> ArtifactSource {
    ArtifactSource {
        provider: provider.to_owned(),
        url: format!("https://{provider}.example.invalid/file"),
    }
}

fn sources(providers: &[&str]) -> Vec<ArtifactSource> {
    providers.iter().copied().map(source).collect()
}

fn stats(entries: &[(&str, u32, u32)]) -> HashMap<String, ProviderStat> {
    entries
        .iter()
        .map(|(provider, weight, failures)| {
            let stat = ProviderStat {
                weight: *weight,
                consecutive_failures: *failures,
            };
            ((*provider).to_owned(), stat)
        })
        .collect()
}

fn order(providers: &[&str], table: &[(&str, u32, u32)], threshold: u32) -> Vec<String> {
    let list = sources(providers);
    let table = stats(table);
    order_sources(&list, &table, threshold)
        .iter()
        .map(|source| source.provider.clone())
        .collect()
}

#[test]
fn empty_stats_keep_original_order() {
    assert_eq!(
        order(&["a", "b", "c"], &[], THRESHOLD),
        ["a", "b", "c"].map(String::from)
    );
    assert_eq!(
        order(&["c", "b", "a"], &[], THRESHOLD),
        ["c", "b", "a"].map(String::from)
    );
}

#[test]
fn higher_weight_comes_first() {
    let ordered = order(
        &["slow", "fast", "medium"],
        &[("slow", 1, 0), ("fast", 10, 0), ("medium", 5, 0)],
        THRESHOLD,
    );
    assert_eq!(ordered, ["fast", "medium", "slow"].map(String::from));
}

#[test]
fn fewer_failures_come_first_among_equal_weights() {
    let ordered = order(
        &["stale", "broken", "fresh"],
        &[("stale", 5, 1), ("broken", 5, 1), ("fresh", 5, 0)],
        THRESHOLD,
    );
    assert_eq!(ordered, ["fresh", "stale", "broken"].map(String::from));
}

#[test]
fn tripped_provider_sinks_below_healthy_sources() {
    // `dead` 的权重远高于其它源，但连续失败已达阈值，必须排在最后。
    let ordered = order(
        &["dead", "unweighted", "tiny"],
        &[("dead", 1_000, THRESHOLD), ("tiny", 1, 0)],
        THRESHOLD,
    );
    assert_eq!(ordered, ["tiny", "unweighted", "dead"].map(String::from));
}

#[test]
fn tripped_provider_sinks_even_when_everything_is_tripped() {
    // 全部熔断时退化为按有效权重（这里全为 0）、再按失败次数与下标排序。
    let ordered = order(
        &["a", "b", "c"],
        &[("a", 10, 9), ("b", 20, 4), ("c", 30, 7)],
        THRESHOLD,
    );
    assert_eq!(ordered, ["b", "c", "a"].map(String::from));
}

#[test]
fn unknown_provider_counts_as_zero_weight_and_zero_failures() {
    let ordered = order(
        &["known", "mystery", "known2"],
        &[("known", 3, 0), ("known2", 0, 0)],
        THRESHOLD,
    );
    // 有效权重 3/0/0；后两者同权重、同失败次数，按原始下标稳定排列。
    assert_eq!(ordered, ["known", "mystery", "known2"].map(String::from));
}

#[test]
fn unknown_provider_outranks_tripped_provider() {
    let ordered = order(&["dead", "mystery"], &[("dead", 50, THRESHOLD)], THRESHOLD);
    assert_eq!(ordered, ["mystery", "dead"].map(String::from));
}

#[test]
fn ties_are_stable_in_original_order() {
    let ordered = order(
        &["p", "q", "r", "s"],
        &[("p", 2, 0), ("q", 2, 0), ("r", 2, 0), ("s", 2, 0)],
        0,
    );
    assert_eq!(ordered, ["p", "q", "r", "s"].map(String::from));
}

#[test]
fn duplicate_providers_keep_relative_order() {
    let ordered = order(
        &["mirror", "mirror", "primary"],
        &[("mirror", 5, 0), ("primary", 5, 0)],
        THRESHOLD,
    );
    assert_eq!(ordered, ["mirror", "mirror", "primary"].map(String::from));
}

#[test]
fn record_success_resets_consecutive_failures() {
    let mut stat = ProviderStat {
        weight: 7,
        consecutive_failures: 4,
    };
    stat.record_failure();
    assert_eq!(stat.consecutive_failures, 5);
    stat.record_success();
    assert_eq!(stat.consecutive_failures, 0);
    assert_eq!(stat.weight, 7);
}

#[test]
fn record_success_and_failure_are_chainable() {
    let mut stat = ProviderStat::default();
    stat.record_failure().record_failure().record_success();
    assert_eq!(
        stat,
        ProviderStat {
            weight: 0,
            consecutive_failures: 0,
        }
    );
}

#[test]
fn record_failure_increments_and_saturates() {
    let mut stat = ProviderStat::default();
    for expected in 1..=3 {
        stat.record_failure();
        assert_eq!(stat.consecutive_failures, expected);
    }

    let mut saturated = ProviderStat {
        weight: 0,
        consecutive_failures: u32::MAX,
    };
    saturated.record_failure();
    assert_eq!(saturated.consecutive_failures, u32::MAX);
}

#[test]
fn is_tripped_boundary() {
    let at = |failures: u32| ProviderStat {
        weight: 1,
        consecutive_failures: failures,
    };

    assert!(!at(0).is_tripped(2));
    assert!(!at(1).is_tripped(2));
    assert!(at(2).is_tripped(2));
    assert!(at(3).is_tripped(2));

    // 阈值为 0 表示关闭熔断，任何失败次数都不算熔断。
    assert!(!at(0).is_tripped(0));
    assert!(!at(1).is_tripped(0));
    assert!(!at(u32::MAX).is_tripped(0));
}

#[test]
fn degraded_weight_is_zero() {
    assert_eq!(degraded_weight(0), 0);
    assert_eq!(degraded_weight(1), 0);
    assert_eq!(degraded_weight(u32::MAX), 0);
}

#[test]
fn effective_weight_respects_trip() {
    let healthy = ProviderStat {
        weight: 42,
        consecutive_failures: 1,
    };
    let tripped = ProviderStat {
        weight: 42,
        consecutive_failures: 2,
    };
    let tripped_zero = ProviderStat {
        weight: 0,
        consecutive_failures: 2,
    };

    assert_eq!(effective_weight(&healthy, THRESHOLD), 42);
    assert_eq!(effective_weight(&tripped, THRESHOLD), degraded_weight(42));
    assert_eq!(effective_weight(&tripped, THRESHOLD), 0);
    // 关闭熔断时即使是「失败很多」的源也保留原权重。
    assert_eq!(effective_weight(&tripped, 0), 42);
    assert_eq!(effective_weight(&tripped_zero, 0), 0);
}

#[test]
fn order_sources_returns_borrowed_sources() {
    let list = sources(&["b", "a"]);
    let table = stats(&[("a", 2, 0), ("b", 1, 0)]);
    let ordered = order_sources(&list, &table, THRESHOLD);
    assert_eq!(ordered.len(), 2);
    assert_eq!(ordered[0].provider, "a");
    assert_eq!(ordered[1].provider, "b");
    // 返回的引用必须指向入参切片里的元素。
    assert!(std::ptr::eq(ordered[0], &list[1]));
}

#[test]
fn order_sources_of_empty_slice_is_empty() {
    let empty: Vec<ArtifactSource> = Vec::new();
    assert!(order_sources(&empty, &HashMap::new(), THRESHOLD).is_empty());
}
