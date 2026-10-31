//! Narrow, domain-shaped Tauri commands.
//!
//! Every command in this module follows the IPC rules from
//! `docs/architecture.md`: the caller passes typed identifiers or plain
//! configuration values, never an executable path, a game directory or an
//! arbitrary file system location. The backend owns those roots.

pub mod catalog;
pub mod content;
mod host;
pub mod import;
pub mod install;
pub mod instances;
pub mod java;
pub mod launch;
pub mod manage;
pub mod microsoft;
pub mod modloader;
pub mod schematics;
pub mod skin_library;
