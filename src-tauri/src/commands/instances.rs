//! The instance list command.
//!
//! Listing, the summary type and the rule that the store root comes from the host and never
//! from IPC are in `aether_app::instances`. Instances are created by the installer
//! (`install::install_vanilla_version`).

use aether_app::instances::{self, InstanceListing};

use super::host::run;

#[tauri::command]
pub(crate) async fn list_instances(app: tauri::AppHandle) -> Result<InstanceListing, String> {
    run(app, |host| instances::list(host)).await
}
