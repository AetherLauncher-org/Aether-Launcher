//! What this layer needs from whoever embeds it.

use std::path::PathBuf;

use aether_core::InstanceId;

use crate::import::ImportItemEvent;
use crate::install::InstallProgress;
use crate::microsoft::LoginEvent;

/// Platform folders used by the bounded launcher search.
pub struct KnownFolders {
    pub desktop: Option<PathBuf>,
    pub downloads: Option<PathBuf>,
    pub documents: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

/// The outside world, as the operations need it. Errors are plain sentences that are safe to
/// show: no paths, no secrets.
pub trait Host: Send + Sync {
    /// The application data directory: `instances/`, `game/` and `accounts/` live in it. It
    /// is chosen by the host, never by a caller of an operation.
    ///
    /// # Errors
    ///
    /// When the platform has no such directory.
    fn data_dir(&self) -> Result<PathBuf, String>;

    /// Whether this session has a running game for the instance. Changing an instance's files
    /// is refused while it does.
    ///
    /// # Errors
    ///
    /// When the process registry cannot be read.
    fn is_instance_running(&self, id: InstanceId) -> Result<bool, String>;

    /// A step of a Microsoft sign-in (waiting, signed in, failed, cancelled). Called from the
    /// sign-in's own thread, so it must not block. Carries an account's identity at most,
    /// never a token.
    fn emit_login(&self, event: &LoginEvent);

    /// Installation progress. Called from a download thread, so it must not block.
    /// Carries no secrets.
    fn emit_install(&self, progress: &InstallProgress);

    /// Import item progress. Called from the import worker thread, so it must not block.
    fn emit_import_item(&self, event: &ImportItemEvent);

    /// Opens a native folder picker on a blocking worker thread. Dismissal returns `None`.
    ///
    /// # Errors
    ///
    /// When the selected folder path is not usable.
    fn pick_folder(&self, title: &str) -> Result<Option<PathBuf>, String>;

    /// Opens a native file picker on a blocking worker thread. Dismissal returns `None`, and so
    /// does a pick that is not a local file path (a dialog can hand back a URL).
    ///
    /// # Errors
    ///
    /// When a host cannot open its picker. The Tauri host never fails: whatever it cannot use
    /// counts as dismissal.
    fn pick_file(&self, filter_name: &str, extensions: &[&str]) -> Result<Option<PathBuf>, String>;

    /// Platform folders to search for launcher installations, in the operation's fixed order.
    fn known_folders(&self) -> KnownFolders;
}
