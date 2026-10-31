//! A `Host` for the spikes: a data directory under the system temp folder, no running games,
//! no sign-in. The same seam the Tauri shell fills with `TauriHost`.

use std::path::PathBuf;

use aether_app::Host;
use aether_app::microsoft::LoginEvent;
use aether_core::InstanceId;

pub struct SpikeHost {
    dir: PathBuf,
}

impl SpikeHost {
    /// Opens (and fills, when empty) the spike's data directory.
    ///
    /// # Panics
    ///
    /// When the directory cannot be created: a spike has nothing useful to do then.
    #[must_use]
    pub fn open() -> Self {
        let dir = std::env::temp_dir().join("aether-native-ui-spike");
        std::fs::create_dir_all(&dir).expect("cannot create the spike data directory");
        let host = Self { dir };
        host.seed();
        host
    }

    fn seed(&self) {
        let empty = aether_app::instances::list(self).map_or(true, |l| l.instances().is_empty());
        if !empty {
            return;
        }
        for (name, version) in [
            ("生存 1.21.1", "1.21.1"),
            ("Creative Build", "1.20.1"),
            ("模组整合包 Fabric", "1.21.1"),
            ("Redstone Lab", "1.19.4"),
            ("サバイバル", "1.21.4"),
        ] {
            let _ = aether_storage::create_instance(&self.dir, name, version);
        }
    }
}

impl Host for SpikeHost {
    fn data_dir(&self) -> Result<PathBuf, String> {
        Ok(self.dir.clone())
    }

    fn is_instance_running(&self, _id: InstanceId) -> Result<bool, String> {
        Ok(false)
    }

    fn emit_login(&self, _event: &LoginEvent) {}
}
