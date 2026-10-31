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
fn the_path_rule_is_the_one_the_vanilla_installer_uses() {
    let root = std::path::Path::new("C:/game");

    // 原版安装拒绝、加载器以前会放过的写法：空段、`.`、结尾的点、数据流、保留名。
    for bad in [
        "libraries//x.jar",
        "libraries/./x.jar",
        "libraries/x.jar.",
        "libraries/x.jar:stream",
        "libraries/CON/x.jar",
        "libraries/a<b>.jar",
    ] {
        assert!(safe_target(root, bad).is_err(), "should be refused: {bad}");
    }
    assert!(
        safe_target(
            root,
            "libraries/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar"
        )
        .is_ok()
    );
}

#[test]
fn copying_libraries_from_a_local_cache_is_refused() {
    let error = copy_libraries(
        "https://maven.example.invalid/",
        &["net.fabricmc:fabric-loader:0.20.1".to_owned()],
    )
    .unwrap_err();
    assert!(error.contains("官方源"), "{error}");
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

/// 真联网、真起 Java：按 installer 的描述做一次完整的客户端安装。
///
/// 先准备原版（版本 JSON 和客户端 jar，处理器要给它打补丁），再把计划里的步骤逐条执行：
/// 下载并校验 installer，然后读它的描述、下载并校验全部库、按顺序跑处理器。
fn live_profile_install(
    kind: ModLoaderKind,
    game_version: &str,
    loader_version: &str,
    expected_id: &str,
    patched_client: &str,
) {
    let base =
        std::env::temp_dir().join(format!("aether-live-{expected_id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let game_root = base.join("game");
    std::fs::create_dir_all(&game_root).expect("create game root");

    let transport = KernelHttpTransport::new(UreqKernel::new());
    let cancel = CancellationToken::new();

    crate::vanilla::prepare_version(&transport, &game_root, game_version)
        .expect("vanilla metadata");
    let resolved = aether_manifest::load_resolved_version_manifest(&game_root, game_version)
        .expect("resolved manifest");
    let client = resolved
        .manifest
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.client.as_ref())
        .expect("client download");
    download_into(
        &transport,
        &game_root,
        &client.url,
        &format!("versions/{game_version}/{game_version}.jar"),
        Some(&client.sha1),
        Some(client.size),
    )
    .expect("vanilla client jar");

    let plan = build_plan(kind, game_version, loader_version).expect("plan");
    let mut installed = None;
    for step in &plan.steps {
        if let Some(id) = step_execute(&transport, &game_root, step, &cancel)
            .unwrap_or_else(|error| panic!("{} failed: {error}", step.describe()))
        {
            installed = Some(id);
        }
    }

    assert_eq!(installed.as_deref(), Some(expected_id));
    assert!(
        game_root
            .join(format!("versions/{expected_id}/{expected_id}.json"))
            .is_file()
    );
    // 最后一个处理器（binarypatcher）的输出：打过补丁的客户端。
    let patched = game_root.join(patched_client);
    let size = std::fs::metadata(&patched)
        .map(|meta| meta.len())
        .unwrap_or_default();
    assert!(size > 1_000_000, "patched client looks wrong: {size} bytes");

    let _ = std::fs::remove_dir_all(&base);
}

/// Forge 1.20.1。默认跳过：
///
/// ```text
/// cargo test -p aether-desktop --lib -- --ignored live_forge
/// ```
#[test]
#[ignore = "needs the official mavens, a network connection and a Java 17+"]
fn live_forge_client_install_runs_the_installer_profile() {
    live_profile_install(
        ModLoaderKind::Forge,
        "1.20.1",
        "47.4.0",
        "1.20.1-forge-47.4.0",
        "libraries/net/minecraftforge/forge/1.20.1-47.4.0/forge-1.20.1-47.4.0-client.jar",
    );
}

/// NeoForge 21.1.72（Minecraft 1.21.1）。默认跳过：
///
/// ```text
/// cargo test -p aether-desktop --lib -- --ignored live_neoforge
/// ```
#[test]
#[ignore = "needs the official mavens, a network connection and a Java 21+"]
fn live_neoforge_client_install_runs_the_installer_profile() {
    live_profile_install(
        ModLoaderKind::NeoForge,
        "1.21.1",
        "21.1.72",
        "neoforge-21.1.72",
        "libraries/net/neoforged/neoforge/21.1.72/neoforge-21.1.72-client.jar",
    );
}
