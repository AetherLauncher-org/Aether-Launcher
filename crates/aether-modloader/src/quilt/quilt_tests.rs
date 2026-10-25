use super::{
    QuiltGameVersion, QuiltLibrary, QuiltLoaderVersion, fetch_game_versions, fetch_loader_versions,
    parse_game_versions, parse_loader_versions,
};
use crate::json::parse_json;
use crate::transport::{MetaSource, StaticFetcher};

/// 真实 `GET /v3/versions/game` 的裁剪样本（形状与线上一致）。
const GAME_VERSIONS: &str = r#"
[
  {"version": "1.21.1", "stable": true},
  {"version": "1.21", "stable": true},
  {"version": "26.1-snapshot-3", "stable": false}
]
"#;

/// 真实 `GET /v3/versions/loader/1.21.1` 的裁剪样本：loader 段**没有** `stable` 字段。
const LOADER_VERSIONS: &str = r#"
[
  {
    "loader": {
      "maven": "org.quiltmc:quilt-loader:0.20.0-beta.9",
      "version": "0.20.0-beta.9",
      "build": 9,
      "separator": ".",
      "file_size": 1550096,
      "hashes": {"sha1": "bed0a01be87c4378d9002611d9642e8f9e9c2cff"}
    },
    "hashed": {
      "maven": "org.quiltmc:hashed:1.21.1",
      "version": "1.21.1",
      "file_size": 913310
    },
    "intermediary": {"maven": "net.fabricmc:intermediary:1.21.1", "version": "1.21.1"},
    "launcherMeta": {
      "version": 1,
      "libraries": {
        "client": [],
        "common": [
          {"name": "net.fabricmc:tiny-mappings-parser:0.3.0+build.17", "url": "https://maven.fabricmc.net/"},
          {"name": "org.quiltmc:quilt-json5:1.0.4+final", "url": "https://maven.quiltmc.org/repository/release/"},
          {"name": "org.ow2.asm:asm:9.5", "url": "https://maven.fabricmc.net/"}
        ],
        "server": []
      },
      "mainClass": {
        "client": "org.quiltmc.loader.impl.launch.knot.KnotClient",
        "server": "org.quiltmc.loader.impl.launch.knot.KnotServer",
        "serverLauncher": "org.quiltmc.loader.impl.launch.server.QuiltServerLauncher"
      }
    }
  },
  {
    "loader": {"maven": "org.quiltmc:quilt-loader:0.19.2", "version": "0.19.2", "build": 2},
    "hashed": {"maven": "org.quiltmc:hashed:1.20.4", "version": "1.20.4"},
    "intermediary": {"version": "1.20.4"},
    "launcherMeta": {
      "libraries": {"common": [{"name": "org.quiltmc:quilt-json5:1.0.4+final"}]}
    }
  }
]
"#;

fn loader_fixture() -> Vec<QuiltLoaderVersion> {
    parse_loader_versions("1.21.1", LOADER_VERSIONS).unwrap()
}

#[test]
fn parses_game_versions_with_stable_flags() {
    let versions = parse_game_versions(GAME_VERSIONS).unwrap();
    let expected = [
        QuiltGameVersion {
            version: "1.21.1".to_owned(),
            stable: true,
        },
        QuiltGameVersion {
            version: "1.21".to_owned(),
            stable: true,
        },
        QuiltGameVersion {
            version: "26.1-snapshot-3".to_owned(),
            stable: false,
        },
    ];
    assert_eq!(versions, expected);
}

#[test]
fn parses_loader_versions_and_extracts_the_three_core_versions() {
    let versions = loader_fixture();
    assert_eq!(versions.len(), 2);

    let latest = &versions[0];
    assert_eq!(latest.loader_version, "0.20.0-beta.9");
    assert_eq!(latest.intermediary_version, "1.21.1");
    assert_eq!(latest.hashed_version, "1.21.1");
    assert_eq!(
        latest.main_class,
        "org.quiltmc.loader.impl.launch.knot.KnotClient"
    );
}

#[test]
fn stable_is_a_heuristic_because_upstream_has_no_flag() {
    let versions = loader_fixture();
    // `0.20.0-beta.9` 含 `-beta`；`0.19.2` 不含预发布片段。
    assert!(!versions[0].stable);
    assert!(versions[1].stable);
}

#[test]
fn stable_heuristic_covers_all_pre_release_markers() {
    assert!(super::is_stable("0.19.2"));
    assert!(!super::is_stable("0.20.0-beta.9"));
    assert!(!super::is_stable("0.20.0-alpha.1"));
    assert!(!super::is_stable("0.20.0-pre.2"));
    assert!(!super::is_stable("0.20.0-rc.1"));
    assert!(!super::is_stable("0.20.0-BETA.9"));
}

#[test]
fn missing_launcher_meta_yields_no_libraries_and_fallback_main_class() {
    let json = r#"[{"loader": {"version": "0.19.1"}, "intermediary": {"version": "1.20.1"}}]"#;
    let versions = parse_loader_versions("1.20.1", json).unwrap();

    assert_eq!(versions.len(), 1);
    assert!(versions[0].libraries.is_empty());
    assert_eq!(versions[0].main_class, super::MAIN_CLASS_CLIENT);
}

#[test]
fn missing_main_class_falls_back_to_the_constant() {
    let json = r#"[{
      "loader": {"version": "0.19.1"},
      "launcherMeta": {"libraries": {"common": []}}
    }]"#;
    let versions = parse_loader_versions("1.20.1", json).unwrap();
    assert_eq!(versions[0].main_class, super::MAIN_CLASS_CLIENT);
}

#[test]
fn pure_string_main_class_is_accepted() {
    let json = r#"[{
      "loader": {"version": "0.1.0"},
      "launcherMeta": {"mainClass": "org.quiltmc.loader.impl.launch.knot.KnotClient"}
    }]"#;
    let versions = parse_loader_versions("1.20.1", json).unwrap();
    assert_eq!(
        versions[0].main_class,
        "org.quiltmc.loader.impl.launch.knot.KnotClient"
    );
}

#[test]
fn missing_hashed_falls_back_to_an_empty_version() {
    let json = r#"[{"loader": {"version": "0.19.1"}, "intermediary": {"version": "1.20.1"}}]"#;
    let versions = parse_loader_versions("1.20.1", json).unwrap();
    assert_eq!(versions[0].hashed_version, "");
}

#[test]
fn missing_intermediary_falls_back_to_the_game_version() {
    let json = r#"[{"loader": {"version": "0.19.1"}}]"#;
    let versions = parse_loader_versions("1.20.4", json).unwrap();
    assert_eq!(versions[0].intermediary_version, "1.20.4");
}

#[test]
fn libraries_are_deduped_by_name_across_groups() {
    let json = r#"[{
      "loader": {"version": "0.19.1"},
      "launcherMeta": {"libraries": {
        "client": [{"name": "a:b:1", "url": "https://client.invalid/"}],
        "common": [
          {"name": "a:b:1", "url": "https://common.invalid/"},
          {"name": "c:d:2", "url": "https://common.invalid/"}
        ],
        "server": [{"name": "c:d:2"}]
      }}
    }]"#;
    let versions = parse_loader_versions("1.20.1", json).unwrap();
    let names: Vec<String> = versions[0]
        .libraries
        .iter()
        .map(|library| library.name.clone())
        .collect();

    assert_eq!(names, ["a:b:1", "c:d:2"]);
    // 保留第一次出现的那条（client 组）。
    assert_eq!(
        versions[0].libraries[0].url.as_deref(),
        Some("https://client.invalid/")
    );
}

#[test]
fn development_libraries_are_not_collected() {
    let json = r#"[{
      "loader": {"version": "0.19.1"},
      "launcherMeta": {"libraries": {
        "common": [{"name": "a:b:1"}],
        "development": [{"name": "dev:only:1"}]
      }}
    }]"#;
    let versions = parse_loader_versions("1.20.1", json).unwrap();
    let names: Vec<&str> = versions[0]
        .libraries
        .iter()
        .map(|library| library.name.as_str())
        .collect();
    assert_eq!(names, ["a:b:1"]);
}

#[test]
fn target_version_id_uses_the_quilt_prefix() {
    let version = &loader_fixture()[0];
    assert_eq!(
        version.target_version_id("1.21.1"),
        "quilt-loader-0.20.0-beta.9-1.21.1"
    );
}

#[test]
fn version_json_puts_the_three_core_libraries_first_with_their_repositories() {
    let version = &loader_fixture()[0];
    let json = version.to_version_json("1.21.1");

    assert_eq!(json["id"], "quilt-loader-0.20.0-beta.9-1.21.1");
    assert_eq!(json["inheritsFrom"], "1.21.1");
    assert_eq!(json["type"], "release");
    assert_eq!(
        json["mainClass"],
        "org.quiltmc.loader.impl.launch.knot.KnotClient"
    );

    let libraries = json["libraries"].as_array().unwrap();
    let names: Vec<&str> = libraries
        .iter()
        .map(|library| library["name"].as_str().unwrap())
        .collect();
    let expected = [
        "org.quiltmc:quilt-loader:0.20.0-beta.9",
        "org.quiltmc:hashed:1.21.1",
        "net.fabricmc:intermediary:1.21.1",
        "net.fabricmc:tiny-mappings-parser:0.3.0+build.17",
        "org.quiltmc:quilt-json5:1.0.4+final",
        "org.ow2.asm:asm:9.5",
    ];
    assert_eq!(names, expected);

    assert_eq!(
        libraries[0]["url"],
        "https://maven.quiltmc.org/repository/release/"
    );
    assert_eq!(
        libraries[1]["url"],
        "https://maven.quiltmc.org/repository/release/"
    );
    assert_eq!(libraries[2]["url"], "https://maven.fabricmc.net/");
}

#[test]
fn version_json_has_no_arguments_key_and_no_duplicate_names() {
    let version = &loader_fixture()[0];
    let json = version.to_version_json("1.21.1");

    assert!(json.get("arguments").is_none());

    let libraries = json["libraries"].as_array().unwrap();
    let mut names: Vec<&str> = libraries
        .iter()
        .map(|library| library["name"].as_str().unwrap())
        .collect();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), total);
}

#[test]
fn install_plan_downloads_libraries_then_writes_the_version_json() {
    let version = &loader_fixture()[0];
    let plan = version.install_plan("1.21.1", "https://maven.quiltmc.org/repository/release/");

    assert_eq!(plan.kind, crate::ModLoaderKind::Quilt);
    assert_eq!(plan.target_version_id, "quilt-loader-0.20.0-beta.9-1.21.1");
    assert_eq!(plan.inherits_from, "1.21.1");
    assert!(plan.validate(&["1.21.1"]).is_ok());

    let destinations: Vec<&str> = plan
        .steps
        .iter()
        .filter_map(|step| step.destination())
        .collect();
    let expected = [
        "libraries/net/fabricmc/tiny-mappings-parser/0.3.0+build.17/tiny-mappings-parser-0.3.0+build.17.jar",
        "libraries/org/quiltmc/quilt-json5/1.0.4+final/quilt-json5-1.0.4+final.jar",
        "libraries/org/ow2/asm/asm/9.5/asm-9.5.jar",
        "versions/quilt-loader-0.20.0-beta.9-1.21.1/quilt-loader-0.20.0-beta.9-1.21.1.json",
    ];
    assert_eq!(destinations, expected);
}

#[test]
fn install_plan_skips_libraries_without_a_repository() {
    // 第二条 loader 只有一件没有 `url` 的库，所以只剩写入版本 JSON 这一步。
    let version = &loader_fixture()[1];
    let plan = version.install_plan("1.20.4", "");

    assert_eq!(plan.steps.len(), 1);
    assert!(plan.steps[0].destination().is_some());
}

#[test]
fn malformed_json_reports_invalid_json() {
    let error = parse_loader_versions("1.21.1", "{").unwrap_err();
    assert!(matches!(
        error,
        crate::transport::MetaError::InvalidJson { .. }
    ));
}

#[test]
fn a_json_object_that_is_not_an_array_is_rejected() {
    let error = parse_game_versions(r#"{"versions": []}"#).unwrap_err();
    assert!(matches!(
        error,
        crate::transport::MetaError::InvalidJson { .. }
    ));
}

#[test]
fn empty_arrays_are_valid_results() {
    assert!(parse_game_versions("[]").unwrap().is_empty());
    assert!(parse_loader_versions("1.21.1", "[]").unwrap().is_empty());
}

#[test]
fn entries_without_a_loader_version_are_skipped() {
    let json = r#"[{"loader": {}}, {"intermediary": {"version": "1.21.1"}}]"#;
    assert!(parse_loader_versions("1.21.1", json).unwrap().is_empty());
}

#[test]
fn fetch_game_versions_requests_the_v3_game_endpoint() {
    let url = "https://meta.quiltmc.org/v3/versions/game";
    let source = MetaSource::new(
        super::DEFAULT_META_BASE,
        StaticFetcher::new().with(url, GAME_VERSIONS),
    );

    let versions = fetch_game_versions(&source).unwrap();
    assert_eq!(versions.len(), 3);
}

#[test]
fn fetch_loader_versions_requests_the_v3_loader_endpoint() {
    let url = "https://meta.quiltmc.org/v3/versions/loader/1.21.1";
    let source = MetaSource::new(
        super::DEFAULT_META_BASE,
        StaticFetcher::new().with(url, LOADER_VERSIONS),
    );

    let versions = fetch_loader_versions(&source, "1.21.1").unwrap();
    assert_eq!(versions.len(), 2);
}

#[test]
fn fetch_without_a_registered_fixture_never_reaches_the_network() {
    let source = MetaSource::new(super::DEFAULT_META_BASE, StaticFetcher::new());
    let error = fetch_game_versions(&source).unwrap_err();
    assert!(matches!(
        error,
        crate::transport::MetaError::Transport { .. }
    ));
}

#[test]
fn library_defaults_are_empty() {
    assert_eq!(
        QuiltLibrary::default(),
        QuiltLibrary {
            name: String::new(),
            url: None,
        }
    );
    let default_loader = QuiltLoaderVersion::default();
    assert_eq!(default_loader.main_class, super::MAIN_CLASS_CLIENT);
    assert!(!default_loader.stable);
}

#[test]
fn parse_json_helper_rejects_garbage() {
    assert!(parse_json("https://example.invalid/", "not json").is_err());
}
