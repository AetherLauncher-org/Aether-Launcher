//! Names, summaries and icons for the mods, resource packs and shader packs of an instance,
//! from Modrinth.
//!
//! A file is looked up by its SHA-1, never by its name: what leaves the machine is a list of
//! hashes (so Modrinth learns which files are in the folder, nothing else), and what comes
//! back is only a label. The hash is the identity; a platform answer cannot make the launcher
//! trust a file. Folders (unpacked packs) and files that are not on Modrinth are simply not
//! labelled.
//!
//! Answers are cached on disk (`<data>/cache`): a found file for a week, a missing one for a
//! day, hashes by file size and modification time so a big folder is not read again on every
//! visit. Icons are fetched by the backend (only from Modrinth's CDN, only real images, at
//! most 256 KiB) and handed to the interface as `data:` URLs, so the page's content security
//! policy stays closed to the network. With no network the cache answers, and the result says
//! so; a lookup never fails the list it decorates.
//!
//! Which addresses may be contacted is decided in [`net`]. See
//! `docs/modules/content-metadata.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aether_content::modrinth::{self, MAX_ICON_BYTES, MAX_PER_REQUEST, Project};
use aether_content::{Platform, image_mime};
use aether_storage::manage as store;
use aether_storage::manage::{ContentKind, game_directory};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::host::Host;
use crate::instances::load;
use crate::platform;

pub mod net;

pub use net::{ContentNet, UreqNet};

/// Files looked up per call; a folder with more is looked up in part.
const MAX_FILES: usize = 500;
/// Files bigger than this are not hashed.
const MAX_HASHED_BYTES: u64 = 512 * 1024 * 1024;
/// Icons fetched per call; the rest come on the next visit.
const MAX_NEW_ICONS: usize = 60;
/// How long a found file is trusted without asking again, in seconds.
const FOUND_TTL: u64 = 7 * 24 * 60 * 60;
/// How long a file Modrinth does not know is not asked about again, in seconds.
const MISSING_TTL: u64 = 24 * 60 * 60;
const MAX_CACHE_ENTRIES: usize = 5000;
const MAX_CACHE_BYTES: u64 = 8 * 1024 * 1024;
const CACHE_FILE: &str = "modrinth.json";

// --------------------------------------------------------------------------------------
// What the interface gets
// --------------------------------------------------------------------------------------

/// One file Modrinth knows. `name` is the file's plain name on disk; there is no path.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Identified {
    pub name: String,
    pub platform: Platform,
    /// For [`open_project_page`].
    pub project_slug: String,
    pub title: String,
    pub summary: String,
    pub version: String,
    /// A `data:` URL (PNG, JPEG, GIF or WebP), when the icon is known.
    pub icon: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Lookup {
    /// Only the files that were identified.
    pub items: Vec<Identified>,
    /// `true` when Modrinth could not be asked and what is shown is from the cache (or nothing).
    pub unreachable: bool,
}

// --------------------------------------------------------------------------------------
// Cache
// --------------------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    /// Answers by SHA-1.
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
    /// Hashes by `<instance>/<kind>/<name>`.
    #[serde(default)]
    files: BTreeMap<String, FileHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    checked: u64,
    found: Option<Found>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Found {
    project: Project,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileHash {
    size: u64,
    modified: u64,
    sha1: String,
}

impl Cache {
    fn load(path: &Path) -> Self {
        let Ok(metadata) = fs::metadata(path) else {
            return Self::default();
        };
        if metadata.len() > MAX_CACHE_BYTES {
            return Self::default();
        }
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes the cache (best effort: a cache that cannot be saved costs a request next time).
    fn save(&mut self, path: &Path) {
        if self.entries.len() > MAX_CACHE_ENTRIES {
            let mut by_age: Vec<(u64, String)> = self
                .entries
                .iter()
                .map(|(hash, entry)| (entry.checked, hash.clone()))
                .collect();
            by_age.sort();
            for (_, hash) in by_age.into_iter().take(self.entries.len() - MAX_CACHE_ENTRIES) {
                self.entries.remove(&hash);
            }
        }
        while self.files.len() > MAX_CACHE_ENTRIES {
            let Some(key) = self.files.keys().next().cloned() else {
                break;
            };
            self.files.remove(&key);
        }
        let Ok(text) = serde_json::to_string(self) else {
            return;
        };
        let Some(parent) = path.parent() else {
            return;
        };
        let temporary = path.with_extension("json.tmp");
        if fs::create_dir_all(parent).is_ok()
            && fs::write(&temporary, text).is_ok()
            && fs::rename(&temporary, path).is_err()
        {
            let _ = fs::remove_file(&temporary);
        }
    }

    fn is_fresh(&self, hash: &str, now: u64) -> bool {
        self.entries.get(hash).is_some_and(|entry| {
            let ttl = if entry.found.is_some() {
                FOUND_TTL
            } else {
                MISSING_TTL
            };
            entry.checked.saturating_add(ttl) > now
        })
    }
}

// --------------------------------------------------------------------------------------
// Hashing
// --------------------------------------------------------------------------------------

fn sha1_of_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The regular files of a folder with their hashes. The hash of a file that has not changed
/// (same size, same modification time) comes from the cache.
fn hashed_files(
    cache: &mut Cache,
    instance_key: &str,
    game: &Path,
    kind: ContentKind,
) -> Result<Vec<(String, String)>, String> {
    let entries = store::list_content(game, kind).map_err(|error| error.to_string())?;
    let mut hashed = Vec::new();
    for entry in entries
        .into_iter()
        .filter(|entry| !entry.is_folder && entry.size <= MAX_HASHED_BYTES)
        .take(MAX_FILES)
    {
        let Ok(path) = store::content_file(game, kind, &entry.name) else {
            continue;
        };
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let modified = metadata.modified().map_or(0, seconds);
        let key = format!("{instance_key}/{}/{}", kind.label(), entry.name);
        let known = cache
            .files
            .get(&key)
            .filter(|known| known.size == metadata.len() && known.modified == modified);
        let sha1 = match known {
            Some(known) => known.sha1.clone(),
            None => {
                let Ok(sha1) = sha1_of_file(&path) else {
                    continue;
                };
                cache.files.insert(
                    key,
                    FileHash {
                        size: metadata.len(),
                        modified,
                        sha1: sha1.clone(),
                    },
                );
                sha1
            }
        };
        hashed.push((entry.name, sha1));
    }
    Ok(hashed)
}

// --------------------------------------------------------------------------------------
// Asking Modrinth
// --------------------------------------------------------------------------------------

/// Asks about `wanted` and records the answers in `cache`. Nothing is recorded unless every
/// request succeeded, so a half-answered lookup does not turn unasked files into misses.
fn refresh(
    net: &dyn ContentNet,
    cache: &mut Cache,
    wanted: &BTreeSet<String>,
    now: u64,
) -> Result<(), String> {
    let hashes: Vec<String> = wanted.iter().cloned().collect();
    let mut versions = BTreeMap::new();
    for chunk in hashes.chunks(MAX_PER_REQUEST) {
        let body = modrinth::version_files_body(chunk).map_err(|error| error.to_string())?;
        let answer = net.post_json(&format!("{}/version_files", modrinth::API_BASE), &body)?;
        versions.extend(modrinth::parse_version_files(&answer).map_err(|error| error.to_string())?);
    }

    let project_ids: BTreeSet<String> = versions
        .values()
        .map(|version| version.project_id.clone())
        .collect();
    let project_ids: Vec<String> = project_ids.into_iter().collect();
    let mut projects: BTreeMap<String, Project> = BTreeMap::new();
    for chunk in project_ids.chunks(MAX_PER_REQUEST) {
        let url = modrinth::projects_url(chunk).map_err(|error| error.to_string())?;
        let answer = net.get_text(&url)?;
        for project in modrinth::parse_projects(&answer).map_err(|error| error.to_string())? {
            projects.insert(project.id.clone(), project);
        }
    }

    for hash in wanted {
        let found = versions.get(hash).and_then(|version| {
            projects.get(&version.project_id).map(|project| Found {
                project: project.clone(),
                version: version.version.clone(),
            })
        });
        cache.entries.insert(
            hash.clone(),
            Entry {
                checked: now,
                found,
            },
        );
    }
    Ok(())
}

fn icon_path(cache_dir: &Path, project_id: &str) -> Option<PathBuf> {
    modrinth::is_project_id(project_id).then(|| cache_dir.join("icons").join(project_id))
}

/// Fetches the icons of `projects` that are not on disk yet, up to [`MAX_NEW_ICONS`]. Stops at
/// the first network failure. Only real images are kept.
fn fetch_icons(net: &dyn ContentNet, cache_dir: &Path, projects: &[&Project]) {
    let mut fetched = 0;
    for project in projects {
        let (Some(url), Some(path)) = (
            project.icon_url.as_deref(),
            icon_path(cache_dir, &project.id),
        ) else {
            continue;
        };
        if path.exists() {
            continue;
        }
        if fetched >= MAX_NEW_ICONS {
            break;
        }
        fetched += 1;
        let Ok(bytes) = net.get_bytes(url, MAX_ICON_BYTES) else {
            break;
        };
        if bytes.len() <= MAX_ICON_BYTES
            && image_mime(&bytes).is_some()
            && fs::create_dir_all(cache_dir.join("icons")).is_ok()
        {
            let _ = fs::write(&path, bytes);
        }
    }
}

/// The stored icon as a `data:` URL, if there is one and it is still a small real image.
fn icon_data_url(cache_dir: &Path, project_id: &str) -> Option<String> {
    let path = icon_path(cache_dir, project_id)?;
    if fs::metadata(&path).ok()?.len() > MAX_ICON_BYTES as u64 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let mime = image_mime(&bytes)?;
    Some(format!("data:{mime};base64,{}", crate::schematics::base64(&bytes)))
}

// --------------------------------------------------------------------------------------
// The operation
// --------------------------------------------------------------------------------------

/// Looks up the mods, resource packs or shader packs of an instance on Modrinth.
///
/// # Errors
///
/// For an unknown instance, a kind that is not looked up (worlds and schematics are not on
/// Modrinth), or a folder that cannot be read. Network trouble is not an error: see
/// [`Lookup::unreachable`].
pub fn identify(host: &dyn Host, id: &str, kind: &str) -> Result<Lookup, String> {
    identify_with(&UreqNet, host, id, kind, seconds(SystemTime::now()))
}

pub(crate) fn identify_with(
    net: &dyn ContentNet,
    host: &dyn Host,
    id: &str,
    kind: &str,
    now: u64,
) -> Result<Lookup, String> {
    let kind = match ContentKind::parse(kind) {
        Some(kind @ (ContentKind::Mods | ContentKind::ResourcePacks | ContentKind::ShaderPacks)) => {
            kind
        }
        _ => return Err("this kind of content is not looked up".to_owned()),
    };
    let (_, instance) = load(host, id)?;
    let game = game_directory(&instance);
    let cache_dir = host.data_dir()?.join("cache");
    let cache_file = cache_dir.join(CACHE_FILE);
    let mut cache = Cache::load(&cache_file);

    let files = hashed_files(&mut cache, &instance.id.to_string(), &game, kind)?;

    let wanted: BTreeSet<String> = files
        .iter()
        .filter(|(_, hash)| !cache.is_fresh(hash, now))
        .map(|(_, hash)| hash.clone())
        .collect();
    let unreachable = !wanted.is_empty() && refresh(net, &mut cache, &wanted, now).is_err();

    let known: Vec<(&String, &Found)> = files
        .iter()
        .filter_map(|(name, hash)| {
            let found = cache.entries.get(hash)?.found.as_ref()?;
            Some((name, found))
        })
        .collect();

    if !unreachable {
        let mut projects: Vec<&Project> = known.iter().map(|(_, found)| &found.project).collect();
        projects.sort_by(|a, b| a.id.cmp(&b.id));
        projects.dedup_by(|a, b| a.id == b.id);
        fetch_icons(net, &cache_dir, &projects);
    }

    let items = known
        .iter()
        .map(|(name, found)| Identified {
            name: (*name).clone(),
            platform: Platform::Modrinth,
            project_slug: found.project.slug.clone(),
            title: found.project.title.clone(),
            summary: found.project.summary.clone(),
            version: found.version.clone(),
            icon: icon_data_url(&cache_dir, &found.project.id),
        })
        .collect();

    cache.save(&cache_file);
    Ok(Lookup { items, unreachable })
}

/// Opens a project's page on modrinth.com in the default browser. `slug` must be a slug as
/// returned by [`identify`]; the address is built here, so the interface cannot name another.
///
/// # Errors
///
/// For a slug that is not one, or when the browser cannot be started.
pub fn open_project_page(slug: &str) -> Result<(), String> {
    let url = modrinth::project_page_url(slug).ok_or_else(|| "not a project".to_owned())?;
    platform::open_in_browser(&url)
}

#[cfg(test)]
#[path = "identify_tests.rs"]
mod tests;
