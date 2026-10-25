use std::path::PathBuf;

use aether_manifest::parse_version_manifest;
use aether_manifest::rules::Environment;

use super::{InstallPlanError, build_install_plan};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn windows() -> Environment {
    Environment {
        os_name: "windows".to_owned(),
        os_arch: "x86_64".to_owned(),
        ..Environment::default()
    }
}

fn manifest(libraries: &str, client_url: &str) -> aether_manifest::VersionManifest {
    let json = format!(
        r#"{{
        "id":"1.21",
        "downloads":{{"client":{{"url":"{client_url}","sha1":"{SHA}","size":10}}}},
        "libraries":[{libraries}]
    }}"#
    );
    parse_version_manifest(&json).unwrap()
}

fn library(name: &str, path: &str, url: &str) -> String {
    format!(
        r#"{{"name":"{name}","downloads":{{"artifact":{{"path":"{path}","sha1":"{SHA}","size":5,"url":"{url}"}}}}}}"#
    )
}

#[test]
fn plans_client_libraries_and_natives_without_duplicates() {
    let libraries = [
        library(
            "com.example:a:1",
            "com/example/a/1/a-1.jar",
            "https://x.test/a",
        ),
        library(
            "org.lwjgl:lwjgl:3:natives-windows",
            "org/lwjgl/lwjgl-3-natives-windows.jar",
            "https://x.test/n",
        ),
        library(
            "org.lwjgl:lwjgl:3:natives-linux",
            "org/lwjgl/lwjgl-3-natives-linux.jar",
            "https://x.test/l",
        ),
    ]
    .join(",");
    let plan = build_install_plan(
        &manifest(&libraries, "https://x.test/c"),
        "1.21",
        &windows(),
    )
    .unwrap();

    let destinations: Vec<_> = plan
        .artifacts
        .iter()
        .map(|a| a.destination.clone())
        .collect();
    assert_eq!(
        destinations,
        [
            PathBuf::from("versions/1.21/1.21.jar"),
            PathBuf::from("libraries/com/example/a/1/a-1.jar"),
            PathBuf::from("libraries/org/lwjgl/lwjgl-3-natives-windows.jar"),
        ]
    );
}

#[test]
fn rejects_library_without_official_metadata() {
    let libraries = r#"{"name":"com.example:b:1","url":"https://maven.example/"}"#;
    assert_eq!(
        build_install_plan(&manifest(libraries, "https://x.test/c"), "1.21", &windows()),
        Err(InstallPlanError::MissingLibraryMetadata(
            "com.example:b:1".to_owned()
        ))
    );
}

#[test]
fn rejects_traversing_library_path_and_plain_http() {
    let traversal = library("com.example:a:1", "../../evil.jar", "https://x.test/a");
    assert!(matches!(
        build_install_plan(
            &manifest(&traversal, "https://x.test/c"),
            "1.21",
            &windows()
        ),
        Err(InstallPlanError::UnsafeLibraryPath(_))
    ));

    let insecure = library("com.example:a:1", "a/a.jar", "http://x.test/a");
    assert!(matches!(
        build_install_plan(&manifest(&insecure, "https://x.test/c"), "1.21", &windows()),
        Err(InstallPlanError::InsecureUrl(_))
    ));
    assert!(matches!(
        build_install_plan(&manifest("", "http://x.test/c"), "1.21", &windows()),
        Err(InstallPlanError::InsecureUrl(_))
    ));
}

#[test]
fn rejects_unsafe_client_version_and_missing_client() {
    assert!(matches!(
        build_install_plan(&manifest("", "https://x.test/c"), "../x", &windows()),
        Err(InstallPlanError::UnsafeClientVersion(_))
    ));
    let no_client = parse_version_manifest(r#"{"id":"1.21"}"#).unwrap();
    assert_eq!(
        build_install_plan(&no_client, "1.21", &windows()),
        Err(InstallPlanError::MissingClient)
    );
}

#[test]
fn duplicate_library_destinations_fail_plan_validation() {
    let libraries = [
        library("com.example:a:1", "a/a.jar", "https://x.test/1"),
        library("com.example:a:2", "A/a.jar", "https://x.test/2"),
    ]
    .join(",");
    assert!(matches!(
        build_install_plan(
            &manifest(&libraries, "https://x.test/c"),
            "1.21",
            &windows()
        ),
        Err(InstallPlanError::Download(_))
    ));
}
