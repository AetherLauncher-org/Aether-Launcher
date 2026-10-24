use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, ThreadId};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use sha1::{Digest, Sha1};

use super::*;
use crate::ArtifactSource;
use crate::hash::HashMismatch;
use crate::transport::{ContentRange, TransportError, TransportRequest, TransportResponse};

// ---------------------------------------------------------------------------
// 测试工具
// ---------------------------------------------------------------------------

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 每个测试独享的临时根目录，析构时递归删除。
struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        let path = std::env::temp_dir().join(format!(
            "aether-download-{}-{label}-{sequence}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create temp root");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn sha1_of(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    let mut hex = String::with_capacity(40);
    for byte in hasher.finalize() {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn body_of(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|index| seed.wrapping_add(index as u8))
        .collect()
}

fn write_partial(root: &TempRoot, destination: &str, prefix: &[u8]) {
    let final_path = root.join(destination);
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(part_path(&final_path), prefix).expect("write partial");
}

/// 每次 `read` 最多返回 `chunk` 字节的读取器；可以指定在第几次读取时触发取消。
struct ChunkedBody {
    data: Vec<u8>,
    position: usize,
    chunk: usize,
    reads: usize,
    cancel_on_read: Option<usize>,
    cancel: CancellationToken,
}

impl ChunkedBody {
    fn new(
        data: Vec<u8>,
        chunk: usize,
        cancel_on_read: Option<usize>,
        cancel: &CancellationToken,
    ) -> Self {
        Self {
            data,
            position: 0,
            chunk: chunk.max(1),
            reads: 0,
            cancel_on_read,
            cancel: cancel.clone(),
        }
    }
}

impl Read for ChunkedBody {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        if self.cancel_on_read == Some(self.reads) {
            self.cancel.cancel();
        }
        let remaining = self.data.len() - self.position;
        let count = remaining.min(self.chunk).min(buf.len());
        buf[..count].copy_from_slice(&self.data[self.position..self.position + count]);
        self.position += count;
        Ok(count)
    }
}

/// 脚本对 Content-Range 的控制方式。
enum RangeMode {
    /// 按请求的 Range 自动合成一个正确对齐的区间（常规用例）。
    Synthesized,
    /// 显式给出（可以故意错位）。
    Explicit(ContentRange),
    /// 服务器没给 Content-Range（206 但没有该头）。
    Absent,
}

enum Script {
    Ok {
        status: u16,
        body: Vec<u8>,
        range_mode: RangeMode,
    },
    /// 每次 `read` 最多返回 `chunk` 字节；第 `cancel_on_read` 次读取时触发取消。
    CancelOnRead {
        status: u16,
        body: Vec<u8>,
        chunk: usize,
        cancel_on_read: usize,
        cancel: CancellationToken,
    },
    Connect,
    Timeout,
    HttpStatus(u16),
}

#[derive(Default)]
struct FakeTransport {
    scripts: Mutex<VecDeque<Script>>,
    /// 按 URL 应答的脚本：并发下载时不受 worker 抢队列的顺序影响，保证确定性。
    by_url: Mutex<HashMap<String, Script>>,
    requests: Mutex<Vec<TransportRequest>>,
    threads: Mutex<Vec<ThreadId>>,
}

impl FakeTransport {
    fn new(scripts: Vec<Script>) -> Self {
        Self {
            scripts: Mutex::new(scripts.into()),
            ..Self::default()
        }
    }

    fn by_url(pairs: Vec<(String, Script)>) -> Self {
        Self {
            by_url: Mutex::new(pairs.into_iter().collect()),
            ..Self::default()
        }
    }

    fn request_count(&self) -> usize {
        lock(&self.requests).len()
    }

    fn request_urls(&self) -> Vec<String> {
        lock(&self.requests)
            .iter()
            .map(|request| request.url.clone())
            .collect()
    }

    fn request_ranges(&self) -> Vec<Option<Range>> {
        lock(&self.requests)
            .iter()
            .map(|request| request.range)
            .collect()
    }
}

impl HttpTransport for FakeTransport {
    fn get(&self, request: &TransportRequest) -> Result<TransportResponse, TransportError> {
        lock(&self.requests).push(request.clone());
        lock(&self.threads).push(thread::current().id());
        let keyed = lock(&self.by_url).remove(&request.url);
        let script = keyed
            .or_else(|| lock(&self.scripts).pop_front())
            .unwrap_or(Script::Timeout);
        match script {
            Script::Ok {
                status,
                body,
                range_mode,
            } => {
                let length = body.len() as u64;
                let content_range = match range_mode {
                    RangeMode::Synthesized if status == 206 => {
                        let start = request.range.map_or(0, |range| range.start);
                        Some(ContentRange {
                            start,
                            end: start + length - 1,
                            total: Some(start + length),
                        })
                    }
                    RangeMode::Explicit(range) => Some(range),
                    RangeMode::Synthesized | RangeMode::Absent => None,
                };
                Ok(TransportResponse {
                    status,
                    content_length: Some(length),
                    supports_range: status == 206,
                    content_range,
                    body: Box::new(io::Cursor::new(body)),
                })
            }
            Script::CancelOnRead {
                status,
                body,
                chunk,
                cancel_on_read,
                cancel,
            } => {
                let length = body.len() as u64;
                Ok(TransportResponse {
                    status,
                    content_length: Some(length),
                    supports_range: status == 206,
                    content_range: None,
                    body: Box::new(ChunkedBody::new(body, chunk, Some(cancel_on_read), &cancel)),
                })
            }
            Script::Connect => Err(TransportError::Connect {
                url: request.url.clone(),
                source: io::Error::other("connection refused"),
            }),
            Script::Timeout => Err(TransportError::Timeout {
                url: request.url.clone(),
                timeout_secs: request.timeout_secs,
            }),
            Script::HttpStatus(status) => Err(TransportError::HttpStatus {
                url: request.url.clone(),
                status,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProgressEvent {
    destination: PathBuf,
    provider: String,
    downloaded: u64,
    total: u64,
}

#[derive(Clone, Default)]
struct CollectProgress(Arc<Mutex<Vec<ProgressEvent>>>);

impl CollectProgress {
    fn events(&self) -> Vec<ProgressEvent> {
        lock(&self.0).clone()
    }

    fn last(&self) -> Option<ProgressEvent> {
        lock(&self.0).last().cloned()
    }

    fn count(&self) -> usize {
        lock(&self.0).len()
    }
}

impl ProgressSink for CollectProgress {
    fn progress(&self, destination: &Path, provider: &str, downloaded: u64, total: u64) {
        lock(&self.0).push(ProgressEvent {
            destination: destination.to_path_buf(),
            provider: provider.to_owned(),
            downloaded,
            total,
        });
    }
}

/// 测试里替代 `thread::sleep` 的记录器：立即返回，只记下时长。
#[derive(Clone, Default)]
struct SleepRecorder(Arc<Mutex<Vec<Duration>>>);

impl SleepRecorder {
    fn record(&self) -> impl Fn(Duration) + Send + Sync + 'static {
        let store = Arc::clone(&self.0);
        move |duration: Duration| lock(&store).push(duration)
    }

    fn durations(&self) -> Vec<Duration> {
        lock(&self.0).clone()
    }
}

fn source(provider: &str) -> ArtifactSource {
    ArtifactSource {
        provider: provider.to_owned(),
        url: format!("https://{provider}.example.invalid/file.jar"),
    }
}

fn artifact(destination: &str, size: u64, sha1: &str, providers: &[&str]) -> DownloadArtifact {
    DownloadArtifact {
        destination: PathBuf::from(destination),
        size,
        sha1: sha1.to_owned(),
        sources: providers.iter().copied().map(source).collect(),
    }
}

const NO_SLEEP: Duration = Duration::ZERO;

fn policy(max_attempts: u32) -> RetryPolicy {
    RetryPolicy::new(
        max_attempts,
        Duration::from_millis(10),
        Duration::from_secs(1),
    )
}

fn engine<'a>(
    transport: &'a FakeTransport,
    root: &'a Path,
    max_attempts: u32,
    base: Duration,
    max: Duration,
    sleeps: &SleepRecorder,
) -> DownloadEngine<'a> {
    DownloadEngine::new(transport, root)
        .with_retry(RetryPolicy::new(max_attempts, base, max))
        .with_sleeper(sleeps.record())
        .with_timeout_secs(7)
        .with_trip_threshold(3)
}

fn cancelled_ok(result: Result<PathBuf, DownloadError>) -> (PathBuf, Vec<SourceFailure>) {
    match result {
        Err(DownloadError::AllSourcesFailed {
            destination,
            sources,
        }) => (destination, sources),
        other => panic!("expected AllSourcesFailed, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// 单文件下载
// ---------------------------------------------------------------------------

#[test]
fn downloads_full_file_on_200() {
    let root = TempRoot::new("full");
    let body = body_of(3 * CHUNK_SIZE + 7, 5);
    let transport = FakeTransport::new(vec![Script::Ok {
        status: 200,
        body: body.clone(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();
    let progress = CollectProgress::default();
    let cancel = CancellationToken::new();

    let result = engine(
        &transport,
        root.path(),
        3,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact(
            "libraries/full.jar",
            body.len() as u64,
            &sha1_of(&body),
            &["a"],
        ),
        &cancel,
        &progress,
    );

    assert_eq!(result.expect("download"), root.join("libraries/full.jar"));
    assert_eq!(
        fs::read(root.join("libraries/full.jar")).expect("read"),
        body
    );
    assert!(!root.join("libraries/full.jar.part").exists());
    assert_eq!(transport.request_count(), 1);
    assert_eq!(transport.request_ranges()[0], None);

    let last = progress.last().expect("progress");
    assert_eq!(last.downloaded, body.len() as u64);
    assert_eq!(last.total, body.len() as u64);
    assert_eq!(last.destination, PathBuf::from("libraries/full.jar"));
    assert_eq!(last.provider, "a");
    // 3 个满块 + 1 个尾块
    assert_eq!(progress.count(), 4);
    assert!(sleeps.durations().is_empty());
}

#[test]
fn oversized_part_is_deleted_before_full_request() {
    let root = TempRoot::new("oversized-part");
    let body = body_of(8, 1);
    write_partial(&root, "oversized.jar", &body_of(16, 9));
    let transport = FakeTransport::new(vec![Script::Ok {
        status: 200,
        body: body.clone(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();
    let result = engine(
        &transport,
        root.path(),
        1,
        Duration::ZERO,
        Duration::ZERO,
        &sleeps,
    )
    .download_artifact(
        &artifact("oversized.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(transport.request_ranges(), vec![None]);
    assert_eq!(fs::read(root.join("oversized.jar")).expect("read"), body);
}

#[test]
fn resumes_from_existing_part_file() {
    let root = TempRoot::new("resume");
    let body = body_of(8192, 9);
    let half = body.len() / 2;
    write_partial(&root, "libraries/resume.jar", &body[..half]);

    let transport = FakeTransport::new(vec![Script::Ok {
        status: 206,
        body: body[half..].to_vec(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();
    let progress = CollectProgress::default();

    let result = engine(
        &transport,
        root.path(),
        3,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact(
            "libraries/resume.jar",
            body.len() as u64,
            &sha1_of(&body),
            &["a"],
        ),
        &CancellationToken::new(),
        &progress,
    );

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        fs::read(root.join("libraries/resume.jar")).expect("read"),
        body
    );
    assert!(!root.join("libraries/resume.jar.part").exists());
    assert_eq!(
        transport.request_ranges(),
        vec![Some(Range {
            start: half as u64,
            end: None,
        })]
    );
    // 进度里包含续传前缀。
    let last = progress.last().expect("progress");
    assert_eq!(last.downloaded, body.len() as u64);
}

#[test]
fn misaligned_partial_response_discards_part_and_retries_full() {
    let root = TempRoot::new("misaligned");
    let body = body_of(32, 4);
    write_partial(&root, "misaligned.jar", &body[..5]);
    let transport = FakeTransport::new(vec![
        Script::Ok {
            status: 206,
            body: body[5..].to_vec(),
            range_mode: RangeMode::Explicit(ContentRange {
                start: 0,
                end: 4,
                total: Some(body.len() as u64),
            }),
        },
        Script::Ok {
            status: 200,
            body: body.clone(),
            range_mode: RangeMode::Synthesized,
        },
    ]);
    let sleeps = SleepRecorder::default();
    let result = engine(
        &transport,
        root.path(),
        2,
        Duration::ZERO,
        Duration::ZERO,
        &sleeps,
    )
    .download_artifact(
        &artifact("misaligned.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        transport.request_ranges(),
        vec![
            Some(Range {
                start: 5,
                end: None
            }),
            None,
        ]
    );
    assert_eq!(fs::read(root.join("misaligned.jar")).expect("read"), body);
    assert!(!root.join("misaligned.jar.part").exists());
    assert_eq!(sleeps.durations(), vec![Duration::ZERO]);
}

#[test]
fn partial_response_without_content_range_is_not_appended() {
    let root = TempRoot::new("missing-range");
    let body = body_of(32, 4);
    write_partial(&root, "missing.jar", &body[..5]);
    let transport = FakeTransport::new(vec![
        Script::Ok {
            status: 206,
            body: body[5..].to_vec(),
            range_mode: RangeMode::Absent,
        },
        Script::Ok {
            status: 200,
            body: body.clone(),
            range_mode: RangeMode::Synthesized,
        },
    ]);
    let sleeps = SleepRecorder::default();
    let result = engine(
        &transport,
        root.path(),
        2,
        Duration::ZERO,
        Duration::ZERO,
        &sleeps,
    )
    .download_artifact(
        &artifact("missing.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(fs::read(root.join("missing.jar")).expect("read"), body);
    assert!(!root.join("missing.jar.part").exists());
    assert_eq!(
        transport.request_ranges(),
        vec![
            Some(Range {
                start: 5,
                end: None
            }),
            None,
        ]
    );
}

#[test]
fn server_ignoring_range_restarts_from_scratch() {
    let root = TempRoot::new("ignore-range");
    let junk = body_of(4096, 200);
    write_partial(&root, "libraries/ignored.jar", &junk);
    let body = body_of(2048, 3);

    let transport = FakeTransport::new(vec![Script::Ok {
        status: 200,
        body: body.clone(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();

    let result = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact(
            "libraries/ignored.jar",
            body.len() as u64,
            &sha1_of(&body),
            &["a"],
        ),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        fs::read(root.join("libraries/ignored.jar")).expect("read"),
        body
    );
    assert_eq!(transport.request_count(), 1);
    assert_eq!(transport.request_ranges()[0], None);
    assert!(!root.join("libraries/ignored.jar.part").exists());
    assert!(sleeps.durations().is_empty());
}

#[test]
fn retries_with_exponential_backoff() {
    let root = TempRoot::new("retry");
    let body = body_of(1024, 1);
    let transport = FakeTransport::new(vec![
        Script::Connect,
        Script::Timeout,
        Script::Ok {
            status: 200,
            body: body.clone(),
            range_mode: RangeMode::Synthesized,
        },
    ]);
    let sleeps = SleepRecorder::default();

    let result = engine(
        &transport,
        root.path(),
        3,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact("retry.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(transport.request_count(), 3);
    assert_eq!(
        sleeps.durations(),
        vec![Duration::from_millis(10), Duration::from_millis(20)]
    );
    assert_eq!(fs::read(root.join("retry.jar")).expect("read"), body);
}

#[test]
fn falls_back_to_the_next_source() {
    let root = TempRoot::new("fallback");
    let body = body_of(2048, 2);
    let transport = FakeTransport::new(vec![
        Script::Connect,
        Script::Timeout,
        Script::Ok {
            status: 200,
            body: body.clone(),
            range_mode: RangeMode::Synthesized,
        },
    ]);
    let sleeps = SleepRecorder::default();
    let engine = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    );

    let result = engine.download_artifact(
        &artifact(
            "fallback.jar",
            body.len() as u64,
            &sha1_of(&body),
            &["primary", "backup"],
        ),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    assert!(result.is_ok(), "{result:?}");
    let first = source("primary").url;
    let second = source("backup").url;
    assert_eq!(transport.request_urls(), vec![first.clone(), first, second]);
    let stats = engine.provider_stats();
    // 失败按“源重试耗尽”记一次，而不是按尝试次数累计。
    assert_eq!(stats["primary"].consecutive_failures, 1);
    assert_eq!(stats["backup"].consecutive_failures, 0);
    // 每个源内部各退避一次（2 次尝试 → 1 次退避）。
    assert_eq!(sleeps.durations(), vec![Duration::from_millis(10)]);
}

#[test]
fn http_404_falls_back_without_retry_or_sleep() {
    let root = TempRoot::new("http-404");
    let body = body_of(32, 2);
    let transport = FakeTransport::new(vec![
        Script::HttpStatus(404),
        Script::Ok {
            status: 200,
            body: body.clone(),
            range_mode: RangeMode::Synthesized,
        },
    ]);
    let sleeps = SleepRecorder::default();
    let result = engine(
        &transport,
        root.path(),
        3,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact(
            "http-404.jar",
            body.len() as u64,
            &sha1_of(&body),
            &["a", "b"],
        ),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(transport.request_count(), 2);
    assert_eq!(
        transport.request_urls(),
        vec![source("a").url, source("b").url]
    );
    assert!(sleeps.durations().is_empty());
}

#[test]
fn reports_every_source_when_all_fail() {
    let root = TempRoot::new("all-fail");
    let body = body_of(64, 0);
    let transport = FakeTransport::new(vec![Script::Connect, Script::Timeout]);
    let sleeps = SleepRecorder::default();

    let result = engine(
        &transport,
        root.path(),
        1,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact("fail.jar", body.len() as u64, &sha1_of(&body), &["a", "b"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    let (destination, sources) = cancelled_ok(result);
    assert_eq!(destination, PathBuf::from("fail.jar"));
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].provider, "a");
    assert_eq!(sources[1].provider, "b");
    assert_eq!(sources[0].attempts.len(), 1);
    assert_eq!(sources[1].attempts.len(), 1);
    assert!(matches!(sources[0].attempts[0], AttemptError::Transport(_)));
    assert!(matches!(sources[1].attempts[0], AttemptError::Transport(_)));
    assert_eq!(transport.request_count(), 2);
    assert!(sleeps.durations().is_empty());
    assert!(!root.join("fail.jar").exists());
}

#[test]
fn retries_after_digest_mismatch_and_discards_the_part() {
    let root = TempRoot::new("digest");
    let body = body_of(1024, 4);
    let mut wrong = body.clone();
    wrong[0] ^= 0xff;
    assert_eq!(wrong.len(), body.len());

    let transport = FakeTransport::new(vec![
        Script::Ok {
            status: 200,
            body: wrong.clone(),
            range_mode: RangeMode::Synthesized,
        },
        Script::Ok {
            status: 200,
            body: body.clone(),
            range_mode: RangeMode::Synthesized,
        },
    ]);
    let sleeps = SleepRecorder::default();

    let result = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact("digest.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(transport.request_count(), 2);
    assert_eq!(fs::read(root.join("digest.jar")).expect("read"), body);
    assert!(!root.join("digest.jar.part").exists());
    // 摘要失败会删除 `.part`，所以第二次仍是完整请求而非续传。
    assert_eq!(transport.request_ranges(), vec![None, None]);
}

#[test]
fn rejects_bodies_of_the_wrong_size() {
    let root = TempRoot::new("size");
    let body = body_of(100, 6);
    let transport = FakeTransport::new(vec![Script::Ok {
        status: 200,
        body: body.clone(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();

    let result = engine(
        &transport,
        root.path(),
        1,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact("size.jar", 4096, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    let (_, sources) = cancelled_ok(result);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].attempts.len(), 1);
    assert!(
        matches!(
            sources[0].attempts[0],
            AttemptError::Hash(HashMismatch::Size {
                expected: 4096,
                actual: 100
            })
        ),
        "{:?}",
        sources[0].attempts[0]
    );
    assert!(!root.join("size.jar.part").exists());
    assert!(!root.join("size.jar").exists());
}

#[test]
fn rejects_artifacts_without_issuing_requests() {
    let root = TempRoot::new("preflight");
    let body = body_of(16, 0);
    let digest = sha1_of(&body);
    let transport = FakeTransport::new(Vec::new());
    let sleeps = SleepRecorder::default();
    let engine = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    );
    let cancel = CancellationToken::new();
    let progress = CollectProgress::default();

    let mut unsafe_artifact = artifact("../escape.jar", body.len() as u64, &digest, &["a"]);
    unsafe_artifact.destination = PathBuf::from("../escape.jar");
    let outcome = engine.download_artifact(&unsafe_artifact, &cancel, &progress);
    assert!(
        matches!(&outcome, Err(DownloadError::UnsafeDestination(path)) if path == Path::new("../escape.jar")),
        "{outcome:?}"
    );

    let mut no_source = artifact("no-source.jar", body.len() as u64, &digest, &[]);
    no_source.sources.clear();
    let outcome = engine.download_artifact(&no_source, &cancel, &progress);
    assert!(
        matches!(&outcome, Err(DownloadError::NoSource(path)) if path == Path::new("no-source.jar")),
        "{outcome:?}"
    );

    let invalid = artifact("invalid.jar", body.len() as u64, "not-a-digest", &["a"]);
    let outcome = engine.download_artifact(&invalid, &cancel, &progress);
    assert!(
        matches!(&outcome, Err(DownloadError::InvalidSha1(path)) if path == Path::new("invalid.jar")),
        "{outcome:?}"
    );

    assert_eq!(transport.request_count(), 0);
    assert!(sleeps.durations().is_empty());
    assert!(progress.events().is_empty());
}

#[test]
fn pre_cancelled_token_issues_no_request() {
    let root = TempRoot::new("pre-cancel");
    let body = body_of(256, 1);
    let transport = FakeTransport::new(vec![Script::Ok {
        status: 200,
        body: body.clone(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(cancel.is_cancelled());

    let result = engine(
        &transport,
        root.path(),
        3,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact("cancel.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &cancel,
        &CollectProgress::default(),
    );

    assert!(
        matches!(&result, Err(DownloadError::Cancelled(path)) if path == Path::new("cancel.jar")),
        "{result:?}"
    );
    assert_eq!(transport.request_count(), 0);
    assert!(sleeps.durations().is_empty());
    assert!(!root.join("cancel.jar.part").exists());
    assert!(!root.join("cancel.jar").exists());
}

#[test]
fn cancelling_mid_stream_keeps_the_partial_file() {
    let root = TempRoot::new("mid-cancel");
    let body = body_of(4096, 8);
    let cancel = CancellationToken::new();
    let transport = FakeTransport::new(vec![Script::CancelOnRead {
        status: 200,
        body: body.clone(),
        chunk: 100,
        cancel_on_read: 2,
        cancel: cancel.clone(),
    }]);
    let sleeps = SleepRecorder::default();

    let result = engine(
        &transport,
        root.path(),
        3,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact("mid.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &cancel,
        &CollectProgress::default(),
    );

    assert!(
        matches!(&result, Err(DownloadError::Cancelled(path)) if path == Path::new("mid.jar")),
        "{result:?}"
    );
    // 取消不重试、不换源。
    assert_eq!(transport.request_count(), 1);
    assert!(sleeps.durations().is_empty());
    assert!(!root.join("mid.jar").exists());
    let part = fs::read(root.join("mid.jar.part")).expect("partial file survives cancellation");
    // 第一次读取写入 100 字节，第二次读取后取消，因此只有第一块的字节落盘。
    assert_eq!(part, body[..100]);
}

#[test]
fn commit_replaces_existing_final_without_backup() {
    let root = TempRoot::new("commit-replace");
    let destination = root.join("replace.jar");
    fs::write(&destination, b"old").expect("write old");
    let body = body_of(32, 7);
    let transport = FakeTransport::new(vec![Script::Ok {
        status: 200,
        body: body.clone(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();
    let result = engine(
        &transport,
        root.path(),
        1,
        Duration::ZERO,
        Duration::ZERO,
        &sleeps,
    )
    .download_artifact(
        &artifact("replace.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(fs::read(&destination).expect("read"), body);
    assert!(!root.join("replace.jar.bak").exists());
    assert!(
        backup_like_files(root.path()).is_empty(),
        "替换成功后不应留下任何备份文件"
    );
}

/// 目标目录里形如 `*.bak` / `*.bak-*` 的文件名（排序后比较用）。
fn backup_like_files(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("read dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".bak"))
        .collect();
    names.sort();
    names
}

#[test]
fn commit_preserves_unrelated_backup_like_files() {
    let root = TempRoot::new("commit-user-bak");
    let destination = root.join("user.jar");
    fs::write(&destination, b"old").expect("write old");
    // 用户自己的备份文件，绝不能被引擎覆盖或删除。
    fs::write(root.join("user.jar.bak"), b"user-bak").expect("write user bak");
    fs::write(root.join("user.jar.bak-123"), b"user-bak-123").expect("write user bak 123");
    let before = backup_like_files(root.path());

    let body = body_of(24, 9);
    let transport = FakeTransport::new(vec![Script::Ok {
        status: 200,
        body: body.clone(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();
    let result = engine(
        &transport,
        root.path(),
        1,
        Duration::ZERO,
        Duration::ZERO,
        &sleeps,
    )
    .download_artifact(
        &artifact("user.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(fs::read(&destination).expect("read"), body);
    assert_eq!(
        fs::read(root.join("user.jar.bak")).expect("read user bak"),
        b"user-bak"
    );
    assert_eq!(
        fs::read(root.join("user.jar.bak-123")).expect("read user bak 123"),
        b"user-bak-123"
    );
    assert_eq!(backup_like_files(root.path()), before, "不应新增备份文件");
}

#[test]
fn failed_replace_restores_previous_file_and_leaves_no_backup() {
    let root = TempRoot::new("commit-fail");
    let destination = root.join("blocked.jar");
    fs::write(&destination, b"previous-content").expect("write previous");

    let body = body_of(16, 3);
    write_partial(&root, "blocked.jar", &body);
    let sleeps = SleepRecorder::default();
    let transport = FakeTransport::new(Vec::new());
    let commit_engine = engine(
        &transport,
        root.path(),
        1,
        Duration::ZERO,
        Duration::ZERO,
        &sleeps,
    );

    // 让替换必然失败：`.part` 在提交前被移走，于是第二次 rename 找不到源文件。
    let part = commit_engine.root.join("blocked.jar.part");
    let misplaced = commit_engine.root.join("misplaced.tmp");
    fs::rename(&part, &misplaced).expect("move part away");

    let outcome = super::commit(&part, &destination);

    assert!(outcome.is_err(), "源文件缺失时提交必须失败");
    // 旧内容必须原样恢复，用户不能同时失去新旧文件。
    assert_eq!(
        fs::read(&destination).expect("read previous"),
        b"previous-content"
    );
    assert!(
        backup_like_files(root.path()).is_empty(),
        "失败后不应留下任何备份文件"
    );
}

#[test]
fn ranged_416_restarts_with_a_full_request() {
    let root = TempRoot::new("range-416");
    let stale = body_of(1024, 77);
    write_partial(&root, "stale.jar", &stale);
    let body = body_of(2048, 12);

    let transport = FakeTransport::new(vec![Script::Ok {
        status: 200,
        body: body.clone(),
        range_mode: RangeMode::Synthesized,
    }]);
    let sleeps = SleepRecorder::default();

    let result = engine(
        &transport,
        root.path(),
        1,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact("stale.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        transport.request_ranges(),
        vec![Some(Range {
            start: 1024,
            end: None
        })]
    );
    assert_eq!(fs::read(root.join("stale.jar")).expect("read"), body);
    assert!(!root.join("stale.jar.part").exists());
    assert!(sleeps.durations().is_empty());
}

// ---------------------------------------------------------------------------
// 计划下载
// ---------------------------------------------------------------------------

fn plan_artifact(destination: &str, seed: u8) -> (DownloadArtifact, Vec<u8>) {
    let body = body_of(4096, seed);
    let artifact = artifact(destination, body.len() as u64, &sha1_of(&body), &["a"]);
    (artifact, body)
}

#[test]
fn downloads_a_plan_in_parallel_and_in_order() {
    let root = TempRoot::new("plan");
    let mut artifacts = Vec::new();
    let mut bodies = Vec::new();
    let mut pairs = Vec::new();
    for (index, seed) in [1_u8, 2, 3, 4].into_iter().enumerate() {
        let provider = format!("p{index}");
        let (artifact, body) = plan_artifact(&format!("libraries/p{index}.jar"), seed);
        let mut artifact = artifact;
        artifact.sources = vec![source(&provider)];
        // 并发时 worker 抢脚本队列的顺序不确定，改按 URL 应答保证确定性。
        pairs.push((
            source(&provider).url,
            Script::Ok {
                status: 200,
                body: body.clone(),
                range_mode: RangeMode::Synthesized,
            },
        ));
        artifacts.push(artifact);
        bodies.push(body);
    }

    let transport = FakeTransport::by_url(pairs);
    let sleeps = SleepRecorder::default();
    let plan = DownloadPlan { artifacts };
    let engine = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    );

    let results = engine
        .download_plan(
            &plan,
            2,
            &CancellationToken::new(),
            &CollectProgress::default(),
        )
        .expect("plan downloads");

    assert_eq!(results.len(), 4);
    for (index, (result, body)) in results.iter().zip(&bodies).enumerate() {
        let destination = PathBuf::from(format!("libraries/p{index}.jar"));
        assert_eq!(result.destination, destination);
        assert_eq!(
            result.outcome.as_ref().expect("ok"),
            &root.join(destination.to_str().expect("utf-8"))
        );
        assert_eq!(
            fs::read(root.join(destination.to_str().unwrap())).expect("read"),
            *body
        );
        assert!(
            !root
                .join(destination.to_str().unwrap())
                .with_extension("jar.part")
                .exists()
        );
    }
    assert_eq!(transport.request_count(), 4);

    let threads = lock(&transport.threads);
    let mut distinct: Vec<String> = threads.iter().map(|id| format!("{id:?}")).collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(
        distinct.len() >= 2,
        "expected at least two worker threads, got {distinct:?}"
    );
}

#[test]
fn plan_validation_failure_issues_no_requests() {
    let root = TempRoot::new("plan-invalid");
    let transport = FakeTransport::new(Vec::new());
    let sleeps = SleepRecorder::default();
    let plan = DownloadPlan {
        artifacts: vec![artifact("../bad.jar", 1, &"0".repeat(40), &["a"])],
    };

    let result = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_plan(
        &plan,
        2,
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    assert_eq!(
        result.expect_err("unsafe destination"),
        DownloadPlanError::UnsafeDestination(PathBuf::from("../bad.jar"))
    );
    assert_eq!(transport.request_count(), 0);
    assert!(sleeps.durations().is_empty());
}

#[test]
fn plan_with_mixed_outcomes_keeps_order() {
    let root = TempRoot::new("plan-mixed");
    let (ok_artifact, ok_body) = plan_artifact("libraries/ok.jar", 21);
    let bad_body = body_of(1024, 22);
    let bad_artifact = artifact(
        "libraries/bad.jar",
        bad_body.len() as u64,
        &sha1_of(&bad_body),
        &["dead"],
    );

    // 并发时按 URL 应答，两个源各自拿到注定好的脚本，顺序无关。
    let transport = FakeTransport::by_url(vec![
        (source("dead").url, Script::Connect),
        (
            source("a").url,
            Script::Ok {
                status: 200,
                body: ok_body.clone(),
                range_mode: RangeMode::Synthesized,
            },
        ),
    ]);
    let sleeps = SleepRecorder::default();
    let plan = DownloadPlan {
        artifacts: vec![bad_artifact, ok_artifact],
    };

    let results = engine(
        &transport,
        root.path(),
        1,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_plan(
        &plan,
        2,
        &CancellationToken::new(),
        &CollectProgress::default(),
    )
    .expect("plan downloads");

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].destination, PathBuf::from("libraries/bad.jar"));
    assert!(matches!(
        results[0].outcome,
        Err(DownloadError::AllSourcesFailed { .. })
    ));
    assert_eq!(results[1].destination, PathBuf::from("libraries/ok.jar"));
    assert_eq!(
        results[1].outcome.as_ref().expect("ok").clone(),
        root.join("libraries/ok.jar")
    );
    assert_eq!(
        fs::read(root.join("libraries/ok.jar")).expect("read"),
        ok_body
    );
    assert!(!root.join("libraries/bad.jar").exists());
}

#[test]
fn plan_without_concurrency_limit_still_completes() {
    let root = TempRoot::new("plan-zero");
    let (first, first_body) = plan_artifact("a.jar", 31);
    let (second, second_body) = plan_artifact("b.jar", 32);
    let transport = FakeTransport::new(vec![
        Script::Ok {
            status: 200,
            body: first_body.clone(),
            range_mode: RangeMode::Synthesized,
        },
        Script::Ok {
            status: 200,
            body: second_body.clone(),
            range_mode: RangeMode::Synthesized,
        },
    ]);
    let sleeps = SleepRecorder::default();
    let plan = DownloadPlan {
        artifacts: vec![first, second],
    };

    let results = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_plan(
        &plan,
        0,
        &CancellationToken::new(),
        &CollectProgress::default(),
    )
    .expect("plan downloads");

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| result.outcome.is_ok()));
    assert_eq!(fs::read(root.join("a.jar")).expect("read"), first_body);
    assert_eq!(fs::read(root.join("b.jar")).expect("read"), second_body);
}

#[test]
fn empty_plan_is_a_no_op() {
    let root = TempRoot::new("plan-empty");
    let transport = FakeTransport::new(Vec::new());
    let sleeps = SleepRecorder::default();
    let results = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_plan(
        &DownloadPlan::default(),
        8,
        &CancellationToken::new(),
        &CollectProgress::default(),
    )
    .expect("empty plan");

    assert!(results.is_empty());
    assert_eq!(transport.request_count(), 0);
}

#[test]
fn cancelled_workers_record_cancelled_entries() {
    let root = TempRoot::new("plan-cancel");
    let (first, _) = plan_artifact("a.jar", 41);
    let (second, _) = plan_artifact("b.jar", 42);
    let transport = FakeTransport::new(Vec::new());
    let sleeps = SleepRecorder::default();
    let plan = DownloadPlan {
        artifacts: vec![first, second],
    };
    let cancel = CancellationToken::new();
    cancel.cancel();

    let results = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_plan(&plan, 2, &cancel, &CollectProgress::default())
    .expect("cancelled plan still reports every artifact");

    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .all(|result| matches!(result.outcome, Err(DownloadError::Cancelled(_))))
    );
    assert_eq!(transport.request_count(), 0);
}

// ---------------------------------------------------------------------------
// RetryPolicy
// ---------------------------------------------------------------------------

#[test]
fn delay_for_attempt_follows_the_backoff_curve() {
    let policy = RetryPolicy::new(5, Duration::from_millis(10), Duration::from_secs(1));
    assert_eq!(policy.delay_for_attempt(0), NO_SLEEP);
    assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(10));
    assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(20));
    assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(40));
    assert_eq!(
        policy.delay_for_attempt(8),
        Duration::from_millis(1_000).min(Duration::from_millis(1_280))
    );
    assert_eq!(policy.delay_for_attempt(u32::MAX), Duration::from_secs(1));
}

#[test]
fn delay_for_attempt_is_capped_by_max_delay() {
    let policy = RetryPolicy::new(10, Duration::from_millis(300), Duration::from_millis(1_000));
    assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(300));
    assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(600));
    assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(1_000));
    assert_eq!(policy.delay_for_attempt(4), Duration::from_millis(1_000));
    assert_eq!(
        policy.delay_for_attempt(1_000),
        Duration::from_millis(1_000)
    );
}

#[test]
fn delay_for_attempt_handles_zero_base() {
    let policy = RetryPolicy::new(3, Duration::ZERO, Duration::from_secs(1));
    assert_eq!(policy.delay_for_attempt(0), Duration::ZERO);
    assert_eq!(policy.delay_for_attempt(1), Duration::ZERO);
    assert_eq!(policy.delay_for_attempt(5), Duration::ZERO);
}

#[test]
fn default_policy_values() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.max_attempts, 3);
    assert_eq!(policy.base_delay, Duration::from_millis(500));
    assert_eq!(policy.max_delay, Duration::from_secs(8));
    assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(500));
    assert_eq!(policy.delay_for_attempt(2), Duration::from_secs(1));
    // 500ms * 2^4 = 8s，再往后被 max_delay 截断。
    assert_eq!(policy.delay_for_attempt(5), Duration::from_secs(8));
    assert_eq!(policy.delay_for_attempt(6), Duration::from_secs(8));
}

#[test]
fn engine_defaults_are_off_the_shelf() {
    let root = TempRoot::new("defaults");
    let transport = FakeTransport::new(Vec::new());
    let engine = DownloadEngine::new(&transport, root.path());
    assert_eq!(engine.policy, RetryPolicy::default());
    assert_eq!(engine.timeout_secs, 30);
    assert_eq!(engine.trip_threshold, 3);
    assert!(engine.provider_stats().is_empty());
}

#[test]
fn builder_overrides_are_applied() {
    let root = TempRoot::new("builder");
    let transport = FakeTransport::new(Vec::new());
    let engine = DownloadEngine::new(&transport, root.path())
        .with_retry(policy(4))
        .with_timeout_secs(2)
        .with_trip_threshold(1)
        .with_sleeper(|_| {});
    assert_eq!(engine.policy, policy(4));
    assert_eq!(engine.timeout_secs, 2);
    assert_eq!(engine.trip_threshold, 1);
}

#[test]
fn cancellation_token_is_shared_between_clones() {
    let token = CancellationToken::new();
    let clone = token.clone();
    assert!(!token.is_cancelled());
    clone.cancel();
    assert!(token.is_cancelled());
    assert!(clone.is_cancelled());
    assert!(!CancellationToken::default().is_cancelled());
}

#[test]
fn noop_progress_accepts_every_call() {
    let progress = NoopProgress;
    progress.progress(Path::new("a.jar"), "a", 1, 2);
}

// ---------------------------------------------------------------------------
// 回归测试
// ---------------------------------------------------------------------------

#[test]
fn exhausted_source_reports_every_attempt_in_order() {
    let root = TempRoot::new("attempt-history");
    let body = body_of(64, 3);
    let transport = FakeTransport::new(vec![
        Script::Connect,
        Script::Timeout,
        Script::HttpStatus(500),
    ]);
    let sleeps = SleepRecorder::default();

    let result = engine(
        &transport,
        root.path(),
        3,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_artifact(
        &artifact("history.jar", body.len() as u64, &sha1_of(&body), &["a"]),
        &CancellationToken::new(),
        &CollectProgress::default(),
    );

    let (destination, sources) = cancelled_ok(result);
    assert_eq!(destination, PathBuf::from("history.jar"));
    assert_eq!(sources.len(), 1);
    assert_eq!(transport.request_count(), 3);
    let attempts = &sources[0].attempts;
    assert_eq!(attempts.len(), 3);
    assert!(matches!(
        attempts[0],
        AttemptError::Transport(TransportError::Connect { .. })
    ));
    assert!(matches!(
        attempts[1],
        AttemptError::Transport(TransportError::Timeout { .. })
    ));
    assert!(matches!(
        attempts[2],
        AttemptError::Transport(TransportError::HttpStatus { status: 500, .. })
    ));
    // 3 次尝试 → 2 次退避（仅在还有下一次尝试时才睡）。
    assert_eq!(
        sleeps.durations(),
        vec![Duration::from_millis(10), Duration::from_millis(20)]
    );
    assert!(!root.join("history.jar").exists());
}

#[test]
fn plan_results_have_one_entry_per_artifact_in_order() {
    let root = TempRoot::new("plan-length");
    let mut artifacts = Vec::new();
    // 所有 artifact 用同一份内容：并发 worker 抢脚本队列的顺序无关紧要。
    let body = body_of(4096, 51);
    let digest = sha1_of(&body);
    for index in 0..5 {
        artifacts.push(artifact(
            &format!("libraries/q{index}.jar"),
            body.len() as u64,
            &digest,
            &["a"],
        ));
    }
    let scripts = (0..5)
        .map(|_| Script::Ok {
            status: 200,
            body: body.clone(),
            range_mode: RangeMode::Synthesized,
        })
        .collect();

    let transport = FakeTransport::new(scripts);
    let sleeps = SleepRecorder::default();
    let plan = DownloadPlan { artifacts };

    let results = engine(
        &transport,
        root.path(),
        2,
        Duration::from_millis(10),
        Duration::from_secs(1),
        &sleeps,
    )
    .download_plan(
        &plan,
        3,
        &CancellationToken::new(),
        &CollectProgress::default(),
    )
    .expect("plan downloads");

    assert_eq!(results.len(), plan.artifacts.len());
    for (index, result) in results.iter().enumerate() {
        assert_eq!(
            result.destination,
            PathBuf::from(format!("libraries/q{index}.jar")),
            "result {index} out of plan order"
        );
        assert!(
            result.outcome.is_ok(),
            "artifact {index} should succeed: {:?}",
            result.outcome
        );
    }
    assert_eq!(transport.request_count(), 5);
    assert!(sleeps.durations().is_empty());
}
