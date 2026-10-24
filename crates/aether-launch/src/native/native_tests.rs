use std::path::PathBuf;

use aether_download::DownloadPlanError;
use aether_manifest::parse_version_manifest;
use aether_manifest::rules::Environment;

use super::{NativePlanError, build_native_plan};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn environment(os: &str, arch: &str) -> Environment {
    Environment {
        os_name: os.to_owned(),
        os_arch: arch.to_owned(),
        ..Environment::default()
    }
}

#[test]
fn selects_legacy_native_classifier_by_windows_architecture() {
    let json = format!(r#"{{
        "id":"1.8.8",
        "libraries":[{{
            "name":"tv.twitch:twitch-external-platform:4.5",
            "natives":{{"windows":"natives-windows-${{arch}}"}},
            "extract":{{"exclude":["META-INF/"]}},
            "downloads":{{"classifiers":{{
                "natives-windows-32":{{"path":"tv/twitch/platform-32.jar","sha1":"{SHA}","size":30,"url":"https://example.invalid/32"}},
                "natives-windows-64":{{"path":"tv/twitch/platform-64.jar","sha1":"{SHA}","size":60,"url":"https://example.invalid/64"}}
            }}}}
        }}]
    }}"#);
    let manifest = parse_version_manifest(&json).unwrap();

    let x64 = build_native_plan(&manifest, &environment("windows", "x86_64")).unwrap();
    assert_eq!(x64.archives.len(), 1);
    assert_eq!(x64.archives[0].classifier, "natives-windows-64");
    assert_eq!(
        x64.download_plan().artifacts[0].destination,
        PathBuf::from("libraries/tv/twitch/platform-64.jar")
    );
    assert_eq!(x64.archives[0].exclusions, ["META-INF/"]);

    let x86 = build_native_plan(&manifest, &environment("windows", "x86")).unwrap();
    assert_eq!(x86.archives[0].classifier, "natives-windows-32");
    assert_eq!(x86.download_plan().artifacts[0].size, 30);

    let linux = build_native_plan(&manifest, &environment("linux", "x86_64")).unwrap();
    assert!(linux.archives.is_empty());
}

#[test]
fn modern_classified_lwjgl_is_platform_specific() {
    let json = format!(r#"{{
        "id":"1.21",
        "libraries":[
            {{"name":"org.lwjgl:lwjgl:3.3.3:natives-windows","downloads":{{"artifact":{{
                "path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar",
                "sha1":"{SHA}","size":42,"url":"https://example.invalid/windows"
            }}}}}},
            {{"name":"org.lwjgl:lwjgl:3.3.3:natives-linux","downloads":{{"artifact":{{
                "path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar",
                "sha1":"{SHA}","size":43,"url":"https://example.invalid/linux"
            }}}}}},
            {{"name":"org.lwjgl:lwjgl:3.3.3:natives-windows-arm64","downloads":{{"artifact":{{
                "path":"org/lwjgl/arm.jar","sha1":"{SHA}","size":44,"url":"https://example.invalid/arm"
            }}}}}}
        ]
    }}"#);
    let manifest = parse_version_manifest(&json).unwrap();
    let win = build_native_plan(&manifest, &environment("windows", "x86_64")).unwrap();
    assert_eq!(win.archives.len(), 1);
    assert_eq!(win.archives[0].classifier, "natives-windows");
    let arm = build_native_plan(&manifest, &environment("windows", "arm64")).unwrap();
    assert_eq!(arm.archives.len(), 2);
    let linux = build_native_plan(&manifest, &environment("linux", "x86_64")).unwrap();
    assert_eq!(linux.archives.len(), 1);
    assert_eq!(linux.archives[0].classifier, "natives-linux");
}

#[test]
fn respects_library_platform_rules_and_missing_native_metadata() {
    let manifest = parse_version_manifest(r#"{
        "id":"v","libraries":[
            {"name":"org.lwjgl:lwjgl:3.3:natives-windows",
             "rules":[{"action":"allow","os":{"name":"linux"}}]}
        ]
    }"#).unwrap();
    assert!(build_native_plan(&manifest, &environment("windows", "x86_64"))
        .unwrap().archives.is_empty());

    let missing = parse_version_manifest(r#"{
        "id":"v","libraries":[{"name":"group:artifact:1",
        "natives":{"windows":"natives-windows"}}]
    }"#).unwrap();
    assert!(matches!(
        build_native_plan(&missing, &environment("windows", "x86_64")),
        Err(NativePlanError::MissingMetadata { .. })
    ));
}

#[test]
fn rejects_unsafe_artifact_paths_and_exclusions() {
    let json = format!(r#"{{
        "id":"v","libraries":[{{"name":"a:b:1:natives-windows",
        "downloads":{{"artifact":{{"path":"../escape.jar","sha1":"{SHA}",
            "size":1,"url":"https://example.invalid"}}}}}}
        ]
    }}"#);
    let manifest = parse_version_manifest(&json).unwrap();
    assert!(matches!(
        build_native_plan(&manifest, &environment("windows", "x86_64")),
        Err(NativePlanError::UnsafePath(_))
    ));
    let json = format!(r#"{{
        "id":"v","libraries":[{{"name":"a:b:1:natives-windows",
        "extract":{{"exclude":["../outside"]}},
        "downloads":{{"artifact":{{"path":"ok.jar","sha1":"{SHA}",
            "size":1,"url":"https://example.invalid"}}}}}}
        ]
    }}"#);
    let manifest = parse_version_manifest(&json).unwrap();
    assert_eq!(
        build_native_plan(&manifest, &environment("windows", "x86_64")),
        Err(NativePlanError::InvalidExclusion("../outside".to_owned()))
    );
}

#[test]
fn rejects_invalid_checksums_and_duplicate_destinations() {
    let json = r#"{
        "id":"v","libraries":[{"name":"a:b:1:natives-windows",
        "downloads":{"artifact":{"path":"x.jar","sha1":"bad","size":1,"url":"x"}}}]
    }"#;
    let manifest = parse_version_manifest(json).unwrap();
    assert!(matches!(
        build_native_plan(&manifest, &environment("windows", "x86_64")),
        Err(NativePlanError::Download(DownloadPlanError::InvalidSha1(_)))
    ));

    let json = format!(r#"{{
        "id":"v","libraries":[
            {{"name":"a:b:1:natives-windows","downloads":{{"artifact":{{
                "path":"a.jar","sha1":"{SHA}","size":1,"url":"x"}}}}}},
            {{"name":"c:d:2:natives-windows","downloads":{{"artifact":{{
                "path":"A.jar","sha1":"{SHA}","size":1,"url":"x"}}}}}}
        ]
    }}"#);
    let manifest = parse_version_manifest(&json).unwrap();
    assert!(matches!(
        build_native_plan(&manifest, &environment("windows", "x86_64")),
        Err(NativePlanError::Download(DownloadPlanError::DuplicateDestination(_)))
    ));
}

#[test]
fn unknown_architecture_does_not_guess_a_legacy_classifier() {
    let manifest = parse_version_manifest(r#"{
        "id":"v","libraries":[{"name":"a:b:1",
        "natives":{"windows":"natives-windows-${arch}"}}]
    }"#).unwrap();
    assert_eq!(
        build_native_plan(&manifest, &environment("windows", "unknown")),
        Err(NativePlanError::UnsupportedArchitecture("unknown".to_owned()))
    );
}
