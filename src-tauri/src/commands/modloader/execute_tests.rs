//! `execute` 的单元测试。
//!
//! 这里只测**不联网、不启动进程**的部分：路径约束、maven 坐标解析、错误文案截断。
//! 真正下载和跑 installer 依赖官方源与网络，留在手工验证里。

use super::*;

#[test]
fn rejects_destinations_that_escape_the_game_root() {
    let root = std::path::Path::new("C:/game");

    assert!(safe_target(root, "versions/1.21.1/1.21.1.json").is_ok());

    // 上跳两级就会跳出 game 根目录。
    assert!(safe_target(root, "../../evil.json").is_err());
    // 绝对路径同样拒绝。
    assert!(safe_target(root, "C:/Windows/evil.dll").is_err());
    // 反斜杠也要当成分隔符处理，否则 Windows 上能绕过去。
    assert!(safe_target(root, "..\\..\\evil.json").is_err());
}

#[test]
fn builds_maven_paths_for_plain_and_classified_artifacts() {
    assert_eq!(
        maven_relative_path("net.fabricmc:fabric-loader:0.16.0").as_deref(),
        Some("net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar")
    );
    assert_eq!(
        maven_relative_path("net.minecraftforge:installer:1.20.1-47.4.0:shrunk").as_deref(),
        Some("net/minecraftforge/installer/1.20.1-47.4.0/installer-1.20.1-47.4.0-shrunk.jar")
    );
}

#[test]
fn rejects_malformed_maven_coordinates() {
    // 段数不对。
    assert!(maven_relative_path("net.fabricmc").is_none());
    assert!(maven_relative_path("net.fabricmc:fabric-loader").is_none());
    assert!(maven_relative_path("a:b:c:d:e").is_none());
    // 空段。
    assert!(maven_relative_path(":artifact:1.0").is_none());
    assert!(maven_relative_path("group::1.0").is_none());
    // 路径穿越。
    assert!(maven_relative_path("group:../evil:1.0").is_none());
    assert!(maven_relative_path("group:artifact:../../../etc/passwd").is_none());
}

#[test]
fn tail_keeps_the_end_of_a_long_message() {
    let text = "a".repeat(1000);
    let kept = tail(&text, 10);
    assert_eq!(kept.chars().count(), 10);

    // 短消息原样返回，并且去掉首尾空白。
    assert_eq!(tail("  short  ", 100), "short");
}

#[test]
fn tail_counts_characters_not_bytes() {
    // 600 个汉字是 1800 字节，按字节截会切坏 UTF-8。
    let text = "汉".repeat(700);
    let kept = tail(&text, 600);
    assert_eq!(kept.chars().count(), 600);
}

#[test]
fn quilt_is_reported_as_unsupported_before_any_io() {
    let error = build_plan(ModLoaderKind::Quilt, "1.21.1", "0.27.0").unwrap_err();
    assert!(error.contains("Quilt"), "unexpected error: {error}");
}

#[test]
fn every_downloaded_library_is_a_safe_destination() {
    // 计划的每个写入目标都要能通过 `safe_target`，否则执行时会中途失败。
    let game_root = std::path::Path::new("C:/game");
    for coordinate in [
        "net.fabricmc:intermediary:1.21.1",
        "net.fabricmc:fabric-loader:0.20.1",
        "org.ow2.asm:asm:9.10.1",
    ] {
        let relative = maven_relative_path(coordinate).expect("valid coordinate");
        assert!(
            safe_target(game_root, &format!("libraries/{relative}")).is_ok(),
            "rejected: {relative}"
        );
    }
}

/// 不关心进度的 reporter。
struct SilentReporter;

impl Reporter for SilentReporter {
    fn report(&self, _progress: &InstallProgress) {}
}

/// 真联网、真写盘的端到端验证，默认跳过。
///
/// ```text
/// cargo test -p aether-desktop --lib -- --ignored live_install
/// ```
///
/// 这条用例存在的意义是：下载校验、路径约束和实例创建都改过之后，
/// 仍要能拿官方源把一个加载器版本真正装出来。它依赖网络和官方 maven，
/// 所以不进默认的测试集合。
#[test]
#[ignore = "needs the official maven and a network connection"]
fn live_install_downloads_verifies_and_creates_an_instance() {
    let base = std::env::temp_dir().join(format!("aether-live-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create temp base");

    let transport = KernelHttpTransport::new(UreqKernel::new());
    let cancel = CancellationToken::new();

    let result = install_with(
        &transport,
        InstallRequest {
            base: &base,
            kind: ModLoaderKind::Fabric,
            game_version: "26.1.2",
            loader_version: "0.20.1",
            name: "live-e2e",
        },
        &cancel,
        &SilentReporter,
    )
    .expect("a real fabric install should succeed");

    // 版本 JSON 落在游戏树里。
    let version_json = base
        .join("game")
        .join("versions")
        .join(&result.version_id)
        .join(format!("{}.json", result.version_id));
    assert!(
        version_json.is_file(),
        "version json missing: {}",
        version_json.display()
    );

    // fabric-loader 的 jar 必须真的下下来了 —— 这一步以前是漏的。
    let loader_jar = base
        .join("game")
        .join("libraries")
        .join("net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar");
    let size = std::fs::metadata(&loader_jar)
        .map(|meta| meta.len())
        .unwrap_or_default();
    assert!(
        size > 1_000_000,
        "fabric-loader jar looks wrong: {size} bytes at {}",
        loader_jar.display()
    );

    // 实例也建好了，而且用的是刚建出来的那个 id。
    let record = base.join("instances").join(result.instance.id());
    assert!(
        record.is_dir(),
        "instance record missing: {}",
        record.display()
    );

    let _ = std::fs::remove_dir_all(&base);
}
