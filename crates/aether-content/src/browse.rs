//! Modrinth: search for projects and read the versions (and files) of one, so a mod, data
//! pack, resource pack or shader pack can be installed.
//!
//! Like the rest of this crate it does no I/O: it builds request addresses and reads answers.
//! Everything in an answer is untrusted. A file is only offered when its name is a safe plain
//! file name, its address is on Modrinth's own CDN, its size is within [`MAX_FILE_BYTES`] and
//! it carries a SHA-1 (and a SHA-512 when Modrinth gives one), because the launcher checks the
//! bytes it downloads against those hashes and refuses a file it cannot check.

use serde::Deserialize;

use crate::modrinth::{API_BASE, ICON_HOST, MAX_BODY_BYTES, is_project_id, is_sha1, is_slug, trusted_icon_url};
use crate::{ContentError, clean_text};

/// The biggest file the launcher installs from a platform, in bytes.
pub const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Hits per search page.
pub const MAX_PAGE: u32 = 50;
const MAX_QUERY_CHARS: usize = 100;
const MAX_VERSIONS: usize = 500;
const MAX_FILES: usize = 20;
const MAX_DEPENDENCIES: usize = 100;
const MAX_LIST: usize = 100;

/// What kind of project to search for. The names are Modrinth's `project_type` facets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectType {
    Mod,
    ResourcePack,
    Shader,
    DataPack,
    ModPack,
}

impl ProjectType {
    #[must_use]
    pub fn facet(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::ResourcePack => "resourcepack",
            Self::Shader => "shader",
            Self::DataPack => "datapack",
            Self::ModPack => "modpack",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "mod" => Self::Mod,
            "resourcepack" => Self::ResourcePack,
            "shader" => Self::Shader,
            "datapack" => Self::DataPack,
            "modpack" => Self::ModPack,
            _ => return None,
        })
    }
}

/// The loaders a mod search may be narrowed to.
pub const LOADERS: [&str; 4] = ["fabric", "forge", "neoforge", "quilt"];

/// How results are ordered (Modrinth's `index`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Relevance,
    Downloads,
    Newest,
    Updated,
}

impl Sort {
    fn index(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "relevance" => Self::Relevance,
            "downloads" => Self::Downloads,
            "newest" => Self::Newest,
            "updated" => Self::Updated,
            _ => return None,
        })
    }
}

pub struct SearchQuery<'a> {
    pub text: &'a str,
    pub project_type: ProjectType,
    /// A game version (`1.21.1`) the project must support.
    pub game_version: Option<&'a str>,
    /// One of [`LOADERS`]; only meaningful for mods and modpacks.
    pub loader: Option<&'a str>,
    pub sort: Sort,
    pub offset: u32,
    pub limit: u32,
}

/// `true` for a game version id as Mojang names them: `1.21.1`, `24w14a`, `1.20.5-rc1`.
#[must_use]
pub fn is_game_version(text: &str) -> bool {
    (1..=32).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The address of `GET /search` for a query.
///
/// # Errors
///
/// [`ContentError::InvalidId`] for a game version or loader that is not one.
pub fn search_url(query: &SearchQuery<'_>) -> Result<String, ContentError> {
    let mut facets = vec![format!("[\"project_type:{}\"]", query.project_type.facet())];
    if let Some(version) = query.game_version {
        if !is_game_version(version) {
            return Err(ContentError::InvalidId);
        }
        facets.push(format!("[\"versions:{version}\"]"));
    }
    if let Some(loader) = query.loader {
        if !LOADERS.contains(&loader) {
            return Err(ContentError::InvalidId);
        }
        facets.push(format!("[\"categories:{loader}\"]"));
    }
    let text = clean_text(query.text, MAX_QUERY_CHARS);
    Ok(format!(
        "{API_BASE}/search?query={}&facets={}&index={}&offset={}&limit={}",
        percent_encode(&text),
        percent_encode(&format!("[{}]", facets.join(","))),
        query.sort.index(),
        query.offset.min(10_000),
        query.limit.clamp(1, MAX_PAGE),
    ))
}

/// One search hit, as far as the launcher shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
    pub project_type: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SearchPage {
    pub hits: Vec<Hit>,
    pub total: u64,
}

#[derive(Deserialize)]
struct RawHit {
    project_id: String,
    slug: Option<String>,
    title: String,
    description: Option<String>,
    author: Option<String>,
    downloads: Option<u64>,
    icon_url: Option<String>,
    project_type: String,
}

#[derive(Deserialize)]
struct RawSearch {
    hits: Vec<RawHit>,
    total_hits: Option<u64>,
}

/// Reads the answer to `GET /search`. A hit without a usable id or title, or of a type the
/// launcher does not know, is left out.
///
/// # Errors
///
/// [`ContentError::TooLarge`] and [`ContentError::Malformed`].
pub fn parse_search(body: &str) -> Result<SearchPage, ContentError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ContentError::TooLarge(MAX_BODY_BYTES));
    }
    let raw: RawSearch = serde_json::from_str(body).map_err(|_| ContentError::Malformed)?;
    let hits: Vec<Hit> = raw
        .hits
        .into_iter()
        .take(MAX_PAGE as usize)
        .filter(|hit| is_project_id(&hit.project_id) && ProjectType::parse(&hit.project_type).is_some())
        .filter_map(|hit| {
            let title = clean_text(&hit.title, 100);
            if title.is_empty() {
                return None;
            }
            let slug = hit
                .slug
                .filter(|slug| is_slug(slug))
                .unwrap_or_else(|| hit.project_id.clone());
            Some(Hit {
                project_id: hit.project_id,
                slug,
                title,
                summary: clean_text(hit.description.as_deref().unwrap_or(""), 300),
                author: clean_text(hit.author.as_deref().unwrap_or(""), 40),
                downloads: hit.downloads.unwrap_or(0),
                icon_url: hit.icon_url.as_deref().and_then(trusted_icon_url),
                project_type: hit.project_type,
            })
        })
        .collect();
    Ok(SearchPage {
        total: raw.total_hits.unwrap_or(hits.len() as u64).max(hits.len() as u64),
        hits,
    })
}

// --------------------------------------------------------------------------------------
// Versions and files
// --------------------------------------------------------------------------------------

/// The address of `GET /project/{id}/version`, narrowed to a game version and loaders.
///
/// # Errors
///
/// [`ContentError::InvalidId`] for a project id, game version or loader that is not one.
pub fn versions_url(
    project_id: &str,
    game_version: Option<&str>,
    loaders: &[&str],
) -> Result<String, ContentError> {
    if !is_project_id(project_id) {
        return Err(ContentError::InvalidId);
    }
    let mut url = format!("{API_BASE}/project/{project_id}/version");
    let mut separator = '?';
    if !loaders.is_empty() {
        if !loaders.iter().all(|loader| LOADERS.contains(loader)) {
            return Err(ContentError::InvalidId);
        }
        let list = loaders
            .iter()
            .map(|loader| format!("%22{loader}%22"))
            .collect::<Vec<_>>()
            .join("%2C");
        url.push_str(&format!("{separator}loaders=%5B{list}%5D"));
        separator = '&';
    }
    if let Some(version) = game_version {
        if !is_game_version(version) {
            return Err(ContentError::InvalidId);
        }
        url.push_str(&format!("{separator}game_versions=%5B%22{version}%22%5D"));
    }
    Ok(url)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    Release,
    Beta,
    Alpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    /// `None` when the dependency names only a version of some project.
    pub project_id: Option<String>,
    pub kind: DependencyKind,
}

/// A file that may be downloaded and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionFile {
    /// A safe plain file name.
    pub filename: String,
    /// HTTPS on [`ICON_HOST`] (Modrinth's CDN).
    pub url: String,
    pub size: u64,
    pub sha1: String,
    pub sha512: Option<String>,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectVersion {
    pub id: String,
    pub number: String,
    pub channel: Channel,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    /// ISO 8601, as Modrinth writes it; sorts as text.
    pub published: String,
    pub files: Vec<VersionFile>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Deserialize)]
struct RawFileHashes {
    sha1: Option<String>,
    sha512: Option<String>,
}

#[derive(Deserialize)]
struct RawFile {
    url: String,
    filename: String,
    size: u64,
    #[serde(default)]
    primary: bool,
    hashes: RawFileHashes,
}

#[derive(Deserialize)]
struct RawDependency {
    project_id: Option<String>,
    dependency_type: String,
}

#[derive(Deserialize)]
struct RawProjectVersion {
    id: String,
    version_number: Option<String>,
    name: Option<String>,
    version_type: String,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    loaders: Vec<String>,
    date_published: Option<String>,
    #[serde(default)]
    files: Vec<RawFile>,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
}

/// `true` for a lower-case, 128-digit hexadecimal SHA-512.
#[must_use]
pub fn is_sha512(text: &str) -> bool {
    text.len() == 128 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `true` for a name that is safe as the name of a file in any folder on any system: no path
/// separators or characters Windows refuses, no control characters, not a Windows device name,
/// no trailing dot or space.
#[must_use]
pub fn is_safe_filename(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 128
        || name.ends_with('.')
        || name.ends_with(' ')
        || name.starts_with(' ')
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    !reserved
}

/// `url` when it is an HTTPS address on Modrinth's CDN with no user info, port or odd
/// characters; `None` otherwise.
#[must_use]
pub fn trusted_file_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let clean = url.len() <= 1024
        && !url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\\' | '@'));
    (host == ICON_HOST && !path.is_empty() && clean).then(|| url.to_owned())
}

fn checked_file(raw: RawFile) -> Option<VersionFile> {
    let sha1 = raw.hashes.sha1.filter(|hash| is_sha1(hash))?;
    let sha512 = match raw.hashes.sha512 {
        Some(hash) if is_sha512(&hash) => Some(hash),
        Some(_) => return None,
        None => None,
    };
    (is_safe_filename(&raw.filename) && raw.size > 0 && raw.size <= MAX_FILE_BYTES).then_some(())?;
    Some(VersionFile {
        url: trusted_file_url(&raw.url)?,
        filename: raw.filename,
        size: raw.size,
        sha1,
        sha512,
        primary: raw.primary,
    })
}

fn short_list(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .take(MAX_LIST)
        .filter(|item| is_game_version(item))
        .collect()
}

/// Reads the answer to `GET /project/{id}/version`. A version without a usable id or channel,
/// or without at least one file the launcher could check, is left out.
///
/// # Errors
///
/// [`ContentError::TooLarge`] and [`ContentError::Malformed`].
pub fn parse_versions(body: &str) -> Result<Vec<ProjectVersion>, ContentError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ContentError::TooLarge(MAX_BODY_BYTES));
    }
    let raw: Vec<RawProjectVersion> = serde_json::from_str(body).map_err(|_| ContentError::Malformed)?;
    Ok(raw
        .into_iter()
        .take(MAX_VERSIONS)
        .filter_map(|version| {
            if !is_project_id(&version.id) {
                return None;
            }
            let channel = match version.version_type.as_str() {
                "release" => Channel::Release,
                "beta" => Channel::Beta,
                "alpha" => Channel::Alpha,
                _ => return None,
            };
            let files: Vec<VersionFile> = version
                .files
                .into_iter()
                .take(MAX_FILES)
                .filter_map(checked_file)
                .collect();
            if files.is_empty() {
                return None;
            }
            let number = version
                .version_number
                .filter(|text| !text.trim().is_empty())
                .or(version.name)
                .unwrap_or_default();
            let dependencies = version
                .dependencies
                .into_iter()
                .take(MAX_DEPENDENCIES)
                .filter_map(|dependency| {
                    let kind = match dependency.dependency_type.as_str() {
                        "required" => DependencyKind::Required,
                        "optional" => DependencyKind::Optional,
                        "incompatible" => DependencyKind::Incompatible,
                        "embedded" => DependencyKind::Embedded,
                        _ => return None,
                    };
                    Some(Dependency {
                        project_id: dependency.project_id.filter(|id| is_project_id(id)),
                        kind,
                    })
                })
                .collect();
            Some(ProjectVersion {
                id: version.id,
                number: clean_text(&number, 64),
                channel,
                game_versions: short_list(version.game_versions),
                loaders: version
                    .loaders
                    .into_iter()
                    .take(MAX_LIST)
                    .filter(|loader| is_slug(loader))
                    .collect(),
                published: clean_text(version.date_published.as_deref().unwrap_or(""), 40),
                files,
                dependencies,
            })
        })
        .collect())
}

/// The file to download for a version: the one Modrinth marks primary, else the first.
#[must_use]
pub fn primary_file(version: &ProjectVersion) -> Option<&VersionFile> {
    version
        .files
        .iter()
        .find(|file| file.primary)
        .or_else(|| version.files.first())
}

/// The version to install for a game (and loader): among those that really list the game
/// version (and the loader), a release over a beta over an alpha, and the most recently
/// published among equals. The answer is checked again here, not taken on the platform's word
/// that its filter worked.
#[must_use]
pub fn pick_version<'a>(
    versions: &'a [ProjectVersion],
    game_version: &str,
    loader: Option<&str>,
) -> Option<&'a ProjectVersion> {
    versions
        .iter()
        .filter(|version| version.game_versions.iter().any(|game| game == game_version))
        .filter(|version| loader.is_none_or(|wanted| version.loaders.iter().any(|have| have == wanted)))
        .min_by(|a, b| {
            a.channel
                .cmp(&b.channel)
                .then_with(|| b.published.cmp(&a.published))
        })
}

#[cfg(test)]
#[path = "browse_tests.rs"]
mod tests;
