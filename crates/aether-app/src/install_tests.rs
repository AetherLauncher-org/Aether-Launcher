use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use aether_download::HashingWriter;
use aether_download::transport::{TransportError, TransportRequest, TransportResponse};
use aether_manifest::catalog::VERSION_CATALOG_URL;

use super::*;

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CLIENT_URL: &str = "https://piston-data.mojang.com/client.jar";
const LIBRARY_URL: &str = "https://libraries.minecraft.net/a-1.jar";
const VERSION_URL: &str = "https://piston-meta.mojang.com/v1/packages/x/1.21.1.json";

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

struct FakeTransport {
    responses: HashMap<String, Vec<u8>>,
    requests: AtomicUsize,
}

impl FakeTransport {
    fn new(responses: HashMap<String, Vec<u8>>) -> Self {
        Self {
            responses,
            requests: AtomicUsize::new(0),
        }
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl HttpTransport for FakeTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let bytes = self.responses.get(&request.url).cloned().ok_or_else(|| {
            TransportError::HttpStatus {
                url: request.url.clone(),
                status: 404,
            }
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

#[derive(Default)]
struct Recorder(Mutex<Vec<InstallProgress>>);

impl Reporter for Recorder {
    fn report(&self, progress: &InstallProgress) {
        self.0.lock().unwrap().push(progress.clone());
    }
}

struct NoopReporter;

impl Reporter for NoopReporter {
    fn report(&self, _progress: &InstallProgress) {}
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

fn install(
    transport: &FakeTransport,
    fixture: &Fixture,
    cancel: &CancellationToken,
) -> Result<usize, String> {
    install_version(transport, fixture.root(), "1.21.1", cancel, &NoopReporter)
}

fn standard_transport() -> (FakeTransport, String) {
    let (client, library) = (b"client-bytes".to_vec(), b"library-bytes".to_vec());
    let json = version_json(&client, &library);
    let transport = FakeTransport::new(HashMap::from([
        (CLIENT_URL.to_owned(), client),
        (LIBRARY_URL.to_owned(), library),
    ]));
    (transport, json)
}

#[test]
fn downloads_and_verifies_client_and_libraries() {
    let fixture = Fixture::new();
    let (transport, json) = standard_transport();
    fixture.cache_version(&json);

    assert_eq!(
        install(&transport, &fixture, &CancellationToken::new()),
        Ok(2)
    );
    assert_eq!(
        fs::read(fixture.root().join("versions/1.21.1/1.21.1.jar")).unwrap(),
        b"client-bytes"
    );
    assert_eq!(
        fs::read(fixture.root().join("libraries/com/example/a/1/a-1.jar")).unwrap(),
        b"library-bytes"
    );
}

#[test]
fn a_second_run_skips_files_that_already_verify() {
    let fixture = Fixture::new();
    let (transport, json) = standard_transport();
    fixture.cache_version(&json);
    let cancel = CancellationToken::new();

    assert_eq!(install(&transport, &fixture, &cancel), Ok(2));
    let after_first = transport.requests();
    assert_eq!(install(&transport, &fixture, &cancel), Ok(2));
    assert_eq!(
        transport.requests(),
        after_first,
        "nothing should be fetched again"
    );

    // A damaged file is fetched again; the intact one is not.
    fs::write(fixture.root().join("versions/1.21.1/1.21.1.jar"), b"broken").unwrap();
    assert_eq!(install(&transport, &fixture, &cancel), Ok(2));
    assert_eq!(transport.requests(), after_first + 1);
    assert_eq!(
        fs::read(fixture.root().join("versions/1.21.1/1.21.1.jar")).unwrap(),
        b"client-bytes"
    );
}

fn install_seeded(
    transport: &FakeTransport,
    fixture: &Fixture,
    seed: &Path,
) -> Result<usize, String> {
    install_version_with(
        transport,
        fixture.root(),
        Some(seed),
        "1.21.1",
        &CancellationToken::new(),
        &NoopReporter,
    )
}

#[test]
fn a_seed_with_matching_files_replaces_the_downloads() {
    let fixture = Fixture::new();
    let seed = Fixture::new();
    let (transport, json) = standard_transport();
    fixture.cache_version(&json);
    fs::create_dir_all(seed.root().join("versions/1.21.1")).unwrap();
    fs::write(
        seed.root().join("versions/1.21.1/1.21.1.jar"),
        b"client-bytes",
    )
    .unwrap();
    fs::create_dir_all(seed.root().join("libraries/com/example/a/1")).unwrap();
    fs::write(
        seed.root().join("libraries/com/example/a/1/a-1.jar"),
        b"library-bytes",
    )
    .unwrap();

    assert_eq!(install_seeded(&transport, &fixture, seed.root()), Ok(2));
    assert_eq!(transport.requests(), 0, "everything came from the seed");
    assert_eq!(
        fs::read(fixture.root().join("versions/1.21.1/1.21.1.jar")).unwrap(),
        b"client-bytes"
    );
    assert!(
        !fixture
            .root()
            .join("versions/1.21.1/1.21.1.jar.seed")
            .exists(),
        "no staging file is left behind"
    );
}

#[test]
fn a_seed_file_that_does_not_match_the_official_hash_is_ignored() {
    let fixture = Fixture::new();
    let seed = Fixture::new();
    let (transport, json) = standard_transport();
    fixture.cache_version(&json);
    fs::create_dir_all(seed.root().join("versions/1.21.1")).unwrap();
    fs::write(
        seed.root().join("versions/1.21.1/1.21.1.jar"),
        b"tampered!!!!",
    )
    .unwrap();

    assert_eq!(install_seeded(&transport, &fixture, seed.root()), Ok(2));
    assert_eq!(transport.requests(), 2, "both files were downloaded");
    assert_eq!(
        fs::read(fixture.root().join("versions/1.21.1/1.21.1.jar")).unwrap(),
        b"client-bytes"
    );
    assert_eq!(
        fs::read(seed.root().join("versions/1.21.1/1.21.1.jar")).unwrap(),
        b"tampered!!!!",
        "the seed is never written to"
    );
}

#[test]
fn reports_monotonic_progress_that_ends_complete() {
    let fixture = Fixture::new();
    let (transport, json) = standard_transport();
    fixture.cache_version(&json);
    let recorder = Recorder::default();

    install_version(
        &transport,
        fixture.root(),
        "1.21.1",
        &CancellationToken::new(),
        &recorder,
    )
    .unwrap();

    let reports = recorder.0.lock().unwrap();
    assert!(reports.iter().all(|item| item.stage == Stage::Files));
    assert!(
        reports
            .windows(2)
            .all(|pair| pair[0].bytes_done <= pair[1].bytes_done)
    );
    let last = reports.last().unwrap();
    assert_eq!(last.files_done, 2);
    assert_eq!(last.files_total, 2);
    assert_eq!(last.bytes_done, last.bytes_total);
    assert_eq!(
        last.bytes_total,
        (b"client-bytes".len() + b"library-bytes".len()) as u64
    );
}

#[test]
fn a_cancelled_install_fetches_nothing() {
    let fixture = Fixture::new();
    let (transport, json) = standard_transport();
    fixture.cache_version(&json);
    let cancel = CancellationToken::new();
    cancel.cancel();

    assert_eq!(
        install(&transport, &fixture, &cancel),
        Err(CANCELLED.to_owned())
    );
    assert_eq!(transport.requests(), 0);
}

#[test]
fn refuses_uncached_or_unsafe_versions_without_any_request() {
    let fixture = Fixture::new();
    let transport = FakeTransport::new(HashMap::new());
    let cancel = CancellationToken::new();

    for version in ["1.21.1", "../escape", ""] {
        assert!(
            install_version(&transport, fixture.root(), version, &cancel, &NoopReporter).is_err()
        );
    }
    assert_eq!(transport.requests(), 0);
}

#[test]
fn error_summary_never_contains_download_urls() {
    let fixture = Fixture::new();
    fixture.cache_version(&version_json(b"client-bytes", b"library-bytes"));
    // Both artifacts answer 404, so every file fails after the retry budget.
    let transport = FakeTransport::new(HashMap::new());

    let error = install(&transport, &fixture, &CancellationToken::new()).unwrap_err();
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
    let transport = FakeTransport::new(HashMap::from([
        (CLIENT_URL.to_owned(), client),
        (index_url.to_owned(), index),
        (object_url, object.clone()),
    ]));

    assert_eq!(
        install(&transport, &fixture, &CancellationToken::new()),
        Ok(3)
    );
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

fn catalog_transport() -> FakeTransport {
    let client = b"client-bytes".to_vec();
    let json = format!(
        r#"{{"id":"1.21.1","mainClass":"Main",
        "downloads":{{"client":{{"url":"{CLIENT_URL}","size":{},"sha1":"{}"}}}},
        "libraries":[]}}"#,
        client.len(),
        digest(&client),
    )
    .into_bytes();
    let catalog = format!(
        r#"{{"latest":{{"release":"1.21.1","snapshot":"1.21.1"}},
          "versions":[{{"id":"1.21.1","type":"release","url":"{VERSION_URL}",
          "sha1":"{}","releaseTime":"2024-08-08T00:00:00+00:00"}}]}}"#,
        digest(&json),
    )
    .into_bytes();
    FakeTransport::new(HashMap::from([
        (VERSION_CATALOG_URL.to_owned(), catalog),
        (VERSION_URL.to_owned(), json),
        (CLIENT_URL.to_owned(), client),
    ]))
}

#[test]
fn install_and_create_produces_a_ready_instance_and_reports_every_stage() {
    let fixture = Fixture::new();
    let transport = catalog_transport();
    let recorder = Recorder::default();

    let summary = install_and_create(
        &transport,
        fixture.root(),
        "1.21.1",
        "Survival",
        &CancellationToken::new(),
        &recorder,
    )
    .unwrap();

    let value = serde_json::to_value(&summary).unwrap();
    assert_eq!(value["name"], "Survival");
    assert_eq!(value["version"], "1.21.1");
    assert_eq!(value["state"], "ready");

    let stages: Vec<Stage> = recorder
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|item| item.stage)
        .collect();
    assert_eq!(stages.first(), Some(&Stage::Metadata));
    assert!(stages.contains(&Stage::Files));
    assert_eq!(stages.last(), Some(&Stage::Finalize));
}

#[test]
fn install_and_create_rejects_a_bad_or_duplicate_name_before_downloading() {
    let fixture = Fixture::new();
    let transport = catalog_transport();
    let cancel = CancellationToken::new();

    let run = |name: &str| {
        install_and_create(
            &transport,
            fixture.root(),
            "1.21.1",
            name,
            &cancel,
            &NoopReporter,
        )
    };
    assert!(run("   ").is_err());
    assert_eq!(transport.requests(), 0);

    run("Survival").unwrap();
    let before = transport.requests();
    assert!(run("survival").is_err());
    assert_eq!(
        transport.requests(),
        before,
        "duplicate must fail before any request"
    );
}

#[test]
fn only_one_installation_can_run_and_it_can_be_cancelled() {
    let state = InstallState::default();
    assert!(!state.cancel(), "nothing to cancel yet");

    let token = state.begin().unwrap();
    assert!(state.begin().is_err());
    assert!(!token.is_cancelled());
    assert!(state.cancel());
    assert!(token.is_cancelled());

    state.finish();
    assert!(state.begin().is_ok());
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
    let count = install_version(
        &transport,
        root,
        &version,
        &CancellationToken::new(),
        &NoopReporter,
    )
    .unwrap();
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
    let runtimes = aether_runtime::discover_java_runtimes(&[]);
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
