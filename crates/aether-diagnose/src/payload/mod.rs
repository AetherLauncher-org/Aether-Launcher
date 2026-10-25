//! Structured crash payloads with an explicit prompt-injection trust boundary.
//!
//! Mods, servers, and chat can write arbitrary text into the log, so every log
//! value is untrusted input. The server prompt is fixed; this payload supplies
//! data only. Model output is advice rendered as plain text and is never
//! executed.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::classify::LocalDiagnosis;
use crate::evidence::LaunchEvidence;
use crate::phase::{PhaseVerdict, classify_phase};
use crate::redact::redacted;

pub const SECTION_BUDGET_BYTES: usize = 64 * 1024;
const LOG_TAIL_BYTES: usize = 16 * 1024;

/// The log excerpts chosen for the payload.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractedSections {
    pub exception_stack: Vec<String>,
    pub mod_list: Vec<String>,
    pub system_info: Vec<String>,
    /// System.exitCode lines and similar short decisive lines.
    pub exit_context: Vec<String>,
    /// `true` when the size limit forced a cut.
    pub truncated: bool,
}

/// A compact, structured projection of process and file evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceSummary {
    pub exit_code: Option<i32>,
    pub duration_millis: u64,
    pub crash_reports: Vec<String>,
    pub hs_err_logs: Vec<String>,
}

/// Picks relevant excerpts while enforcing one aggregate byte budget.
#[must_use]
pub fn extract_sections(evidence: &LaunchEvidence) -> ExtractedSections {
    let mut sections = ExtractedSections {
        exception_stack: exception_lines(evidence),
        exit_context: matching_lines(evidence, |line| {
            line.contains("System.exitCode=")
                || line.contains("Unrecognized option:")
                || line.contains("Error: A fatal exception")
                || line.contains("Exception in thread \"main\"")
                || line.contains("JVM crash")
                || line.contains("# A fatal error has been detected")
        }),
        system_info: system_lines(evidence),
        mod_list: matching_lines(evidence, |line| {
            line.contains("Mod List:")
                || line.contains("mods.toml")
                || line.contains("Loading ") && line.contains(" mods")
                || line.contains("Forge mod list")
                || line.trim_start().starts_with("- ") && line.contains('@')
                || line.starts_with("\t\t") && line.split_whitespace().count() >= 2
        }),
        truncated: false,
    };
    let mut used = 0;
    let source_had_extra = all_text(evidence).lines().count()
        > sections.exception_stack.len()
            + sections.exit_context.len()
            + sections.system_info.len()
            + sections.mod_list.len();
    sections.truncated = source_had_extra;
    for lines in [
        &mut sections.exception_stack,
        &mut sections.exit_context,
        &mut sections.system_info,
        &mut sections.mod_list,
    ] {
        let mut kept = Vec::with_capacity(lines.len());
        for line in lines.drain(..) {
            let bytes = line.len() + usize::from(!kept.is_empty());
            if used + bytes <= SECTION_BUDGET_BYTES {
                used += bytes;
                kept.push(line);
            } else {
                sections.truncated = true;
                break;
            }
        }
        *lines = kept;
    }
    sections
}

/// The normalised cache signature of a crash.
#[must_use]
pub fn crash_signature(evidence: &LaunchEvidence) -> String {
    let text = evidence.combined_text();
    let mut classes = BTreeSet::new();
    for line in text.lines() {
        if let Some(class) = exception_class(line) {
            classes.insert(class);
        }
    }
    let deepest = deepest_cause_frames(&text);
    let mut mods = BTreeSet::new();
    for frame in &deepest {
        for candidate in frame.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-') {
            if candidate.len() >= 2
                && candidate.chars().any(|c| c.is_ascii_lowercase())
                && candidate.contains(['_', '-'])
            {
                mods.insert(candidate.to_string());
            }
        }
    }
    let phase = classify_phase(evidence).phase.as_str();
    let class_list = classes.into_iter().collect::<Vec<_>>().join("\n");
    let frame_list = deepest.join("\n");
    let mod_list = mods.into_iter().collect::<Vec<_>>().join("\n");
    // A log with no recognisable stack still needs a cache key: the launcher
    // caches on this signature, and an empty string would collide across every
    // stackless failure. The phase, the Java major and a fixed marker give a
    // stable, non-empty identity in that case.
    let stack_marker = if class_list.is_empty() && frame_list.is_empty() {
        format!(
            "no-stack:{}:{}",
            phase,
            evidence
                .java_major
                .map_or_else(|| "unknown".to_string(), |major| major.to_string())
        )
    } else {
        String::new()
    };
    let input = format!("{class_list}\n{frame_list}\n{mod_list}\n{phase}\n{stack_marker}");
    let digest = Sha1::digest(input.as_bytes());
    digest
        .iter()
        .fold(String::with_capacity(40), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// The structured payload sent to the narrow endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPayload {
    pub signature: String,
    pub phase: String,
    pub needs_external_analysis: bool,
    pub local: Option<LocalDiagnosis>,
    pub sections: ExtractedSections,
    pub java_major: Option<u16>,
    pub log_tail: Vec<String>,
    pub evidence_summary: EvidenceSummary,
    pub placeholder_count: usize,
}

impl SessionPayload {
    /// Builds a payload. The phase is already decided locally; the signature is
    /// derived from the same evidence represented by the extracted sections.
    #[must_use]
    pub fn new(
        verdict: &PhaseVerdict,
        local: Option<LocalDiagnosis>,
        sections: ExtractedSections,
        java_major: Option<u16>,
    ) -> Self {
        let needs_external_analysis = local
            .as_ref()
            .is_none_or(|diagnosis| !diagnosis.confident());
        Self {
            signature: String::new(),
            phase: verdict.phase.as_str().to_string(),
            needs_external_analysis,
            local,
            sections,
            java_major,
            log_tail: Vec::new(),
            evidence_summary: EvidenceSummary {
                exit_code: None,
                duration_millis: 0,
                crash_reports: Vec::new(),
                hs_err_logs: Vec::new(),
            },
            placeholder_count: 0,
        }
    }

    /// Builds the complete payload from evidence while retaining the legacy API.
    #[must_use]
    pub fn from_evidence(
        evidence: &LaunchEvidence,
        verdict: &PhaseVerdict,
        local: Option<LocalDiagnosis>,
        sections: ExtractedSections,
        java_major: Option<u16>,
    ) -> Self {
        let mut payload = Self::new(verdict, local, sections, java_major);
        payload.signature = crash_signature(evidence);
        payload.log_tail = crate::evidence::tail_at_most(&evidence.combined_text(), LOG_TAIL_BYTES)
            .lines()
            .map(str::to_string)
            .collect();
        payload.evidence_summary = EvidenceSummary {
            exit_code: evidence.exit_code,
            duration_millis: evidence.duration_millis,
            crash_reports: evidence.crash_reports.clone(),
            hs_err_logs: evidence.hs_err_logs.clone(),
        };
        payload
    }

    /// The user-visible preview of exactly the redacted data fields.
    #[must_use]
    pub fn preview(&self) -> PayloadPreview {
        let mut count = 0;
        let mut redact = |value: &str| {
            let result = redacted(value);
            count += result.report.replacement_count;
            result.text
        };
        let mut output = String::new();
        writeln!(
            output,
            "签名：{}\n阶段：{}\n需要外部分析：{}",
            self.signature, self.phase, self.needs_external_analysis
        )
        .ok();
        writeln!(output, "本地诊断：{}", redact(&format!("{:?}", self.local))).ok();
        for (heading, lines) in [
            ("异常栈", &self.sections.exception_stack),
            ("退出上下文", &self.sections.exit_context),
            ("系统信息", &self.sections.system_info),
            ("模组列表", &self.sections.mod_list),
            ("日志尾部", &self.log_tail),
        ] {
            writeln!(output, "\n【{heading}】").ok();
            for line in lines {
                writeln!(output, "{}", redact(line)).ok();
            }
        }
        PayloadPreview {
            redacted_preview: output,
            placeholder_count: count,
        }
    }

    /// Serializes structured fields so untrusted excerpts remain data, never instructions.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut value = serde_json::to_value(self).unwrap_or_default();
        let mut count = 0;
        if let Some(object) = value.as_object_mut() {
            // `local` carries raw log lines in `evidence_lines`; it must be
            // redacted exactly like the excerpts. Leaving it out here let an
            // `--accessToken` that matched a rule reach the wire verbatim.
            for key in ["sections", "log_tail", "local"] {
                if let Some(field) = object.get_mut(key) {
                    redact_json(field, &mut count);
                }
            }
        }
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "placeholder_count".to_string(),
                serde_json::Value::from(count),
            );
        }
        serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())
    }
}

/// The user-visible preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadPreview {
    pub redacted_preview: String,
    pub placeholder_count: usize,
}

fn redact_json(value: &mut serde_json::Value, count: &mut usize) {
    match value {
        serde_json::Value::String(text) => {
            let result = redacted(text);
            *count += result.report.replacement_count;
            *text = result.text;
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json(item, count);
            }
        }
        serde_json::Value::Object(items) => {
            for item in items.values_mut() {
                redact_json(item, count);
            }
        }
        _ => {}
    }
}

fn all_text(evidence: &LaunchEvidence) -> String {
    let mut text = evidence.combined_text();
    if let Some(info) = &evidence.system_info {
        text.push('\n');
        text.push_str(info);
    }
    text
}

fn clean_line(line: &str) -> String {
    static ANSI: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\x1b\[[0-?]*[ -/]*[@-~]").expect("valid ansi regex"));
    ANSI.replace_all(line, "").chars().take(400).collect()
}

fn matching_lines<F>(evidence: &LaunchEvidence, predicate: F) -> Vec<String>
where
    F: Fn(&str) -> bool,
{
    all_text(evidence)
        .lines()
        .filter(|line| predicate(line))
        .map(clean_line)
        .collect()
}

fn system_lines(evidence: &LaunchEvidence) -> Vec<String> {
    let mut lines = matching_lines(evidence, |line| {
        [
            "java.version=",
            "java.vendor=",
            "os.name=",
            "os.arch=",
            "OpenGL",
            "GL_VERSION",
            "Pixel Format",
            "CPU:",
            "Memory:",
            "Max Memory:",
        ]
        .iter()
        .any(|marker| line.contains(marker))
    });
    if let Some(info) = &evidence.system_info {
        lines.extend(info.lines().map(clean_line));
    }
    // `all_text` already contains the system block, so a fact can be seen
    // twice — once behind a log4j prefix (`[main/INFO]: os.name=Windows`) and
    // once bare. De-duplicating on the whole line misses that, so the identity
    // is the `key=value` payload when the line has one, and the whole line
    // otherwise. The first occurrence wins, which keeps the log4j-prefixed
    // form when it comes first.
    let mut seen = std::collections::BTreeSet::new();
    lines.retain(|line| seen.insert(system_fact_key(line)));
    lines
}

/// The identity of a system line for de-duplication purposes.
///
/// Strips a leading log4j prefix (`[12:00:00] [main/INFO]: `) and then keys on
/// the `key=value` pair when the line carries one.
fn system_fact_key(line: &str) -> String {
    let trimmed = line.trim_start_matches(|c: char| c == '[' || c.is_whitespace() || c == ']');
    let fact = trimmed.rsplit("]: ").next().unwrap_or(trimmed);
    fact.trim().to_string()
}

fn exception_lines(evidence: &LaunchEvidence) -> Vec<String> {
    let text = all_text(evidence);
    let lines: Vec<_> = text.lines().collect();
    let mut starts = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if exception_class(line).is_some() || line.contains("Caused by:") {
            starts.push(index);
        }
    }
    let Some(start) = starts.last().copied() else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for line in lines.iter().skip(start).take(120) {
        if result.is_empty()
            || line.trim_start().starts_with("at ")
            || line.trim_start().starts_with("Caused by:")
            || line.trim() == "... N more"
            || line.starts_with('\t')
        {
            result.push(clean_line(line));
        } else if !line.trim().is_empty() {
            break;
        }
    }
    result
}

fn exception_class(line: &str) -> Option<String> {
    let candidate = line
        .split_once("Caused by:")
        .map_or(line, |(_, rest)| rest)
        .split_whitespace()
        .next()?;
    let class = candidate
        .trim_end_matches(':')
        .split('<')
        .next()?
        .trim_end_matches(':');
    (class.contains("Exception") || class.contains("Error") || class.contains("Throwable"))
        .then(|| class.to_string())
}

fn deepest_cause_frames(text: &str) -> Vec<String> {
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        if line.contains("Caused by:") && !current.is_empty() {
            blocks.push(std::mem::take(&mut current));
        }
        if line.trim_start().starts_with("at ") {
            current.push(normalize_frame(line));
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }
    blocks
        .into_iter()
        .last()
        .unwrap_or_default()
        .into_iter()
        .take(5)
        .collect()
}

fn normalize_frame(line: &str) -> String {
    static LINE_NO: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\.java:\d+").expect("valid frame regex"));
    static LAMBDA: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\$\$Lambda\$\d+/0x[0-9a-fA-F]+\b").expect("valid lambda regex")
    });
    let line = LAMBDA.replace_all(line, "$$Lambda");
    LINE_NO
        .replace_all(&line, ".java")
        .to_string()
        .replace(|c: char| c.is_ascii_digit(), "")
}

#[cfg(test)]
#[path = "payload_tests.rs"]
mod tests;
