//! Trusted, instance-ID-based preparation of modern Minecraft launch plans.
//! Only trusted Rust callers may supply account variables and installed runtimes.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use aether_core::{InstanceId, InstanceState};
use aether_download::verify::{ArtifactVerificationError, verify_local_artifact};
use aether_download::{ArtifactSource, DownloadArtifact};
use aether_manifest::rules::Environment;
use aether_manifest::{VersionLoadError, load_resolved_version_manifest};
use aether_runtime::JavaRuntime;
use aether_storage::{InstanceLoadError, InstanceStore};
use thiserror::Error;

use crate::native::{NativePlanError, build_native_plan};
use crate::plan::{LaunchBuildError, LaunchOptions, build_launch_plan};
use crate::process::LogRecord;
use crate::registry::{InstanceProcessRegistry, InstanceProcessStatus, RegistryError};
use crate::{JavaSelectionError, LaunchPlan, LaunchRequest, select_java_runtime};

#[derive(Debug, Error)]
pub enum CoordinatorError {
    #[error("game data directory must exist and contain a versions directory")]
    InvalidGameDirectory,
    #[error("only ready instances can be launched")]
    InstanceNotReady,
    #[error("client JAR metadata is missing from the installed version chain")]
    MissingClientMetadata,
    #[error("natives are required but not yet prepared by the integrated launcher")]
    NativesNotReady,
    #[error("asset index and assets must be verified before launching this version")]
    AssetsNotReady,
    #[error(transparent)]
    Instance(#[from] InstanceLoadError),
    #[error(transparent)]
    Version(#[from] VersionLoadError),
    #[error(transparent)]
    Java(#[from] JavaSelectionError),
    #[error(transparent)]
    Artifact(#[from] ArtifactVerificationError),
    #[error(transparent)]
    Native(#[from] NativePlanError),
    #[error(transparent)]
    Build(#[from] LaunchBuildError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error("game storage error: {0}")]
    Io(#[from] std::io::Error),
}

/// Owned by the backend. The only instance lookup selector is a typed UUID.
/// Account variables/runtimes are supplied from trusted backend services, not
/// arbitrary frontend payloads.
pub struct InstanceLaunchCoordinator {
    store: InstanceStore,
    game_directory: PathBuf,
    processes: InstanceProcessRegistry,
}

impl InstanceLaunchCoordinator {
    pub fn open(
        instance_storage_root: &Path,
        game_directory: &Path,
    ) -> Result<Self, CoordinatorError> {
        let game_directory = fs::canonicalize(game_directory)?;
        if !game_directory.join("versions").is_dir() {
            return Err(CoordinatorError::InvalidGameDirectory);
        }
        let store = InstanceStore::open(instance_storage_root)?;
        Ok(Self {
            store,
            game_directory,
            processes: InstanceProcessRegistry::new(),
        })
    }

    /// Resolve an installed instance, select Java, verify the *real* client JAR
    /// of the ancestor that provided downloads.client, and build modern argv.
    ///
    /// We deliberately fail closed for versions whose native extraction or
    /// assets have not been incorporated into this coordinator.
    pub fn prepare(
        &self,
        instance_id: InstanceId,
        memory_mib: u32,
        runtimes: &[JavaRuntime],
        environment: &Environment,
        account_variables: &BTreeMap<String, String>,
    ) -> Result<LaunchPlan, CoordinatorError> {
        let instance = self.store.load(instance_id)?;
        if instance.state != InstanceState::Ready {
            return Err(CoordinatorError::InstanceNotReady);
        }
        let resolved =
            load_resolved_version_manifest(&self.game_directory, instance.version.as_str())?;
        let native_plan = build_native_plan(&resolved.manifest, environment)?;
        if !native_plan.archives.is_empty() {
            return Err(CoordinatorError::NativesNotReady);
        }
        if resolved.manifest.asset_index.is_some() {
            return Err(CoordinatorError::AssetsNotReady);
        }
        let runtime = select_java_runtime(&resolved.manifest, runtimes)?;
        let source = resolved
            .client_jar_version
            .ok_or(CoordinatorError::MissingClientMetadata)?;
        let client = resolved
            .manifest
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.client.as_ref())
            .ok_or(CoordinatorError::MissingClientMetadata)?;
        let artifact = DownloadArtifact {
            destination: PathBuf::from("versions")
                .join(&source)
                .join(format!("{source}.jar")),
            size: client.size,
            sha1: client.sha1.clone(),
            sources: vec![ArtifactSource {
                provider: "mojang".to_owned(),
                url: client.url.clone(),
            }],
        };
        let client_jar = verify_local_artifact(&self.game_directory, &artifact)?;
        let options = LaunchOptions {
            game_directory: self.game_directory.clone(),
            working_directory: instance.root.clone(),
            client_jar,
            natives_directory: instance.root.join("natives"),
            variables: account_variables.clone(),
        };
        let request = LaunchRequest {
            instance_id,
            memory_mib,
        };
        Ok(build_launch_plan(
            &request,
            &resolved.manifest,
            runtime,
            environment,
            &options,
        )?)
    }

    /// Only a prepared trusted plan can reach the process registry. On a
    /// preparation failure, no new child process is started.
    pub fn start(
        &mut self,
        instance_id: InstanceId,
        memory_mib: u32,
        runtimes: &[JavaRuntime],
        environment: &Environment,
        account_variables: &BTreeMap<String, String>,
        redactions: &[String],
    ) -> Result<InstanceProcessStatus, CoordinatorError> {
        // Reject duplicate launches before performing file inspection.
        match self.processes.status(instance_id) {
            Ok(status) if status.state == crate::registry::InstanceRunState::Running => {
                return Err(CoordinatorError::Registry(RegistryError::AlreadyRunning));
            }
            Err(RegistryError::UnknownInstance) | Ok(_) => {}
            Err(error) => return Err(CoordinatorError::Registry(error)),
        }
        let plan = self.prepare(
            instance_id,
            memory_mib,
            runtimes,
            environment,
            account_variables,
        )?;
        Ok(self.processes.start(instance_id, &plan, redactions)?)
    }

    pub fn status(&mut self, id: InstanceId) -> Result<InstanceProcessStatus, CoordinatorError> {
        Ok(self.processes.status(id)?)
    }

    pub fn list(&mut self) -> Result<Vec<InstanceProcessStatus>, CoordinatorError> {
        Ok(self.processes.list()?)
    }

    pub fn drain_logs(&mut self, id: InstanceId) -> Result<Vec<LogRecord>, CoordinatorError> {
        Ok(self.processes.drain_logs(id)?)
    }

    pub fn stop(&mut self, id: InstanceId) -> Result<InstanceProcessStatus, CoordinatorError> {
        Ok(self.processes.stop(id)?)
    }

    pub fn forget_finished(
        &mut self,
        id: InstanceId,
    ) -> Result<InstanceProcessStatus, CoordinatorError> {
        Ok(self.processes.forget_finished(id)?)
    }
}

#[cfg(test)]
#[path = "coordinator/coordinator_tests.rs"]
mod tests;
