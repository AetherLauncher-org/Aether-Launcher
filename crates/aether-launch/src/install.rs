//! Build the download plan for a resolved version's client JAR, Java
//! libraries and native archives. Every hash and URL comes from the official
//! version JSON; nothing is derived from a mirror or the frontend.

use std::path::{Path, PathBuf};

use aether_download::{
    ArtifactSource, DownloadArtifact, DownloadPlan, DownloadPlanError, is_safe_relative_path,
};
use aether_manifest::rules::Environment;
use aether_manifest::{LibraryArtifact, VersionManifest};
use thiserror::Error;

use crate::native::{NativePlanError, build_native_plan};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InstallPlanError {
    #[error("version JSON declares no client download")]
    MissingClient,
    #[error("unsafe client JAR version identifier: {0:?}")]
    UnsafeClientVersion(String),
    #[error("library {0:?} has no verified artifact metadata")]
    MissingLibraryMetadata(String),
    #[error("unsafe library path: {0:?}")]
    UnsafeLibraryPath(PathBuf),
    #[error("download URL for {0:?} is not HTTPS")]
    InsecureUrl(PathBuf),
    #[error(transparent)]
    Native(#[from] NativePlanError),
    #[error(transparent)]
    Download(#[from] DownloadPlanError),
}

/// Plan the client JAR, classpath libraries and native archives.
///
/// `client_jar_version` is the version whose `<id>.jar` carries the client
/// (the base version for loader profiles that inherit from it).
pub fn build_install_plan(
    manifest: &VersionManifest,
    client_jar_version: &str,
    environment: &Environment,
) -> Result<DownloadPlan, InstallPlanError> {
    let client = manifest
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.client.as_ref())
        .ok_or(InstallPlanError::MissingClient)?;
    let client_destination = PathBuf::from("versions")
        .join(client_jar_version)
        .join(format!("{client_jar_version}.jar"));
    if !is_safe_relative_path(Path::new(client_jar_version))
        || !is_safe_relative_path(&client_destination)
    {
        return Err(InstallPlanError::UnsafeClientVersion(
            client_jar_version.to_owned(),
        ));
    }

    let mut artifacts = vec![artifact(
        client_destination,
        client.size,
        &client.sha1,
        &client.url,
    )?];

    for library in manifest.active_libraries(environment) {
        // Classified and legacy native JARs are planned below, from the same
        // selection the launch coordinator uses to verify them.
        let classifier = library
            .name
            .split('@')
            .next()
            .unwrap_or("")
            .split(':')
            .nth(3);
        if classifier.is_some_and(|value| value.starts_with("natives-"))
            || !library.natives.is_empty()
        {
            continue;
        }
        let metadata = library
            .downloads
            .as_ref()
            .and_then(|downloads| downloads.artifact.as_ref())
            .ok_or_else(|| InstallPlanError::MissingLibraryMetadata(library.name.clone()))?;
        artifacts.push(library_artifact(metadata)?);
    }

    artifacts.extend(
        build_native_plan(manifest, environment)?
            .download_plan()
            .artifacts,
    );

    let plan = DownloadPlan { artifacts };
    plan.validate()?;
    for item in &plan.artifacts {
        if item.sources.iter().any(|s| !s.url.starts_with("https://")) {
            return Err(InstallPlanError::InsecureUrl(item.destination.clone()));
        }
    }
    Ok(plan)
}

fn library_artifact(metadata: &LibraryArtifact) -> Result<DownloadArtifact, InstallPlanError> {
    let relative = PathBuf::from(metadata.path.replace('\\', "/"));
    if !is_safe_relative_path(&relative) {
        return Err(InstallPlanError::UnsafeLibraryPath(relative));
    }
    artifact(
        PathBuf::from("libraries").join(relative),
        metadata.size,
        &metadata.sha1,
        &metadata.url,
    )
}

fn artifact(
    destination: PathBuf,
    size: u64,
    sha1: &str,
    url: &str,
) -> Result<DownloadArtifact, InstallPlanError> {
    if !url.starts_with("https://") {
        return Err(InstallPlanError::InsecureUrl(destination));
    }
    Ok(DownloadArtifact {
        destination,
        size,
        sha1: sha1.to_ascii_lowercase(),
        sources: vec![ArtifactSource {
            provider: "mojang".to_owned(),
            url: url.to_owned(),
        }],
    })
}

#[cfg(test)]
#[path = "install/install_tests.rs"]
mod tests;
