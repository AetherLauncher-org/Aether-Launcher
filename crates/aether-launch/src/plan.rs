//! Assemble a modern Minecraft launch plan without starting a process.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aether_download::is_safe_relative_path;
use aether_manifest::VersionManifest;
use aether_manifest::arguments::expand_arguments;
use aether_manifest::rules::Environment;
use aether_runtime::JavaRuntime;
use thiserror::Error;

use crate::{JavaSelectionError, LaunchError, LaunchPlan, LaunchRequest, select_java_runtime};

/// The client JAR is explicit: an inherited version may use its parent's JAR.
#[derive(Clone)]
pub struct LaunchOptions {
    pub game_directory: PathBuf,
    pub working_directory: PathBuf,
    pub client_jar: PathBuf,
    pub natives_directory: PathBuf,
    /// May contain account secrets; never log.
    pub variables: BTreeMap<String, String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LaunchBuildError {
    #[error(transparent)]
    InvalidRequest(#[from] LaunchError),
    #[error(transparent)]
    JavaSelection(#[from] JavaSelectionError),
    #[error("version manifest is missing mainClass")]
    MissingMainClass,
    #[error("legacy Minecraft arguments are not supported yet")]
    UnsupportedArguments,
    #[error("required local file does not exist: {0:?}")]
    MissingFile(PathBuf),
    #[error("required directory does not exist: {0:?}")]
    MissingDirectory(PathBuf),
    #[error("unsafe library path in manifest: {0}")]
    UnsafeLibraryPath(String),
    #[error("library escapes the libraries directory: {0:?}")]
    EscapedLibraryPath(PathBuf),
    #[error("cannot resolve Maven JAR location for {0}")]
    InvalidCoordinate(String),
    #[error("native classified library requires extraction support: {0}")]
    UnsupportedNativeLibrary(String),
    #[error("cannot construct Java classpath: {0}")]
    InvalidClasspath(String),
    #[error("unclosed Minecraft argument placeholder: {0}")]
    UnclosedPlaceholder(String),
    #[error("missing value for Minecraft argument placeholder: {0}")]
    MissingPlaceholder(String),
}

/// Assemble argv: JVM options, main class, and game arguments, in that order.
/// No shell is used; callers must prepare natives and authentication separately.
pub fn build_launch_plan(
    request: &LaunchRequest,
    manifest: &VersionManifest,
    runtime: &JavaRuntime,
    environment: &Environment,
    options: &LaunchOptions,
) -> Result<LaunchPlan, LaunchBuildError> {
    request.validate()?;
    select_java_runtime(manifest, std::slice::from_ref(runtime))?;

    let main_class = manifest.main_class.as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(LaunchBuildError::MissingMainClass)?;
    let args = manifest.arguments.as_ref().ok_or(LaunchBuildError::UnsupportedArguments)?;

    ensure_file(&options.client_jar)?;
    ensure_directory(&options.working_directory)?;
    ensure_directory(&options.natives_directory)?;

    let library_root = options.game_directory.join("libraries");
    let canonical_root = std::fs::canonicalize(&library_root)
        .map_err(|_| LaunchBuildError::MissingDirectory(library_root.clone()))?;

    let mut library_jars = Vec::new();
    for library in manifest.active_libraries(environment) {
        let relative = library_path(library)?;
        let absolute = canonical_root.join(relative);
        ensure_file(&absolute)?;
        let canonical = std::fs::canonicalize(&absolute)
            .map_err(|_| LaunchBuildError::MissingFile(absolute.clone()))?;
        if !canonical.starts_with(&canonical_root) {
            return Err(LaunchBuildError::EscapedLibraryPath(absolute));
        }
        library_jars.push(absolute);
    }
    library_jars.push(options.client_jar.clone());

    let classpath = std::env::join_paths(&library_jars)
        .map_err(|error| LaunchBuildError::InvalidClasspath(error.to_string()))?
        .to_string_lossy()
        .into_owned();
    let mut variables = options.variables.clone();
    // Intrinsic placeholders cannot be overridden by account-supplied variables.
    variables.insert("classpath".to_owned(), classpath.clone());
    variables.insert("classpath_separator".to_owned(), if cfg!(windows) { ";" } else { ":" }.to_owned());
    variables.insert("natives_directory".to_owned(), options.natives_directory.to_string_lossy().into_owned());
    variables.insert("library_directory".to_owned(), library_root.to_string_lossy().into_owned());
    variables.insert("game_directory".to_owned(), options.working_directory.to_string_lossy().into_owned());
    variables.insert("assets_root".to_owned(), options.game_directory.join("assets").to_string_lossy().into_owned());
    variables.insert("version_name".to_owned(), manifest.id.clone());
    if let Some(asset_id) = manifest.asset_index.as_ref().map(|index| index.id.as_str())
        .or(manifest.assets.as_deref())
    {
        variables.insert("assets_index_name".to_owned(), asset_id.to_owned());
    }

    let jvm = expand_arguments(&args.jvm, environment);
    let game = expand_arguments(&args.game, environment);
    let classpath_provided = jvm.iter().any(|arg| matches!(arg.as_str(), "-cp" | "-classpath" | "--class-path"));

    let mut arguments = vec![format!("-Xmx{}m", request.memory_mib)];
    for arg in jvm {
        arguments.push(substitute(&arg, &variables)?);
    }
    if !classpath_provided {
        arguments.extend(["-cp".to_owned(), classpath]);
    }
    arguments.push(main_class.to_owned());
    for arg in game {
        arguments.push(substitute(&arg, &variables)?);
    }

    Ok(LaunchPlan {
        runtime: runtime.clone(),
        working_directory: options.working_directory.clone(),
        arguments,
    })
}

fn ensure_file(path: &Path) -> Result<(), LaunchBuildError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(LaunchBuildError::MissingFile(path.to_owned()))
    }
}

fn ensure_directory(path: &Path) -> Result<(), LaunchBuildError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(LaunchBuildError::MissingDirectory(path.to_owned()))
    }
}

fn library_path(library: &aether_manifest::Library) -> Result<PathBuf, LaunchBuildError> {
    let name = library.name.split('@').next().unwrap_or("");
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() == 4 && parts[3].starts_with("natives-") {
        return Err(LaunchBuildError::UnsupportedNativeLibrary(library.name.clone()));
    }

    let path = if let Some(artifact) = library.downloads.as_ref()
        .and_then(|download| download.artifact.as_ref())
    {
        artifact.path.clone()
    } else {
        if !(3..=4).contains(&parts.len())
            || parts.iter().any(|part| part.is_empty())
            || library.name.contains('@')
        {
            return Err(LaunchBuildError::InvalidCoordinate(library.name.clone()));
        }
        let (group, artifact, version) = (parts[0], parts[1], parts[2]);
        let filename = if parts.len() == 4 {
            format!("{artifact}-{version}-{}.jar", parts[3])
        } else {
            format!("{artifact}-{version}.jar")
        };
        format!("{}/{artifact}/{version}/{filename}", group.replace('.', "/"))
    };

    let relative = PathBuf::from(path.replace('\\', "/"));
    if !is_safe_relative_path(&relative) {
        return Err(LaunchBuildError::UnsafeLibraryPath(path));
    }
    Ok(relative)
}

fn substitute(argument: &str, variables: &BTreeMap<String, String>) -> Result<String, LaunchBuildError> {
    let mut expanded = String::with_capacity(argument.len());
    let mut rest = argument;
    while let Some(start) = rest.find("${") {
        expanded.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let Some(end) = tail.find('}') else {
            return Err(LaunchBuildError::UnclosedPlaceholder(argument.to_owned()));
        };
        let name = &tail[..end];
        let value = variables.get(name)
            .ok_or_else(|| LaunchBuildError::MissingPlaceholder(name.to_owned()))?;
        expanded.push_str(value);
        rest = &tail[end + 1..];
    }
    expanded.push_str(rest);
    Ok(expanded)
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
