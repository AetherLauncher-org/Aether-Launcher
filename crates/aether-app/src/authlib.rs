//! Official metadata is the trust root; artifact mirrors supply only hash-checked bytes.
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::{Host, cachefile};
use aether_auth::authlib::{self, ArtifactManifest};
use aether_auth::http::{HttpError, HttpRequest, HttpResponse, HttpTransport};

const MANIFEST_ROOT: &str = "https://authlib-injector.yushi.moe/artifact/";
const MAX_JAR_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 8 * 1024;
const STORAGE: &str = "authlib-storage-error";
// Serializes cache publication and index replacement across all callers in this process.
static CACHE_GATE: Mutex<()> = Mutex::new(());

fn agent_root(host: &dyn Host) -> Result<PathBuf, String> {
    host.data_dir()
        .map(|base| base.join("authlib-injector"))
        .map_err(|_| STORAGE.to_owned())
}
#[must_use]
pub fn build_urls(build: u32, version: &str) -> [String; 3] {
    [
        format!(
            "https://authlib-injector.yushi.moe/artifact/{build}/authlib-injector-{version}.jar"
        ),
        format!(
            "https://github.com/yushijinhun/authlib-injector/releases/download/v{version}/authlib-injector-{version}.jar"
        ),
        format!(
            "https://bmclapi2.bangbang93.com/mirrors/authlib-injector/artifact/{build}/authlib-injector-{version}.jar"
        ),
    ]
}
fn jar_name(version: &str) -> String {
    format!("authlib-injector-{version}.jar")
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    build_number: u32,
    version: String,
    sha256: String,
}
impl Index {
    fn manifest(&self) -> Result<ArtifactManifest, String> {
        // Reuse exactly the official metadata's bounds and character rules on local input.
        let body = serde_json::json!({"build_number":self.build_number,"version":self.version,"checksums":{"sha256":self.sha256}}).to_string();
        authlib::parse_artifact_manifest(&HttpResponse::new(200, body)).map_err(str::to_owned)
    }
}
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
fn regular(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
}
fn prepare_root(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root).map_err(|_| STORAGE)?;
    let metadata = fs::symlink_metadata(root).map_err(|_| STORAGE)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(STORAGE.to_owned());
    }
    Ok(())
}
fn read_index(root: &Path) -> Option<ArtifactManifest> {
    let path = root.join("verified.json");
    if !regular(&path) {
        return None;
    }
    let bytes = cachefile::read_capped(&path, MAX_MANIFEST_BYTES)?;
    serde_json::from_slice::<Index>(&bytes)
        .ok()?
        .manifest()
        .ok()
}
fn write_index(root: &Path, manifest: &ArtifactManifest) -> Result<(), String> {
    let index = Index {
        build_number: manifest.build_number,
        version: manifest.version.clone(),
        sha256: manifest.sha256.clone(),
    };
    let bytes = serde_json::to_vec(&index).map_err(|_| STORAGE)?;
    cachefile::write_replacing(&root.join("verified.json"), &bytes).map_err(|_| STORAGE.to_owned())
}
fn offline(root: &Path) -> Result<PathBuf, String> {
    let manifest = read_index(root).ok_or("authlib-network-error")?;
    let path = root.join(jar_name(&manifest.version));
    if verify_jar(&path, &manifest.sha256) {
        Ok(path)
    } else {
        Err("authlib-cache-invalid".to_owned())
    }
}
/// Downloads raw bytes from one candidate URL, capped by the caller. The production
/// implementation is [`crate::yggdrasil::public_get_artifact_bytes`].
type JarFetcher = dyn Fn(&str, u64) -> Result<Vec<u8>, ()>;

/// Metadata is requested only from the fixed official endpoint. No mirror supplies hashes.
pub fn ensure_agent(
    host: &dyn Host,
    transport: &dyn HttpTransport,
    fetch: &JarFetcher,
) -> Result<PathBuf, String> {
    let _guard = CACHE_GATE.lock().map_err(|_| STORAGE)?;
    let root = agent_root(host)?;
    prepare_root(&root)?;
    let request = HttpRequest::get(format!("{MANIFEST_ROOT}latest.json"));
    let response = match transport.execute(&request) {
        Ok(response) => response,
        Err(_) => return offline(&root),
    };
    let manifest = authlib::parse_artifact_manifest(&response).map_err(str::to_owned)?;
    let target = root.join(jar_name(&manifest.version));
    if verify_jar(&target, &manifest.sha256) {
        write_index(&root, &manifest)?;
        return Ok(target);
    }
    // Never overwrite any existing version, including a conflicting verified artifact.
    // Corruption remains visible; removing it is an explicit maintenance operation.
    if fs::symlink_metadata(&target).is_ok() {
        return Err("authlib-cache-conflict".to_owned());
    }
    let bytes = build_urls(manifest.build_number, &manifest.version)
        .iter()
        .find_map(|url| {
            let bytes = fetch(url, MAX_JAR_BYTES).ok()?;
            (!bytes.is_empty()
                && bytes.len() as u64 <= MAX_JAR_BYTES
                && sha256_hex(&bytes) == manifest.sha256)
                .then_some(bytes)
        })
        .ok_or("authlib-download-error")?;
    // Stage under an unpredictable, exclusive name, then publish by rename. On Windows a
    // rename into an existing destination fails rather than replacing it, which is exactly
    // the no-overwrite guarantee a verified artifact needs. (hard_link is not portable here:
    // some temp filesystems reject it.)
    let temporary = root.join(format!(".{}.part", aether_auth::AccountId::new().as_uuid()));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| STORAGE)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| STORAGE)?;
        drop(file);
        match fs::rename(&temporary, &target) {
            Ok(()) => Ok(()),
            // Destination already exists (or another writer won the race): do not overwrite.
            Err(_) if target.exists() => Err("authlib-cache-conflict".to_owned()),
            Err(_) => Err(STORAGE.to_owned()),
        }
    })();
    let _ = fs::remove_file(&temporary);
    result?;
    // Rehash the published file before returning a launch path.
    if !verify_jar(&target, &manifest.sha256) {
        return Err("authlib-cache-invalid".to_owned());
    }
    write_index(&root, &manifest)?;
    Ok(target)
}
struct MetadataTransport;
impl HttpTransport for MetadataTransport {
    fn execute(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        if request.url != format!("{MANIFEST_ROOT}latest.json") {
            return Err(HttpError::transport("authlib-injector.yushi.moe"));
        }
        let bytes = crate::yggdrasil::public_get_bytes(&request.url, MAX_MANIFEST_BYTES)
            .map_err(|_| HttpError::transport("authlib-injector.yushi.moe"))?;
        let body = String::from_utf8(bytes)
            .map_err(|_| HttpError::transport("authlib-injector.yushi.moe"))?;
        Ok(HttpResponse::new(200, body))
    }
}
pub(crate) fn ensure_agent_production(host: &dyn Host) -> Result<PathBuf, String> {
    ensure_agent(
        host,
        &MetadataTransport,
        &crate::yggdrasil::public_get_artifact_bytes,
    )
}
/// Every launch, including offline launch, rehashes bounded bytes before using the path.
pub fn verify_jar(path: &Path, expected: &str) -> bool {
    if !regular(path)
        || expected.len() != 64
        || !expected
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return false;
    }
    cachefile::read_capped(path, MAX_JAR_BYTES)
        .is_some_and(|bytes| !bytes.is_empty() && sha256_hex(&bytes) == expected)
}
#[cfg(test)]
#[path = "authlib_tests.rs"]
mod tests;
