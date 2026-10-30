//! `fabric` 模块的单元测试。
//!
//! fixture 全部内联（形状照 `GET /v2/versions/game` 与
//! `GET /v2/versions/loader/1.21.1` 的真实响应裁剪而来），测试不访问真实网络：
//! `fetch_*` 用 [`StaticFetcher`] 提供固定响应，未注册的 URL 一律报错。

use serde_json::Value;

use super::{
    DEFAULT_MAVEN_BASE, DEFAULT_META_BASE, FabricLibrary, FabricLoaderVersion, MAIN_CLASS_CLIENT,
    fetch_game_versions, fetch_loader_versions, maven_relative_path, parse_game_versions,
    parse_loader_versions,
};
use crate::ModLoaderKind;
use crate::installer::InstallStep;
use crate::transport::{MetaError, MetaSource, StaticFetcher};

/// `GET /v2/versions/game` 的真实形状（截取 4 条；新版本号体系 `26.4-snapshot-5` 也在）。
const GAME_VERSIONS: &str = r#"[
  {"version":"26.4-snapshot-5","stable":false},
  {"version":"1.21.1","stable":true},
  {"version":"1.21","stable":true},
  {"version":"1.20.1","stable":true}
]"#;

/// `GET /v2/versions/loader/1.21.1` 的真实形状（截取 3 条）。
///
/// 1. `0.16.0`：`mainClass` 是对象，库带 `url` / `size`，另有必须被跳过的 `development` 库。
/// 2. 没有 `launcherMeta` 的加载器（解析时主类回退常量）。
/// 3. `0.1.0.48`：`mainClass` 是纯字符串，库缺 `url` / `size`，`loaderMeta.libraries` 缺 `client` 键。
const LOADER_VERSIONS: &str = r#"[
  {
    "loader":{"separator":".","build":1,"maven":"net.fabricmc:fabric-loader:0.16.0","version":"0.16.0","stable":true},
    "intermediary":{"maven":"net.fabricmc:intermediary:1.21.1","version":"1.21.1","stable":true},
    "launcherMeta":{
      "version":2,
      "min_java_version":8,
      "libraries":{
        "client":[],
        "common":[
          {"name":"org.ow2.asm:asm:9.6","url":"https://maven.fabricmc.net/","md5":"9cb72080438acf27f2607002e1444a7b","sha1":"ada2141c0cc52ee8f5c48cd5fa4ce0e794f22236","sha256":"ed825d10ab1","sha512":"8db6efa37d4d","size":123598},
          {"name":"org.ow2.asm:asm-analysis:9.6","url":"https://maven.fabricmc.net/","size":33636},
          {"name":"net.fabricmc:sponge-mixin:0.15.4+mixin.0.8.7","url":"https://maven.fabricmc.net/","size":1551593}
        ],
        "server":[],
        "development":[
          {"name":"io.github.llamalad7:mixinextras-fabric:0.5.5","url":"https://maven.fabricmc.net/","size":727864}
        ]
      },
      "mainClass":{
        "client":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        "server":"net.fabricmc.loader.impl.launch.knot.KnotServer"
      }
    }
  },
  {
    "loader":{"maven":"net.fabricmc:fabric-loader:0.15.11","version":"0.15.11","stable":true},
    "intermediary":{"maven":"net.fabricmc:intermediary:1.21.1","version":"1.21.1","stable":true}
  },
  {
    "loader":{"separator":".","build":48,"maven":"net.fabricmc:fabric-loader:0.1.0.48","version":"0.1.0.48"},
    "intermediary":{"maven":"net.fabricmc:intermediary:1.21.1","version":"1.21.1"},
    "launcherMeta":{
      "version":1,
      "libraries":{
        "common":[
          {"name":"net.minecraft:launchwrapper:1.12"},
          {"name":"org.ow2.asm:asm:7.0","url":"http://repo.maven.apache.org/maven2/"}
        ]
      },
      "mainClass":"net.minecraft.launchwrapper.Launch"
    }
  }
]"#;

fn first_loader() -> FabricLoaderVersion {
    let mut versions = parse_loader_versions("1.21.1", LOADER_VERSIONS).unwrap();
    let first = versions.remove(0);
    assert_eq!(versions.len(), 2, "fixture 应当有 3 条");
    first
}

fn library_names(json: &Value) -> Vec<String> {
    json["libraries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|library| library["name"].as_str().unwrap().to_owned())
        .collect()
}

fn downloads(plan: &crate::installer::ModLoaderInstallPlan) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for step in &plan.steps {
        if let InstallStep::Download {
            url, destination, ..
        } = step
        {
            found.push((url.clone(), destination.clone()));
        }
    }
    found
}

// ---- 游戏版本 ----

#[test]
fn parses_real_game_versions_with_stable_flags() {
    let versions = parse_game_versions(GAME_VERSIONS).unwrap();

    assert_eq!(versions.len(), 4);
    assert_eq!(versions[0].version, "26.4-snapshot-5");
    assert!(!versions[0].stable);
    assert_eq!(versions[1].version, "1.21.1");
    assert!(versions[1].stable);
}

#[test]
fn game_versions_default_missing_stable_to_false_and_skip_entries_without_version() {
    let versions = parse_game_versions(r#"[{"version":"1.21.1"},{"stable":true}]"#).unwrap();

    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].version, "1.21.1");
    assert!(!versions[0].stable);
}

#[test]
fn game_versions_accept_an_empty_array() {
    assert!(parse_game_versions("[]").unwrap().is_empty());
}

#[test]
fn game_versions_reject_malformed_json() {
    let error = parse_game_versions("[{").unwrap_err();

    match error {
        MetaError::InvalidJson { url, .. } => {
            assert_eq!(url, "https://meta.fabricmc.net/v2/versions/game");
        }
        other => panic!("期望 InvalidJson，实际 {other:?}"),
    }
}

#[test]
fn game_versions_reject_a_non_array_payload() {
    assert!(matches!(
        parse_game_versions(r#"{"version":"1.21.1"}"#),
        Err(MetaError::InvalidJson { .. })
    ));
}

// ---- 加载器版本 ----

#[test]
fn parses_real_loader_versions() {
    let first = first_loader();

    assert_eq!(first.loader_version, "0.16.0");
    assert_eq!(first.intermediary_version, "1.21.1");
    assert!(first.stable);
    assert_eq!(first.main_class, MAIN_CLASS_CLIENT);
    assert_eq!(
        first.libraries,
        vec![
            FabricLibrary {
                name: "org.ow2.asm:asm:9.6".to_owned(),
                url: Some(DEFAULT_MAVEN_BASE.to_owned()),
                size: Some(123_598),
            },
            FabricLibrary {
                name: "org.ow2.asm:asm-analysis:9.6".to_owned(),
                url: Some(DEFAULT_MAVEN_BASE.to_owned()),
                size: Some(33_636),
            },
            FabricLibrary {
                name: "net.fabricmc:sponge-mixin:0.15.4+mixin.0.8.7".to_owned(),
                url: Some(DEFAULT_MAVEN_BASE.to_owned()),
                size: Some(1_551_593),
            },
        ]
    );
}

#[test]
fn loader_version_without_launcher_meta_uses_the_fallback_main_class() {
    let versions = parse_loader_versions("1.21.1", LOADER_VERSIONS).unwrap();
    let without_meta = &versions[1];

    assert_eq!(without_meta.loader_version, "0.15.11");
    assert_eq!(without_meta.main_class, MAIN_CLASS_CLIENT);
    assert!(without_meta.libraries.is_empty());
}

#[test]
fn loader_version_without_main_class_uses_the_fallback_main_class() {
    let json = r#"[
      {"loader":{"version":"0.16.0"},"intermediary":{"version":"1.21.1"},
       "launcherMeta":{"libraries":{"client":[],"common":[],"server":[]}}}
    ]"#;

    let versions = parse_loader_versions("1.21.1", json).unwrap();

    assert_eq!(versions[0].main_class, MAIN_CLASS_CLIENT);
}

#[test]
fn loader_version_with_a_string_main_class_keeps_it() {
    let versions = parse_loader_versions("1.21.1", LOADER_VERSIONS).unwrap();
    let legacy = &versions[2];

    assert_eq!(legacy.main_class, "net.minecraft.launchwrapper.Launch");
    assert_eq!(legacy.libraries.len(), 2);
    assert_eq!(legacy.libraries[0].url, None);
    assert_eq!(
        legacy.libraries[1].url.as_deref(),
        Some("http://repo.maven.apache.org/maven2/")
    );
}

#[test]
fn loader_version_without_stable_flags_is_unstable() {
    let versions = parse_loader_versions("1.21.1", LOADER_VERSIONS).unwrap();

    assert!(!versions[2].stable, "0.1.0.48 没有 stable 字段");
    assert!(
        !parse_loader_versions(
            "1.21.1",
            r#"[{"loader":{"version":"1.a"},"intermediary":{"version":"1.21.1"}}]"#
        )
        .unwrap()[0]
            .stable
    );
}

#[test]
fn intermediary_version_falls_back_to_the_game_version() {
    let json = r#"[{"loader":{"version":"0.16.0","stable":true}}]"#;

    let versions = parse_loader_versions("1.20.1", json).unwrap();

    assert_eq!(versions[0].intermediary_version, "1.20.1");
}

#[test]
fn entries_without_a_loader_version_are_skipped() {
    let json = r#"[{"intermediary":{"version":"1.21.1"}}]"#;

    assert!(parse_loader_versions("1.21.1", json).unwrap().is_empty());
}

#[test]
fn a_placeholder_intermediary_is_reported_as_unsupported() {
    // 26.x 这类游戏版本 Fabric 还没发布 intermediary 映射，接口给 `0.0.0`。
    // 该构件只有几百字节，装上必定启动失败，所以要能被识别出来。
    let json = r#"[{"loader":{"version":"0.20.1","stable":true},"intermediary":{"version":"0.0.0","stable":true}}]"#;

    let versions = parse_loader_versions("26.1.2", json).unwrap();

    assert_eq!(versions[0].intermediary_version, "0.0.0");
    let reason = versions[0]
        .unsupported_reason()
        .expect("0.0.0 must be reported as unsupported");
    assert!(
        reason.contains("intermediary"),
        "unexpected reason: {reason}"
    );
}

#[test]
fn a_real_intermediary_is_supported() {
    let json = r#"[{"loader":{"version":"0.16.9","stable":true},"intermediary":{"version":"1.21.1","stable":true}}]"#;

    let versions = parse_loader_versions("1.21.1", json).unwrap();

    assert_eq!(versions[0].unsupported_reason(), None);
}

#[test]
fn libraries_follow_client_then_common_then_server_and_dedupe_by_name() {
    let json = r#"[
      {"loader":{"version":"0.16.0","stable":true},"intermediary":{"version":"1.21.1"},
       "launcherMeta":{"libraries":{
         "client":[{"name":"com.example:client-only:1.0","url":"https://first.invalid/"}],
         "common":[{"name":"org.ow2.asm:asm:9.6","url":"https://maven.fabricmc.net/","size":123598},
                   {"name":"com.example:client-only:1.0","url":"https://second.invalid/"}],
         "server":[{"name":"org.ow2.asm:asm:9.6","url":"https://maven.fabricmc.net/"}],
         "development":[{"name":"io.github.llamalad7:mixinextras-fabric:0.5.5","url":"https://maven.fabricmc.net/"}]}}}
    ]"#;

    let libraries = parse_loader_versions("1.21.1", json)
        .unwrap()
        .remove(0)
        .libraries;
    let names: Vec<&str> = libraries
        .iter()
        .map(|library| library.name.as_str())
        .collect();

    assert_eq!(
        names,
        ["com.example:client-only:1.0", "org.ow2.asm:asm:9.6"]
    );
    assert_eq!(libraries[0].url.as_deref(), Some("https://first.invalid/"));
    assert_eq!(libraries[1].size, Some(123_598));
    assert!(
        !names.iter().any(|name| name.contains("mixinextras")),
        "development 里的库不应被收集"
    );
}

#[test]
fn libraries_tolerate_missing_library_lists_and_missing_entries() {
    let json = r#"[
      {"loader":{"version":"0.16.0"},"intermediary":{"version":"1.21.1"},
       "launcherMeta":{"mainClass":{"client":"a.B"}}},
      {"loader":{"version":"0.15.0"},"intermediary":{"version":"1.21.1"},
       "launcherMeta":{"libraries":{"common":[{"url":"https://maven.fabricmc.net/"}]}}}
    ]"#;

    let versions = parse_loader_versions("1.21.1", json).unwrap();

    assert!(versions[0].libraries.is_empty());
    assert!(versions[1].libraries.is_empty(), "没有 name 的条目跳过");
}

#[test]
fn loader_versions_reject_malformed_json() {
    let error = parse_loader_versions("1.21.1", "{not json").unwrap_err();

    match error {
        MetaError::InvalidJson { url, .. } => {
            assert_eq!(url, "https://meta.fabricmc.net/v2/versions/loader/1.21.1");
        }
        other => panic!("期望 InvalidJson，实际 {other:?}"),
    }
}

#[test]
fn loader_versions_accept_an_empty_array() {
    assert!(parse_loader_versions("1.21.1", "[]").unwrap().is_empty());
}

// ---- 版本 JSON ----

#[test]
fn target_version_id_uses_the_fabric_prefix() {
    let loader = first_loader();

    assert_eq!(
        loader.target_version_id("1.21.1"),
        "fabric-loader-0.16.0-1.21.1"
    );
}

#[test]
fn version_json_has_the_expected_top_level_fields() {
    let json = first_loader().to_version_json("1.21.1");

    assert_eq!(json["id"], "fabric-loader-0.16.0-1.21.1");
    assert_eq!(json["inheritsFrom"], "1.21.1");
    assert_eq!(json["type"], "release");
    assert_eq!(json["mainClass"], MAIN_CLASS_CLIENT);
    assert!(json["libraries"].is_array());
}

#[test]
fn version_json_omits_arguments() {
    let json = first_loader().to_version_json("1.21.1");

    assert!(json.get("arguments").is_none());
    assert!(json.get("minecraftArguments").is_none());
}

#[test]
fn version_json_puts_the_core_libraries_first() {
    let json = first_loader().to_version_json("1.21.1");

    let expected = [
        "net.fabricmc:intermediary:1.21.1",
        "net.fabricmc:fabric-loader:0.16.0",
        "org.ow2.asm:asm:9.6",
        "org.ow2.asm:asm-analysis:9.6",
        "net.fabricmc:sponge-mixin:0.15.4+mixin.0.8.7",
    ];
    assert_eq!(library_names(&json), expected);

    let libraries = json["libraries"].as_array().unwrap();
    assert_eq!(libraries[0]["url"], DEFAULT_MAVEN_BASE);
    assert_eq!(libraries[1]["url"], DEFAULT_MAVEN_BASE);
}

#[test]
fn version_json_never_repeats_a_library_name() {
    let loader = FabricLoaderVersion {
        loader_version: "0.16.0".to_owned(),
        intermediary_version: "1.21.1".to_owned(),
        stable: true,
        libraries: vec![
            FabricLibrary {
                name: "net.fabricmc:intermediary:1.21.1".to_owned(),
                url: Some(DEFAULT_MAVEN_BASE.to_owned()),
                size: None,
            },
            FabricLibrary {
                name: "net.fabricmc:fabric-loader:0.16.0".to_owned(),
                url: Some(DEFAULT_MAVEN_BASE.to_owned()),
                size: None,
            },
            FabricLibrary {
                name: "org.ow2.asm:asm:9.6".to_owned(),
                url: Some(DEFAULT_MAVEN_BASE.to_owned()),
                size: Some(123_598),
            },
        ],
        main_class: MAIN_CLASS_CLIENT.to_owned(),
    };

    let json = loader.to_version_json("1.21.1");
    let names = library_names(&json);

    assert_eq!(
        names,
        [
            "net.fabricmc:intermediary:1.21.1",
            "net.fabricmc:fabric-loader:0.16.0",
            "org.ow2.asm:asm:9.6",
        ]
    );
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "库名重复：{names:?}");
}

#[test]
fn version_json_keeps_libraries_that_have_no_repository() {
    let loader = parse_loader_versions("1.21.1", LOADER_VERSIONS)
        .unwrap()
        .remove(2);
    let json = loader.to_version_json("1.21.1");

    assert_eq!(
        library_names(&json),
        [
            "net.fabricmc:intermediary:1.21.1",
            "net.fabricmc:fabric-loader:0.1.0.48",
            "net.minecraft:launchwrapper:1.12",
            "org.ow2.asm:asm:7.0",
        ]
    );
    assert!(json["libraries"][2].get("url").is_none());
}

// ---- 安装计划 ----

#[test]
fn install_plan_downloads_every_library_and_writes_the_version_json() {
    let plan = first_loader().install_plan("1.21.1", DEFAULT_MAVEN_BASE);

    assert_eq!(plan.kind, ModLoaderKind::Fabric);
    assert_eq!(plan.loader_version, "0.16.0");
    assert_eq!(plan.game_version, "1.21.1");
    assert_eq!(plan.inherits_from, "1.21.1");
    assert_eq!(plan.target_version_id, "fabric-loader-0.16.0-1.21.1");
    assert_eq!(plan.main_class, MAIN_CLASS_CLIENT);

    let downloads = downloads(&plan);

    // 核心两件在最前：版本 JSON 会引用它们，所以必须先下载。
    assert_eq!(downloads.len(), 5, "两个核心库 + 三个 launcherMeta 库");
    assert_eq!(
        downloads[0].1,
        "libraries/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar"
    );
    assert_eq!(
        downloads[0].0,
        "https://maven.fabricmc.net/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar"
    );
    assert_eq!(
        downloads[1].1,
        "libraries/net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar"
    );
    assert_eq!(
        downloads[2].1, "libraries/org/ow2/asm/asm/9.6/asm-9.6.jar",
        "destination 用 maven 相对路径"
    );
    assert_eq!(
        downloads[2].0,
        "https://maven.fabricmc.net/org/ow2/asm/asm/9.6/asm-9.6.jar"
    );
    assert_eq!(
        downloads[3].1,
        "libraries/org/ow2/asm/asm-analysis/9.6/asm-analysis-9.6.jar"
    );
    assert_eq!(
        downloads[4].1,
        "libraries/net/fabricmc/sponge-mixin/0.15.4+mixin.0.8.7/sponge-mixin-0.15.4+mixin.0.8.7.jar"
    );

    // 版本 JSON 里写的每一个库都必须有对应的下载步骤 —— 这是启动时不报
    // 「required local file does not exist」的前提。
    let json = plan
        .steps
        .iter()
        .find_map(|step| match step {
            InstallStep::WriteVersionJson { content, .. } => Some(content),
            _ => None,
        })
        .expect("plan ends with a version json");
    let listed = json["libraries"].as_array().expect("libraries array");
    assert_eq!(listed.len(), downloads.len(), "JSON 与下载步骤一一对应");

    assert!(matches!(
        plan.steps.last(),
        Some(InstallStep::WriteVersionJson { path, .. })
            if path == "versions/fabric-loader-0.16.0-1.21.1/fabric-loader-0.16.0-1.21.1.json"
    ));
    assert_eq!(plan.steps.len(), 6);
}

#[test]
fn install_plan_passes_validation() {
    let plan = first_loader().install_plan("1.21.1", DEFAULT_MAVEN_BASE);

    assert_eq!(plan.validate(&[]), Ok(()));
    assert!(matches!(
        plan.validate(&["fabric-loader-0.16.0-1.21.1"]),
        Err(MetaError::VersionAlreadyInstalled { .. })
    ));
}

#[test]
fn install_plan_skips_libraries_without_a_repository() {
    let loader = parse_loader_versions("1.21.1", LOADER_VERSIONS)
        .unwrap()
        .remove(2);
    let plan = loader.install_plan("1.21.1", DEFAULT_MAVEN_BASE);

    let downloads = downloads(&plan);
    // 核心两件总有仓库地址；launcherMeta 里只有 org.ow2.asm:asm:7.0 有。
    assert_eq!(downloads.len(), 3, "两个核心库 + 有仓库地址的那一个");
    assert_eq!(
        downloads[0].1,
        "libraries/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar"
    );
    assert_eq!(downloads[2].1, "libraries/org/ow2/asm/asm/7.0/asm-7.0.jar");
    assert_eq!(
        downloads[2].0,
        "http://repo.maven.apache.org/maven2/org/ow2/asm/asm/7.0/asm-7.0.jar"
    );
}

#[test]
fn install_plan_accepts_a_mirror_maven_base() {
    let plan = first_loader().install_plan("1.21.1", "https://mirror.example.invalid/maven");

    let downloads = downloads(&plan);
    assert_eq!(
        downloads[0].0,
        "https://mirror.example.invalid/maven/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar",
        "核心库也跟着换成镜像地址"
    );
}

#[test]
fn install_plan_of_a_loader_without_libraries_only_writes_the_version_json() {
    let loader = parse_loader_versions("1.21.1", LOADER_VERSIONS)
        .unwrap()
        .remove(0)
        .to_owned();
    let bare = FabricLoaderVersion {
        libraries: Vec::new(),
        ..loader
    };

    let plan = bare.install_plan("1.21.1", DEFAULT_MAVEN_BASE);

    // 没有 launcherMeta 库，但核心两件仍要下载，否则版本 JSON 指不到文件。
    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.validate(&[]), Ok(()));
}

// ---- maven 相对路径 ----

#[test]
fn maven_relative_path_derives_the_maven_layout() {
    let cases = [
        (
            "org.ow2.asm:asm:9.6",
            Some("org/ow2/asm/asm/9.6/asm-9.6.jar"),
        ),
        (
            "net.fabricmc:intermediary:1.21.1",
            Some("net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar"),
        ),
        (
            "com.example:native:1.0:natives-windows",
            Some("com/example/native/1.0/native-1.0-natives-windows.jar"),
        ),
        (
            "com.example:archives:1.0@zip",
            Some("com/example/archives/1.0/archives-1.0.jar"),
        ),
        ("org.ow2.asm:asm", None),
        ("com.example:artifact:1.0:a:b", None),
        ("com.example:artifact:", None),
        ("com..example:artifact:1.0", None),
        ("com.example:../../etc:1.0", None),
        ("com.example:artifact:1.0:../x", None),
    ];

    for (coordinate, expected) in cases {
        assert_eq!(
            maven_relative_path(coordinate).as_deref(),
            expected,
            "坐标 {coordinate}"
        );
    }
}

// ---- 网络（全部走 StaticFetcher，不触网） ----

#[test]
fn fetch_game_versions_requests_the_exact_url() {
    let url = "https://meta.fabricmc.net/v2/versions/game";
    let source = MetaSource::new(
        DEFAULT_META_BASE,
        StaticFetcher::new().with(url, GAME_VERSIONS),
    );

    let versions = fetch_game_versions(&source).unwrap();

    assert_eq!(versions.len(), 4);
    assert_eq!(versions[1].version, "1.21.1");
}

#[test]
fn fetch_loader_versions_requests_the_exact_url() {
    let url = "https://meta.fabricmc.net/v2/versions/loader/1.21.1";
    let source = MetaSource::new(
        DEFAULT_META_BASE,
        StaticFetcher::new().with(url, LOADER_VERSIONS),
    );

    let versions = fetch_loader_versions(&source, "1.21.1").unwrap();

    assert_eq!(versions.len(), 3);
    assert_eq!(versions[0].loader_version, "0.16.0");
}

#[test]
fn fetch_reports_an_error_for_an_unregistered_url() {
    let source = MetaSource::new(DEFAULT_META_BASE, StaticFetcher::new());

    let game_error = fetch_game_versions(&source).unwrap_err();
    assert!(matches!(game_error, MetaError::Transport { .. }));
    assert_eq!(
        game_error.url(),
        Some("https://meta.fabricmc.net/v2/versions/game")
    );

    let loader_error = fetch_loader_versions(&source, "1.21.1").unwrap_err();
    assert!(matches!(loader_error, MetaError::Transport { .. }));
    assert_eq!(
        loader_error.url(),
        Some("https://meta.fabricmc.net/v2/versions/loader/1.21.1")
    );
}

#[test]
fn fetch_surfaces_malformed_json_from_the_source() {
    let url = "https://meta.fabricmc.net/v2/versions/game";
    let source = MetaSource::new(DEFAULT_META_BASE, StaticFetcher::new().with(url, "<html>"));

    assert!(matches!(
        fetch_game_versions(&source),
        Err(MetaError::InvalidJson { .. })
    ));
}
