//! `transport.rs` 的单元测试。

use super::{MetaError, MetaFetcher, MetaSource, StaticFetcher};

#[test]
fn static_fetcher_returns_registered_body() {
    let fetcher = StaticFetcher::new().with("https://a/x.json", r#"{"ok":true}"#);
    let body = fetcher.fetch_text("https://a/x.json").unwrap();
    assert_eq!(body, r#"{"ok":true}"#);
}

#[test]
fn static_fetcher_missing_url_is_transport_error() {
    let fetcher = StaticFetcher::new();
    let err = fetcher.fetch_text("https://a/missing.json").unwrap_err();
    match err {
        MetaError::Transport { url, reason } => {
            assert_eq!(url, "https://a/missing.json");
            assert_eq!(reason, "no fixture registered for this URL");
        }
        other => panic!("expected Transport, got {other:?}"),
    }
}

#[test]
fn static_fetcher_with_is_a_builder() {
    let fetcher = StaticFetcher::new()
        .with("https://a/1", "one")
        .with("https://a/2", "two");
    assert_eq!(fetcher.fetch_text("https://a/1").unwrap(), "one");
    assert_eq!(fetcher.fetch_text("https://a/2").unwrap(), "two");
    // 后注册的覆盖先注册的同 URL fixture。
    let overridden = StaticFetcher::new()
        .with("https://a/1", "first")
        .with("https://a/1", "second");
    assert_eq!(overridden.fetch_text("https://a/1").unwrap(), "second");
}

#[test]
fn meta_source_fetch_joins_base_and_leading_slash_path() {
    let source = MetaSource::new(
        "https://meta.example",
        StaticFetcher::new().with("https://meta.example/v2/versions/game", r#"["1.21.1"]"#),
    );
    let body = source.fetch("/v2/versions/game").unwrap();
    assert_eq!(body, r#"["1.21.1"]"#);
}

#[test]
fn meta_source_fetch_no_double_slash_when_base_ends_with_slash() {
    let source = MetaSource::new(
        "https://meta.example/",
        StaticFetcher::new().with("https://meta.example/v2/game", r#"{"a":1}"#),
    );
    // 即使 path 也带前导斜杠，拼接结果也必须只有一个斜杠。
    let body = source.fetch("//v2/game").unwrap();
    assert_eq!(body, r#"{"a":1}"#);
}

#[test]
fn meta_source_url_joins_base_and_path() {
    let source = MetaSource::new("https://meta.example", StaticFetcher::new());
    assert_eq!(source.url("v2/game"), "https://meta.example/v2/game");
    assert_eq!(source.url("/v2/game"), "https://meta.example/v2/game");
}

#[test]
fn meta_source_url_never_double_slashes() {
    let trailing = MetaSource::new("https://meta.example/", StaticFetcher::new());
    assert_eq!(trailing.url("v2/game"), "https://meta.example/v2/game");
    assert_eq!(trailing.url("/v2/game"), "https://meta.example/v2/game");
    assert_eq!(trailing.url("//v2/game"), "https://meta.example/v2/game");

    let plain = MetaSource::new("https://meta.example", StaticFetcher::new());
    assert_eq!(plain.url("/v2/game"), "https://meta.example/v2/game");
    assert_eq!(plain.url("//v2/game"), "https://meta.example/v2/game");
}

#[test]
fn meta_error_url_returns_some_for_url_carrying_variants() {
    let cases = [
        MetaError::InvalidUrl {
            url: "u".to_owned(),
        },
        MetaError::Transport {
            url: "u".to_owned(),
            reason: "r".to_owned(),
        },
        MetaError::InvalidJson {
            url: "u".to_owned(),
            reason: "r".to_owned(),
        },
        MetaError::InvalidXml {
            url: "u".to_owned(),
            reason: "r".to_owned(),
        },
        MetaError::NotAFile {
            url: "u".to_owned(),
        },
    ];
    for err in cases {
        assert_eq!(err.url(), Some("u"), "expected Some for {err:?}");
    }
}

#[test]
fn meta_error_url_returns_none_for_other_variants() {
    let cases = [
        MetaError::MissingField { what: "x" },
        MetaError::InvalidVersion {
            what: "x",
            value: "v".to_owned(),
        },
        MetaError::UnsupportedGameVersion {
            value: "v".to_owned(),
        },
        MetaError::UnknownGameVersion {
            game: "g".to_owned(),
        },
        MetaError::UnsafeVersionId {
            id: "i".to_owned(),
            reason: "r",
        },
        MetaError::VersionAlreadyInstalled { id: "i".to_owned() },
        MetaError::EmptyPlan,
    ];
    for err in cases {
        assert_eq!(err.url(), None, "expected None for {err:?}");
    }
}

#[test]
fn display_is_non_empty_and_contains_url_for_url_carrying_variants() {
    let with_url = [
        MetaError::Transport {
            url: "https://a/b.json".to_owned(),
            reason: "boom".to_owned(),
        },
        MetaError::InvalidJson {
            url: "https://a/b.json".to_owned(),
            reason: "syntax".to_owned(),
        },
        MetaError::NotAFile {
            url: "https://a/dir/".to_owned(),
        },
    ];
    for err in with_url {
        let text = err.to_string();
        assert!(!text.is_empty());
        assert!(
            text.contains("https://a/b.json") || text.contains("https://a/dir/"),
            "display {text:?} should contain the URL"
        );
    }

    let without_url = [
        MetaError::EmptyPlan,
        MetaError::UnsupportedGameVersion {
            value: "1.2".to_owned(),
        },
    ];
    for err in without_url {
        assert!(!err.to_string().is_empty());
    }
}
