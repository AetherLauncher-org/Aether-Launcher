use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_download::HashingWriter;
use aether_download::transport::{TransportError, TransportResponse};

use super::*;

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const VERSION_URL: &str = "https://piston-meta.mojang.com/v1/packages/example/version.json";
const SHA1_CLIENT: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let id = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aether-version-cache-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn root(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct FakeTransport {
    responses: HashMap<String, Vec<u8>>,
    requests: Mutex<Vec<String>>,
}

impl HttpTransport for FakeTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        self.requests.lock().unwrap().push(request.url.clone());
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

fn manifest(id: &str) -> Vec<u8> {
    format!(
        r#"{{"id":"{id}","mainClass":"net.minecraft.client.main.Main",
            "downloads":{{"client":{{"url":"https://piston-data.mojang.com/client.jar",
            "size":3,"sha1":"{SHA1_CLIENT}"}}}}}}"#
    )
    .into_bytes()
}

fn digest(bytes: &[u8]) -> String {
    let mut writer = HashingWriter::new(Vec::new());
    writer.write_all(bytes).unwrap();
    let (_, _, digest) = writer.finish();
    digest
}

fn transport_with(metadata: Vec<u8>, declared_hash: &str) -> FakeTransport {
    let catalog = format!(
        r#"{{"latest":{{"release":"1.21.1","snapshot":"24w14a"}},
          "versions":[
          {{"id":"1.21.1","type":"release","url":"{VERSION_URL}",
          "sha1":"{declared_hash}","releaseTime":"2024-08-08"}},
          {{"id":"24w14a","type":"snapshot","url":"https://piston-meta.mojang.com/snapshot.json",
          "sha1":"{declared_hash}","releaseTime":"2024-04-01"}}]}}"#
    );
    FakeTransport {
        responses: HashMap::from([
            (
                aether_manifest::catalog::VERSION_CATALOG_URL.to_owned(),
                catalog.into_bytes(),
            ),
            (VERSION_URL.to_owned(), metadata),
        ]),
        requests: Mutex::new(Vec::new()),
    }
}

#[test]
fn downloads_and_atomically_caches_verified_json_only() {
    let fixture = Fixture::new();
    let metadata = manifest("1.21.1");
    let fake = transport_with(metadata.clone(), &digest(&metadata));
    let game = fixture.root().join("game");

    assert_eq!(prepare_version(&fake, &game, "1.21.1").unwrap(), "1.21.1");
    let version_path = game.join("versions/1.21.1/1.21.1.json");
    assert_eq!(fs::read(&version_path).unwrap(), metadata);
    assert_eq!(prepare_version(&fake, &game, "1.21.1").unwrap(), "1.21.1");
    assert_eq!(fs::read(&version_path).unwrap(), metadata);
    assert_eq!(
        fake.requests.lock().unwrap().as_slice(),
        [
            aether_manifest::catalog::VERSION_CATALOG_URL,
            VERSION_URL,
            aether_manifest::catalog::VERSION_CATALOG_URL,
            VERSION_URL
        ]
    );
    let files: Vec<_> = fs::read_dir(version_path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(files.len(), 1, "no abandoned staging file");
}

#[test]
fn rejects_hash_mismatch_without_publishing_any_file() {
    let fixture = Fixture::new();
    let fake = transport_with(manifest("1.21.1"), &"0".repeat(40));
    let game = fixture.root().join("game");
    assert!(prepare_version(&fake, &game, "1.21.1").is_err());
    assert!(!game.join("versions/1.21.1/1.21.1.json").exists());
}

#[test]
fn rejects_missing_versions_and_wrong_manifest_ids() {
    let fixture = Fixture::new();
    let metadata = manifest("different");
    let fake = transport_with(metadata.clone(), &digest(&metadata));
    let game = fixture.root().join("game");
    assert!(prepare_version(&fake, &game, "../escape").is_err());
    assert!(prepare_version(&fake, &game, "1.21.1").is_err());
    assert!(!game.exists());
}

#[test]
fn rejects_malformed_or_oversized_json_without_writing() {
    let fixture = Fixture::new();
    for bytes in [
        b"not json".to_vec(),
        vec![b'A'; MAX_VERSION_JSON_BYTES + 1],
    ] {
        let fake = transport_with(bytes.clone(), &digest(&bytes));
        let game = fixture.root().join("game");
        assert!(prepare_version(&fake, &game, "1.21.1").is_err());
        assert!(!game.exists());
    }
}

#[test]
fn corrupted_existing_cache_is_not_overwritten() {
    let fixture = Fixture::new();
    let metadata = manifest("1.21.1");
    let fake = transport_with(metadata.clone(), &digest(&metadata));
    let game = fixture.root().join("game");
    let location = game.join("versions/1.21.1");
    fs::create_dir_all(&location).unwrap();
    fs::write(location.join("1.21.1.json"), b"corrupt").unwrap();
    assert!(prepare_version(&fake, &game, "1.21.1").is_err());
    assert_eq!(fs::read(location.join("1.21.1.json")).unwrap(), b"corrupt");
}

#[cfg(unix)]
#[test]
fn refuses_symlinked_version_directory() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let metadata = manifest("1.21.1");
    let fake = transport_with(metadata.clone(), &digest(&metadata));
    let game = fixture.root().join("game");
    fs::create_dir_all(game.join("versions")).unwrap();
    symlink(fixture.root(), game.join("versions/1.21.1")).unwrap();

    assert!(prepare_version(&fake, &game, "1.21.1").is_err());
    assert!(!fixture.root().join("1.21.1.json").exists());
}
