//! 下载引擎：把一份 [`DownloadPlan`] 变成磁盘上的文件。
//!
//! 引擎逐字节地把响应体写进 `<destination>.part`，边写边算 SHA-1；
//! 只有大小与摘要都匹配时才把它改名为最终文件，因此**最终文件一旦出现就一定是完整的**。
//! 中断或校验失败时 `.part` 的去留依情形而定：取消时保留（方便续传），
//! 校验失败或服务器回 416 时删除（下次从头下载）。
//!
//! 失败的处理分三层：
//!
//! 1. 单次尝试失败 → 按 [`RetryPolicy`] 退避后重试；
//! 2. 一个源的重试耗尽 → 记录 provider 失败，换下一个源（顺序由
//!    [`crate::selector`] 决定，连续失败的源会被熔断降权）；
//! 3. 全部源都失败 → [`DownloadError::AllSourcesFailed`]。
//!
//! 取消是最高优先级的信号：它在发起请求前、每次读取前、每次读取后各检查一次，
//! 一旦发现就立刻返回，不再重试、不再退避、也不再尝试别的源。

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use thiserror::Error;

use crate::hash::{HashMismatch, HashingWriter};
use crate::selector::{ProviderStat, order_sources};
use crate::transport::{HttpTransport, Range, TransportError, TransportRequest};
use crate::{DownloadArtifact, DownloadPlan, DownloadPlanError, is_safe_relative_path, is_sha1};

/// 从响应体里一次读取的上限。
const CHUNK_SIZE: usize = 64 * 1024;

/// 续传成功时服务器应返回的状态码。
const PARTIAL_CONTENT: u16 = 206;

/// 「请求的区间无法满足」：说明本地 `.part` 与远端不一致，必须从头下载。
const RANGE_NOT_SATISFIABLE: u16 = 416;

/// 默认重试次数。
const DEFAULT_MAX_ATTEMPTS: u32 = 3;

/// 默认退避基数。
const DEFAULT_BASE_DELAY: Duration = Duration::from_millis(500);

/// 默认退避上限。
const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(8);

/// 默认单次请求超时（秒）。
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// 默认熔断阈值。
const DEFAULT_TRIP_THRESHOLD: u32 = 3;

/// 取消令牌。克隆共享同一个标志，可以从任意线程触发。
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// 创建未取消的令牌。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 触发取消。重复调用无副作用。
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// 是否已被取消。
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// 进度回调。实现方必须便宜且不阻塞：它在下载线程上被频繁调用。
pub trait ProgressSink: Send + Sync {
    /// `destination` 为相对目标路径；`downloaded` 为已写入 `.part` 的字节数
    /// （含续传前缀）；`total` 为期望大小。
    fn progress(&self, destination: &Path, provider: &str, downloaded: u64, total: u64);
}

/// 不关心进度时使用。
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopProgress;

impl ProgressSink for NoopProgress {
    fn progress(&self, _destination: &Path, _provider: &str, _downloaded: u64, _total: u64) {}
}

/// 重试与退避策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// 每个源的最大尝试次数（含首次）。
    pub max_attempts: u32,
    /// 指数退避的基数。
    pub base_delay: Duration,
    /// 单次退避的上限。
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            base_delay: DEFAULT_BASE_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
        }
    }
}

impl RetryPolicy {
    /// `max_attempts` 必须 >= 1。
    #[must_use]
    pub fn new(max_attempts: u32, base_delay: Duration, max_delay: Duration) -> Self {
        assert!(max_attempts >= 1, "max_attempts must be at least 1");
        Self {
            max_attempts,
            base_delay,
            max_delay,
        }
    }

    /// 第 `failed_attempt` 次尝试失败后退避多久：`0` 表示不退避；
    /// 否则是 `base * 2^(failed_attempt-1)`，被 `max_delay` 截断，溢出时取 `max_delay`。
    #[must_use]
    pub fn delay_for_attempt(&self, failed_attempt: u32) -> Duration {
        let Some(exponent) = failed_attempt.checked_sub(1) else {
            return Duration::ZERO;
        };
        if self.base_delay.is_zero() {
            return Duration::ZERO;
        }
        match 2_u32.checked_pow(exponent) {
            Some(factor) => self
                .base_delay
                .checked_mul(factor)
                .map_or(self.max_delay, |delay| delay.min(self.max_delay)),
            None => self.max_delay,
        }
    }
}

/// 单次尝试的失败原因。
#[derive(Debug, Error)]
pub enum AttemptError {
    /// 请求或读取响应体失败。
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// 内容与期望不符。
    #[error(transparent)]
    Hash(#[from] HashMismatch),
    /// 本地文件操作失败。
    #[error("local file operation failed: {0}")]
    Io(#[from] io::Error),
    /// 用户取消。
    #[error("download cancelled")]
    Cancelled,
}

/// 一个下载源上的全部尝试记录。
#[derive(Debug)]
pub struct SourceFailure {
    /// 下载源名称。
    pub provider: String,
    /// 下载源地址。
    pub url: String,
    /// 每次尝试的失败原因，按发生顺序。
    pub attempts: Vec<AttemptError>,
}

/// 单个文件的下载失败。
#[derive(Debug, Error)]
pub enum DownloadError {
    /// 目标路径不安全（绝对路径、含 `..` 等）。
    #[error("artifact destination {0:?} is not a safe relative path")]
    UnsafeDestination(PathBuf),
    /// 没有任何下载源。
    #[error("artifact {0:?} has no download source")]
    NoSource(PathBuf),
    /// SHA-1 字段不是合法的摘要。
    #[error("artifact {0:?} has an invalid SHA-1 digest")]
    InvalidSha1(PathBuf),
    /// 所有下载源都失败了。
    #[error("all sources failed for {:?}", destination)]
    AllSourcesFailed {
        /// 相对目标路径。
        destination: PathBuf,
        /// 每个源各自的失败记录。
        sources: Vec<SourceFailure>,
    },
    /// 下载被取消。
    #[error("download of {0:?} cancelled")]
    Cancelled(PathBuf),
    /// 计划的某个条目没有产出结果（工作线程异常退出）。
    #[error("artifact {0:?} produced no result")]
    Incomplete(PathBuf),
}

/// 计划中单个文件的下载结果。
#[derive(Debug)]
pub struct ArtifactResult {
    /// 相对目标路径。
    pub destination: PathBuf,
    /// 成功时给出最终文件的完整路径。
    pub outcome: Result<PathBuf, DownloadError>,
}

/// 单次尝试的结局。
enum Outcome {
    /// 文件已提交到最终路径。
    Done,
    /// 本次尝试失败，可以重试。
    Failed(AttemptError),
    /// 被取消：不重试、不换源、保留 `.part`。
    Cancelled,
}

/// 单个源上重试循环的结局。
enum SourceOutcome {
    /// 重试耗尽：按发生顺序记录每一次尝试的失败原因。
    Exhausted(Vec<AttemptError>),
    /// 被取消：不重试、不换源、保留 `.part`。
    Cancelled,
}

/// 下载引擎。用 builder 方法调整策略后再调用下载方法。
pub struct DownloadEngine<'a> {
    transport: &'a dyn HttpTransport,
    root: &'a Path,
    policy: RetryPolicy,
    timeout_secs: u64,
    trip_threshold: u32,
    sleeper: Box<dyn Fn(Duration) + Send + Sync>,
    history: Mutex<HashMap<String, ProviderStat>>,
}

impl<'a> DownloadEngine<'a> {
    /// 默认：重试 3 次、500ms 基数退避、30s 超时、熔断阈值 3、真实 `thread::sleep`。
    #[must_use]
    pub fn new(transport: &'a dyn HttpTransport, root: &'a Path) -> Self {
        Self {
            transport,
            root,
            policy: RetryPolicy::default(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            trip_threshold: DEFAULT_TRIP_THRESHOLD,
            sleeper: Box::new(thread::sleep),
            history: Mutex::new(HashMap::new()),
        }
    }

    /// 替换重试策略。
    #[must_use]
    pub fn with_retry(mut self, policy: RetryPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// 替换等待实现。测试注入记录器，永不真睡。
    #[must_use]
    pub fn with_sleeper(mut self, sleeper: impl Fn(Duration) + Send + Sync + 'static) -> Self {
        self.sleeper = Box::new(sleeper);
        self
    }

    /// 设置单次请求超时（秒）。
    #[must_use]
    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// 设置熔断阈值：连续失败达到该值的源会被降权。`0` 关闭熔断。
    #[must_use]
    pub fn with_trip_threshold(mut self, threshold: u32) -> Self {
        self.trip_threshold = threshold;
        self
    }

    /// 当前会话的 provider 统计快照。
    #[must_use]
    pub fn provider_stats(&self) -> HashMap<String, ProviderStat> {
        self.history().clone()
    }

    /// 下载单个文件。
    ///
    /// 校验、排序、重试、续传、校验和都封装在这里；返回最终文件的完整路径。
    pub fn download_artifact(
        &self,
        artifact: &DownloadArtifact,
        cancel: &CancellationToken,
        progress: &dyn ProgressSink,
    ) -> Result<PathBuf, DownloadError> {
        let destination = &artifact.destination;
        if !is_safe_relative_path(destination) {
            return Err(DownloadError::UnsafeDestination(destination.clone()));
        }
        if artifact.sources.is_empty() {
            return Err(DownloadError::NoSource(destination.clone()));
        }
        if !is_sha1(&artifact.sha1) {
            return Err(DownloadError::InvalidSha1(destination.clone()));
        }
        // 预取消必须一次请求都不发。
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled(destination.clone()));
        }

        let ordered = {
            let history = self.history();
            order_sources(&artifact.sources, &history, self.trip_threshold)
        };
        let dest_full = self.root.join(destination);
        let mut failures = Vec::with_capacity(ordered.len());

        for source in ordered {
            match self.attempt_source(artifact, source, &dest_full, cancel, progress) {
                Ok(()) => {
                    self.record(&source.provider, ProviderStat::record_success);
                    return Ok(dest_full);
                }
                Err(SourceOutcome::Cancelled) => {
                    return Err(DownloadError::Cancelled(destination.clone()));
                }
                Err(SourceOutcome::Exhausted(attempts)) => {
                    self.record(&source.provider, ProviderStat::record_failure);
                    failures.push(SourceFailure {
                        provider: source.provider.clone(),
                        url: source.url.clone(),
                        attempts,
                    });
                }
            }
        }

        Err(DownloadError::AllSourcesFailed {
            destination: destination.clone(),
            sources: failures,
        })
    }

    /// 下载整个计划。
    ///
    /// 先校验计划，再用 `concurrency` 个线程并行下载；结果按计划顺序返回。
    pub fn download_plan(
        &self,
        plan: &DownloadPlan,
        concurrency: usize,
        cancel: &CancellationToken,
        progress: &dyn ProgressSink,
    ) -> Result<Vec<ArtifactResult>, DownloadPlanError> {
        plan.validate()?;
        let count = plan.artifacts.len();
        if count == 0 {
            return Ok(Vec::new());
        }

        let workers = concurrency.max(1).min(count);
        let results: Mutex<Vec<Option<ArtifactResult>>> =
            Mutex::new((0..count).map(|_| None).collect());

        thread::scope(|scope| {
            let (sender, receiver) = mpsc::sync_channel::<usize>(workers);
            let receiver: Arc<Mutex<Receiver<usize>>> = Arc::new(Mutex::new(receiver));

            for _ in 0..workers {
                let receiver = Arc::clone(&receiver);
                let results = &results;
                scope.spawn(move || {
                    loop {
                        let index = {
                            let channel = lock(&receiver);
                            // 发送端随时可能退出，此时通道关闭即代表「没有更多任务」。
                            match channel.recv() {
                                Ok(index) => index,
                                Err(_) => break,
                            }
                        };
                        let artifact = &plan.artifacts[index];
                        let outcome = if cancel.is_cancelled() {
                            // 取消后不再发请求，但仍要为每个条目留下结果。
                            Err(DownloadError::Cancelled(artifact.destination.clone()))
                        } else {
                            self.download_artifact(artifact, cancel, progress)
                        };
                        lock(results)[index] = Some(ArtifactResult {
                            destination: artifact.destination.clone(),
                            outcome,
                        });
                    }
                });
            }

            // 有界通道 + 主线程投喂：任何时刻队列里最多积压 `workers` 个下标
            // （容量为 `workers`，阻塞式发送，不会提前预取整个计划）。
            // 没有 worker 能接收时发送失败，说明无人再需要任务，提前停止投喂。
            for index in 0..count {
                if sender.send(index).is_err() {
                    break;
                }
            }
            drop(sender);
        });

        // 每个下标恰好投喂一次，正常情况下不会触发；这里显式兜底而不是 panic。
        Ok(lock(&results)
            .iter_mut()
            .enumerate()
            .map(|(index, slot)| {
                slot.take().unwrap_or_else(|| ArtifactResult {
                    destination: plan.artifacts[index].destination.clone(),
                    outcome: Err(DownloadError::Incomplete(
                        plan.artifacts[index].destination.clone(),
                    )),
                })
            })
            .collect())
    }

    /// 按顺序在单个源上尝试，直到成功、被取消或重试耗尽。
    fn attempt_source(
        &self,
        artifact: &DownloadArtifact,
        source: &crate::ArtifactSource,
        dest_full: &Path,
        cancel: &CancellationToken,
        progress: &dyn ProgressSink,
    ) -> Result<(), SourceOutcome> {
        let part = part_path(dest_full);
        let mut attempts = Vec::with_capacity(self.policy.max_attempts as usize);

        for attempt in 1..=self.policy.max_attempts {
            if cancel.is_cancelled() {
                return Err(SourceOutcome::Cancelled);
            }

            let failed =
                match self.attempt_once(artifact, source, dest_full, &part, cancel, progress) {
                    Outcome::Done => return Ok(()),
                    Outcome::Cancelled => return Err(SourceOutcome::Cancelled),
                    Outcome::Failed(error) => error,
                };

            let retryable = is_retryable(&failed);
            if wants_clean_slate(&failed) {
                remove_if_exists(&part)
                    .map_err(|error| SourceOutcome::Exhausted(vec![AttemptError::Io(error)]))?;
            }
            attempts.push(failed);

            if attempt < self.policy.max_attempts && retryable {
                (self.sleeper)(self.policy.delay_for_attempt(attempt));
            } else if !retryable {
                break;
            }
        }

        Err(SourceOutcome::Exhausted(attempts))
    }

    /// 执行一次完整的「请求 → 写入 `.part` → 校验 → 提交」。
    fn attempt_once(
        &self,
        artifact: &DownloadArtifact,
        source: &crate::ArtifactSource,
        dest_full: &Path,
        part: &Path,
        cancel: &CancellationToken,
        progress: &dyn ProgressSink,
    ) -> Outcome {
        match self.stream_into_part(artifact, source, dest_full, part, cancel, progress) {
            Ok(()) => {
                if let Err(error) = commit(part, dest_full) {
                    return Outcome::Failed(AttemptError::Io(error));
                }
                Outcome::Done
            }
            // 取消时保留 `.part`，下次可以续传。
            Err(AttemptError::Cancelled) => Outcome::Cancelled,
            Err(error) => Outcome::Failed(error),
        }
    }

    /// 一次尝试的 I/O 部分：截断或续写 `.part` 并校验内容。
    fn stream_into_part(
        &self,
        artifact: &DownloadArtifact,
        source: &crate::ArtifactSource,
        dest_full: &Path,
        part: &Path,
        cancel: &CancellationToken,
        progress: &dyn ProgressSink,
    ) -> Result<(), AttemptError> {
        // Reject pre-existing links before touching the directory tree, then
        // recheck after creating any missing parent directories.
        reject_linked_destination(self.root, &artifact.destination)?;
        if let Some(parent) = dest_full.parent() {
            fs::create_dir_all(parent)?;
        }
        reject_linked_destination(self.root, &artifact.destination)?;

        let mut existing = part_length(part);
        if existing >= artifact.size && existing > 0 {
            // 即使长度恰好相等也不代表内容已验证，简单删除后重新下载。
            remove_if_exists(part)?;
            existing = 0;
        }
        let request = TransportRequest {
            url: source.url.clone(),
            range: existing.gt(&0).then_some(Range {
                start: existing,
                end: None,
            }),
            timeout_secs: self.timeout_secs,
        };
        let mut response = self.transport.get(&request)?;
        let resume = existing > 0
            && response.status == PARTIAL_CONTENT
            && response.supports_range
            && response.content_range.map(|range| range.start) == Some(existing);
        if existing > 0 && response.status == PARTIAL_CONTENT && !resume {
            // Content-Range 对不上本地前缀时，禁止拼接错误偏移（最危险的静默损坏）。
            remove_if_exists(part)?;
            return Err(AttemptError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "misaligned Content-Range",
            )));
        }

        let mut writer = if resume {
            let file = OpenOptions::new().create(true).append(true).open(part)?;
            let mut writer = HashingWriter::new(file);
            // 把已有前缀纳入摘要与计数，续传后才能与完整文件比对。
            writer.absorb_existing(File::open(part)?)?;
            writer
        } else {
            // 服务器忽略了 Range（200）：丢弃旧的 `.part`，从头写。
            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(part)?;
            HashingWriter::new(file)
        };

        let mut buffer = vec![0_u8; CHUNK_SIZE];
        loop {
            if cancel.is_cancelled() {
                return Err(AttemptError::Cancelled);
            }
            let read = response.body.read(&mut buffer).map_err(|source| {
                AttemptError::Transport(TransportError::ReadInterrupted {
                    url: request.url.clone(),
                    source,
                })
            })?;
            if read == 0 {
                break;
            }
            // 读取可能很慢，返回后再确认一次，避免把已取消的数据写进磁盘。
            if cancel.is_cancelled() {
                return Err(AttemptError::Cancelled);
            }
            io::Write::write_all(&mut writer, &buffer[..read])?;
            progress.progress(
                &artifact.destination,
                &source.provider,
                writer.total_bytes(),
                artifact.size,
            );
        }

        let (file, total, sha1) = writer.finish();
        file.sync_all()?;

        // 校验失败必须让「明文」消失，避免下一轮续传接着一份坏前缀。
        if total != artifact.size {
            remove_if_exists(part)?;
            return Err(HashMismatch::Size {
                expected: artifact.size,
                actual: total,
            }
            .into());
        }
        if sha1 != artifact.sha1 {
            remove_if_exists(part)?;
            return Err(HashMismatch::Sha1 {
                expected: artifact.sha1.clone(),
                actual: sha1,
            }
            .into());
        }
        Ok(())
    }

    /// 在历史记录上对某个 provider 做一次原子更新。
    fn record(&self, provider: &str, update: impl FnOnce(&mut ProviderStat) -> &mut ProviderStat) {
        let mut history = self.history();
        update(history.entry(provider.to_owned()).or_default());
    }

    fn history(&self) -> MutexGuard<'_, HashMap<String, ProviderStat>> {
        lock(&self.history)
    }
}

/// `<destination>.part`，与最终文件同目录，便于原子改名。
fn part_path(dest_full: &Path) -> PathBuf {
    let mut name = dest_full.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest_full.with_file_name(name)
}

/// Refuse pre-existing filesystem links inside the managed download tree.
/// A lexically safe relative path can still escape through a symlinked parent
/// or a symlinked .part file. This is a preflight check, not a substitute for
/// OS-level no-follow/openat primitives against concurrent local attackers.
fn reject_linked_destination(root: &Path, destination: &Path) -> io::Result<()> {
    fn reject_link(path: &Path) -> io::Result<()> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "download target contains a symbolic link",
            )),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    reject_link(root)?;
    let mut current = root.to_path_buf();
    for component in destination.components() {
        current.push(component.as_os_str());
        reject_link(&current)?;
    }
    reject_link(&part_path(&root.join(destination)))
}

/// `.part` 的现有长度；不存在（或不是常规文件）时视为 0。
fn part_length(part: &Path) -> u64 {
    fs::metadata(part).map_or(0, |metadata| {
        if metadata.is_file() {
            metadata.len()
        } else {
            0
        }
    })
}

/// 按失败性质决定是否值得在同一下载源上再次尝试。
/// 网络瞬断、服务端限流/故障、本地 I/O 与内容校验失败可恢复；其他 HTTP
/// 4xx 和所有 3xx 都不会因重试而改变，应立即转到下一个源。
fn is_retryable(error: &AttemptError) -> bool {
    match error {
        AttemptError::Transport(TransportError::Connect { .. })
        | AttemptError::Transport(TransportError::Timeout { .. })
        | AttemptError::Transport(TransportError::ReadInterrupted { .. })
        | AttemptError::Io(_)
        | AttemptError::Hash(_) => true,
        AttemptError::Transport(TransportError::HttpStatus { status, .. }) => {
            *status == 429 || *status >= 500
        }
        AttemptError::Cancelled => false,
    }
}

/// 这个失败是否意味着 `.part` 已经不可信。
fn wants_clean_slate(error: &AttemptError) -> bool {
    matches!(
        error,
        AttemptError::Transport(TransportError::HttpStatus {
            status: RANGE_NOT_SATISFIABLE,
            ..
        })
    )
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// 把 `.part` 改名成最终文件。
///
/// 崩溃安全约定：
///
/// * 最终路径只会出现「已校验完整」的内容；
/// * 替换已有文件时，旧文件先被移到一个**进程内唯一**的备份名
///   （`<文件名>.bak-<pid>-<序号>`，同目录），绝不会覆盖用户自己的 `.bak`；
/// * 替换失败时把备份恢复回原位，因此用户不会同时失去旧文件和新文件；
///   若连恢复都失败，错误消息会同时包含两次失败的原因，不会静默吞掉；
/// * 替换成功后清理备份失败不算失败（新文件已经就位，残留备份无害）。
///
/// Windows 不允许 `rename` 覆盖已存在的文件，所以「先试一次直接改名」在其他
/// 平台上成功、在 Windows 上失败后才走备份替换。
fn commit(part: &Path, dest_full: &Path) -> io::Result<()> {
    if !dest_full.exists() {
        return fs::rename(part, dest_full);
    }
    if dest_full.is_dir() {
        // 目标是目录（用户自己建的东西或损坏的布局）：不搬走、更不删除它。
        return Err(io::Error::other(format!(
            "cannot replace {dest_full:?}: the destination is an existing directory"
        )));
    }
    // 非 Windows 平台可以直接覆盖，省掉备份。
    if fs::rename(part, dest_full).is_ok() {
        return Ok(());
    }

    let backup = unique_backup_path(dest_full);
    fs::rename(dest_full, &backup)?;
    match fs::rename(part, dest_full) {
        Ok(()) => {
            // 新文件已就位；备份清理失败只留下一个无害的临时文件，不影响结果。
            let _ = fs::remove_file(&backup);
            Ok(())
        }
        Err(replace_error) => match fs::rename(&backup, dest_full) {
            Ok(()) => Err(replace_error),
            Err(restore_error) => Err(io::Error::other(format!(
                "failed to replace {dest_full:?}: {replace_error}; \
                     additionally failed to restore the previous file from \
                     {backup:?}: {restore_error}"
            ))),
        },
    }
}

/// 生成一个当前不存在、且不会被用户文件占用的备份路径。
///
/// 形如 `<文件名>.bak-<pid>-<序号>`，与目标同目录（保证 `rename` 不跨卷）。
/// 极端情况下候选名已被占用就递增序号重试，绝不删除已存在的文件。
fn unique_backup_path(dest_full: &Path) -> PathBuf {
    static NEXT_BACKUP_ID: AtomicU64 = AtomicU64::new(1);
    loop {
        let sequence = NEXT_BACKUP_ID.fetch_add(1, Ordering::Relaxed);
        let mut name = dest_full.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".bak-{}-{sequence}", std::process::id()));
        let candidate = dest_full.with_file_name(name);
        if !candidate.exists() {
            return candidate;
        }
    }
}

/// 取锁，遇到中毒时继续使用内部数据：引擎的统计没有不变量，毒化不影响正确性。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
