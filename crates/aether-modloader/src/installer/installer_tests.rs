//! `installer.rs` 的单元测试：全部是纯逻辑检查，不访问网络、不读写文件。

use crate::ModLoaderKind;
use crate::installer::{InstallStep, MAX_VERSION_ID_LEN, ModLoaderInstallPlan, is_safe_version_id};
use crate::transport::MetaError;

const FABRIC: &str = "net.fabricmc.loader.impl.launch.knot.KnotClient";
const FABRIC_MAVEN: &str = "https://maven.fabricmc.net/";
const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";

fn unsafe_id(id: &str, reason: &'static str) -> MetaError {
    MetaError::UnsafeVersionId {
        id: id.to_owned(),
        reason,
    }
}

fn download(destination: &str) -> InstallStep {
    InstallStep::Download {
        url: "https://example.invalid/a.jar".to_owned(),
        destination: destination.to_owned(),
        sha1: Some(SHA1.to_owned()),
        size: Some(1024),
        label: "asm".to_owned(),
    }
}

/// 最小可用的计划：一个下载 + 一个版本 json。
fn minimal_plan(target: &str) -> ModLoaderInstallPlan {
    ModLoaderInstallPlan::new(ModLoaderKind::Fabric, "0.16.0", "1.21.1")
        .with_target(target)
        .with_main_class(FABRIC)
        .push(download("libraries/org/ow2/asm/asm/9.6/asm-9.6.jar"))
        .push(InstallStep::WriteVersionJson {
            path: format!("versions/{target}/{target}.json"),
            content: serde_json::json!({"id": target, "inheritsFrom": "1.21.1"}),
        })
}

// ---------------------------------------------------------------- is_safe_version_id

#[test]
fn rejects_an_empty_version_id() {
    let expected = unsafe_id("", "version identifier is empty");
    assert_eq!(is_safe_version_id(""), Err(expected));
}

#[test]
fn rejects_a_version_id_longer_than_the_limit() {
    let too_long = "a".repeat(MAX_VERSION_ID_LEN + 1);
    let expected = unsafe_id(&too_long, "version identifier is too long");
    assert_eq!(is_safe_version_id(&too_long), Err(expected));
}

#[test]
fn rejects_path_separators() {
    let cases = ["fabric/1.21.1", "fabric\\1.21.1", "a/b/c", "a\\b\\c", "/"];
    for id in cases {
        let expected = unsafe_id(id, "version identifier contains a path separator");
        assert_eq!(is_safe_version_id(id), Err(expected), "{id:?}");
    }
}

#[test]
fn rejects_control_characters() {
    for id in ["1.21.1\t", "1.21\n1", "1.21.1\u{7f}", "\u{1}loader"] {
        let expected = unsafe_id(id, "version identifier contains a control character");
        assert_eq!(is_safe_version_id(id), Err(expected), "{id:?}");
    }
}

#[test]
fn rejects_a_trailing_dot_or_space() {
    let dot = unsafe_id("1.21.1.", "version identifier ends with a dot or space");
    assert_eq!(is_safe_version_id("1.21.1."), Err(dot));

    let space = unsafe_id("1.21.1 ", "version identifier ends with a dot or space");
    assert_eq!(is_safe_version_id("1.21.1 "), Err(space));
}

#[test]
fn rejects_the_relative_path_components() {
    let dot = unsafe_id(".", "version identifier is a relative path component");
    assert_eq!(is_safe_version_id("."), Err(dot));

    let dot_dot = unsafe_id("..", "version identifier is a relative path component");
    assert_eq!(is_safe_version_id(".."), Err(dot_dot));
}

#[test]
fn rejects_windows_reserved_device_names() {
    for id in [
        "CON", "con.txt", "COM5.sys", "LPT9", "NUL", "aux", "Prn.log", "COM1", "lpt1",
    ] {
        let expected = unsafe_id(id, "version identifier is a reserved device name");
        assert_eq!(is_safe_version_id(id), Err(expected), "{id:?}");
    }
}

#[test]
fn accepts_realistic_version_ids() {
    let valid = [
        "fabric-loader-0.16.0-1.21.1",
        "1.20.1-forge-47.4.28",
        "neoforge-20.4.237",
        "quilt-loader-0.20.0-beta.9-1.21.1",
        "1.21.1",
        "26.4-snapshot-5",
        // 保留名只拦「整段」与「点之前的部分」，这些不是保留名
        "console",
        "COM0",
        "COM10",
        "nexus.1.0",
    ];
    for id in valid {
        assert_eq!(is_safe_version_id(id), Ok(()), "{id:?}");
    }
}

#[test]
fn accepts_a_version_id_at_the_length_limit() {
    let id = "a".repeat(MAX_VERSION_ID_LEN);
    assert_eq!(id.len(), 63);
    assert_eq!(is_safe_version_id(&id), Ok(()));
}

// ---------------------------------------------------------------- ModLoaderInstallPlan

#[test]
fn new_sets_inherits_from_and_leaves_target_empty() {
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::Forge, "47.4.28", "1.20.1");

    assert_eq!(plan.kind, ModLoaderKind::Forge);
    assert_eq!(plan.loader_version, "47.4.28");
    assert_eq!(plan.game_version, "1.20.1");
    assert_eq!(plan.inherits_from, "1.20.1");
    assert_eq!(plan.target_version_id, "");
    assert_eq!(plan.main_class, "");
    assert!(plan.steps.is_empty());
    // 未设置 target 的计划不可能通过校验
    assert_eq!(
        plan.validate(&[]),
        Err(unsafe_id("", "version identifier is empty"))
    );
}

#[test]
fn builders_are_chainable_and_keep_step_order() {
    let plan = minimal_plan("fabric-loader-0.16.0-1.21.1");

    assert_eq!(plan.target_version_id, "fabric-loader-0.16.0-1.21.1");
    assert_eq!(plan.main_class, FABRIC);
    assert_eq!(plan.steps.len(), 2);
    assert!(plan.steps[0].is_download());
    assert!(matches!(
        plan.steps[1],
        InstallStep::WriteVersionJson { .. }
    ));
}

#[test]
fn validate_accepts_a_minimal_plan() {
    let plan = minimal_plan("fabric-loader-0.16.0-1.21.1");
    assert_eq!(plan.validate(&[]), Ok(()));
}

#[test]
fn validate_rejects_an_invalid_target_id() {
    let plan = minimal_plan("bad/../id");
    let expected = unsafe_id("bad/../id", "version identifier contains a path separator");
    assert_eq!(plan.validate(&[]), Err(expected));
}

#[test]
fn validate_rejects_an_already_installed_version() {
    let plan = minimal_plan("fabric-loader-0.16.0-1.21.1");
    let existing = ["1.21.1", "fabric-loader-0.16.0-1.21.1"];
    let expected = MetaError::VersionAlreadyInstalled {
        id: "fabric-loader-0.16.0-1.21.1".to_owned(),
    };
    assert_eq!(plan.validate(&existing), Err(expected));
}

#[test]
fn validate_checks_the_target_before_the_collision() {
    // id 非法优先报 UnsafeVersionId，即使 existing 里出现了同样的字符串
    let plan = minimal_plan("CON");
    let existing = ["CON"];
    let expected = unsafe_id("CON", "version identifier is a reserved device name");
    assert_eq!(plan.validate(&existing), Err(expected));
}

#[test]
fn validate_rejects_an_empty_plan() {
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::Quilt, "0.20.0-beta.9", "1.21.1")
        .with_target("quilt-loader-0.20.0-beta.9-1.21.1")
        .with_main_class("org.quiltmc.loader.impl.launch.knot.KnotClient");
    assert_eq!(plan.validate(&[]), Err(MetaError::EmptyPlan));
}

#[test]
fn validate_rejects_an_absolute_download_destination() {
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::Fabric, "0.16.0", "1.21.1")
        .with_target("fabric-loader-0.16.0-1.21.1")
        .push(download("C:\\Windows\\asm-9.6.jar"));
    let expected = unsafe_id(
        "C:\\Windows\\asm-9.6.jar",
        "destination path is not a safe relative path",
    );
    assert_eq!(plan.validate(&[]), Err(expected));

    let plan = ModLoaderInstallPlan::new(ModLoaderKind::Fabric, "0.16.0", "1.21.1")
        .with_target("fabric-loader-0.16.0-1.21.1")
        .push(download("/opt/minecraft/asm-9.6.jar"));
    let expected = unsafe_id(
        "/opt/minecraft/asm-9.6.jar",
        "destination path is not a safe relative path",
    );
    assert_eq!(plan.validate(&[]), Err(expected));
}

#[test]
fn validate_rejects_parent_directory_segments() {
    for destination in [
        "../outside.jar",
        "libraries/../../outside.jar",
        "libraries\\..\\libraries\\a.jar",
        "./a.jar",
        "libraries//a.jar",
        "libraries/",
    ] {
        let plan = ModLoaderInstallPlan::new(ModLoaderKind::Fabric, "0.16.0", "1.21.1")
            .with_target("fabric-loader-0.16.0-1.21.1")
            .push(download(destination));
        let expected = unsafe_id(destination, "destination path is not a safe relative path");
        assert_eq!(plan.validate(&[]), Err(expected), "{destination:?}");
    }
}

#[test]
fn validate_rejects_reserved_names_in_destination_segments() {
    for destination in [
        "libraries/CON.jar",
        "versions/con/a.jar",
        "libraries/com1/a.jar",
    ] {
        let plan = ModLoaderInstallPlan::new(ModLoaderKind::Fabric, "0.16.0", "1.21.1")
            .with_target("fabric-loader-0.16.0-1.21.1")
            .push(download(destination));
        let expected = unsafe_id(destination, "destination path is not a safe relative path");
        assert_eq!(plan.validate(&[]), Err(expected), "{destination:?}");
    }
}

#[test]
fn validate_checks_write_version_json_and_write_file_paths() {
    let json_path = "versions/a/../b.json";
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::NeoForge, "20.4.237", "1.20.4")
        .with_target("neoforge-20.4.237")
        .push(InstallStep::WriteVersionJson {
            path: json_path.to_owned(),
            content: serde_json::json!({"id": "neoforge-20.4.237"}),
        });
    let expected = unsafe_id(json_path, "destination path is not a safe relative path");
    assert_eq!(plan.validate(&[]), Err(expected));

    let write_path = "C:notes.txt";
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::NeoForge, "20.4.237", "1.20.4")
        .with_target("neoforge-20.4.237")
        .push(InstallStep::WriteFile {
            path: write_path.to_owned(),
            content: "x".to_owned(),
        });
    let expected = unsafe_id(write_path, "destination path is not a safe relative path");
    assert_eq!(plan.validate(&[]), Err(expected));
}

#[test]
fn validate_checks_copy_file_target_and_installer_output_dir() {
    let target = "versions/../a.jar";
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::Forge, "47.4.28", "1.20.1")
        .with_target("1.20.1-forge-47.4.28")
        .push(InstallStep::CopyFile {
            from: "libraries/a.jar".to_owned(),
            to: target.to_owned(),
        });
    let expected = unsafe_id(target, "destination path is not a safe relative path");
    assert_eq!(plan.validate(&[]), Err(expected));

    let output_dir = "\\server";
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::Forge, "47.4.28", "1.20.1")
        .with_target("1.20.1-forge-47.4.28")
        .push(InstallStep::RunInstaller {
            jar_path: "libraries/forge-installer.jar".to_owned(),
            args: vec!["--installClient".to_owned()],
            output_dir: Some(output_dir.to_owned()),
        });
    let expected = unsafe_id(output_dir, "destination path is not a safe relative path");
    assert_eq!(plan.validate(&[]), Err(expected));
}

#[test]
fn validate_accepts_a_relative_installer_output_dir() {
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::Forge, "47.4.28", "1.20.1")
        .with_target("1.20.1-forge-47.4.28")
        .push(InstallStep::RunInstaller {
            jar_path: "libraries/forge-installer.jar".to_owned(),
            args: vec!["--installClient".to_owned()],
            output_dir: Some("instances/demo".to_owned()),
        })
        .push(InstallStep::CopyLibraries {
            repository: FABRIC_MAVEN.to_owned(),
            coordinates: vec!["org.ow2.asm:asm:9.6".to_owned()],
        });
    assert_eq!(plan.validate(&[]), Ok(()));
}

/// 手工搭一个 Fabric 风格的完整计划，覆盖所有变体，并确认能通过校验。
#[test]
fn hand_built_fabric_plan_passes_validation() {
    let target = "fabric-loader-0.16.0-1.21.1";
    let plan = ModLoaderInstallPlan::new(ModLoaderKind::Fabric, "0.16.0", "1.21.1")
        .with_target(target)
        .with_main_class(FABRIC)
        .push(InstallStep::Download {
            url: format!("{FABRIC_MAVEN}net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar"),
            destination: "libraries/net/fabricmc/intermediary/1.21.1/intermediary-1.21.1.jar"
                .to_owned(),
            sha1: Some(SHA1.to_owned()),
            size: Some(1_234_567),
            label: "intermediary".to_owned(),
        })
        .push(InstallStep::Download {
            url: format!(
                "{FABRIC_MAVEN}net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar"
            ),
            destination: "libraries/net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar"
                .to_owned(),
            sha1: Some(SHA1.to_owned()),
            size: Some(2_000_000),
            label: "fabric-loader".to_owned(),
        })
        .push(InstallStep::Download {
            url: format!("{FABRIC_MAVEN}org/ow2/asm/asm/9.6/asm-9.6.jar"),
            destination: "libraries/org/ow2/asm/asm/9.6/asm-9.6.jar".to_owned(),
            sha1: None,
            size: None,
            label: "asm".to_owned(),
        })
        .push(InstallStep::CopyLibraries {
            repository: FABRIC_MAVEN.to_owned(),
            coordinates: vec![
                "net.fabricmc:intermediary:1.21.1".to_owned(),
                "net.fabricmc:fabric-loader:0.16.0".to_owned(),
            ],
        })
        .push(InstallStep::CopyFile {
            from: "libraries/net/fabricmc/fabric-loader/0.16.0/fabric-loader-0.16.0.jar".to_owned(),
            to: format!("versions/{target}/fabric-loader.jar"),
        })
        .push(InstallStep::WriteVersionJson {
            path: format!("versions/{target}/{target}.json"),
            content: serde_json::json!({
                "id": target,
                "inheritsFrom": "1.21.1",
                "type": "release",
                "mainClass": FABRIC,
                "libraries": [
                    {"name": "net.fabricmc:intermediary:1.21.1", "url": FABRIC_MAVEN}
                ]
            }),
        })
        .push(InstallStep::WriteFile {
            path: format!("versions/{target}/notes.txt"),
            content: "安装完成".to_owned(),
        });

    assert_eq!(plan.steps.len(), 7);
    assert_eq!(plan.inherits_from, "1.21.1");
    assert_eq!(plan.validate(&["1.21.1"]), Ok(()));
}

// ---------------------------------------------------------------- InstallStep

#[test]
fn describe_covers_every_variant() {
    // `needles` 用切片而不是定长数组：每个变体的期望片段数量不同，
    // 定长数组会要求所有元素同为 `[&str; N]`。断言语义保持不变。
    let steps: [(InstallStep, &[&str]); 6] = [
        (
            download("libraries/org/ow2/asm/asm/9.6/asm-9.6.jar"),
            &["下载", "asm", "libraries/org/ow2/asm/asm/9.6/asm-9.6.jar"],
        ),
        (
            InstallStep::RunInstaller {
                jar_path: "libraries/forge-installer.jar".to_owned(),
                args: vec!["--installClient".to_owned()],
                output_dir: None,
            },
            &["运行安装器", "libraries/forge-installer.jar"],
        ),
        (
            InstallStep::WriteVersionJson {
                path: "versions/fabric-loader-0.16.0-1.21.1/fabric-loader-0.16.0-1.21.1.json"
                    .to_owned(),
                content: serde_json::json!({"id": "x"}),
            },
            &[
                "写入版本文件",
                "versions/fabric-loader-0.16.0-1.21.1/fabric-loader-0.16.0-1.21.1.json",
            ],
        ),
        (
            InstallStep::CopyLibraries {
                repository: FABRIC_MAVEN.to_owned(),
                coordinates: vec![
                    "org.ow2.asm:asm:9.6".to_owned(),
                    "net.fabricmc:fabric-loader:0.16.0".to_owned(),
                ],
            },
            &["从", FABRIC_MAVEN, "复制 2 个库"],
        ),
        (
            InstallStep::CopyFile {
                from: "libraries/a.jar".to_owned(),
                to: "versions/a/a.jar".to_owned(),
            },
            &["复制", "libraries/a.jar", "versions/a/a.jar"],
        ),
        (
            InstallStep::WriteFile {
                path: "versions/a/notes.txt".to_owned(),
                content: "x".to_owned(),
            },
            &["写入", "versions/a/notes.txt"],
        ),
    ];

    for (step, needles) in &steps {
        let described = step.describe();
        assert!(!described.contains('\n'), "{described:?}");
        for needle in *needles {
            assert!(described.contains(needle), "{described:?} 缺少 {needle:?}");
        }
    }
}

#[test]
fn is_download_is_true_only_for_downloads() {
    assert!(download("libraries/a.jar").is_download());
    let others = [
        InstallStep::RunInstaller {
            jar_path: "libraries/a-installer.jar".to_owned(),
            args: Vec::new(),
            output_dir: None,
        },
        InstallStep::WriteVersionJson {
            path: "versions/a/a.json".to_owned(),
            content: serde_json::json!({}),
        },
        InstallStep::CopyLibraries {
            repository: FABRIC_MAVEN.to_owned(),
            coordinates: Vec::new(),
        },
        InstallStep::CopyFile {
            from: "a.jar".to_owned(),
            to: "b.jar".to_owned(),
        },
        InstallStep::WriteFile {
            path: "versions/a/notes.txt".to_owned(),
            content: String::new(),
        },
    ];
    for step in &others {
        assert!(!step.is_download(), "{step:?}");
    }
}

#[test]
fn destination_covers_every_variant() {
    assert_eq!(
        download("libraries/a.jar").destination(),
        Some("libraries/a.jar")
    );
    assert_eq!(
        InstallStep::WriteVersionJson {
            path: "versions/a/a.json".to_owned(),
            content: serde_json::json!({}),
        }
        .destination(),
        Some("versions/a/a.json")
    );
    assert_eq!(
        InstallStep::WriteFile {
            path: "versions/a/notes.txt".to_owned(),
            content: String::new(),
        }
        .destination(),
        Some("versions/a/notes.txt")
    );
    // 只报告写入端，不报告源文件
    assert_eq!(
        InstallStep::CopyFile {
            from: "libraries/a.jar".to_owned(),
            to: "versions/a/a.jar".to_owned(),
        }
        .destination(),
        Some("versions/a/a.jar")
    );
    assert_eq!(
        InstallStep::CopyLibraries {
            repository: FABRIC_MAVEN.to_owned(),
            coordinates: vec!["org.ow2.asm:asm:9.6".to_owned()],
        }
        .destination(),
        None
    );
    assert_eq!(
        InstallStep::RunInstaller {
            jar_path: "libraries/a-installer.jar".to_owned(),
            args: Vec::new(),
            output_dir: None,
        }
        .destination(),
        None
    );
}
