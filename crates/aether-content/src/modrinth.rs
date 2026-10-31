//! Modrinth: look files up by SHA-1, then read the projects they belong to.
//!
//! Two requests, both documented at <https://docs.modrinth.com/api/> and needing no token:
//!
//! 1. `POST /v2/version_files` with the hashes answers, for every hash Modrinth knows, the
//!    version that file belongs to (and so its project id);
//! 2. `GET /v2/projects?ids=[…]` answers the title, summary and icon of those projects.
//!
//! Modrinth asks every application to send a `User-Agent` that identifies it, and limits
//! requests per address; the caller does both.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ContentError, clean_text};

pub const API_BASE: &str = "https://api.modrinth.com/v2";

/// The only host an icon may come from.
pub const ICON_HOST: &str = "cdn.modrinth.com";

/// Hashes (or project ids) per request. Keeps the request line and the answer small.
pub const MAX_PER_REQUEST: usize = 100;

/// The longest answer read, in bytes.
pub const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// The biggest icon taken, in bytes.
pub const MAX_ICON_BYTES: usize = 256 * 1024;

/// Entries read from one `version_files` answer.
const MAX_VERSION_ENTRIES: usize = 1000;

/// What a hash turned out to be: the project it belongs to and which version of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionRef {
    pub project_id: String,
    /// The version number the author gave (`0.6.3`), or the version's name.
    pub version: String,
}

/// A project, as far as the launcher shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    /// Used for the project's page; the id when Modrinth has no slug.
    pub slug: String,
    pub title: String,
    pub summary: String,
    /// `mod`, `modpack`, `resourcepack` or `shader`.
    pub project_type: String,
    /// Always on [`ICON_HOST`], over HTTPS.
    pub icon_url: Option<String>,
}

/// `true` for a lower-case, 40-digit hexadecimal SHA-1.
#[must_use]
pub fn is_sha1(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `true` for a Modrinth id: a short base-62 string.
#[must_use]
pub fn is_project_id(text: &str) -> bool {
    (1..=32).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// `true` for a slug: letters, digits, `-`, `_` and `.` (some older slugs have one), never
/// starting with a dot.
#[must_use]
pub fn is_slug(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && !text.starts_with('.')
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The JSON body of `POST /version_files` for these hashes.
///
/// # Errors
///
/// [`ContentError::TooMany`] past [`MAX_PER_REQUEST`], [`ContentError::InvalidHash`] for
/// anything that is not a lower-case SHA-1.
pub fn version_files_body(hashes: &[String]) -> Result<String, ContentError> {
    if hashes.len() > MAX_PER_REQUEST {
        return Err(ContentError::TooMany(MAX_PER_REQUEST));
    }
    if !hashes.iter().all(|hash| is_sha1(hash)) {
        return Err(ContentError::InvalidHash);
    }
    serde_json::to_string(&serde_json::json!({ "hashes": hashes, "algorithm": "sha1" }))
        .map_err(|_| ContentError::Malformed)
}

#[derive(Deserialize)]
struct RawVersion {
    project_id: String,
    version_number: Option<String>,
    name: Option<String>,
}

/// Reads the answer to `POST /version_files`: for each known hash, its version.
///
/// Entries whose key is not a SHA-1 or whose project id is not an id are left out; an unknown
/// hash is simply absent from the answer.
///
/// # Errors
///
/// [`ContentError::TooLarge`] and [`ContentError::Malformed`].
pub fn parse_version_files(body: &str) -> Result<BTreeMap<String, VersionRef>, ContentError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ContentError::TooLarge(MAX_BODY_BYTES));
    }
    let raw: BTreeMap<String, RawVersion> =
        serde_json::from_str(body).map_err(|_| ContentError::Malformed)?;
    Ok(raw
        .into_iter()
        .take(MAX_VERSION_ENTRIES)
        .filter(|(hash, version)| is_sha1(hash) && is_project_id(&version.project_id))
        .map(|(hash, version)| {
            let label = version
                .version_number
                .filter(|text| !text.trim().is_empty())
                .or(version.name)
                .unwrap_or_default();
            (
                hash,
                VersionRef {
                    project_id: version.project_id,
                    version: clean_text(&label, 64),
                },
            )
        })
        .collect())
}

/// The URL of `GET /projects` for these project ids.
///
/// # Errors
///
/// [`ContentError::TooMany`] past [`MAX_PER_REQUEST`], [`ContentError::InvalidId`] for an id
/// that is not base-62.
pub fn projects_url(ids: &[String]) -> Result<String, ContentError> {
    if ids.len() > MAX_PER_REQUEST {
        return Err(ContentError::TooMany(MAX_PER_REQUEST));
    }
    if !ids.iter().all(|id| is_project_id(id)) {
        return Err(ContentError::InvalidId);
    }
    // `["a","b"]`, percent-encoded. The ids are alphanumeric, so only the punctuation needs it.
    let list = ids
        .iter()
        .map(|id| format!("%22{id}%22"))
        .collect::<Vec<_>>()
        .join("%2C");
    Ok(format!("{API_BASE}/projects?ids=%5B{list}%5D"))
}

#[derive(Deserialize)]
struct RawProject {
    id: String,
    slug: Option<String>,
    title: String,
    description: Option<String>,
    project_type: String,
    icon_url: Option<String>,
}

/// `url` when it is an HTTPS address on [`ICON_HOST`] with no user info, port or odd
/// characters; `None` otherwise.
#[must_use]
pub fn trusted_icon_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let clean = url.len() <= 512
        && !url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\\' | '@'));
    (host == ICON_HOST && !path.is_empty() && clean).then(|| url.to_owned())
}

/// Reads the answer to `GET /projects`. A project of a type the launcher does not show, or
/// without a usable id or title, is left out.
///
/// # Errors
///
/// [`ContentError::TooLarge`] and [`ContentError::Malformed`].
pub fn parse_projects(body: &str) -> Result<Vec<Project>, ContentError> {
    if body.len() > MAX_BODY_BYTES {
        return Err(ContentError::TooLarge(MAX_BODY_BYTES));
    }
    let raw: Vec<RawProject> = serde_json::from_str(body).map_err(|_| ContentError::Malformed)?;
    Ok(raw
        .into_iter()
        .take(MAX_PER_REQUEST)
        .filter(|project| {
            is_project_id(&project.id)
                && matches!(
                    project.project_type.as_str(),
                    "mod" | "modpack" | "resourcepack" | "shader"
                )
        })
        .filter_map(|project| {
            let title = clean_text(&project.title, 100);
            if title.is_empty() {
                return None;
            }
            let slug = project
                .slug
                .filter(|slug| is_slug(slug))
                .unwrap_or_else(|| project.id.clone());
            Some(Project {
                id: project.id,
                slug,
                title,
                summary: clean_text(project.description.as_deref().unwrap_or(""), 300),
                project_type: project.project_type,
                icon_url: project.icon_url.as_deref().and_then(trusted_icon_url),
            })
        })
        .collect())
}

/// The project's page on modrinth.com. `slug_or_id` must be a slug or an id.
#[must_use]
pub fn project_page_url(slug_or_id: &str) -> Option<String> {
    is_slug(slug_or_id).then(|| format!("https://modrinth.com/project/{slug_or_id}"))
}

#[cfg(test)]
#[path = "modrinth_tests.rs"]
mod tests;
