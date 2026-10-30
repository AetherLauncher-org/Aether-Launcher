//! Instance listing command.
//!
//! The store root is resolved by the backend from the application data
//! directory, so the frontend never chooses a path on disk. Listing tolerates
//! individual broken records and reports why they were skipped. Instances are
//! created by the installer (`install::install_vanilla_version`).

use std::path::PathBuf;

use aether_core::{GameInstance, InstanceState};
use serde::Serialize;
use tauri::Manager;

/// One entry of the instance list as the frontend sees it.
///
/// Every type that crosses the IPC boundary must be public: Tauri generates a
/// wrapper whose signature mentions it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    id: String,
    name: String,
    version: String,
    state: &'static str,
}

impl InstanceSummary {
    pub(super) fn from_instance(instance: &GameInstance) -> Self {
        Self {
            id: instance.id.to_string(),
            name: instance.name.clone(),
            version: instance.version.as_str().to_owned(),
            state: instance_state_label(&instance.state),
        }
    }
}

/// A record that the listing pass refused to load, with a stable category
/// instead of a raw file system path.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedInstance {
    id: String,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceListing {
    instances: Vec<InstanceSummary>,
    skipped: Vec<SkippedInstance>,
}

fn instance_state_label(state: &InstanceState) -> &'static str {
    match state {
        InstanceState::Ready => "ready",
        InstanceState::Incomplete => "incomplete",
        InstanceState::Running => "running",
        InstanceState::Failed => "failed",
    }
}

/// Resolve the application-owned instance store root and make sure the
/// `instances/` directory exists. The path comes from Tauri, never from IPC.
fn instance_store_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("application data directory is unavailable: {error}"))?;

    std::fs::create_dir_all(base.join("instances"))
        .map_err(|error| format!("cannot prepare the instance store: {error}"))?;

    Ok(base)
}

#[tauri::command]
pub(crate) async fn list_instances(app: tauri::AppHandle) -> Result<InstanceListing, String> {
    let root = instance_store_root(&app)?;

    tauri::async_runtime::spawn_blocking(move || match aether_storage::list_instances(&root) {
        Ok(listing) => Ok(InstanceListing {
            instances: listing
                .instances
                .iter()
                .map(InstanceSummary::from_instance)
                .collect(),
            skipped: listing
                .skipped
                .iter()
                .map(|skipped| SkippedInstance {
                    id: skipped.id.clone(),
                    reason: skipped.reason,
                })
                .collect(),
        }),
        Err(error) => Err(format!("cannot list instances: {error}")),
    })
    .await
    .map_err(|error| format!("instance listing task failed: {error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> GameInstance {
        GameInstance {
            id: aether_core::InstanceId::new(),
            name: "Survival".to_owned(),
            version: aether_core::VersionId::parse("1.21.1").unwrap(),
            root: PathBuf::from("instances"),
            state: InstanceState::Ready,
        }
    }

    #[test]
    fn summary_fields_are_camel_case_and_stable() {
        let value = serde_json::to_value(InstanceSummary::from_instance(&sample())).unwrap();

        assert_eq!(value["name"], "Survival");
        assert_eq!(value["version"], "1.21.1");
        assert_eq!(value["state"], "ready");
        assert!(value.get("memory_mib").is_none());
        assert!(value.get("root").is_none());
    }

    #[test]
    fn every_instance_state_has_a_lowercase_label() {
        for (state, label) in [
            (InstanceState::Ready, "ready"),
            (InstanceState::Incomplete, "incomplete"),
            (InstanceState::Running, "running"),
            (InstanceState::Failed, "failed"),
        ] {
            assert_eq!(instance_state_label(&state), label);
        }
    }
}
