pub mod registry;
pub use registry::{
    InstanceProcessRegistry, InstanceProcessStatus, InstanceRunState, RegistryError,
};

pub mod process;
pub use process::{GameProcess, LogRecord, LogStream, ProcessError};

pub mod native;
pub use native::{NativeArchive, NativePlan, NativePlanError, build_native_plan};

pub mod plan;
pub use plan::{LaunchBuildError, LaunchOptions, build_launch_plan};

use std::path::PathBuf;

use aether_core::InstanceId;
use aether_manifest::VersionManifest;
use aether_runtime::JavaRuntime;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchRequest {
    pub instance_id: InstanceId,
    pub memory_mib: u32,
}

impl LaunchRequest {
    pub fn validate(&self) -> Result<(), LaunchError> {
        if !(512..=131_072).contains(&self.memory_mib) {
            return Err(LaunchError::InvalidMemory(self.memory_mib));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchPlan {
    pub runtime: JavaRuntime,
    pub working_directory: PathBuf,
    pub arguments: Vec<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LaunchError {
    #[error("requested memory {0} MiB is outside the supported range")]
    InvalidMemory(u32),
}

/// Pick a discovered Java installation matching an already resolved version
/// manifest. Discovery order is retained for equally suitable installations.
///
/// No default is guessed for legacy manifests without `javaVersion`: the
/// caller must supply a version requirement through a separate policy.
pub fn select_java_runtime<'a>(
    manifest: &VersionManifest,
    runtimes: &'a [JavaRuntime],
) -> Result<&'a JavaRuntime, JavaSelectionError> {
    let required = manifest
        .java_version
        .as_ref()
        .ok_or(JavaSelectionError::MissingRequirement)?
        .major_version;
    let major =
        u16::try_from(required).map_err(|_| JavaSelectionError::UnsupportedMajor(required))?;

    runtimes
        .iter()
        .find(|runtime| runtime.supports(major))
        .ok_or(JavaSelectionError::NotInstalled(required))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum JavaSelectionError {
    #[error("version manifest does not specify a required Java version")]
    MissingRequirement,
    #[error("Java major version {0} cannot be represented by the runtime model")]
    UnsupportedMajor(u32),
    #[error("no installed Java runtime matches required major version {0}")]
    NotInstalled(u32),
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aether_manifest::parse_version_manifest;
    use aether_runtime::JavaRuntime;

    use super::{JavaSelectionError, select_java_runtime};

    fn runtime(major: u16, name: &str) -> JavaRuntime {
        JavaRuntime {
            executable: PathBuf::from(name),
            major_version: major,
            architecture: "x64".to_owned(),
            vendor: None,
        }
    }

    #[test]
    fn selects_exact_java_version_preserving_discovery_priority() {
        let manifest = parse_version_manifest(
            r#"{"id":"fabric","javaVersion":{"component":"java-runtime-delta","majorVersion":21}}"#,
        )
        .unwrap();
        let runtimes = [
            runtime(25, "java-25"),
            runtime(21, "java-21-first"),
            runtime(17, "java-17"),
            runtime(21, "java-21-second"),
        ];
        let selected = select_java_runtime(&manifest, &runtimes).unwrap();
        assert_eq!(selected.executable, PathBuf::from("java-21-first"));
    }

    #[test]
    fn missing_requirement_is_not_assumed_to_mean_java_8() {
        let manifest = parse_version_manifest(r#"{"id":"1.12.2"}"#).unwrap();
        assert_eq!(
            select_java_runtime(&manifest, &[runtime(8, "java-8")]).unwrap_err(),
            JavaSelectionError::MissingRequirement
        );
    }

    #[test]
    fn unavailable_java_major_has_an_actionable_error() {
        let manifest = parse_version_manifest(
            r#"{"id":"1.21","javaVersion":{"component":"java-runtime-delta","majorVersion":21}}"#,
        )
        .unwrap();
        assert_eq!(
            select_java_runtime(&manifest, &[runtime(17, "java-17")]).unwrap_err(),
            JavaSelectionError::NotInstalled(21)
        );
    }

    #[test]
    fn unrepresentable_java_major_is_rejected() {
        let manifest = parse_version_manifest(
            r#"{"id":"future","javaVersion":{"component":"custom","majorVersion":65536}}"#,
        )
        .unwrap();
        assert_eq!(
            select_java_runtime(&manifest, &[]).unwrap_err(),
            JavaSelectionError::UnsupportedMajor(65536)
        );
    }
}
