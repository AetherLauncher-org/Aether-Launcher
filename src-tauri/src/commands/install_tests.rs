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
    assert_eq!(error, "2 of 2 files failed to download or verify");
    assert!(!error.contains("https://"));
}
