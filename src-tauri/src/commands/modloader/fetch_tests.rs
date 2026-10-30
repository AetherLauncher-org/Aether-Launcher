//! `download_into` 的校验行为：哈希、大小、体积上限和路径。
//!
//! 这些测试刻意不联网：用 `HttpTransport` 的假实现直接喂字节，
//! 这样校验失败的分支是可复现的。

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Mutex;

use aether_download::transport::{
    HttpTransport, TransportError, TransportRequest, TransportResponse,
};

use super::download_into;

/// 每次都返回同一份字节的假传输层，并记录被请求的 URL。
struct FixedTransport {
    body: Vec<u8>,
    requested: Mutex<Vec<String>>,
}

impl FixedTransport {
    fn new(body: &[u8]) -> Self {
        Self {
            body: body.to_vec(),
            requested: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requested.lock().expect("lock").clone()
    }
}

impl HttpTransport for FixedTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        self.requested
            .lock()
            .expect("lock")
            .push(request.url.clone());
        Ok(TransportResponse {
            status: 200,
            content_length: Some(self.body.len() as u64),
            supports_range: false,
            content_range: None,
            body: Box::new(Cursor::new(self.body.clone())),
        })
    }
}

/// 每个测试一个独立的空目录，结束后删掉。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "aether-fetch-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

const URL: &str = "https://maven.example.invalid/library.jar";
const BODY: &[u8] = b"hello";
const BODY_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

#[test]
fn keeps_the_file_when_hash_and_size_match() {
    let dir = TempDir::new("ok");
    let transport = FixedTransport::new(BODY);

    let written = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(BODY_SHA1),
        Some(BODY.len() as u64),
    )
    .expect("matching hash and size should be accepted");

    assert_eq!(std::fs::read(&written).expect("read back"), BODY);
    assert_eq!(transport.requests(), vec![URL.to_owned()]);
}

#[test]
fn deletes_the_file_when_the_sha1_does_not_match() {
    let dir = TempDir::new("badsha");
    let transport = FixedTransport::new(BODY);

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some("0000000000000000000000000000000000000000"),
        None,
    )
    .expect_err("a mismatched hash must fail");

    assert!(
        error.contains("SHA-1"),
        "error should name the mismatch: {error}"
    );
    assert!(
        !dir.path.join("libraries/thing.jar").exists(),
        "the downloaded file must be removed, not kept"
    );
}

#[test]
fn deletes_the_file_when_the_size_does_not_match() {
    let dir = TempDir::new("badsize");
    let transport = FixedTransport::new(BODY);

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        None,
        Some(BODY.len() as u64 + 1),
    )
    .expect_err("a mismatched size must fail");

    assert!(
        error.contains("bytes"),
        "error should report both sizes: {error}"
    );
    assert!(!dir.path.join("libraries/thing.jar").exists());
}

#[test]
fn rejects_a_body_larger_than_the_declared_size() {
    let dir = TempDir::new("toolarge");
    // 声明 3 字节，实际给 5 字节：不能靠「读够 3 个字节就停」把它当成功。
    let transport = FixedTransport::new(BODY);

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        None,
        Some(3),
    )
    .expect_err("a body longer than declared must fail");

    assert!(
        error.contains("larger") || error.contains("bytes"),
        "error should explain the size problem: {error}"
    );
    assert!(!dir.path.join("libraries/thing.jar").exists());
}

#[test]
fn refuses_to_escape_the_root_directory() {
    let dir = TempDir::new("escape");
    let transport = FixedTransport::new(BODY);

    for destination in ["../evil.jar", "libraries/../../evil.jar", "/etc/evil.jar"] {
        let error = download_into(&transport, &dir.path, URL, destination, None, None)
            .expect_err("escaping destinations must be refused");
        assert!(
            error.contains("outside the game directory"),
            "{destination} should be refused by path: {error}"
        );
    }
    assert!(
        transport.requests().is_empty(),
        "a rejected path must fail before any network request"
    );
}

#[test]
fn refuses_plain_http() {
    let dir = TempDir::new("http");
    let transport = FixedTransport::new(BODY);

    let error = download_into(
        &transport,
        &dir.path,
        "http://maven.example.invalid/library.jar",
        "libraries/thing.jar",
        None,
        None,
    )
    .expect_err("plain http must be refused");

    assert!(error.contains("https"), "{error}");
    assert!(transport.requests().is_empty());
}

#[test]
fn accepts_an_upper_case_hash() {
    let dir = TempDir::new("upper");
    let transport = FixedTransport::new(BODY);

    download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(&BODY_SHA1.to_uppercase()),
        None,
    )
    .expect("hashes are case-insensitive hex");
}

#[test]
fn accepts_a_hash_with_surrounding_whitespace() {
    let dir = TempDir::new("padded");
    let transport = FixedTransport::new(BODY);
    let padded = format!("  {BODY_SHA1}\n");

    download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(&padded),
        None,
    )
    .expect(".sha1 sidecar files usually end with a newline");
}
