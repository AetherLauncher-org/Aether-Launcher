use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use aether_download::HashingWriter;
use aether_download::transport::{TransportError, TransportRequest, TransportResponse};

use super::*;

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CLIENT_URL: &str = "https://piston-data.mojang.com/client.jar";
const LIBRARY_URL: &str = "https://libraries.minecraft.net/a-1.jar";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let id = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("aether-install-{}-{id}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn cache_version(&self, json: &str) {
        let dir = self.0.join("versions/1.21.1");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("1.21.1.json"), json).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct FakeTransport(HashMap<String, Vec<u8>>);

impl HttpTransport for FakeTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let bytes =
            self.0
                .get(&request.url)
                .cloned()
                .ok_or_else(|| TransportError::HttpStatus {
                    url: request.url.clone(),
                    status: 404,
                })?;
        Ok(TransportResponse {
            status: 200,
            content_length: Some(bytes.len() as u64),
            supports_range: false,
            content_range: None,
            body: Box::new(Cursor::new(bytes)),
        })
    }
}

fn digest(bytes: &[u8]) -> String {
    let mut writer = HashingWriter::new(Vec::new());
    writer.write_all(bytes).unwrap();
    let (_, _, digest) = writer.finish();
    digest
}

fn version_json(client: &[u8], library: &[u8]) -> String {
    format!(
        r#"{{"id":"1.21.1","mainClass":"Main",
        "downloads":{{"client":{{"url":"{CLIENT_URL}","size":{},"sha1":"{}"}}}},
        "libraries":[{{"name":"com.example:a:1","downloads":{{"artifact":{{
            "path":"com/example/a/1/a-1.jar","url":"{LIBRARY_URL}","size":{},"sha1":"{}"}}}}}}]}}"#,
        client.len(),
        digest(client),
        library.len(),
        digest(library),
    )
}

#[test]
fn downloads_and_verifies_client_and_libraries() {
    let (client, library) = (b"client-bytes".to_vec(), b"library-bytes".to_vec());
    let fixture = Fixture::new();
    fixture.cache_version(&version_json(&client, &library));
    let transport = FakeTransport(HashMap::from([
        (CLIENT_URL.to_owned(), client.clone()),
        (LIBRARY_URL.to_owned(), library.clone()),
    ]));

    assert_eq!(install_version(&transport, fixture.root(), "1.21.1"), Ok(2));
    assert_eq!(
        fs::read(fixture.root().join("versions/1.21.1/1.21.1.jar")).unwrap(),
        client
    );
    assert_eq!(
        fs::read(fixture.root().join("libraries/com/example/a/1/a-1.jar")).unwrap(),
        library
    );
}

#[test]
fn refuses_uncached_or_unsafe_versions_without_any_request() {
    let fixture = Fixture::new();
    let transport = FakeTransport(HashMap::new());

    assert!(install_version(&transport, fixture.root(), "1.21.1").is_err());
    assert!(install_version(&transport, fixture.root(), "../escape").is_err());
    assert!(install_version(&transport, fixture.root(), "").is_err());
}

#[test]
fn error_summary_never_contains_download_urls() {
    let fixture = Fixture::new();
    fixture.cache_version(&version_json(b"client-bytes", b"library-bytes"));
    // Both artifacts answer 404, so every file fails after the retry budget.
    let transport = FakeTransport(HashMap::new());

    let error = install_version(&transport, fixture.root(), "1.21.1").unwrap_err();
    assert_eq!(
        error,
        "2 of 2 files failed to download or verify (HTTP 404 x2)"
    );
    assert!(!error.contains("https://"));
}

#[test]
fn downloads_asset_index_and_objects_after_libraries() {
    let (client, object) = (b"client-bytes".to_vec(), b"sound-bytes".to_vec());
    let object_hash = digest(&object);
    let index = format!(
        r#"{{"objects":{{"minecraft/sounds/a.ogg":{{"hash":"{object_hash}","size":{}}}}}}}"#,
        object.len()
    )
    .into_bytes();
    let index_url = "https://piston-meta.mojang.com/v1/packages/x/17.json";
    let version = format!(
        r#"{{"id":"1.21.1","mainClass":"Main",
        "downloads":{{"client":{{"url":"{CLIENT_URL}","size":{},"sha1":"{}"}}}},
        "assetIndex":{{"id":"17","url":"{index_url}","size":{},"sha1":"{}","totalSize":0}},
        "libraries":[]}}"#,
        client.len(),
        digest(&client),
        index.len(),
        digest(&index),
    );
    let fixture = Fixture::new();
    fixture.cache_version(&version);
    let object_url = format!(
        "https://resources.download.minecraft.net/{}/{object_hash}",
        &object_hash[..2]
    );
    let transport = FakeTransport(HashMap::from([
        (CLIENT_URL.to_owned(), client),
        (index_url.to_owned(), index),
        (object_url, object.clone()),
    ]));

    assert_eq!(install_version(&transport, fixture.root(), "1.21.1"), Ok(3));
    assert_eq!(
        fs::read(
            fixture
                .root()
                .join("assets/objects")
                .join(&object_hash[..2])
                .join(&object_hash)
        )
        .unwrap(),
        object
    );
    assert!(fixture.root().join("assets/indexes/17.json").is_file());
}

/// Live smoke test against Mojang. Run with:
/// `cargo test -p aether-desktop --lib live_install -- --ignored --nocapture`
#[test]
#[ignore = "downloads hundreds of MiB from the real Mojang servers"]
fn live_install_of_a_real_version_verifies_end_to_end() {
    use aether_download::transport::KernelHttpTransport;
    use aether_download::transport::http::UreqKernel;

    let version = std::env::var("AETHER_LIVE_VERSION").unwrap_or_else(|_| "1.16.5".to_owned());
    // AETHER_LIVE_DIR keeps downloads between runs so a retry resumes.
    let kept = std::env::var_os("AETHER_LIVE_DIR").map(PathBuf::from);
    let fixture = Fixture::new();
    let root: &Path = kept.as_deref().unwrap_or_else(|| fixture.root());
    fs::create_dir_all(root).unwrap();
    let transport = KernelHttpTransport::new(UreqKernel::new());
    super::super::vanilla::prepare_version(&transport, root, &version).unwrap();
    let count = install_version(&transport, root, &version).unwrap();
    println!("installed {count} files at {}", root.display());

    let resolved = load_resolved_version_manifest(root, &version).unwrap();
    let index = resolved.manifest.asset_index.unwrap();
    let verified =
        aether_download::assets::verify_installed_assets(root, &index.id, &index.sha1, index.size)
            .unwrap();
    println!("assets verified: {}", verified.objects_verified);
}

/// Launch a version previously installed by `live_install...` (same
/// `AETHER_LIVE_DIR` / `AETHER_LIVE_VERSION`). Opens a real game window.
#[test]
#[ignore = "starts a real Minecraft process"]
fn live_launch_of_an_installed_version_stays_running() {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use aether_core::InstanceState;
    use aether_launch::InstanceLaunchCoordinator;

    let version = std::env::var("AETHER_LIVE_VERSION").unwrap_or_else(|_| "1.16.5".to_owned());
    let game = PathBuf::from(std::env::var_os("AETHER_LIVE_DIR").expect("AETHER_LIVE_DIR"));
    let store = Fixture::new();
    let instance = aether_storage::create_instance(store.root(), "live", &version).unwrap();
    aether_storage::set_instance_state(&instance.root, &instance, InstanceState::Ready).unwrap();

    let vars = BTreeMap::from([
        ("auth_player_name".to_owned(), "Player".to_owned()),
        ("auth_uuid".to_owned(), "0".repeat(32)),
        ("auth_access_token".to_owned(), "0".to_owned()),
        ("user_type".to_owned(), "legacy".to_owned()),
        ("version_type".to_owned(), "release".to_owned()),
        ("clientid".to_owned(), String::new()),
        ("xuid".to_owned(), String::new()),
        ("auth_xuid".to_owned(), String::new()),
        ("launcher_name".to_owned(), "Aether".to_owned()),
        ("launcher_version".to_owned(), "0".to_owned()),
    ]);
    let runtimes = aether_runtime::discover_java_runtimes();
    let mut coordinator = InstanceLaunchCoordinator::open(store.root(), &game).unwrap();
    let started = coordinator
        .start(
            instance.id,
            2048,
            &runtimes,
            &super::super::launch::current_environment(),
            &vars,
            &[],
        )
        .unwrap();
    println!("started: {started:?}");
    std::thread::sleep(Duration::from_secs(30));
    let status = coordinator.status(instance.id).unwrap();
    let logs = coordinator.drain_logs(instance.id).unwrap();
    for record in logs.iter().rev().take(15).rev() {
        println!("log: {record:?}");
    }
    let _ = coordinator.stop(instance.id);
    assert!(
        format!("{:?}", status.state).contains("Running"),
        "game did not stay running: {status:?}"
    );
}
