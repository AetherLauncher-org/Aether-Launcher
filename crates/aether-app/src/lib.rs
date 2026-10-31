//! What the launcher does, without any user interface.
//!
//! This layer holds the operations a window offers (list and manage instances, open and
//! convert schematics, …) as plain functions over the `aether-*` crates. It knows nothing of
//! Tauri or of any widget toolkit: what it needs from the outside (where the data directory
//! is, whether an instance's game is running) comes through the [`Host`] trait, so the same
//! code serves the Tauri shell and a native interface, and can be tested without either.
//!
//! The rules in `docs/design/launch-and-process.md` hold here and are enforced here, not left
//! to the interface: callers name an instance by id, a folder by a fixed label and a file by
//! a plain name, never by a path; credentials are not part of any value returned; the
//! contents of mods, worlds, logs and schematics are untrusted. See
//! `docs/design/native-ui.md` for why this layer exists.

pub mod host;
pub mod identify;
pub mod instances;
pub mod manage;
pub mod microsoft;
pub mod platform;
pub mod schematics;

pub use host::Host;
