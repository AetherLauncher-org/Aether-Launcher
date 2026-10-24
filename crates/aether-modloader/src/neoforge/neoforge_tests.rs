//! `neoforge.rs` 的单元测试。所有 fixture 都内联，绝不访问网络。

use super::*;
use crate::ModLoaderKind;
use crate::installer::InstallStep;
use crate::transport::{MetaSource, StaticFetcher};

/// 真实的现代坐标 maven-metadata（从 `maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml` 抓取后裁剪）。
const REAL_MODERN_METADATA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>net.neoforged</groupId>
  <artifactId>neoforge</artifactId>
  <versioning>
    <latest>26.3.0.97</latest>
    <release>26.3.0.97</release>
    <versions>
      <version>26.1.0.0-alpha.1+snapshot-1</version>
      <version>0.25w14craftmine.3-beta</version>
      <version>20.2.86</version>
      <version>20.4.237</version>
      <version>20.6.119</version>
      <version>21.1.80</version>
      <version>26.3.0.97</version>
    </versions>
    <lastUpdated>20261024024648</lastUpdated>
  </versioning>
</metadata>
"#;

/// 真实的 legacy 坐标 maven-metadata（`net/neoforged/forge`，含一条没有游戏版本前缀的裸版本）。
const REAL_LEGACY_METADATA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>net.neoforged</groupId>
  <artifactId>forge</artifactId>
  <versioning>
    <latest>1.20.1-47.1.106</latest>
    <release>1.20.1-47.1.106</release>
    <versions>
      <version>1.20.1-47.1.7</version>
      <version>1.20.1-47.1.5</version>
      <version>47.1.82</version>
      <version>1.20.1-47.1.106</version>
    </versions>
    <lastUpdated>20240528174429</lastUpdated>
  </versioning>
</metadata>
"#;

fn modern(version: &str, game_version: &str) -> NeoForgeVersion {
    NeoForgeVersion {
        version: version.to_string(),
        game_version: game_version.to_string(),
        artifact: NeoForgeArtifact::Modern,
        pre_release: is_pre_release(version),
    }
}

fn legacy(version: &str, game_version: &str) -> NeoForgeVersion {
    NeoForgeVersion {
        version: version.to_string(),
        game_version: game_version.to_string(),
        artifact: NeoForgeArtifact::Legacy,
        pre_release: is_pre_release(version),
    }
}

#[test]
fn parses_real_modern_metadata() {
    let metadata = parse_maven_metadata(REAL_MODERN_METADATA).expect("real metadata must parse");
    assert_eq!(metadata.group_id.as_deref(), Some("net.neoforged"));
    assert_eq!(metadata.artifact_id.as_deref(), Some("neoforge"));
    assert_eq!(metadata.latest.as_deref(), Some("26.3.0.97"));
    assert_eq!(metadata.release.as_deref(), Some("26.3.0.97"));
    assert_eq!(metadata.versions.len(), 7);
    assert_eq!(metadata.versions[0], "26.1.0.0-alpha.1+snapshot-1");
    assert_eq!(metadata.versions[6], "26.3.0.97");
    assert!(
        !metadata
            .versions
            .iter()
            .any(|version| version == "versions")
    );
}

#[test]
fn parses_real_legacy_metadata() {
    let metadata =
        parse_maven_metadata(REAL_LEGACY_METADATA).expect("real legacy metadata must parse");
    assert_eq!(metadata.group_id.as_deref(), Some("net.neoforged"));
    assert_eq!(metadata.artifact_id.as_deref(), Some("forge"));
    assert_eq!(metadata.latest.as_deref(), Some("1.20.1-47.1.106"));
    assert_eq!(
        metadata.versions,
        vec![
            "1.20.1-47.1.7",
            "1.20.1-47.1.5",
            "47.1.82",
            "1.20.1-47.1.106"
        ]
    );
}

#[test]
fn versions_container_is_not_captured() {
    let input = r#"<metadata><versioning><versions>
        <version>1.0</version>
        <version>2.0</version>
        <version>3.0</version>
    </versions></versioning></metadata>"#;
    let metadata = parse_maven_metadata(input).expect("must parse");
    assert_eq!(
        metadata.versions,
        vec!["1.0", "2.0", "3.0"],
        "`<versions>` container must not be a version"
    );
}

#[test]
fn version_tag_tolerates_attributes_and_whitespace() {
    let input = "<metadata><versioning><versions>\n\t<version>20.4.237</version>\n\t<version >21.1.80</version>\n\t<version foo=\"bar\"> 26.3.0.97 </version>\n\t<version\n  baz=\"qux\"\n  >\n  1.20.1-47.1.106\n  </version>\n</versions></versioning></metadata>";
    let metadata = parse_maven_metadata(input).expect("must parse");
    assert_eq!(
        metadata.versions,
        vec!["20.4.237", "21.1.80", "26.3.0.97", "1.20.1-47.1.106"]
    );
}

#[test]
fn missing_latest_and_release_are_none() {
    let input = r#"<metadata><groupId>net.neoforged</groupId><versioning><versions><version>20.4.237</version></versions></versioning></metadata>"#;
    let metadata = parse_maven_metadata(input).expect("must parse");
    assert_eq!(metadata.group_id.as_deref(), Some("net.neoforged"));
    assert_eq!(metadata.artifact_id, None);
    assert_eq!(metadata.latest, None);
    assert_eq!(metadata.release, None);
    assert_eq!(metadata.versions, vec!["20.4.237"]);
}

#[test]
fn non_metadata_input_is_invalid_xml() {
    for input in ["<html><body>Not Found</body></html>", "", "just some text"] {
        let error = parse_maven_metadata(input).expect_err("must reject non-metadata input");
        match error {
            MetaError::InvalidXml { url, reason } => {
                assert!(url.is_empty(), "parse_maven_metadata has no URL context");
                assert!(
                    reason.contains("<version>"),
                    "reason should point at <version>: {reason}"
                );
            }
            other => panic!("expected InvalidXml, got {other:?}"),
        }
    }
}

#[test]
fn versions_element_without_versions_is_invalid_xml() {
    let input = r#"<metadata><versioning><versions></versions></versioning></metadata>"#;
    assert!(matches!(
        parse_maven_metadata(input),
        Err(MetaError::InvalidXml { .. })
    ));
}

#[test]
fn game_version_mapping_table() {
    let cases: [(&str, Option<&str>); 14] = [
        ("20.4.237", Some("1.20.4")),
        ("20.2.86", Some("1.20.2")),
        ("20.6.119", Some("1.20.6")),
        ("21.1.80", Some("1.21.1")),
        ("21.0.167", Some("1.21.0")),
        ("26.3.0.97", Some("26.3")),
        ("26.1.0.0-alpha.1+snapshot-1", Some("26.1")),
        ("20.4.0-beta", Some("1.20.4")),
        ("0.25w14craftmine.3-beta", None),
        ("20", None),
        ("20.4", None),
        ("abc.1", None),
        ("", None),
        (" 21.1.80 ", Some("1.21.1")),
    ];
    for (input, expected) in cases {
        assert_eq!(
            game_version_for(input).as_deref(),
            expected,
            "game_version_for({input:?})"
        );
    }
}

#[test]
fn pre_release_table() {
    let cases: [(&str, bool); 10] = [
        ("20.4.0-beta", true),
        ("26.1.0.0-alpha.1+snapshot-1", true),
        ("21.0.0-pre.1", true),
        ("20.4.0-rc.2", true),
        ("1.0-snapshot", true),
        ("20.4.0-BETA", true),
        ("20.4.237", false),
        ("21.1.80", false),
        ("1.20.1-47.1.106", false),
        ("", false),
    ];
    for (input, expected) in cases {
        assert_eq!(is_pre_release(input), expected, "is_pre_release({input:?})");
    }
}

#[test]
fn parse_versions_drops_unmappable_entries() {
    let versions = parse_versions(REAL_MODERN_METADATA).expect("must parse");
    let expected = vec![
        modern("26.1.0.0-alpha.1+snapshot-1", "26.1"),
        modern("20.2.86", "1.20.2"),
        modern("20.4.237", "1.20.4"),
        modern("20.6.119", "1.20.6"),
        modern("21.1.80", "1.21.1"),
        modern("26.3.0.97", "26.3"),
    ];
    assert_eq!(versions, expected);
    assert!(
        !versions
            .iter()
            .any(|version| version.version == "0.25w14craftmine.3-beta")
    );
    assert!(
        versions
            .iter()
            .all(|version| version.artifact == NeoForgeArtifact::Modern)
    );
    assert!(versions[0].pre_release);
    assert!(!versions[1].pre_release);
}

#[test]
fn parse_legacy_versions_splits_game_and_loader() {
    let versions = parse_legacy_versions(REAL_LEGACY_METADATA).expect("must parse");
    let expected = vec![
        legacy("1.20.1-47.1.7", "1.20.1"),
        legacy("1.20.1-47.1.5", "1.20.1"),
        legacy("1.20.1-47.1.106", "1.20.1"),
    ];
    assert_eq!(versions, expected, "bare `47.1.82` must be dropped");
    assert!(
        versions
            .iter()
            .all(|version| version.artifact == NeoForgeArtifact::Legacy)
    );
}

#[test]
fn split_legacy_version_rejects_bad_shapes() {
    assert_eq!(
        split_legacy_version("1.20.1-47.1.106"),
        Some(("1.20.1", "47.1.106"))
    );
    assert_eq!(split_legacy_version("47.1.82"), None, "no `-`");
    assert_eq!(
        split_legacy_version("abc-47.1.82"),
        None,
        "game part must start with a digit"
    );
    assert_eq!(
        split_legacy_version("1201-47.1.82"),
        None,
        "game part must contain a dot"
    );
    assert_eq!(split_legacy_version("1.20.1-"), None, "empty loader part");
}

#[test]
fn versions_for_game_filters_modern_only() {
    let versions = versions_for_game(REAL_MODERN_METADATA, "1.20.4").expect("must parse");
    assert_eq!(versions, vec![modern("20.4.237", "1.20.4")]);
    assert_eq!(
        versions_for_game(REAL_MODERN_METADATA, "26.3").expect("must parse"),
        vec![modern("26.3.0.97", "26.3")]
    );
    assert!(
        versions_for_game(REAL_MODERN_METADATA, "1.12.2")
            .expect("must parse")
            .is_empty()
    );
}

#[test]
fn installer_file_name_and_url_for_both_artifacts() {
    let modern_version = modern("20.4.237", "1.20.4");
    assert_eq!(
        modern_version.installer_file_name(),
        "neoforge-20.4.237-installer.jar"
    );
    assert_eq!(
        modern_version.installer_url(DEFAULT_MAVEN_BASE),
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/20.4.237/neoforge-20.4.237-installer.jar"
    );
    assert_eq!(
        modern_version.installer_url("https://maven.neoforged.net/releases/"),
        "https://maven.neoforged.net/releases/net/neoforged/neoforge/20.4.237/neoforge-20.4.237-installer.jar",
        "trailing slash must not produce `//`"
    );
    assert!(
        !modern_version
            .installer_url("https://example.invalid/m//")
            .contains("//net")
    );
    assert_eq!(
        modern_version.installer_url("https://example.invalid/m//"),
        "https://example.invalid/m/net/neoforged/neoforge/20.4.237/neoforge-20.4.237-installer.jar"
    );

    let legacy_version = legacy("1.20.1-47.1.106", "1.20.1");
    assert_eq!(
        legacy_version.installer_file_name(),
        "forge-1.20.1-47.1.106-installer.jar"
    );
    assert_eq!(
        legacy_version.installer_url(DEFAULT_MAVEN_BASE),
        "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar"
    );
}

#[test]
fn target_version_id_is_neoforge_prefixed() {
    assert_eq!(
        modern("21.1.80", "1.21.1").target_version_id(),
        "neoforge-21.1.80"
    );
    assert_eq!(
        legacy("1.20.1-47.1.106", "1.20.1").target_version_id(),
        "neoforge-1.20.1-47.1.106"
    );
}

#[test]
fn install_plan_steps_and_validation() {
    let version = modern("21.1.80", "1.21.1");
    let plan = version.install_plan(DEFAULT_MAVEN_BASE);
    assert_eq!(plan.kind, ModLoaderKind::NeoForge);
    assert_eq!(plan.loader_version, "21.1.80");
    assert_eq!(plan.game_version, "1.21.1");
    assert_eq!(plan.target_version_id, "neoforge-21.1.80");
    assert_eq!(plan.steps.len(), 2);

    match &plan.steps[0] {
        InstallStep::Download {
            url,
            destination,
            sha1,
            size,
            label,
        } => {
            assert_eq!(
                url,
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.80/neoforge-21.1.80-installer.jar"
            );
            assert_eq!(destination, "installers/neoforge-21.1.80-installer.jar");
            assert_eq!(*sha1, None);
            assert_eq!(*size, None);
            assert!(
                label.contains("21.1.80"),
                "label should name the version: {label}"
            );
        }
        other => panic!("first step must be Download, got {other:?}"),
    }

    match &plan.steps[1] {
        InstallStep::RunInstaller {
            jar_path,
            args,
            output_dir,
        } => {
            assert_eq!(jar_path, "installers/neoforge-21.1.80-installer.jar");
            assert_eq!(
                args,
                &vec![
                    "--installClient".to_string(),
                    GAME_DIR_PLACEHOLDER.to_string()
                ]
            );
            assert_eq!(
                *output_dir, None,
                "launcher resolves the real output directory later"
            );
        }
        other => panic!("second step must be RunInstaller, got {other:?}"),
    }

    plan.validate(&[]).expect("plan must be valid");
    assert!(
        plan.validate(&["neoforge-21.1.80"]).is_err(),
        "existing install must be rejected"
    );
}

#[test]
fn fetch_versions_uses_the_modern_metadata_url() {
    let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
    let fetcher = StaticFetcher::new().with(url, REAL_MODERN_METADATA);
    let source = MetaSource::new(DEFAULT_MAVEN_BASE, fetcher);
    let versions = fetch_versions(&source).expect("must fetch");
    assert_eq!(versions.len(), 6);
    assert_eq!(versions[0], modern("26.1.0.0-alpha.1+snapshot-1", "26.1"));
    // 骨架 fetcher 只登记了上面那条 URL；取别的 URL 必然失败，说明请求路径完全一致。
    assert_eq!(source.url(&format!("{GROUP_PATH}/maven-metadata.xml")), url);
}

#[test]
fn fetch_legacy_versions_uses_the_legacy_metadata_url() {
    let url = "https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml";
    let fetcher = StaticFetcher::new().with(url, REAL_LEGACY_METADATA);
    let source = MetaSource::new("https://maven.neoforged.net/releases/", fetcher);
    let versions = fetch_legacy_versions(&source).expect("must fetch");
    assert_eq!(versions.len(), 3);
    assert_eq!(versions[2], legacy("1.20.1-47.1.106", "1.20.1"));
}

#[test]
fn fetch_versions_rewrites_error_url() {
    let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
    let fetcher = StaticFetcher::new().with(url, "<html>oops</html>");
    let source = MetaSource::new(DEFAULT_MAVEN_BASE, fetcher);
    match fetch_versions(&source) {
        Err(MetaError::InvalidXml {
            url: error_url,
            reason,
        }) => {
            assert_eq!(error_url, url);
            assert!(reason.contains("<version>"));
        }
        other => panic!("expected InvalidXml with URL, got {other:?}"),
    }
}

#[test]
fn constant_paths_match_real_repository_layout() {
    assert_eq!(GROUP_PATH, "net/neoforged/neoforge");
    assert_eq!(LEGACY_GROUP_PATH, "net/neoforged/forge");
    assert_eq!(NeoForgeArtifact::Modern.group_path(), GROUP_PATH);
    assert_eq!(NeoForgeArtifact::Legacy.group_path(), LEGACY_GROUP_PATH);
    // 确认没有多写斜杠的陷阱：拼出来的路径就是真实的目录结构。
    let url = modern("20.2.86", "1.20.2").installer_url(DEFAULT_MAVEN_BASE);
    assert!(url.starts_with(&format!("{DEFAULT_MAVEN_BASE}/{GROUP_PATH}/20.2.86/")));
}
