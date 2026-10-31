//! Finding content on Modrinth and installing it into an instance (or one of its worlds).
//!
//! A caller names an instance by id, a project by its Modrinth id and a world by its folder
//! name; it never supplies a path or an address. The folder a file lands in comes from the
//! kind of project and the verified instance record, the address comes from Modrinth's answer
//! and must be on Modrinth's CDN, and the bytes are checked against the SHA-1 (and SHA-512)
//! and the size Modrinth lists before they become a file with their real name. A file that
//! does not check out is deleted and nothing is left behind. An existing file is never
//! overwritten.
//!
//! The mirror is not a trust root: the hash comes from the platform's own API answer, over
//! HTTPS, and a file that arrives with another hash is refused.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use aether_content::browse::{
    self, DependencyKind, Hit, ProjectType, SearchQuery, Sort, VersionFile, is_game_version,
};
use aether_core::GameInstance;
use aether_storage::manage::game_directory;
use serde::Serialize;
use sha1::{Digest as _, Sha1};
use sha2::Sha512;

use crate::host::Host;
use crate::identify::{ContentNet, UreqNet, icons_for};
use crate::instances::load;
use crate::world::world_folder;

/// How deep required dependencies are followed, and how many files one install may add.
const MAX_DEPENDENCY_DEPTH: usize = 3;
const MAX_FILES_PER_INSTALL: usize = 12;

// --------------------------------------------------------------------------------------
// What an instance can take
// --------------------------------------------------------------------------------------

/// The game version and mod loader of an instance, which decide what content fits it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    /// `1.21.1`; `None` when the installed version id does not say.
    pub game_version: Option<String>,
    /// `fabric`, `quilt`, `forge` or `neoforge`; `None` for plain Vanilla.
    pub loader: Option<&'static str>,
}

/// Reads the game version and loader from the installed version id, which the installer builds
/// as `fabric-loader-<loader>-<game>`, `quilt-loader-<loader>-<game>`, `<game>-forge-<ver>`,
/// `neoforge-<ver>` (`21.1.172` is for `1.21.1`) or `neoforge-<game>-<ver>`. Anything else is
/// taken for a plain game version.
#[must_use]
pub fn target_of(version_id: &str) -> Target {
    let game = |text: &str| is_game_version(text).then(|| text.to_owned());
    for (prefix, loader) in [("fabric-loader-", "fabric"), ("quilt-loader-", "quilt")] {
        if let Some(rest) = version_id.strip_prefix(prefix) {
            return Target {
                game_version: rest.split_once('-').and_then(|(_, game_id)| game(game_id)),
                loader: Some(loader),
            };
        }
    }
    if let Some((game_id, _)) = version_id.split_once("-forge-") {
        return Target {
            game_version: game(game_id),
            loader: Some("forge"),
        };
    }
    if let Some(rest) = version_id.strip_prefix("neoforge-") {
        let game_version = match rest.split('.').collect::<Vec<_>>().as_slice() {
            // `neoforge-1.20.1-47.1.106`
            [first, ..] if *first == "1" => rest.split_once('-').and_then(|(id, _)| game(id)),
            // `neoforge-21.1.172` is for 1.21.1, `neoforge-21.0.5` for 1.21
            [minor, patch, _, ..] if minor.bytes().all(|b| b.is_ascii_digit()) && !minor.is_empty() => {
                if *patch == "0" {
                    Some(format!("1.{minor}"))
                } else if patch.bytes().all(|b| b.is_ascii_digit()) && !patch.is_empty() {
                    Some(format!("1.{minor}.{patch}"))
                } else {
                    None
                }
            }
            _ => None,
        };
        return Target {
            game_version,
            loader: Some("neoforge"),
        };
    }
    Target {
        game_version: game(version_id),
        loader: None,
    }
}

/// What content fits an instance, for the download page's default filters.
///
/// # Errors
///
/// For an unknown instance.
pub fn content_target(host: &dyn Host, id: &str) -> Result<Target, String> {
    let (_, instance) = load(host, id)?;
    Ok(target_of(instance.version.as_str()))
}

// --------------------------------------------------------------------------------------
// Search
// --------------------------------------------------------------------------------------

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub project_id: String,
    /// Only for opening the project's page (`open_content_project`).
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub author: String,
    pub downloads: u64,
    pub follows: u64,
    /// `mod`, `resourcepack`, `shader`, `datapack` or `modpack`.
    pub project_type: String,
    /// Mod loaders the project supports.
    pub loaders: Vec<String>,
    /// Category tags.
    pub categories: Vec<String>,
    pub oldest_version: Option<String>,
    pub newest_version: Option<String>,
    /// `required`, `optional`, `unsupported` or `unknown`.
    pub client_side: &'static str,
    pub server_side: &'static str,
    /// ISO 8601; when the project was last changed.
    pub updated: String,
    /// `data:` image of the project's icon, when it has one and it is a small real picture.
    pub icon: Option<String>,
}

impl SearchHit {
    fn new(hit: Hit, icon: Option<String>) -> Self {
        Self {
            project_id: hit.project_id,
            slug: hit.slug,
            title: hit.title,
            summary: hit.summary,
            author: hit.author,
            downloads: hit.downloads,
            follows: hit.follows,
            project_type: hit.project_type,
            loaders: hit.loaders,
            categories: hit.categories,
            oldest_version: hit.oldest_version,
            newest_version: hit.newest_version,
            client_side: hit.client_side,
            server_side: hit.server_side,
            updated: hit.updated,
            icon,
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub total: u64,
}

/// What to search for. All of it is checked before it becomes part of a request.
#[derive(Debug, Clone, Copy)]
pub struct SearchRequest<'a> {
    pub text: &'a str,
    /// `mod`, `datapack`, `resourcepack`, `shader` or `modpack`.
    pub project_type: &'a str,
    pub game_version: Option<&'a str>,
    /// A mod loader; only used for mods and modpacks.
    pub loader: Option<&'a str>,
    /// `relevance`, `downloads`, `newest` or `updated`.
    pub sort: &'a str,
    pub offset: u32,
}

/// One page of Modrinth search results, with the projects' icons (taken from the icon cache,
/// the missing ones fetched a few at a time from Modrinth's CDN).
///
/// # Errors
///
/// For a type, sort, game version or loader that is not one, or a platform that cannot be
/// reached or answers something unusable. A project whose icon cannot be had is still listed.
pub fn search(
    net: &dyn ContentNet,
    host: &dyn Host,
    request: &SearchRequest<'_>,
) -> Result<SearchResult, String> {
    let project_type = ProjectType::parse(request.project_type).ok_or("unknown content type")?;
    let sort = Sort::parse(request.sort).ok_or("unknown sort order")?;
    let url = browse::search_url(&SearchQuery {
        text: request.text,
        project_type,
        game_version: request.game_version,
        // a loader only narrows mods and modpacks
        loader: request
            .loader
            .filter(|_| matches!(project_type, ProjectType::Mod | ProjectType::ModPack)),
        sort,
        offset: request.offset,
        limit: 20,
    })
    .map_err(|error| error.to_string())?;
    let page = browse::parse_search(&net.get_text(&url)?).map_err(|error| error.to_string())?;
    let wanted: Vec<(&str, &str)> = page
        .hits
        .iter()
        .filter_map(|hit| hit.icon_url.as_deref().map(|icon| (hit.project_id.as_str(), icon)))
        .collect();
    let icons = match host.data_dir() {
        Ok(data) => icons_for(net, &data.join("cache"), &wanted),
        Err(_) => BTreeMap::new(),
    };
    Ok(SearchResult {
        hits: page
            .hits
            .into_iter()
            .map(|hit| {
                let icon = icons.get(&hit.project_id).cloned();
                SearchHit::new(hit, icon)
            })
            .collect(),
        total: page.total,
    })
}

/// [`search`] against the real platform.
///
/// # Errors
///
/// As [`search`].
pub fn search_modrinth(host: &dyn Host, request: &SearchRequest<'_>) -> Result<SearchResult, String> {
    search(&UreqNet, host, request)
}

// --------------------------------------------------------------------------------------
// Downloading one verified file
// --------------------------------------------------------------------------------------

/// Downloads `file` into `folder` under its own name, and keeps it only if its size and hashes
/// are what Modrinth listed.
fn download_verified(net: &dyn ContentNet, file: &VersionFile, folder: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(folder).map_err(|_| "cannot create the folder".to_owned())?;
    let target = folder.join(&file.filename);
    let temporary = folder.join(format!(".{}.aether-part", file.filename));

    // claim the name first so a file that appears meanwhile is not replaced
    let claim = fs::OpenOptions::new().write(true).create_new(true).open(&target);
    if claim.is_err() {
        return Err("a file with this name is already there".to_owned());
    }
    drop(claim);

    let result = write_checked(net, file, &temporary).and_then(|()| {
        fs::rename(&temporary, &target).map_err(|_| "cannot place the file".to_owned())
    });
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(&target);
    }
    result.map(|()| target)
}

fn write_checked(net: &dyn ContentNet, file: &VersionFile, temporary: &Path) -> Result<(), String> {
    // a stale temporary (or a link planted at its name) is removed, never followed
    let _ = fs::remove_file(temporary);
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
        .map_err(|_| "cannot write the file".to_owned())?;
    let mut reader = net.get_stream(&file.url, file.size.saturating_add(1))?;

    let mut sha1 = Sha1::new();
    let mut sha512 = Sha512::new();
    let mut total = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| "the download was interrupted".to_owned())?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > file.size {
            return Err("the file is larger than the platform said".to_owned());
        }
        sha1.update(&buffer[..read]);
        sha512.update(&buffer[..read]);
        out.write_all(&buffer[..read])
            .map_err(|_| "cannot write the file".to_owned())?;
    }
    out.flush().map_err(|_| "cannot write the file".to_owned())?;
    if total != file.size {
        return Err("the download ended early".to_owned());
    }
    if format!("{:x}", sha1.finalize()) != file.sha1 {
        return Err("the file does not match its checksum and was discarded".to_owned());
    }
    if let Some(expected) = &file.sha512
        && format!("{:x}", sha512.finalize()) != *expected
    {
        return Err("the file does not match its checksum and was discarded".to_owned());
    }
    Ok(())
}

// --------------------------------------------------------------------------------------
// Installing
// --------------------------------------------------------------------------------------

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledFile {
    pub project_id: String,
    pub file_name: String,
    /// The version the author gave (`0.6.0`).
    pub version: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkippedProject {
    pub project_id: String,
    pub reason: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    pub installed: Vec<InstalledFile>,
    /// Required dependencies that could not be installed, and why; the project itself is an
    /// error when it fails, not an entry here.
    pub skipped: Vec<SkippedProject>,
}

fn refuse_while_running(host: &dyn Host, instance: &GameInstance) -> Result<(), String> {
    if host.is_instance_running(instance.id)? {
        return Err("stop the game before installing content".to_owned());
    }
    Ok(())
}

/// The folder a kind of project goes into, and the extension its files must have.
fn destination(
    game: &Path,
    project_type: ProjectType,
    world: Option<&str>,
) -> Result<(PathBuf, &'static str), String> {
    Ok(match project_type {
        ProjectType::Mod => (game.join("mods"), "jar"),
        ProjectType::ResourcePack => (game.join("resourcepacks"), "zip"),
        ProjectType::Shader => (game.join("shaderpacks"), "zip"),
        ProjectType::DataPack => {
            let world = world.ok_or("choose a world for the data pack")?;
            (world_folder(game, world)?.join("datapacks"), "zip")
        }
        ProjectType::ModPack => return Err("modpacks cannot be installed here yet".to_owned()),
    })
}

fn has_extension(name: &str, extension: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

/// Installs the newest suitable version of a project (and, if asked, the required
/// dependencies of mods) into the instance. See the module notes for what is checked.
///
/// # Errors
///
/// While the instance's game runs; for an unknown instance, project type or world; for an
/// instance whose game version is unknown, or a mod for an instance without a mod loader; when
/// no version of the project fits; and when the file cannot be downloaded or does not match
/// its checksum. Nothing is left behind in those cases.
pub fn install_with(
    net: &dyn ContentNet,
    host: &dyn Host,
    id: &str,
    project_id: &str,
    project_type: &str,
    world: Option<&str>,
    with_dependencies: bool,
) -> Result<InstallReport, String> {
    let (_, instance) = load(host, id)?;
    refuse_while_running(host, &instance)?;
    let project_type = ProjectType::parse(project_type).ok_or("unknown content type")?;
    let target = target_of(instance.version.as_str());
    let game_version = target
        .game_version
        .ok_or("cannot tell which game version this instance is")?;
    let loader = if project_type == ProjectType::Mod {
        Some(target.loader.ok_or("this instance has no mod loader, mods need Fabric, Quilt, Forge or NeoForge")?)
    } else {
        None
    };
    let (folder, extension) = destination(&game_directory(&instance), project_type, world)?;

    let mut report = InstallReport {
        installed: Vec::new(),
        skipped: Vec::new(),
    };
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut queue = vec![(project_id.to_owned(), 0usize)];
    while let Some((project, depth)) = queue.pop() {
        if !seen.insert(project.clone()) {
            continue;
        }
        let is_main = depth == 0;
        if report.installed.len() >= MAX_FILES_PER_INSTALL {
            report.skipped.push(SkippedProject {
                project_id: project,
                reason: "too many dependencies for one install".to_owned(),
            });
            continue;
        }
        match install_one(net, &project, &game_version, loader, &folder, extension) {
            Ok((file, dependencies)) => {
                report.installed.push(file);
                if with_dependencies && depth < MAX_DEPENDENCY_DEPTH {
                    queue.extend(dependencies.into_iter().map(|dependency| (dependency, depth + 1)));
                }
            }
            Err(reason) if is_main => return Err(reason),
            Err(reason) => report.skipped.push(SkippedProject {
                project_id: project,
                reason,
            }),
        }
    }
    Ok(report)
}

/// Installs one project; returns what was installed and the projects it requires.
fn install_one(
    net: &dyn ContentNet,
    project_id: &str,
    game_version: &str,
    loader: Option<&str>,
    folder: &Path,
    extension: &str,
) -> Result<(InstalledFile, Vec<String>), String> {
    let url = browse::versions_url(project_id, Some(game_version), loader.as_slice())
        .map_err(|error| error.to_string())?;
    let versions = browse::parse_versions(&net.get_text(&url)?).map_err(|error| error.to_string())?;
    let version = browse::pick_version(&versions, game_version, loader)
        .ok_or_else(|| format!("no version of this project fits Minecraft {game_version}"))?;
    let file = browse::primary_file(version).ok_or("this version has no downloadable file")?;
    if !has_extension(&file.filename, extension) {
        return Err(format!("the file is not a .{extension} file"));
    }
    download_verified(net, file, folder)?;
    let dependencies = version
        .dependencies
        .iter()
        .filter(|dependency| dependency.kind == DependencyKind::Required)
        .filter_map(|dependency| dependency.project_id.clone())
        .collect();
    Ok((
        InstalledFile {
            project_id: project_id.to_owned(),
            file_name: file.filename.clone(),
            version: version.number.clone(),
        },
        dependencies,
    ))
}

/// [`install_with`] against the real platform. Streams the files to disk, so call it off the
/// interface thread.
///
/// # Errors
///
/// As [`install_with`].
pub fn install(
    host: &dyn Host,
    id: &str,
    project_id: &str,
    project_type: &str,
    world: Option<&str>,
    with_dependencies: bool,
) -> Result<InstallReport, String> {
    install_with(&UreqNet, host, id, project_id, project_type, world, with_dependencies)
}

#[cfg(test)]
#[path = "content_install_tests.rs"]
mod tests;
