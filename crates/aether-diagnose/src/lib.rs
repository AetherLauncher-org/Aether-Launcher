//! Crash and startup-failure analysis for Aether Launcher.
//!
//! The pipeline is described in `docs/design/crash-analysis.md`: collect the
//! evidence of a single launch session, classify it locally first, redact the
//! Minecraft-specific secrets on the client, and only then build a payload that
//! a narrow server endpoint is allowed to see.
//!
//! This crate performs no network I/O. Everything here is deterministic and
//! testable with fixed samples.
