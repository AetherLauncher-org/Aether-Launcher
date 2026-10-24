use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use aether_core::InstanceId;
use aether_manifest::parse_version_manifest;
use aether_manifest::rules::Environment;
use aether_runtime::JavaRuntime;

use super::{LaunchBuildError, LaunchOptions, build_launch_plan};
use crate::LaunchRequest;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    root: PathBuf,
    options: LaunchOptions,
}

impl Fixture {
    fn new() -> Self {
        let index = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("aether-plan-{}-{index}", std::process::id()));
        std::fs::create_dir_all(root.join("libraries")).unwrap();
        std::fs::create_dir_all(root.join("game")).unwrap();
        std::fs::create_dir_all(root.join("natives")).unwrap();
        let client = root.join("parent-version.jar");
        std::fs::write(&client, b"jar").unwrap();
        Self {
            options: LaunchOptions {
                game_directory: root.clone(),
                working_directory: root.join("game"),
                client_jar: client,
                natives_directory: root.join("natives"),
                variables: BTreeMap::from([
                    ("auth_player_name".to_owned(), "Alex".to_owned()),
                    ("auth_access_token".to_owned(), "secret-token".to_owned()),
                ]),
            },
            root,
        }
    }

    fn library(&self, relative: &str) -> PathBuf {
        let path = self.root.join("libraries").join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"jar").unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn runtime() -> JavaRuntime {
    JavaRuntime {
        executable: PathBuf::from("java"),
        major_version: 21,
        architecture: "x64".to_owned(),
        vendor: None,
    }
}

fn request() -> LaunchRequest {
    LaunchRequest {
        instance_id: InstanceId::new(),
        memory_mib: 4096,
    }
}

fn environment() -> Environment {
    Environment {
        os_name: "windows".to_owned(),
        ..Environment::default()
    }
}

#[test]
fn builds_ordered_classpath_and_arguments_for_fabric() {
    let f = Fixture::new();
    let first = f.library("org/example/first/2/first-2.jar");
    let second = f.library("org/example/second/3/second-3.jar");
    let manifest = parse_version_manifest(r#"{
        "id":"fabric-1.21","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
        "javaVersion":{"component":"delta","majorVersion":21},"assets":"17",
        "libraries":[
            {"name":"org.example:first:2","downloads":{"artifact":{"path":"org/example/first/2/first-2.jar","sha1":"x","size":1,"url":"x"}}},
            {"name":"org.example:second:3","url":"https://example.invalid"},
            {"name":"org.example:linux:1","rules":[{"action":"allow","os":{"name":"linux"}}]}
        ],
        "arguments":{
            "jvm":["-Dgame.dir=${game_directory}","-cp","${classpath}"],
            "game":["--username","${auth_player_name}","--accessToken","${auth_access_token}","--version","${version_name}"]
        }
    }"#).unwrap();
    let plan = build_launch_plan(
        &request(),
        &manifest,
        &runtime(),
        &environment(),
        &f.options,
    )
    .unwrap();

    let cp = std::env::join_paths([first, second, f.options.client_jar.clone()]).unwrap();
    assert_eq!(plan.arguments[0], "-Xmx4096m");
    assert_eq!(
        plan.arguments[1],
        format!("-Dgame.dir={}", f.options.working_directory.display())
    );
    assert_eq!(plan.arguments[2], "-cp");
    assert_eq!(plan.arguments[3], cp.to_string_lossy());
    assert_eq!(
        plan.arguments[4],
        "net.fabricmc.loader.impl.launch.knot.KnotClient"
    );
    assert_eq!(
        &plan.arguments[5..],
        [
            "--username",
            "Alex",
            "--accessToken",
            "secret-token",
            "--version",
            "fabric-1.21"
        ]
    );
}

#[test]
fn adds_classpath_if_missing_and_expands_rule_controlled_arguments() {
    let f = Fixture::new();
    let manifest = parse_version_manifest(r#"{
        "id":"vanilla","mainClass":"net.minecraft.Main",
        "javaVersion":{"component":"delta","majorVersion":21},
        "arguments":{"jvm":[{"rules":[{"action":"allow","os":{"name":"windows"}}],"value":["-Dos=${version_name}"]}],
        "game":["--assetsDir","${assets_root}"]}
    }"#).unwrap();
    let plan = build_launch_plan(
        &request(),
        &manifest,
        &runtime(),
        &environment(),
        &f.options,
    )
    .unwrap();
    assert_eq!(plan.arguments[1], "-Dos=vanilla");
    assert_eq!(plan.arguments[2], "-cp");
    assert_eq!(plan.arguments[4], "net.minecraft.Main");
    assert_eq!(plan.arguments[5], "--assetsDir");
    assert_eq!(plan.arguments[6], f.root.join("assets").to_string_lossy());
}

#[test]
fn rejects_missing_and_unclosed_placeholders() {
    let f = Fixture::new();
    for (token, missing) in [("${auth_uuid}", "auth_uuid"), ("${missing", "")] {
        let json = format!(
            r#"{{
            "id":"v","mainClass":"Main",
            "javaVersion":{{"component":"delta","majorVersion":21}},
            "arguments":{{"game":["{token}"]}}
        }}"#
        );
        let manifest = parse_version_manifest(&json).unwrap();
        let err = build_launch_plan(
            &request(),
            &manifest,
            &runtime(),
            &environment(),
            &f.options,
        )
        .unwrap_err();
        if missing.is_empty() {
            assert!(matches!(err, LaunchBuildError::UnclosedPlaceholder(_)));
        } else {
            assert_eq!(
                err,
                LaunchBuildError::MissingPlaceholder(missing.to_owned())
            );
        }
    }
}

#[test]
fn refuses_unsupported_legacy_or_incompatible_java() {
    let f = Fixture::new();
    let legacy = parse_version_manifest(
        r#"{
        "id":"v","mainClass":"Main",
        "javaVersion":{"component":"delta","majorVersion":21},
        "minecraftArguments":"--username ${auth_player_name}"
    }"#,
    )
    .unwrap();
    assert_eq!(
        build_launch_plan(&request(), &legacy, &runtime(), &environment(), &f.options).unwrap_err(),
        LaunchBuildError::UnsupportedArguments
    );
    let wrong_java = parse_version_manifest(
        r#"{
        "id":"v","mainClass":"Main",
        "javaVersion":{"component":"delta","majorVersion":17},
        "arguments":{}
    }"#,
    )
    .unwrap();
    assert!(matches!(
        build_launch_plan(
            &request(),
            &wrong_java,
            &runtime(),
            &environment(),
            &f.options
        ),
        Err(LaunchBuildError::JavaSelection(_))
    ));
}

#[test]
fn rejects_path_traversal_and_missing_libraries() {
    let f = Fixture::new();
    let invalid = parse_version_manifest(r#"{
        "id":"v","mainClass":"Main",
        "javaVersion":{"component":"delta","majorVersion":21},
        "libraries":[{"name":"bad:lib:1","downloads":{"artifact":{"path":"../outside.jar","sha1":"x","size":1,"url":"x"}}}],
        "arguments":{}
    }"#).unwrap();
    assert!(matches!(
        build_launch_plan(&request(), &invalid, &runtime(), &environment(), &f.options),
        Err(LaunchBuildError::UnsafeLibraryPath(_))
    ));
    let missing = parse_version_manifest(
        r#"{
        "id":"v","mainClass":"Main",
        "javaVersion":{"component":"delta","majorVersion":21},
        "libraries":[{"name":"org.example:missing:1"}],"arguments":{}
    }"#,
    )
    .unwrap();
    assert!(matches!(
        build_launch_plan(&request(), &missing, &runtime(), &environment(), &f.options),
        Err(LaunchBuildError::MissingFile(_))
    ));
}

#[test]
fn refuses_native_classifiers_until_extraction_is_supported() {
    let f = Fixture::new();
    let manifest = parse_version_manifest(
        r#"{
        "id":"v","mainClass":"Main",
        "javaVersion":{"component":"delta","majorVersion":21},
        "libraries":[{"name":"org.lwjgl:lwjgl:3.3:natives-windows"}],"arguments":{}
    }"#,
    )
    .unwrap();
    assert!(matches!(
        build_launch_plan(
            &request(),
            &manifest,
            &runtime(),
            &environment(),
            &f.options
        ),
        Err(LaunchBuildError::UnsupportedNativeLibrary(_))
    ));
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_libraries_outside_game_directory() {
    use std::os::unix::fs::symlink;

    let f = Fixture::new();
    let outside = f.root.join("outside.jar");
    std::fs::write(&outside, b"jar").unwrap();
    symlink(&outside, f.root.join("libraries/escape.jar")).unwrap();
    let manifest = parse_version_manifest(r#"{
        "id":"v","mainClass":"Main",
        "javaVersion":{"component":"delta","majorVersion":21},
        "libraries":[{"name":"some:lib:1","downloads":{"artifact":{"path":"escape.jar","sha1":"x","size":1,"url":"x"}}}],
        "arguments":{}
    }"#).unwrap();
    assert!(matches!(
        build_launch_plan(
            &request(),
            &manifest,
            &runtime(),
            &environment(),
            &f.options
        ),
        Err(LaunchBuildError::EscapedLibraryPath(_))
    ));
}

#[test]
fn does_not_recursively_expand_variable_contents() {
    let variables = BTreeMap::from([
        ("name".to_owned(), "${auth_access_token}".to_owned()),
        ("auth_access_token".to_owned(), "secret".to_owned()),
    ]);
    assert_eq!(
        super::substitute("${name}", &variables).unwrap(),
        "${auth_access_token}"
    );
}
