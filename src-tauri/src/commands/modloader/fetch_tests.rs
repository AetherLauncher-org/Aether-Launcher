//! `download_into` 的校验行为：哈希、大小、体积上限、路径和落盘方式。
//!
//! 这些测试刻意不联网：用 `HttpTransport` 的假实现按 URL 直接喂字节，
//! 这样校验失败的分支是可复现的。

use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Mutex;

use aether_download::transport::{
    HttpTransport, TransportError, TransportRequest, TransportResponse,
};

use super::download_into;

/// 按 URL 返回固定字节的假传输层，没登记的 URL 返回 404，并记录被请求的 URL。
struct RoutedTransport {
    routes: HashMap<String, Vec<u8>>,
    requested: Mutex<Vec<String>>,
}

impl RoutedTransport {
    fn new(routes: &[(&str, &[u8])]) -> Self {
        Self {
            routes: routes
                .iter()
                .map(|(url, body)| ((*url).to_owned(), body.to_vec()))
                .collect(),
            requested: Mutex::new(Vec::new()),
        }
    }

    /// 只登记文件本身，不登记 `.sha1` 旁注文件。
    fn serving(body: &[u8]) -> Self {
        Self::new(&[(URL, body)])
    }

    fn requests(&self) -> Vec<String> {
        self.requested.lock().expect("lock").clone()
    }
}

impl HttpTransport for RoutedTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        self.requested
            .lock()
            .expect("lock")
            .push(request.url.clone());
        let Some(body) = self.routes.get(&request.url) else {
            return Err(TransportError::HttpStatus {
                url: request.url.clone(),
                status: 404,
            });
        };
        Ok(TransportResponse {
            status: 200,
            content_length: Some(body.len() as u64),
            supports_range: false,
            content_range: None,
            body: Box::new(Cursor::new(body.clone())),
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

    fn file(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }

    /// 目录里有没有任何 `.part` 残留。
    fn has_part_file(&self) -> bool {
        fn walk(dir: &std::path::Path) -> bool {
            std::fs::read_dir(dir).is_ok_and(|entries| {
                entries.flatten().any(|entry| {
                    let path = entry.path();
                    path.extension()
                        .is_some_and(|extension| extension == "part")
                        || (path.is_dir() && walk(&path))
                })
            })
        }
        walk(&self.path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

const URL: &str = "https://maven.example.invalid/library.jar";
const SIDECAR_URL: &str = "https://maven.example.invalid/library.jar.sha1";
const BODY: &[u8] = b"hello";
const BODY_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

#[test]
fn keeps_the_file_when_hash_and_size_match() {
    let dir = TempDir::new("ok");
    let transport = RoutedTransport::serving(BODY);

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
    assert!(
        !dir.has_part_file(),
        "the staging file must be renamed away"
    );
}

#[test]
fn deletes_the_file_when_the_sha1_does_not_match() {
    let dir = TempDir::new("badsha");
    let transport = RoutedTransport::serving(BODY);

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
        !dir.file("libraries/thing.jar").exists(),
        "the downloaded file must not be kept"
    );
    assert!(!dir.has_part_file());
}

#[test]
fn deletes_the_file_when_the_size_does_not_match() {
    let dir = TempDir::new("badsize");
    let transport = RoutedTransport::serving(BODY);

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(BODY_SHA1),
        Some(BODY.len() as u64 + 1),
    )
    .expect_err("a mismatched size must fail");

    assert!(
        error.contains("bytes"),
        "error should report both sizes: {error}"
    );
    assert!(!dir.file("libraries/thing.jar").exists());
    assert!(!dir.has_part_file());
}

#[test]
fn rejects_a_body_larger_than_the_declared_size() {
    let dir = TempDir::new("toolarge");
    // 声明 3 字节，实际给 5 字节：不能靠「读够 3 个字节就停」把它当成功。
    let transport = RoutedTransport::serving(BODY);

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(BODY_SHA1),
        Some(3),
    )
    .expect_err("a body longer than declared must fail");

    assert!(
        error.contains("larger") || error.contains("bytes"),
        "error should explain the size problem: {error}"
    );
    assert!(!dir.file("libraries/thing.jar").exists());
    assert!(!dir.has_part_file());
}

#[test]
fn refuses_to_escape_the_root_directory() {
    let dir = TempDir::new("escape");
    let transport = RoutedTransport::serving(BODY);

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
    let transport = RoutedTransport::serving(BODY);

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
    let transport = RoutedTransport::serving(BODY);

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
    let transport = RoutedTransport::serving(BODY);
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

// ---- 每个文件都必须有校验值 ------------------------------------------------------

#[test]
fn reads_the_checksum_from_the_official_sidecar_when_the_plan_has_none() {
    let dir = TempDir::new("sidecar");
    let sidecar = format!("{BODY_SHA1}  library.jar\n");
    let transport = RoutedTransport::new(&[(URL, BODY), (SIDECAR_URL, sidecar.as_bytes())]);

    let written = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        None,
        None,
    )
    .expect("a file whose sidecar checksum matches is accepted");

    assert_eq!(std::fs::read(written).expect("read back"), BODY);
    assert_eq!(
        transport.requests(),
        vec![SIDECAR_URL.to_owned(), URL.to_owned()],
        "the checksum is read before the file"
    );
}

#[test]
fn refuses_a_file_that_has_no_checksum_at_all() {
    let dir = TempDir::new("nosum");
    // 只有文件，没有 .sha1：不能装一个没有校验值的文件。
    let transport = RoutedTransport::serving(BODY);

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        None,
        None,
    )
    .expect_err("a file without any checksum must be refused");

    assert!(error.contains("checksum"), "{error}");
    assert!(!dir.file("libraries/thing.jar").exists());
    assert_eq!(
        transport.requests(),
        vec![SIDECAR_URL.to_owned()],
        "the file itself must not even be requested"
    );
}

#[test]
fn refuses_a_sidecar_that_is_not_a_sha1() {
    let dir = TempDir::new("badsidecar");
    for junk in ["", "not a hash", "abc123", "<html>404</html>"] {
        let transport = RoutedTransport::new(&[(URL, BODY), (SIDECAR_URL, junk.as_bytes())]);

        let error = download_into(
            &transport,
            &dir.path,
            URL,
            "libraries/thing.jar",
            None,
            None,
        )
        .expect_err("a malformed checksum must be refused");

        assert!(error.contains("SHA-1"), "{junk:?}: {error}");
        assert!(!dir.file("libraries/thing.jar").exists(), "{junk:?}");
    }
}

#[test]
fn a_file_that_contradicts_its_sidecar_is_not_kept() {
    let dir = TempDir::new("sidecarmismatch");
    let wrong = "0000000000000000000000000000000000000000";
    let transport = RoutedTransport::new(&[(URL, BODY), (SIDECAR_URL, wrong.as_bytes())]);

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        None,
        None,
    )
    .expect_err("a checksum mismatch must fail");

    assert!(error.contains("SHA-1"), "{error}");
    assert!(!dir.file("libraries/thing.jar").exists());
    assert!(!dir.has_part_file());
}

#[test]
fn refuses_a_manifest_hash_that_is_not_a_sha1() {
    let dir = TempDir::new("badformat");
    let transport = RoutedTransport::serving(BODY);

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some("not-a-hash"),
        None,
    )
    .expect_err("a malformed manifest hash must be refused, not silently skipped");

    assert!(error.contains("SHA-1"), "{error}");
    assert!(transport.requests().is_empty());
}

// ---- 落盘方式 ---------------------------------------------------------------------

#[test]
fn a_failed_download_leaves_an_existing_file_untouched() {
    let dir = TempDir::new("atomic");
    let target = dir.file("libraries/thing.jar");
    std::fs::create_dir_all(target.parent().expect("parent")).expect("create parent");
    std::fs::write(&target, b"previously verified").expect("seed file");
    let transport = RoutedTransport::serving(BODY);

    download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some("0000000000000000000000000000000000000000"),
        None,
    )
    .expect_err("a mismatched hash must fail");

    assert_eq!(
        std::fs::read(&target).expect("existing file"),
        b"previously verified",
        "a bad download must not replace a good file"
    );
    assert!(!dir.has_part_file());
}

#[test]
fn a_verified_download_replaces_an_older_file() {
    let dir = TempDir::new("replace");
    let target = dir.file("libraries/thing.jar");
    std::fs::create_dir_all(target.parent().expect("parent")).expect("create parent");
    std::fs::write(&target, b"stale").expect("seed file");
    let transport = RoutedTransport::serving(BODY);

    download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(BODY_SHA1),
        None,
    )
    .expect("a verified file replaces the stale one");

    assert_eq!(std::fs::read(&target).expect("read back"), BODY);
}

/// 真联网：拿官方 maven 上的一个小 jar，走旁注文件校验的完整路径。默认跳过。
///
/// ```text
/// cargo test -p aether-desktop --lib -- --ignored live_download
/// ```
#[test]
#[ignore = "needs the official Fabric maven and a network connection"]
fn live_download_is_verified_against_the_official_sidecar() {
    use aether_download::transport::KernelHttpTransport;
    use aether_download::transport::http::UreqKernel;

    let dir = TempDir::new("live");
    let transport = KernelHttpTransport::new(UreqKernel::new());

    let written = download_into(
        &transport,
        &dir.path,
        "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar",
        "libraries/net/fabricmc/fabric-loader/0.16.9/fabric-loader-0.16.9.jar",
        None,
        None,
    )
    .expect("the official jar matches the checksum next to it");

    let size = std::fs::metadata(&written).expect("metadata").len();
    assert!(
        size > 100_000,
        "fabric-loader jar looks wrong: {size} bytes"
    );
    assert!(!dir.has_part_file());
}

// ---- 瞬时网络错误的重试 -------------------------------------------------------------

/// 前 `failures` 次返回连接错误，之后正常返回 `body`。
struct FlakyTransport {
    failures: usize,
    body: Vec<u8>,
    calls: Mutex<usize>,
}

impl HttpTransport for FlakyTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        let mut calls = self.calls.lock().expect("lock");
        *calls += 1;
        if *calls <= self.failures {
            return Err(TransportError::Connect {
                url: request.url.clone(),
                source: std::io::Error::other("connection reset"),
            });
        }
        Ok(TransportResponse {
            status: 200,
            content_length: Some(self.body.len() as u64),
            supports_range: false,
            content_range: None,
            body: Box::new(Cursor::new(self.body.clone())),
        })
    }
}

#[test]
fn a_connection_blip_is_retried_and_the_download_still_verifies() {
    let dir = TempDir::new("retry");
    let transport = FlakyTransport {
        failures: 2,
        body: BODY.to_vec(),
        calls: Mutex::new(0),
    };

    download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(BODY_SHA1),
        None,
    )
    .expect("two blips then success is fine");

    assert_eq!(*transport.calls.lock().expect("lock"), 3);
}

#[test]
fn a_dead_connection_gives_up_after_a_few_attempts() {
    let dir = TempDir::new("giveup");
    let transport = FlakyTransport {
        failures: usize::MAX,
        body: BODY.to_vec(),
        calls: Mutex::new(0),
    };

    let error = download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(BODY_SHA1),
        None,
    )
    .expect_err("a connection that never works must fail");

    assert!(error.contains("cannot download"), "{error}");
    assert_eq!(
        *transport.calls.lock().expect("lock"),
        3,
        "three attempts, no more"
    );
}

#[test]
fn a_missing_file_is_not_retried() {
    let dir = TempDir::new("noretry");
    let transport = RoutedTransport::new(&[]);

    download_into(
        &transport,
        &dir.path,
        URL,
        "libraries/thing.jar",
        Some(BODY_SHA1),
        None,
    )
    .expect_err("404 must fail");

    assert_eq!(
        transport.requests().len(),
        1,
        "a 404 does not get better by asking again"
    );
}
