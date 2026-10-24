//! The evidence of a single launch session.
//!
//! The orchestrator captures a [`PreLaunchSnapshot`] right before spawning the
//! game and a [`PostExitSnapshot`] right after the process exits. This module
//! turns the two into [`LaunchEvidence`], which is the only input the rest of
//! the crate sees. Everything here is deterministic and works on plain data;
//! nothing touches the file system.
//!
//! Only what this launch added is interesting. Minecraft's log4j rotates
//! `latest.log` on the next start, and crash reports pile up in the instance
//! directory, so the crate compares the length of `latest.log` and the newest
//! timestamp of every evidence directory to isolate the new part.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// A length measured in bytes, kept as `u64` so callers do not have to think
/// about file size limits.
pub type ByteLength = u64;

/// An opaque, absolute timestamp in whole seconds since the Unix epoch.
///
/// The crate never formats or compares this against a wall clock; it only
/// compares timestamps that the orchestrator itself produced.
pub type UnixTimestamp = i64;

/// State the orchestrator records before the game process is started.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreLaunchSnapshot {
    /// Length of `logs/latest.log` before the launch, or `None` when the file
    /// did not exist yet.
    pub latest_log_len: Option<ByteLength>,
    /// Newest modification timestamp of `crash-reports/` before the launch.
    pub crash_reports_newest: Option<UnixTimestamp>,
    /// Newest modification timestamp of the `hs_err` search roots before the
    /// launch.
    pub hs_err_newest: Option<UnixTimestamp>,
    /// Newest modification timestamp of `logs/` before the launch.
    pub logs_newest: Option<UnixTimestamp>,
}

/// State the orchestrator records after the game process has been reaped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostExitSnapshot {
    /// Length of `logs/latest.log` after the launch, or `None` when the file is
    /// still missing.
    pub latest_log_len: Option<ByteLength>,
    /// Newest modification timestamp of `crash-reports/` after the launch.
    pub crash_reports_newest: Option<UnixTimestamp>,
    /// Newest modification timestamp of the `hs_err` search roots after the
    /// launch.
    pub hs_err_newest: Option<UnixTimestamp>,
    /// Newest modification timestamp of `logs/` after the launch.
    pub logs_newest: Option<UnixTimestamp>,
}

/// A directory whose newest file timestamp is tracked. It is deliberately not
/// an absolute path: callers describe the directory by name, the crate never
/// needs to know what happens to be on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceDirectory {
    /// `<instance>/logs`
    Logs,
    /// `<instance>/crash-reports`
    CrashReports,
    /// `<instance>` root, where the JVM writes `hs_err_pid<pid>.log`
    InstanceRoot,
}

impl EvidenceDirectory {
    /// Stable machine readable name, also used in the JSON payload.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Logs => "logs",
            Self::CrashReports => "crash-reports",
            Self::InstanceRoot => "instance-root",
        }
    }
}

/// Everything known about one launch session, ready for classification.
///
/// The struct is meant to be built by the orchestrator and then handed to
/// [`crate::analyze_session`]. All text fields may contain anything the game
/// or a mod wrote, including hostile markup; treat them as untrusted data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchEvidence {
    /// The launch command line, as passed to the JVM.
    pub jvm_arguments: Vec<String>,
    /// Major version of the Java runtime the launcher selected.
    pub java_major: Option<u16>,
    /// Exit code of the game process, if it was reaped normally.
    pub exit_code: Option<i32>,
    /// Wall clock duration of the process in milliseconds.
    pub duration_millis: u64,
    /// The snapshot captured before the launch.
    pub pre: PreLaunchSnapshot,
    /// The snapshot captured after the process exited.
    pub post: PostExitSnapshot,
    /// The bytes this launch appended to `logs/latest.log`.
    pub latest_log_delta: String,
    /// `true` when `latest.log` was rotated or truncated during the launch, so
    /// [`Self::latest_log_delta`] is not a strict append of the old content.
    pub latest_log_rotated: bool,
    /// Everything the launcher captured on the child's standard output.
    pub stdout: String,
    /// Everything the launcher captured on the child's standard error.
    pub stderr: String,
    /// File names of crash reports that appeared during this launch, e.g.
    /// `crash-2025-01-02_03.04.05-client.txt`.
    pub crash_reports: Vec<String>,
    /// File names of JVM fatal error logs that appeared during this launch,
    /// e.g. `hs_err_pid1234.log`.
    pub hs_err_logs: Vec<String>,
    /// Optional free-form system and Java information block, already
    /// assembled by the orchestrator (`os.name`, `java.version`, …).
    pub system_info: Option<String>,
}

impl LaunchEvidence {
    /// The full text a local rule may scan, in a stable order: the `latest.log`
    /// delta first, then standard error, then standard output.
    ///
    /// The order matters for rule priority: loader output lands in
    /// `latest.log`, while a JVM that fails before log4j starts only writes to
    /// standard error.
    #[must_use]
    pub fn combined_text(&self) -> String {
        let mut text = String::with_capacity(
            self.latest_log_delta.len() + self.stderr.len() + self.stdout.len() + 8,
        );
        push_block(&mut text, &self.latest_log_delta);
        push_block(&mut text, &self.stderr);
        push_block(&mut text, &self.stdout);
        text
    }

    /// Whether the launcher captured any text at all.
    #[must_use]
    pub fn has_any_output(&self) -> bool {
        !(self.latest_log_delta.is_empty() && self.stdout.is_empty() && self.stderr.is_empty())
    }

    /// Total number of bytes the game produced on its own streams and in the
    /// log delta.
    #[must_use]
    pub fn output_len(&self) -> usize {
        self.latest_log_delta.len() + self.stdout.len() + self.stderr.len()
    }
}

fn push_block(target: &mut String, block: &str) {
    if block.is_empty() {
        return;
    }
    if !target.is_empty() && !target.ends_with('\n') {
        target.push('\n');
    }
    target.push_str(block);
}

/// What happened between the two snapshots.
///
/// The orchestrator can either build the evidence directly, or describe the
/// change with [`EvidenceSession::from_snapshots`] and convert it with
/// [`EvidenceSession::into_evidence`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EvidenceSession {
    /// The snapshot captured before the launch.
    pub pre: PreLaunchSnapshot,
    /// The snapshot captured after the process exited.
    pub post: PostExitSnapshot,
    /// Whether `latest.log` grew during the launch.
    pub latest_log_grew: bool,
    /// Whether a crash report appeared during the launch.
    pub crash_report_appeared: bool,
    /// Whether a JVM fatal error log appeared during the launch.
    pub hs_err_appeared: bool,
    crash_reports: Vec<String>,
    hs_err_logs: Vec<String>,
}

impl EvidenceSession {
    /// Builds the session from two snapshots alone.
    #[must_use]
    pub fn new(pre: PreLaunchSnapshot, post: PostExitSnapshot) -> Self {
        Self::from_snapshots(pre, post, &[], &[])
    }

    /// Builds the session while keeping the *names* of the new files, so the
    /// payload can mention them later. Names that do not match the documented
    /// patterns are dropped.
    #[must_use]
    pub fn from_snapshots(
        pre: PreLaunchSnapshot,
        post: PostExitSnapshot,
        crash_reports: &[String],
        hs_err_logs: &[String],
    ) -> Self {
        let session = Self {
            latest_log_grew: grew(pre.latest_log_len, post.latest_log_len),
            crash_report_appeared: appeared(pre.crash_reports_newest, post.crash_reports_newest),
            hs_err_appeared: appeared(pre.hs_err_newest, post.hs_err_newest),
            pre,
            post,
            crash_reports: Vec::new(),
            hs_err_logs: Vec::new(),
        };
        session.with_files(crash_reports, hs_err_logs)
    }

    /// Adds the file names the orchestrator saw appear.
    #[must_use]
    pub fn with_files(mut self, crash_reports: &[String], hs_err_logs: &[String]) -> Self {
        self.crash_reports = crash_reports
            .iter()
            .filter(|name| is_crash_report_name(name))
            .cloned()
            .collect();
        self.hs_err_logs = hs_err_logs
            .iter()
            .filter(|name| is_hs_err_name(name))
            .cloned()
            .collect();
        self.crash_report_appeared |= !self.crash_reports.is_empty();
        self.hs_err_appeared |= !self.hs_err_logs.is_empty();
        self
    }

    /// Crash report names recorded for this session.
    #[must_use]
    pub fn crash_reports(&self) -> &[String] {
        &self.crash_reports
    }

    /// JVM fatal error log names recorded for this session.
    #[must_use]
    pub fn hs_err_logs(&self) -> &[String] {
        &self.hs_err_logs
    }

    /// File names of crash reports and fatal error logs that appeared during
    /// this launch, deduplicated and sorted for a stable payload.
    #[must_use]
    pub fn new_files(&self) -> Vec<String> {
        let mut names = BTreeSet::new();
        names.extend(self.crash_reports.iter().cloned());
        names.extend(self.hs_err_logs.iter().cloned());
        names.into_iter().collect()
    }

    /// Turns the session into a [`LaunchEvidence`] with the given text
    /// snapshots and process facts.
    #[must_use]
    pub fn into_evidence(
        self,
        jvm_arguments: Vec<String>,
        exit_code: Option<i32>,
        duration_millis: u64,
        captured: CapturedText,
    ) -> LaunchEvidence {
        LaunchEvidence {
            jvm_arguments,
            java_major: captured.java_major,
            exit_code,
            duration_millis,
            latest_log_delta: captured.latest_log_delta,
            latest_log_rotated: captured.latest_log_rotated,
            stdout: captured.stdout,
            stderr: captured.stderr,
            crash_reports: self.crash_reports,
            hs_err_logs: self.hs_err_logs,
            system_info: captured.system_info,
            pre: self.pre,
            post: self.post,
        }
    }
}

/// The text and process facts captured around one launch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapturedText {
    /// The new part of `logs/latest.log`.
    pub latest_log_delta: String,
    /// Whether `latest.log` was rotated or truncated during the launch.
    pub latest_log_rotated: bool,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Optional system and Java information block.
    pub system_info: Option<String>,
    /// Major version of the selected Java runtime.
    pub java_major: Option<u16>,
}

fn grew(pre: Option<ByteLength>, post: Option<ByteLength>) -> bool {
    match (pre, post) {
        (None, Some(len)) => len > 0,
        (Some(before), Some(after)) => after > before,
        // The file vanished or was never created: not an attributable growth.
        (Some(_), None) | (None, None) => false,
    }
}

fn appeared(pre: Option<UnixTimestamp>, post: Option<UnixTimestamp>) -> bool {
    match (pre, post) {
        (None, Some(_)) => true,
        (Some(before), Some(after)) => after >= before.saturating_add(1),
        _ => false,
    }
}

/// A crash report is named `crash-YYYY-MM-DD_HH.MM.SS-<side>.txt`.
#[must_use]
pub fn is_crash_report_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("crash-")
        .and_then(|r| r.strip_suffix(".txt"))
    else {
        return false;
    };
    let Some((date, remainder)) = stem.split_once('_') else {
        return false;
    };
    let date_bytes = date.as_bytes();
    let has_date = date_bytes.len() == 10
        && date_bytes[4] == b'-'
        && date_bytes[7] == b'-'
        && date_bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit());
    if !has_date {
        return false;
    }
    let Some((time, side)) = remainder.rsplit_once('-') else {
        return false;
    };
    if side.is_empty() || !side.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return false;
    }
    let time_bytes = time.as_bytes();
    time_bytes.len() == 8
        && time_bytes[2] == b'.'
        && time_bytes[5] == b'.'
        && time_bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 2 | 5) || byte.is_ascii_digit())
}

/// A JVM fatal error log is named `hs_err_pid<digits>.log`.
#[must_use]
pub fn is_hs_err_name(name: &str) -> bool {
    let Some(digits) = name
        .strip_prefix("hs_err_pid")
        .and_then(|r| r.strip_suffix(".log"))
    else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Cuts the part of a `latest.log` that belongs to this launch.
///
/// `known_len` is the length recorded before the launch and `full` is the
/// complete file content read after the process exited. The function handles
/// the two cases that actually happen in the wild:
///
/// * the file simply grew → the bytes after `known_len` are new;
/// * the file was rotated or truncated and is now shorter than before →
///   everything is new, because the old content is already gone.
///
/// When `known_len` does not fall on a character boundary of `full`, the cut
/// moves forward to the next boundary instead of panicking.
#[must_use]
pub fn new_since_known_len(known_len: ByteLength, full: &str) -> &str {
    let known_len = usize::try_from(known_len).unwrap_or(usize::MAX);
    if known_len > full.len() {
        // A shorter file means truncation or rotation: all of it is new.
        return full;
    }
    if known_len == full.len() {
        return "";
    }
    let mut cut = known_len;
    while cut < full.len() && !full.is_char_boundary(cut) {
        cut += 1;
    }
    &full[cut..]
}

/// How the post-exit `latest.log` relates to the pre-launch length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogDeltaKind {
    /// There is no `latest.log` at all.
    Missing,
    /// The file is shorter than before: it was rotated or truncated.
    Rotated,
    /// The file has exactly the recorded length.
    Unchanged,
    /// The file grew; the tail is new.
    Appended,
    /// The file did not exist before and has content now.
    Fresh,
}

impl LogDeltaKind {
    /// Whether the tail of the file can be attributed to this launch.
    #[must_use]
    pub fn has_new_content(self) -> bool {
        matches!(self, Self::Appended | Self::Fresh | Self::Rotated)
    }
}

/// Classifies the relationship between the recorded length and the current
/// content, so the orchestrator can tell the payload that the log was rotated.
#[must_use]
pub fn delta_kind(known_len: Option<ByteLength>, full: Option<&str>) -> LogDeltaKind {
    match (known_len, full) {
        (_, None) => LogDeltaKind::Missing,
        (None, Some("")) => LogDeltaKind::Missing,
        (None, Some(_)) => LogDeltaKind::Fresh,
        (Some(known), Some(content)) => {
            let known = usize::try_from(known).unwrap_or(usize::MAX);
            if known > content.len() {
                LogDeltaKind::Rotated
            } else if known == content.len() {
                LogDeltaKind::Unchanged
            } else {
                LogDeltaKind::Appended
            }
        }
    }
}

/// Cuts the newest tail of a text so that at most `max_bytes` are kept.
///
/// Used for the log tail in the payload: a long log is more interesting at its
/// end. When the cut lands inside a character, it moves to the next boundary.
#[must_use]
pub fn tail_at_most(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut start = text.len() - max_bytes;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
