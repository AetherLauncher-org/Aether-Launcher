use super::{
    ForgeLine, ForgePromotion, ForgeVersion, PromotionChannel, fetch_maven_versions,
    fetch_promotions, filter_by_game, parse_maven_versions, parse_promotions, split_coordinate,
};
use crate::transport::{MetaSource, StaticFetcher};

/// 真实 `promotions_slim.json` 的裁剪样本（形状与线上一致）。
const PROMOTIONS: &str = r#"
{
  "homepage": "https://files.minecraftforge.net/net/minecraftforge/forge/",
  "promos": {
    "1.1-latest": "1.3.4.29",
    "1.7.10-latest": "10.13.4.1614",
    "1.7.10-recommended": "10.13.4.1614",
    "1.12.2-latest": "14.23.5.2864",
    "1.12.2-recommended": "14.23.5.2859",
    "1.19.2-recommended": "43.5.0",
    "1.19.2-latest": "43.5.2",
    "1.20.1-latest": "47.4.28",
    "1.20.1-recommended": "47.4.28",
    "1.21.1-latest": "52.1.20",
    "1.21.1-recommended": "52.1.20",
    "1.20.1-beta": "47.0.0",
    "broken": "1.2.3"
  }
}
"#;

/// 真实 maven-metadata.xml 的裁剪样本。
const MAVEN_METADATA: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>net.minecraftforge</groupId>
  <artifactId>forge</artifactId>
  <versioning>
    <latest>26.3-66.1.0</latest>
    <release>26.3-66.1.0</release>
    <versions>
      <version>1.21-51.0.33</version>
      <version>1.19.2-43.5.1</version>
      <version>1.12.2-14.23.5.2860</version>
      <version>1.7.10-10.13.4.1614-1.7.10</version>
      <version>26.3-66.1.0</version>
    </versions>
    <lastUpdated>20261021180547</lastUpdated>
  </versioning>
</metadata>
"#;

fn forge(game_version: &str, version: &str, line: ForgeLine) -> ForgeVersion {
    ForgeVersion {
        version: version.to_owned(),
        game_version: game_version.to_owned(),
        line,
    }
}

#[test]
fn parses_promotions_from_the_real_shape() {
    let promotions = parse_promotions(PROMOTIONS).unwrap();

    assert_eq!(
        promotions.homepage.as_deref(),
        Some("https://files.minecraftforge.net/net/minecraftforge/forge/")
    );
    // 真实 `promotions_slim.json` 里每个键都是独立的 `<游戏版本>-<latest|recommended>` 条目，
    // `latest` 与 `recommended` 不合并。本 fixture 的 13 个键里有 11 个是这种合法形状，
    // 另外 2 个是刻意构造的坏数据：`1.20.1-beta`（渠道未知）、`broken`（没有游戏版本段），
    // 都必须被跳过而不是报错。
    assert_eq!(promotions.promos.len(), 11);
    assert!(
        promotions
            .promos
            .iter()
            .all(|promotion| promotion.game_version != "1.20.1"
                || promotion.channel != PromotionChannel::Latest
                || promotion.version == "47.4.28")
    );
}

#[test]
fn for_game_prefers_the_latest_channel() {
    let promotions = parse_promotions(PROMOTIONS).unwrap();

    let latest = promotions.for_game("1.19.2").unwrap();
    assert_eq!(latest.channel, PromotionChannel::Latest);
    assert_eq!(latest.version, "43.5.2");
}

#[test]
fn for_game_falls_back_to_recommended_when_latest_is_missing() {
    let json = r#"{"promos": {"1.18.2-recommended": "40.3.0"}}"#;
    let promotions = parse_promotions(json).unwrap();

    let promotion = promotions.for_game("1.18.2").unwrap();
    assert_eq!(promotion.channel, PromotionChannel::Recommended);
    assert_eq!(promotion.version, "40.3.0");
}

#[test]
fn promoted_returns_only_the_requested_channel() {
    let promotions = parse_promotions(PROMOTIONS).unwrap();

    let recommended = promotions.promoted("1.12.2", PromotionChannel::Recommended);
    assert_eq!(
        recommended.map(|promotion| promotion.version.as_str()),
        Some("14.23.5.2859")
    );

    let latest = promotions.promoted("1.12.2", PromotionChannel::Latest);
    assert_eq!(
        latest.map(|promotion| promotion.version.as_str()),
        Some("14.23.5.2864")
    );
}

#[test]
fn unknown_promotion_channels_are_skipped() {
    let json = r#"{"promos": {"1.20.1-beta": "47.0.0", "1.20.1-latest": "47.4.28"}}"#;
    let promotions = parse_promotions(json).unwrap();

    assert_eq!(promotions.promos.len(), 1);
    assert_eq!(promotions.promos[0].channel, PromotionChannel::Latest);
}

#[test]
fn malformed_promotions_json_is_rejected() {
    let error = parse_promotions("{").unwrap_err();
    assert!(matches!(
        error,
        crate::transport::MetaError::InvalidJson { .. }
    ));
}

#[test]
fn empty_promotions_are_valid() {
    let promotions = parse_promotions(r#"{"promos": {}}"#).unwrap();
    assert!(promotions.promos.is_empty());
    assert!(promotions.for_game("1.20.1").is_none());
}

#[test]
fn split_coordinate_handles_every_real_world_shape() {
    let cases = [
        ("1.19.2-43.5.1", "1.19.2", "43.5.1"),
        ("1.7.10-10.13.4.1614-1.7.10", "1.7.10", "10.13.4.1614"),
        ("1.21-51.0.33", "1.21", "51.0.33"),
        ("26.3-66.1.0", "26.3", "66.1.0"),
        ("1.12.2-14.23.5.2860", "1.12.2", "14.23.5.2860"),
    ];
    for (input, game, forge_version) in cases {
        let (actual_game, actual_forge) =
            split_coordinate(input).unwrap_or_else(|| panic!("expected {input} to split"));
        assert_eq!(actual_game, game, "{input}");
        assert_eq!(actual_forge, forge_version, "{input}");
    }
}

#[test]
fn split_coordinate_rejects_malformed_coordinates() {
    for input in [
        "1.19.2",
        "",
        "-43.5.1",
        "1.19.2-",
        "1.19.2--43.5.1",
        "x.y-1.0",
        "1.19.2-4 3",
        "1/2-1.0",
    ] {
        assert!(
            split_coordinate(input).is_none(),
            "{input} should not split"
        );
    }
}

#[test]
fn forge_line_distinguishes_legacy_and_modern() {
    assert_eq!(super::line_for_game("1.12.2"), Some(ForgeLine::Legacy));
    assert_eq!(super::line_for_game("1.7.10"), Some(ForgeLine::Legacy));
    assert_eq!(super::line_for_game("1.13"), Some(ForgeLine::Modern));
    assert_eq!(super::line_for_game("1.20.1"), Some(ForgeLine::Modern));
    assert_eq!(super::line_for_game("26.3"), Some(ForgeLine::Modern));
    assert_eq!(super::line_for_game("garbage"), None);
}

#[test]
fn parses_maven_versions_from_the_real_shape() {
    let versions = parse_maven_versions(MAVEN_METADATA).unwrap();

    assert_eq!(versions.len(), 5);
    assert_eq!(versions[0], forge("1.21", "51.0.33", ForgeLine::Modern));
    assert_eq!(
        versions[3],
        forge("1.7.10", "10.13.4.1614", ForgeLine::Legacy)
    );
    assert_eq!(versions[4], forge("26.3", "66.1.0", ForgeLine::Modern));
}

#[test]
fn maven_parsing_ignores_attributes_and_whitespace() {
    let xml = r#"<metadata><versioning><versions>
        <version >1.20.1-47.4.28</version>
        <version foo="bar">1.19.2-43.5.1</version>
        <version
          >1.18.2-40.3.12</version>
      </versions></versioning></metadata>"#;

    let versions = parse_maven_versions(xml).unwrap();
    let games: Vec<&str> = versions
        .iter()
        .map(|version| version.game_version.as_str())
        .collect();
    assert_eq!(games, ["1.20.1", "1.19.2", "1.18.2"]);
}

#[test]
fn maven_parsing_does_not_capture_the_versions_container() {
    let xml = r#"<metadata><versioning><versions><version>1.20.1-47.4.28</version></versions></versioning></metadata>"#;
    let versions = parse_maven_versions(xml).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].game_version, "1.20.1");
}

#[test]
fn maven_parsing_rejects_input_without_versions() {
    let error = parse_maven_versions("<metadata><groupId>net.minecraftforge</groupId></metadata>")
        .unwrap_err();
    assert!(matches!(
        error,
        crate::transport::MetaError::InvalidXml { .. }
    ));
}

#[test]
fn filter_by_game_keeps_the_original_order() {
    let versions = parse_maven_versions(MAVEN_METADATA).unwrap();
    let filtered = filter_by_game(&versions, "1.19.2");

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].version, "43.5.1");

    assert!(filter_by_game(&versions, "1.20.9").is_empty());
}

#[test]
fn installer_file_name_and_url_match_the_maven_layout() {
    let version = forge("1.20.1", "47.4.28", ForgeLine::Modern);

    assert_eq!(
        version.installer_file_name(),
        "forge-1.20.1-47.4.28-installer.jar"
    );
    assert_eq!(
        version.installer_url("https://maven.minecraftforge.net"),
        "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.28/forge-1.20.1-47.4.28-installer.jar"
    );
}

#[test]
fn installer_url_normalises_a_trailing_slash() {
    let version = forge("1.20.1", "47.4.28", ForgeLine::Modern);

    assert_eq!(
        version.installer_url("https://maven.minecraftforge.net/"),
        version.installer_url("https://maven.minecraftforge.net")
    );
}

#[test]
fn target_version_id_includes_the_game_version() {
    assert_eq!(
        forge("1.20.1", "47.4.28", ForgeLine::Modern).target_version_id(),
        "1.20.1-forge-47.4.28"
    );
}

#[test]
fn install_plan_downloads_the_installer_then_runs_it() {
    let version = forge("1.20.1", "47.4.28", ForgeLine::Modern);
    let plan = version.install_plan("https://maven.minecraftforge.net");

    assert_eq!(plan.kind, crate::ModLoaderKind::Forge);
    assert_eq!(plan.target_version_id, "1.20.1-forge-47.4.28");
    assert_eq!(plan.inherits_from, "1.20.1");
    assert!(plan.validate(&["1.21.1"]).is_ok());

    assert_eq!(plan.steps.len(), 2);
    assert!(plan.steps[0].is_download());
    assert_eq!(
        plan.steps[0].destination(),
        Some("installers/forge-1.20.1-47.4.28-installer.jar")
    );
    assert_eq!(plan.steps[1].destination(), None);
}

#[test]
fn install_plan_is_rejected_when_the_target_already_exists() {
    let version = forge("1.20.1", "47.4.28", ForgeLine::Modern);
    let plan = version.install_plan("https://maven.minecraftforge.net");

    let error = plan.validate(&["1.20.1-forge-47.4.28"]).unwrap_err();
    assert!(matches!(
        error,
        crate::transport::MetaError::VersionAlreadyInstalled { .. }
    ));
}

#[test]
fn fetch_promotions_uses_the_injected_source() {
    let url = super::DEFAULT_PROMOTIONS_URL;
    let source = MetaSource::new(
        "https://files.minecraftforge.net/net/minecraftforge/forge",
        StaticFetcher::new().with(url, PROMOTIONS),
    );

    let promotions = fetch_promotions(&source).unwrap();
    // 与 `parses_promotions_from_the_real_shape` 同一个 fixture：11 条合法、2 条坏数据被跳过。
    assert_eq!(promotions.promos.len(), 11);
}

#[test]
fn fetch_maven_versions_requests_the_forge_metadata() {
    let source = MetaSource::new(
        "https://maven.minecraftforge.net",
        StaticFetcher::new().with(
            "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml",
            MAVEN_METADATA,
        ),
    );

    let versions = fetch_maven_versions(&source).unwrap();
    assert_eq!(versions.len(), 5);
}

#[test]
fn fetch_without_a_registered_fixture_never_reaches_the_network() {
    let source = MetaSource::new("https://maven.minecraftforge.net", StaticFetcher::new());
    let error = fetch_maven_versions(&source).unwrap_err();
    assert!(matches!(
        error,
        crate::transport::MetaError::Transport { .. }
    ));
}

#[test]
fn promotion_channel_suffixes_round_trip() {
    assert_eq!(PromotionChannel::Latest.key_suffix(), "latest");
    assert_eq!(PromotionChannel::Recommended.key_suffix(), "recommended");
}

#[test]
fn promotion_structs_expose_the_parsed_fields() {
    let promotions = parse_promotions(PROMOTIONS).unwrap();
    let promotion: &ForgePromotion = promotions.for_game("1.20.1").unwrap();

    assert_eq!(promotion.game_version, "1.20.1");
    assert_eq!(promotion.channel, PromotionChannel::Latest);
    assert_eq!(promotion.version, "47.4.28");
}
