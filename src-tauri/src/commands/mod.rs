//! Narrow, domain-shaped Tauri commands.
//!
//! Every command in this module follows the IPC rules from
//! `docs/architecture.md`: the caller passes typed identifiers or plain
//! configuration values, never an executable path, a game directory or an
//! arbitrary file system location. The backend owns those roots.
//!
//! Only the instance *listing* and *creation* commands live here for now.
//! Instance launching, process control and crash analysis stay behind the
//! Rust-only APIs of `aether-launch` and `aether-diagnose` until their
//! fail-closed preconditions are fully implemented.

pub mod instances;
