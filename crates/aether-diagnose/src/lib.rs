//! Crash and startup-failure analysis for Aether Launcher.
//!
//! The pipeline is described in `docs/design/crash-analysis.md`: collect the
//! evidence of a single launch session, classify it locally first, redact the
//! Minecraft-specific secrets on the client, and only then build a payload that
//! a narrow server endpoint is allowed to see.
//!
//! This crate performs no network I/O. Everything here is deterministic and
//! testable with fixed samples.
//!
//! # Trust model
//!
//! Every string that came from the game, a mod, a server or chat output is
//! **untrusted input**. It can contain prompt injection, markup, ANSI escapes
//! and marker text that pretends to be part of our own format. Nothing in this
//! crate ever executes such text: the payload wraps log excerpts in a `<log>`
//! data element, keeps the machine readable structure in separate fields, and
//! never places log text where the model is asked to read an instruction.

pub mod classify;
pub mod evidence;
pub mod javatab;
pub mod payload;
pub mod phase;
pub mod redact;

pub use classify::{
    Confidence, LocalDiagnosis, Rule, SuggestedAction, classify_locally, redact_log_line,
};
pub use evidence::{
    ByteLength, CapturedText, EvidenceSession, LaunchEvidence, LogDeltaKind, PostExitSnapshot,
    PreLaunchSnapshot, UnixTimestamp, new_since_known_len,
};
pub use javatab::{
    PARAMETER_MIN_JAVA, PreflightIssue, min_java_for_argument, preflight_jvm_arguments,
};
pub use payload::{
    ExtractedSections, PayloadPreview, SessionPayload, crash_signature, extract_sections,
};
pub use phase::{FailurePhase, PhaseVerdict, SessionSummary, classify_phase, decide_phase};
pub use redact::{PlaceholderMap, RedactionReport, Redactor, redacted};

/// The end-to-end result the UI shows after a session ended badly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionReport {
    /// The phase the launch failed in, with the reasons behind it.
    pub verdict: PhaseVerdict,
    /// The local rule result, if a rule matched.
    pub local: Option<LocalDiagnosis>,
    /// Whether the launcher should offer to send the payload to the analysis
    /// service. `false` means the local answer is complete.
    pub needs_external_analysis: bool,
    /// The redacted preview of what would be sent, ready to be shown to the
    /// user. Never contains the raw token values.
    pub payload_preview: String,
    /// The structured payload that would be sent. Kept out of the preview
    /// string so the UI can send it verbatim after the user agrees.
    pub payload: SessionPayload,
}

/// Runs the whole local pipeline for one session.
///
/// `java_major` is the major version of the Java runtime the launcher selected;
/// it is used by the rules that explain a version mismatch. The function does
/// no I/O and performs no network calls: the caller decides whether the
/// payload is ever sent.
#[must_use]
pub fn analyze_session(evidence: &LaunchEvidence, java_major: Option<u16>) -> SessionReport {
    let verdict = classify_phase(evidence);
    let text = evidence.combined_text();
    let local = classify_locally(&text, &evidence.jvm_arguments, java_major);
    let needs_external_analysis = local.as_ref().is_none_or(|d| !d.confident());

    let sections = extract_sections(evidence);
    let payload =
        SessionPayload::from_evidence(evidence, &verdict, local.clone(), sections, java_major);
    let preview = payload.preview();

    SessionReport {
        verdict,
        local,
        needs_external_analysis,
        payload_preview: preview.redacted_preview.clone(),
        payload,
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
