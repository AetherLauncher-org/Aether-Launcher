//! Which stage of the launch failed.
//!
//! The design document (`docs/design/crash-analysis.md`) puts the phase table
//! before everything else: the phase decides which evidence is worth reading,
//! and it decides whether the launcher can answer without calling anybody.
//!
//! ```text
//! 进程很快退出且输出很短      → JvmStartupFailed
//! 出现新的 crash-reports/crash-*.txt → GameCrash
//! 出现新的 hs_err_pid*.log     → NativeCrash
//! 退出码非 0 且无崩溃报告      → LoaderFailure
//! 只有退出码                    → NoOutput
//! ```
//!
//! Two things the table leaves implicit are made explicit here:
//!
//! * a crash report wins over a JVM startup failure, because the report is the
//!   most specific evidence and a very fast exit can still leave one behind
//!   when the game crashes while the window is being created;
//! * the exit code is only one input: a process that ran for a long time and
//!   wrote a lot is classified from its output even when the exit code is
//!   missing (the launcher can lose the code if the process is terminated).
//!
//! The summary is deliberately small: it is the "本次会话证据摘要" the phase
//! decision is allowed to look at, not the whole [`LaunchEvidence`].

use serde::{Deserialize, Serialize};

use crate::evidence::LaunchEvidence;

/// The stage of the launch at which the session failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePhase {
    /// The JVM never reached the game: the process exited almost immediately
    /// with very little output.
    JvmStartupFailed,
    /// The game ran and left a new `crash-reports/crash-*.txt`.
    GameCrash,
    /// Native code killed the JVM and left `hs_err_pid*.log`.
    NativeCrash,
    /// The loader or an early launch step failed without producing a crash
    /// report.
    LoaderFailure,
    /// The process left no output at all; only the exit code remains.
    NoOutput,
    /// The evidence does not match any of the above. Treated as "we do not
    /// know yet", which is the only safe answer for an untrusted input.
    Unknown,
}

impl FailurePhase {
    /// Stable machine readable identifier used in the payload and in cache
    /// keys.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::JvmStartupFailed => "jvm_startup_failed",
            Self::GameCrash => "game_crash",
            Self::NativeCrash => "native_crash",
            Self::LoaderFailure => "loader_failure",
            Self::NoOutput => "no_output",
            Self::Unknown => "unknown",
        }
    }

    /// Chinese label for the UI. The design document asks the launcher to say
    /// what happened in the user's language.
    #[must_use]
    pub fn label_zh(self) -> &'static str {
        match self {
            Self::JvmStartupFailed => "Java 启动失败",
            Self::GameCrash => "游戏崩溃",
            Self::NativeCrash => "JVM 原生崩溃",
            Self::LoaderFailure => "加载器启动失败",
            Self::NoOutput => "进程无输出",
            Self::Unknown => "无法判断",
        }
    }

    /// Whether the phase carries enough information that a local rule can be
    /// trusted more than an external model.
    #[must_use]
    pub fn is_conclusive(self) -> bool {
        !matches!(self, Self::Unknown)
    }
}

/// The "本次会话证据摘要" the phase decision looks at.
///
/// It is a projection of [`LaunchEvidence`]: the full evidence may be
/// megabytes of log, while phase detection only needs sizes and the presence
/// of report files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Exit code of the game process, when the launcher reaped it normally.
    pub exit_code: Option<i32>,
    /// How long the process lived, in milliseconds.
    pub duration_millis: u64,
    /// Number of bytes the process wrote to stdout, stderr and the log delta.
    pub output_bytes: usize,
    /// Whether `latest.log` grew during this launch.
    pub latest_log_grew: bool,
    /// Number of crash reports that appeared during this launch.
    pub crash_report_count: usize,
    /// Number of JVM fatal error logs that appeared during this launch.
    pub hs_err_count: usize,
}

impl SessionSummary {
    /// Projects the process facts out of the full evidence.
    #[must_use]
    pub fn from_evidence(evidence: &LaunchEvidence) -> Self {
        Self {
            exit_code: evidence.exit_code,
            duration_millis: evidence.duration_millis,
            output_bytes: evidence.output_len(),
            latest_log_grew: latest_log_grew(evidence),
            crash_report_count: evidence.crash_reports.len(),
            hs_err_count: evidence.hs_err_logs.len(),
        }
    }

    /// Whether the process failed at all. A clean `0` exit is not a failure;
    /// a missing exit code is treated as a failure, because the orchestrator
    /// only asks for a diagnosis after something went wrong.
    #[must_use]
    pub fn is_failure(self) -> bool {
        self.exit_code != Some(0)
    }
}

fn latest_log_grew(evidence: &LaunchEvidence) -> bool {
    if !evidence.latest_log_delta.is_empty() {
        return true;
    }
    match (evidence.pre.latest_log_len, evidence.post.latest_log_len) {
        (None, Some(len)) => len > 0,
        (Some(before), Some(after)) => after > before,
        _ => false,
    }
}

/// The result of the phase decision: the phase plus the reasons behind it.
///
/// The reasons are short, human readable clauses in Chinese; the UI shows them
/// as "判定依据" and the payload sends them as structured strings. They never
/// contain log text, so they cannot smuggle injected content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseVerdict {
    /// The phase that was decided on.
    pub phase: FailurePhase,
    /// Ordered reasons, most decisive first.
    pub reasons: Vec<String>,
}

impl PhaseVerdict {
    fn new(phase: FailurePhase, reason: impl Into<String>) -> Self {
        Self {
            phase,
            reasons: vec![reason.into()],
        }
    }

    fn and(mut self, reason: impl Into<String>) -> Self {
        self.reasons.push(reason.into());
        self
    }

    /// A one line explanation, used when the launcher wants a single string.
    #[must_use]
    pub fn summary(&self) -> String {
        self.reasons.join("；")
    }
}

/// How long a process may live and still count as "quickly exited".
///
/// Minecraft takes upward of ten seconds to open its window even on a warm
/// cache, and the JVM prints its banner immediately. Twenty seconds leaves a
/// wide margin for slow disks while still catching "the JVM died at once".
pub const QUICK_EXIT_MILLIS: u64 = 20_000;

/// How many bytes of output still count as "very short".
///
/// A successful log4j startup writes a banner, the platform listing and the
/// resource pack scan; that is tens of kilobytes. A JVM that rejects a flag
/// writes a handful of lines, at most a couple of kilobytes.
pub const SHORT_OUTPUT_BYTES: usize = 4 * 1024;

/// Decides the phase from the session summary.
///
/// Priority, most decisive first:
///
/// 1. a new crash report means the game itself crashed;
/// 2. a new `hs_err` file means native code killed the JVM, which is a
///    stronger statement than "the loader failed";
/// 3. a quick exit with almost no output means the JVM never got anywhere;
/// 4. only an exit code and no output at all means there is nothing to read;
/// 5. anything else is a loader or an early launch step;
/// 6. a process that failed without any of these signals is unknown.
#[must_use]
pub fn decide_phase(summary: &SessionSummary) -> PhaseVerdict {
    if !summary.is_failure() {
        return PhaseVerdict::new(
            FailurePhase::Unknown,
            "进程正常退出（退出码 0），没有失败可分析",
        );
    }

    if summary.crash_report_count > 0 {
        return PhaseVerdict::new(
            FailurePhase::GameCrash,
            format!("本次启动新增 {} 个崩溃报告", summary.crash_report_count),
        );
    }

    if summary.hs_err_count > 0 {
        return PhaseVerdict::new(
            FailurePhase::NativeCrash,
            format!(
                "本次启动新增 {} 个 JVM 致命错误日志（hs_err）",
                summary.hs_err_count
            ),
        );
    }

    let output_is_empty = summary.output_bytes == 0 && !summary.latest_log_grew;

    if output_is_empty {
        let reason = match summary.exit_code {
            Some(code) => format!("进程退出码 {code}，没有任何输出"),
            None => "进程没有留下退出码，也没有任何输出".to_string(),
        };
        return PhaseVerdict::new(FailurePhase::NoOutput, reason)
            .and(format!("运行时长 {} 毫秒", summary.duration_millis));
    }

    if summary.duration_millis < QUICK_EXIT_MILLIS && summary.output_bytes < SHORT_OUTPUT_BYTES {
        return PhaseVerdict::new(
            FailurePhase::JvmStartupFailed,
            format!(
                "进程 {:.1} 秒后退出，输出只有 {} 字节",
                summary.duration_millis as f64 / 1000.0,
                summary.output_bytes
            ),
        )
        .and("没有崩溃报告，也没有 hs_err，说明游戏还没开始");
    }

    match summary.exit_code {
        Some(code) if code != 0 => PhaseVerdict::new(
            FailurePhase::LoaderFailure,
            format!(
                "退出码 {code}，没有崩溃报告；运行了 {} 毫秒、输出 {} 字节",
                summary.duration_millis, summary.output_bytes
            ),
        ),
        _ => PhaseVerdict::new(
            FailurePhase::LoaderFailure,
            "没有崩溃报告，但进程写下了日志，失败发生在加载器或启动阶段",
        ),
    }
}

/// Convenience wrapper that decides the phase straight from the evidence.
#[must_use]
pub fn classify_phase(evidence: &LaunchEvidence) -> PhaseVerdict {
    decide_phase(&SessionSummary::from_evidence(evidence))
}

#[cfg(test)]
#[path = "phase_tests.rs"]
mod tests;
