//! Download the client JAR, libraries and natives of a cached Vanilla version.
//!
//! The frontend supplies only a version ID. URLs, hashes and destinations all
//! come from the version JSON that `cache_vanilla_version_manifest` verified
//! against the official catalog. Assets are fetched last, from the asset index
//! whose hash that JSON declares.

use std::path::{Path, PathBuf};

use aether_download::assets::build_asset_object_plan;
use aether_download::transport::http::UreqKernel;
use aether_download::transport::{HttpTransport, KernelHttpTransport};
use aether_download::{
    ArtifactResult, ArtifactSource, CancellationToken, DownloadArtifact, DownloadEngine,
    DownloadPlan, NoopProgress, is_safe_relative_path,
};
use aether_launch::build_install_plan;
use aether_manifest::load_resolved_version_manifest;
use tauri::Manager;

use super::launch::current_environment;

const DOWNLOAD_CONCURRENCY: usize = 4;

fn install_version(
    transport: &dyn HttpTransport,
    game_root: &Path,
    version: &str,
) -> Result<usize, String> {
    if !is_safe_relative_path(Path::new(version)) {
        return Err("invalid version identifier".to_owned());
    }
    let resolved = load_resolved_version_manifest(game_root, version)
        .map_err(|_| "version JSON is not cached; cache it first".to_owned())?;
    let client_version = resolved
        .client_jar_version
        .ok_or_else(|| "version JSON has no official client download".to_owned())?;
    let plan = build_install_plan(&resolved.manifest, &client_version, &current_environment())
        .map_err(|error| format!("cannot plan installation: {error}"))?;

    let engine = DownloadEngine::new(transport, game_root);
    let mut total = run_plan(&engine, &plan)?;

    if let Some(index) = &resolved.manifest.asset_index {
        let index_url = index.url.clone();
        if !index_url.starts_with("https://") {
            return Err("asset index URL is not HTTPS".to_owned());
        }
        let index_plan = DownloadPlan {
            artifacts: vec![DownloadArtifact {
                destination: PathBuf::from("assets")
                    .join("indexes")
                    .join(format!("{}.json", index.id)),
                size: index.size,
                sha1: index.sha1.to_ascii_lowercase(),
                sources: vec![ArtifactSource {
                    provider: "mojang".to_owned(),
                    url: index_url,
                }],
            }],
        };
        total += run_plan(&engine, &index_plan)?;
        let objects = build_asset_object_plan(game_root, &index.id, &index.sha1, index.size)
            .map_err(|error| format!("cannot plan asset download: {error}"))?;
        total += run_plan(&engine, &objects)?;
    }
    Ok(total)
}

fn run_plan(engine: &DownloadEngine<'_>, plan: &DownloadPlan) -> Result<usize, String> {
    let results = engine
        .download_plan(
            plan,
            DOWNLOAD_CONCURRENCY,
            &CancellationToken::new(),
            &NoopProgress,
        )
        .map_err(|error| format!("cannot start download: {error}"))?;
    summarize(&results)
}

/// Report how many files failed without echoing URLs or response details.
fn summarize(results: &[ArtifactResult]) -> Result<usize, String> {
    let failed = results.iter().filter(|item| item.outcome.is_err()).count();
    if failed > 0 {
        return Err(format!(
            "{failed} of {} files failed to download or verify",
            results.len()
        ));
    }
    Ok(results.len())
}

/// Downloads and SHA-1-verifies the client JAR, libraries, native archives
/// and assets of an already cached Vanilla version. Returns the number of
/// verified files. Natives are extracted at launch; no instance is marked ready.
#[tauri::command]
pub async fn install_vanilla_client_libraries(
    app: tauri::AppHandle,
    version: String,
) -> Result<usize, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|_| "application data directory is unavailable".to_owned())?;
    tauri::async_runtime::spawn_blocking(move || {
        let transport = KernelHttpTransport::new(UreqKernel::new());
        install_version(&transport, &base.join("game"), &version)
    })
    .await
    .map_err(|_| "client and library installation task failed".to_owned())?
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
