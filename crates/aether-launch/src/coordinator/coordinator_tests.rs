use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::{GameInstance, InstanceId, InstanceState, VersionId};
use aether_manifest::rules::Environment;
use aether_runtime::JavaRuntime;
use aether_storage::encode_instance;

use super::{CoordinatorError, InstanceLaunchCoordinator};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SHA_ABC: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

struct Fixture {
    base: PathBuf,
    game: PathBuf,
    id: InstanceId,
    instance_dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("aether-coordinator-{}-{n}", std::process::id()));
        let game = base.join("minecraft");
        let id = InstanceId::new();
        let instance_dir = base.join("instances").join(id.to_string());
        fs::create_dir_all(game.join("versions/1.21.1")).unwrap();
        fs::create_dir_all(game.join("versions/fabric")).unwrap();
        fs::create_dir_all(game.join("libraries")).unwrap();
        fs::create_dir_all(instance_dir.join("natives")).unwrap();
        fs::write(game.join("versions/1.21.1/1.21.1.jar"), b"abc").unwrap();
        fs::write(
            game.join("versions/1.21.1/1.21.1.json"),
            format!(r#"{{
                "id":"1.21.1",
                "mainClass":"net.minecraft.client.main.Main",
                "javaVersion":{{"component":"java-runtime-delta","majorVersion":21}},
                "downloads":{{"client":{{"sha1":"{SHA_ABC}","size":3,"url":"https://example.invalid/client.jar"}}}},
                "arguments":{{"jvm":["-cp","${{classpath}}"],"game":["--username","${{auth_player_name}}"]}}
            }}"#),
        ).unwrap();
        fs::write(
            game.join("versions/fabric/fabric.json"),
            r#"{"id":"fabric","inheritsFrom":"1.21.1","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient"}"#,
        ).unwrap();
        let fixture = Self {
            base,
            game,
            id,
            instance_dir,
        };
        fixture.write_instance("fabric", InstanceState::Ready);
        fixture
    }

    fn write_instance(&self, version: &str, state: InstanceState) {
        let instance = GameInstance {
            id: self.id,
            name: "Fabric Test".to_owned(),
            version: VersionId::parse(version).unwrap(),
            root: self.instance_dir.clone(),
            state,
        };
        fs::write(
            self.instance_dir.join("instance.json"),
            encode_instance(&instance).unwrap(),
        )
        .unwrap();
    }

    fn coordinator(&self) -> InstanceLaunchCoordinator {
        InstanceLaunchCoordinator::open(&self.base, &self.game).unwrap()
    }

    fn runtimes(&self) -> Vec<JavaRuntime> {
        vec![JavaRuntime {
            executable: std::env::current_exe().unwrap(),
            major_version: 21,
            architecture: "x64".to_owned(),
            vendor: None,
        }]
    }

    fn env() -> Environment {
        Environment {
            os_name: "windows".to_owned(),
            os_arch: "x86_64".to_owned(),
            ..Environment::default()
        }
    }

    fn vars() -> BTreeMap<String, String> {
        BTreeMap::from([("auth_player_name".to_owned(), "Alex".to_owned())])
    }

    fn prepare(&self) -> Result<crate::LaunchPlan, CoordinatorError> {
        self.coordinator()
            .prepare(self.id, 4096, &self.runtimes(), &Self::env(), &Self::vars())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[test]
fn prepares_fabric_instance_using_parent_client_jar() {
    let f = Fixture::new();
    let prepared = f.prepare().unwrap();
    assert_eq!(prepared.runtime.major_version, 21);
    assert_eq!(prepared.working_directory, f.instance_dir);
    assert_eq!(prepared.arguments[0], "-Xmx4096m");
    assert!(
        prepared
            .arguments
            .contains(&"net.fabricmc.loader.impl.launch.knot.KnotClient".to_owned())
    );
    assert!(
        prepared
            .arguments
            .iter()
            .any(|value| value.contains("1.21.1.jar"))
    );
    assert!(
        prepared
            .arguments
            .iter()
            .all(|value| !value.contains("fabric.jar"))
    );
    assert_eq!(
        &prepared.arguments[prepared.arguments.len() - 2..],
        ["--username", "Alex"]
    );
}

#[test]
fn fails_closed_for_tampered_or_missing_client_artifact() {
    let f = Fixture::new();
    fs::write(f.game.join("versions/1.21.1/1.21.1.jar"), b"bad").unwrap();
    assert!(matches!(f.prepare(), Err(CoordinatorError::Artifact(_))));
    fs::remove_file(f.game.join("versions/1.21.1/1.21.1.jar")).unwrap();
    assert!(matches!(f.prepare(), Err(CoordinatorError::Artifact(_))));
}

#[test]
fn missing_java_or_auth_data_produces_explicit_errors() {
    let f = Fixture::new();
    assert!(matches!(
        f.coordinator()
            .prepare(f.id, 4096, &[], &Fixture::env(), &Fixture::vars()),
        Err(CoordinatorError::Java(_))
    ));
    assert!(matches!(
        f.coordinator()
            .prepare(f.id, 4096, &f.runtimes(), &Fixture::env(), &BTreeMap::new()),
        Err(CoordinatorError::Build(_))
    ));
}

#[test]
fn rejects_unready_or_unknown_instances() {
    let f = Fixture::new();
    f.write_instance("fabric", InstanceState::Incomplete);
    assert!(matches!(
        f.prepare(),
        Err(CoordinatorError::InstanceNotReady)
    ));
    assert!(matches!(
        f.coordinator().prepare(
            InstanceId::new(),
            4096,
            &f.runtimes(),
            &Fixture::env(),
            &Fixture::vars()
        ),
        Err(CoordinatorError::Instance(_))
    ));
}

#[test]
fn missing_version_or_absent_client_metadata_does_not_guess_jar() {
    let f = Fixture::new();
    f.write_instance("missing", InstanceState::Ready);
    assert!(matches!(f.prepare(), Err(CoordinatorError::Version(_))));
    f.write_instance("fabric", InstanceState::Ready);
    fs::write(f.game.join("versions/1.21.1/1.21.1.json"),
        r#"{"id":"1.21.1","mainClass":"Main","javaVersion":{"component":"delta","majorVersion":21},"arguments":{}}"#).unwrap();
    assert!(matches!(
        f.prepare(),
        Err(CoordinatorError::MissingClientMetadata)
    ));
}

#[test]
fn does_not_launch_versions_with_unprepared_native_or_asset_data() {
    let f = Fixture::new();
    let child = f.game.join("versions/fabric/fabric.json");
    fs::write(
        &child,
        format!(
            r#"{{
        "id":"fabric","inheritsFrom":"1.21.1",
        "libraries":[{{"name":"org.lwjgl:lwjgl:3:natives-windows",
            "downloads":{{"artifact":{{"path":"native.jar","sha1":"{SHA_ABC}","size":3,
            "url":"https://example.invalid/native"}}}}}}]
    }}"#
        ),
    )
    .unwrap();
    assert!(matches!(
        f.prepare(),
        Err(CoordinatorError::NativesNotReady)
    ));
    fs::write(
        &child,
        r#"{
        "id":"fabric","inheritsFrom":"1.21.1",
        "assetIndex":{"id":"17","sha1":"a9993e364706816aba3e25717850c26c9cd0d89d",
        "size":3,"url":"https://example.invalid/assets"}
    }"#,
    )
    .unwrap();
    assert!(matches!(f.prepare(), Err(CoordinatorError::Asset(_))));
}

#[test]
fn verified_asset_index_and_objects_allow_launch_plan_preparation() {
    let f = Fixture::new();
    let object = "a9993e364706816aba3e25717850c26c9cd0d89d";
    let object_path = f.game.join("assets/objects").join(&object[..2]).join(object);
    fs::create_dir_all(object_path.parent().unwrap()).unwrap();
    fs::write(&object_path, b"abc").unwrap();

    let index = r#"{"objects":{"test":{"hash":"a9993e364706816aba3e25717850c26c9cd0d89d","size":3}}}"#;
    fs::create_dir_all(f.game.join("assets/indexes")).unwrap();
    fs::write(f.game.join("assets/indexes/17.json"), index).unwrap();
    fs::write(
        f.game.join("versions/fabric/fabric.json"),
        r#"{"id":"fabric","inheritsFrom":"1.21.1","assetIndex":{"id":"17","sha1":"9c93b561ed119366ac6421c25bca8172515fc0fd","size":81,"url":"https://example.invalid/assets"}}"#,
    ).unwrap();
    let plan = f.prepare().unwrap();
    assert!(plan.arguments.contains(&"--username".to_owned()));

    fs::write(&object_path, b"bad").unwrap();
    assert!(matches!(f.prepare(), Err(CoordinatorError::Asset(_))));
}

