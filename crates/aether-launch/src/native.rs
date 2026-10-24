//! Select native archives for a Minecraft version; no download or extraction.

use std::path::{Path, PathBuf};

use aether_download::{
    ArtifactSource, DownloadArtifact, DownloadPlan, DownloadPlanError, is_safe_relative_path,
};
use aether_manifest::rules::Environment;
use aether_manifest::{LibraryArtifact, VersionManifest};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeArchive {
    pub library: String,
    pub classifier: String,
    pub artifact: DownloadArtifact,
    pub exclusions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NativePlan {
    pub archives: Vec<NativeArchive>,
}

impl NativePlan {
    pub fn download_plan(&self) -> DownloadPlan {
        DownloadPlan {
            artifacts: self.archives.iter().map(|native| native.artifact.clone()).collect(),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativePlanError {
    #[error("cannot resolve native architecture from {0:?}")]
    UnsupportedArchitecture(String),
    #[error("native classifier {classifier:?} for library {library:?} lacks download metadata")]
    MissingMetadata { library: String, classifier: String },
    #[error("unsafe native archive destination: {0:?}")]
    UnsafePath(PathBuf),
    #[error("invalid native extract exclusion: {0}")]
    InvalidExclusion(String),
    #[error(transparent)]
    Download(#[from] DownloadPlanError),
}

/// Plan platform-specific native JARs, without extracting or executing them.
/// Supports legacy natives maps with architecture templates and modern
/// libraries with a native classifier in their Maven coordinate.
pub fn build_native_plan(
    manifest: &VersionManifest,
    environment: &Environment,
) -> Result<NativePlan, NativePlanError> {
    let mut archives = Vec::new();
    for library in manifest.active_libraries(environment) {
        let named = library.name.split(':').nth(3).and_then(|part| {
            let classifier = part.split('@').next().unwrap_or(part);
            classifier.starts_with("natives-").then_some(classifier)
        });
        let selected = if let Some(template) = library.natives.get(&environment.os_name) {
            Some(expand_native_architecture(template, &environment.os_arch)?)
        } else if !library.natives.is_empty() {
            None
        } else {
            named
                .filter(|classifier| classifier_applies(classifier, environment))
                .map(str::to_owned)
        };
        let Some(classifier) = selected else {
            continue;
        };
        let metadata = library.downloads.as_ref().and_then(|downloads| {
            if named == Some(classifier.as_str()) {
                downloads.artifact.as_ref().or_else(|| downloads.classifiers.get(&classifier))
            } else {
                downloads.classifiers.get(&classifier)
            }
        }).ok_or_else(|| NativePlanError::MissingMetadata {
            library: library.name.clone(),
            classifier: classifier.clone(),
        })?;
        let exclusions = library.extract.as_ref()
            .map(|extract| extract.exclude.clone())
            .unwrap_or_default();
        for exclusion in &exclusions {
            let prefix = exclusion.trim_end_matches('/');
            if prefix.is_empty() || !is_safe_relative_path(Path::new(prefix)) {
                return Err(NativePlanError::InvalidExclusion(exclusion.clone()));
            }
        }
        archives.push(NativeArchive {
            library: library.name.clone(),
            classifier,
            artifact: download_artifact(metadata)?,
            exclusions,
        });
    }
    let result = NativePlan { archives };
    result.download_plan().validate()?;
    Ok(result)
}

fn download_artifact(metadata: &LibraryArtifact) -> Result<DownloadArtifact, NativePlanError> {
    if !is_safe_relative_path(Path::new(&metadata.path)) {
        return Err(NativePlanError::UnsafePath(PathBuf::from(&metadata.path)));
    }
    Ok(DownloadArtifact {
        destination: PathBuf::from("libraries").join(metadata.path.replace('\\', "/")),
        size: metadata.size,
        sha1: metadata.sha1.clone(),
        sources: vec![ArtifactSource {
            provider: "mojang".to_owned(),
            url: metadata.url.clone(),
        }],
    })
}

fn expand_native_architecture(template: &str, architecture: &str) -> Result<String, NativePlanError> {
    if !template.contains("${arch}") {
        return Ok(template.to_owned());
    }
    let bits = match architecture.to_ascii_lowercase().as_str() {
        "x86" | "i386" | "i486" | "i586" | "i686" | "32" => "32",
        "x86_64" | "amd64" | "x64" | "aarch64" | "arm64" | "64" => "64",
        _ => return Err(NativePlanError::UnsupportedArchitecture(architecture.to_owned())),
    };
    Ok(template.replace("${arch}", bits))
}

fn classifier_applies(classifier: &str, environment: &Environment) -> bool {
    let platform = match environment.os_name.as_str() {
        "windows" => "natives-windows",
        "linux" => "natives-linux",
        "osx" => {
            if classifier.starts_with("natives-macos") {
                "natives-macos"
            } else {
                "natives-osx"
            }
        }
        _ => return false,
    };
    if classifier == platform {
        return true;
    }
    let Some(suffix) = classifier.strip_prefix(platform).and_then(|value| value.strip_prefix('-')) else {
        return false;
    };
    let architecture = environment.os_arch.to_ascii_lowercase();
    match suffix {
        "32" | "x86" => matches!(architecture.as_str(), "32" | "x86" | "i386" | "i686"),
        "64" | "x64" | "x86_64" | "amd64" => matches!(architecture.as_str(), "64" | "x64" | "x86_64" | "amd64"),
        "arm64" | "aarch64" => matches!(architecture.as_str(), "arm64" | "aarch64"),
        _ => false,
    }
}

#[cfg(test)]
#[path = "native/native_tests.rs"]
mod tests;
