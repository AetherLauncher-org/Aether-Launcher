//! The list of instances, and finding one by id.

use std::path::PathBuf;

use aether_core::{GameInstance, InstanceId, InstanceState};
use aether_storage::InstanceStore;
use serde::Serialize;

use crate::host::Host;

/// One entry of the instance list as an interface sees it. No path, no secret.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    id: String,
    name: String,
    version: String,
    state: &'static str,
}

impl InstanceSummary {
    #[must_use]
    pub fn from_instance(instance: &GameInstance) -> Self {
        Self {
            id: instance.id.to_string(),
            name: instance.name.clone(),
            version: instance.version.as_str().to_owned(),
            state: state_label(&instance.state),
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The stable state label (ready, incomplete, …).
    #[must_use]
    pub fn state(&self) -> &'static str {
        self.state
    }
}

/// A record that the listing pass refused to load, with a stable category instead of a raw
/// file system path.
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

impl InstanceListing {
    #[must_use]
    pub fn instances(&self) -> &[InstanceSummary] {
        &self.instances
    }

    #[must_use]
    pub fn skipped_count(&self) -> usize {
        self.skipped.len()
    }
}

#[must_use]
pub fn state_label(state: &InstanceState) -> &'static str {
    match state {
        InstanceState::Ready => "ready",
        InstanceState::Incomplete => "incomplete",
        InstanceState::Running => "running",
        InstanceState::Failed => "failed",
    }
}

/// The data directory, with the instances/ folder made sure of.
///
/// # Errors
///
/// When the host has no data directory or the store cannot be prepared.
pub fn store_root(host: &dyn Host) -> Result<PathBuf, String> {
    let base = host.data_dir()?;
    std::fs::create_dir_all(base.join("instances"))
        .map_err(|_| "cannot prepare the instance store".to_owned())?;
    Ok(base)
}

/// An instance by id, checked against where the store keeps it, with the store root.
///
/// # Errors
///
/// For an id that is not an instance id, or an instance that is missing or does not hold
/// together.
pub fn load(host: &dyn Host, id: &str) -> Result<(PathBuf, GameInstance), String> {
    let id = InstanceId::parse(id).map_err(|_| "invalid instance ID".to_owned())?;
    let base = store_root(host)?;
    let instance = InstanceStore::open(&base)
        .and_then(|store| store.load(id))
        .map_err(|error| format!("cannot open the instance: {error}"))?;
    Ok((base, instance))
}

/// Lists the instances; a broken record is reported in skipped, it does not hide the rest.
///
/// # Errors
///
/// When the store itself cannot be read.
pub fn list(host: &dyn Host) -> Result<InstanceListing, String> {
    let root = store_root(host)?;
    aether_storage::list_instances(&root)
        .map(|listing| InstanceListing {
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
        })
        .map_err(|error| format!("cannot list instances: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> GameInstance {
        GameInstance {
            id: InstanceId::new(),
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
            assert_eq!(state_label(&state), label);
        }
    }
}
