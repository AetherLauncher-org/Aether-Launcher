use std::io::Cursor;

use super::{
    BMCLAPI_BASE, bmclapi_mirror, expand_plan_with_mirrors, fetch_first_available, with_mirrors,
};
use crate::transport::{HttpTransport, TransportError, TransportRequest, TransportResponse};
use crate::{ArtifactSource, DownloadArtifact, DownloadPlan};

const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";

fn artifact(destination: &str, url: &str) -> DownloadArtifact {
    DownloadArtifact {
        destination: destination.into(),
        size: 1,
        sha1: SHA1.to_owned(),
        sources: vec![ArtifactSource {
            provider: "mojang".to_owned(),
            url: url.to_owned(),
        }],
    }
}

#[test]
fn mirrors_piston_hosts_verbatim() {
    assert_eq!(
        bmclapi_mirror("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"),
        Some(format!("{BMCLAPI_BASE}/mc/game/version_manifest_v2.json"))
    );
    assert_eq!(
        bmclapi_mirror("https://piston-data.mojang.com/v1/objects/abc/client.jar"),
        Some(format!("{BMCLAPI_BASE}/v1/objects/abc/client.jar"))
    );
    assert_eq!(
        bmclapi_mirror("https://launchermeta.mojang.com/v1/packages/x.json"),
        Some(format!("{BMCLAPI_BASE}/v1/packages/x.json"))
    );
}

#[test]
fn mirrors_libraries_under_maven_and_resources_under_assets() {
    assert_eq!(
        bmclapi_mirror("https://libraries.minecraft.net/com/google/guava/guava/guava.jar"),
        Some(format!(
            "{BMCLAPI_BASE}/maven/com/google/guava/guava/guava.jar"
        ))
    );
    assert_eq!(
        bmclapi_mirror("https://resources.download.minecraft.net/ab/abcdef"),
        Some(format!("{BMCLAPI_BASE}/assets/ab/abcdef"))
    );
}

#[test]
fn host_case_is_ignored_but_unknown_hosts_have_no_mirror() {
    assert_eq!(
        bmclapi_mirror("https://PISTON-META.mojang.com/v1/packages/x"),
        Some(format!("{BMCLAPI_BASE}/v1/packages/x"))
    );
    // 没有镜像规则的主机不猜测地址。
    assert_eq!(
        bmclapi_mirror("https://meta.fabricmc.net/v2/versions/loader"),
        None
    );
    assert_eq!(bmclapi_mirror("https://example.invalid/file"), None);
    // 非 http(s) 的地址不推导镜像。
    assert_eq!(bmclapi_mirror("ftp://piston-meta.mojang.com/a"), None);
    assert_eq!(bmclapi_mirror("not-a-url"), None);
}

#[test]
fn with_mirrors_puts_official_first() {
    let sources = with_mirrors("mojang", "https://libraries.minecraft.net/a/b.jar");
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].provider, "mojang");
    assert_eq!(sources[0].url, "https://libraries.minecraft.net/a/b.jar");
    assert_eq!(sources[1].provider, "bmclapi");
    assert_eq!(sources[1].url, format!("{BMCLAPI_BASE}/maven/a/b.jar"));
}

#[test]
fn with_mirrors_keeps_single_source_without_mirror() {
    let sources = with_mirrors("mojang", "https://example.invalid/file");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].provider, "mojang");
}

#[test]
fn expand_plan_adds_mirror_once_and_is_idempotent() {
    let plan = DownloadPlan {
        artifacts: vec![artifact(
            "libraries/a.jar",
            "https://libraries.minecraft.net/a.jar",
        )],
    };
    let expanded = expand_plan_with_mirrors(plan);
    let sources = &expanded.artifacts[0].sources;
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].provider, "mojang");
    assert_eq!(sources[1].provider, "bmclapi");

    let expanded_again = expand_plan_with_mirrors(expanded);
    assert_eq!(expanded_again.artifacts[0].sources.len(), 2);
}

#[test]
fn expand_plan_preserves_content_identity_and_order() {
    let plan = DownloadPlan {
        artifacts: vec![
            artifact("libraries/a.jar", "https://piston-data.mojang.com/a.jar"),
            artifact("assets/objects/x/y", "https://example.invalid/no-mirror"),
        ],
    };
    let expected_destinations = ["libraries/a.jar", "assets/objects/x/y"];
    let expanded = expand_plan_with_mirrors(plan);
    for (index, artifact) in expanded.artifacts.iter().enumerate() {
        assert_eq!(
            artifact.destination.to_str(),
            Some(expected_destinations[index])
        );
        assert_eq!(artifact.size, 1);
        assert_eq!(artifact.sha1, SHA1);
        assert_eq!(artifact.sources[0].provider, "mojang");
    }
    // 没有镜像规则的地址保持单源。
    assert_eq!(expanded.artifacts[1].sources.len(), 1);
    // 有镜像规则的地址官方在前。
    assert_eq!(expanded.artifacts[0].sources.len(), 2);
}

/// 官方地址一律 503，只有镜像地址可用。
struct MirrorOnlyTransport;

impl HttpTransport for MirrorOnlyTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        if request.url.contains("bmclapi") {
            Ok(TransportResponse {
                status: 200,
                content_length: Some(2),
                supports_range: false,
                content_range: None,
                body: Box::new(Cursor::new(b"ok".to_vec())),
            })
        } else {
            Err(TransportError::HttpStatus {
                url: request.url.clone(),
                status: 503,
            })
        }
    }
}

struct AlwaysFailingTransport;

impl HttpTransport for AlwaysFailingTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        Err(TransportError::HttpStatus {
            url: request.url.clone(),
            status: 503,
        })
    }
}

#[test]
fn fetch_first_available_falls_back_to_mirror() {
    let transport = MirrorOnlyTransport;
    let urls: Vec<String> = with_mirrors(
        "mojang",
        "https://piston-data.mojang.com/v1/objects/x/client.jar",
    )
    .into_iter()
    .map(|source| source.url)
    .collect();
    assert_eq!(urls.len(), 2);

    let response = fetch_first_available(&transport, &urls, 5).expect("mirror fallback");
    assert_eq!(response.status, 200);
}

#[test]
fn fetch_first_available_reports_last_error_when_all_fail() {
    let transport = AlwaysFailingTransport;
    let urls = vec![
        "https://a.invalid/x".to_owned(),
        "https://b.invalid/x".to_owned(),
    ];
    match fetch_first_available(&transport, &urls, 5) {
        Err(TransportError::HttpStatus { url, status }) => {
            // 最后一个地址的错误原样上抛。
            assert_eq!(url, "https://b.invalid/x");
            assert_eq!(status, 503);
        }
        Ok(_) => panic!("expected an error, got a response"),
        Err(other) => panic!("expected HttpStatus error, got {other:?}"),
    }
}

#[test]
fn fetch_first_available_rejects_empty_url_list() {
    let transport = AlwaysFailingTransport;
    let result = fetch_first_available(&transport, &[], 5);
    assert!(matches!(result, Err(TransportError::Connect { .. })));
}
