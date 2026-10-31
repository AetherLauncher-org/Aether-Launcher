//! What this layer needs from whoever embeds it.

use std::path::PathBuf;

use aether_core::InstanceId;

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
}
