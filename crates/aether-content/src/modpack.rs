//! Pure validation of Modrinth pack indexes. No filesystem or network access.
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_INDEX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FILES: usize = 10_000;
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MAX_DEPTH: usize = 32;
pub const MAX_PATH_DEPTH: usize = 32;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid Modrinth pack: {0}")]
pub struct PackError(pub &'static str);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    pub summary: Option<String>,
    #[serde(deserialize_with = "unique_dependencies")]
    pub dependencies: BTreeMap<String, String>,
    pub files: Vec<File>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub path: String,
    pub hashes: Hashes,
    pub downloads: Vec<String>,
    pub env: Option<Environment>,
    pub file_size: u64,
}

#[derive(Debug, Deserialize)]
pub struct Hashes {
    pub sha1: String,
    pub sha512: String,
}

#[derive(Debug, Deserialize)]
pub struct Environment {
    pub client: Side,
    pub server: Side,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Required,
    Optional,
    Unsupported,
}

fn unique_dependencies<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct Dependencies;
    impl<'de> serde::de::Visitor<'de> for Dependencies {
        type Value = BTreeMap<String, String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a dependency map without duplicate keys")
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: serde::de::MapAccess<'de>,
        {
            let mut dependencies = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, String>()? {
                if dependencies.insert(key, value).is_some() || dependencies.len() > 2 {
                    return Err(serde::de::Error::custom("duplicate or excess dependencies"));
                }
            }
            Ok(dependencies)
        }
    }
    deserializer.deserialize_map(Dependencies)
}

/// Portable paths deliberately reject Windows aliases even on Unix.
#[must_use]
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && path.split('/').count() <= MAX_PATH_DEPTH
        && path.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or("").trim_end_matches(' ').to_uppercase();
            !part.is_empty()
                && part.len() <= 255
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part
                    .chars()
                    .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
                && !matches!(
                    stem.as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                )
                && !(stem.starts_with("COM") || stem.starts_with("LPT"))
                    .then(|| &stem[3..])
                    .is_some_and(|s| {
                        matches!(
                            s,
                            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                        )
                    })
        })
}

/// Strict RFC 3986 ASCII spelling, exact authorities, and GitHub raw/release routes.
#[must_use]
pub fn allowed_download(url: &str) -> bool {
    if url.len() > 4096
        || !url.is_ascii()
        || url
            .bytes()
            .any(|b| b <= 32 || b >= 127 || b"\\<>\"{}|^`".contains(&b))
    {
        return false;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    if path.is_empty() || path.contains('#') {
        return false;
    }
    let bytes = url.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'%'
            && (i + 2 >= bytes.len()
                || !bytes[i + 1].is_ascii_hexdigit()
                || !bytes[i + 2].is_ascii_hexdigit())
        {
            return false;
        }
    }
    let route = path.split('?').next().unwrap_or("");
    let parts: Vec<_> = route.split('/').collect();
    // HTTP clients and servers can normalize escaped dot segments and separators.
    // Validate the decoded route before applying the GitHub route allowlist.
    if parts.iter().any(|part| !safe_url_segment(part)) {
        return false;
    }
    match host {
        "cdn.modrinth.com" | "raw.githubusercontent.com" | "gitlab.com" => true,
        "github.com" => {
            parts.len() >= 5
                && (parts[2] == "raw"
                    || (parts[2] == "releases" && parts[3] == "download" && parts.len() >= 6))
        }
        _ => false,
    }
}

fn safe_url_segment(segment: &str) -> bool {
    let mut decoded = Vec::with_capacity(segment.len());
    let mut bytes = segment.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let Some(hi) = bytes.next().and_then(|b| char::from(b).to_digit(16)) else {
                return false;
            };
            let Some(lo) = bytes.next().and_then(|b| char::from(b).to_digit(16)) else {
                return false;
            };
            decoded.push((hi * 16 + lo) as u8);
        } else {
            decoded.push(byte);
        }
    }
    !decoded.is_empty()
        && decoded != b"."
        && decoded != b".."
        && !decoded.iter().any(|b| *b < 32 || *b == 127 || b"/\\%".contains(b))
}

fn bounded_depth(bytes: &[u8]) -> bool {
    let (mut depth, mut quoted, mut escape) = (0usize, false, false);
    for &b in bytes {
        if quoted {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                quoted = false;
            }
        } else {
            match b {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return false;
                    }
                }
                b'}' | b']' => {
                    let Some(next) = depth.checked_sub(1) else {
                        return false;
                    };
                    depth = next;
                }
                _ => {}
            }
        }
    }
    depth == 0 && !quoted
}

/// Parse and validate before any downloads or filesystem mutations.
///
/// # Errors
/// Rejects unsupported schema, unsafe paths/URLs, invalid hashes and resource-limit violations.
pub fn parse(bytes: &[u8]) -> Result<Index, PackError> {
    if bytes.len() > MAX_INDEX_BYTES || !bounded_depth(bytes) {
        return Err(PackError("index size or depth limit"));
    }
    let index: Index = serde_json::from_slice(bytes).map_err(|_| PackError("index schema"))?;
    if index.format_version != 1
        || index.game != "minecraft"
        || index.name.trim().is_empty()
        || index.name.len() > 512
        || index.version_id.trim().is_empty()
        || index.version_id.len() > 512
        || index.summary.as_ref().is_some_and(|summary| summary.len() > 4096)
    {
        return Err(PackError("unsupported format or identity"));
    }
    if !index.dependencies.contains_key("minecraft")
        || index.dependencies.len() > 2
        || index.dependencies.iter().any(|(key, value)| {
            !matches!(
                key.as_str(),
                "minecraft" | "fabric-loader" | "quilt-loader" | "forge" | "neoforge"
            ) || !crate::browse::is_game_version(value)
                || !safe_path(value)
                || !value.bytes().any(|b| b.is_ascii_alphanumeric())
        })
    {
        return Err(PackError("unsupported or conflicting dependencies"));
    }
    if index.files.len() > MAX_FILES {
        return Err(PackError("file count limit"));
    }
    let mut names = BTreeSet::new();
    let mut total = 0u64;
    for file in &index.files {
        if !safe_path(&file.path) || !names.insert(file.path.to_lowercase()) {
            return Err(PackError("unsafe or duplicate path"));
        }
        let hex = |s: &str, len| s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit());
        if !hex(&file.hashes.sha1, 40) || !hex(&file.hashes.sha512, 128) {
            return Err(PackError("missing or invalid hashes"));
        }
        if file.downloads.is_empty()
            || file.downloads.len() > 8
            || !file.downloads.iter().all(|url| allowed_download(url))
        {
            return Err(PackError("download URL not allowed"));
        }
        total = total
            .checked_add(file.file_size)
            .ok_or(PackError("total size overflow"))?;
        if file.file_size > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES {
            return Err(PackError("file size limit"));
        }
    }
    for name in &names {
        let mut prefix = name.as_str();
        while let Some((parent, _)) = prefix.rsplit_once('/') {
            if names.contains(parent) {
                return Err(PackError("file/directory collision"));
            }
            prefix = parent;
        }
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> serde_json::Value {
        serde_json::json!({"formatVersion":1,"game":"minecraft","versionId":"v1","name":"测试",
            "dependencies":{"minecraft":"1.21.1","fabric-loader":"0.16.0"},
            "files":[{"path":"mods/test.jar","hashes":{"sha1":"a".repeat(40),"sha512":"b".repeat(128)},
                "downloads":["https://cdn.modrinth.com/data/file.jar"],"fileSize":5}]})
    }
    fn valid(v: &serde_json::Value) -> bool {
        parse(&serde_json::to_vec(v).unwrap()).is_ok()
    }
    #[test]
    fn modpack_schema_and_dependencies() {
        let mut v = sample();
        assert!(valid(&v));
        for loader in ["fabric-loader", "quilt-loader", "forge", "neoforge"] {
            v["dependencies"] = serde_json::json!({"minecraft":"1.21.1",loader:"1.0"});
            assert!(valid(&v));
        }
        v["dependencies"]["unknown-loader"] = "1".into();
        assert!(!valid(&v));
        v = sample();
        v["formatVersion"] = 2.into();
        assert!(!valid(&v));
        v = sample();
        v["files"][0]["hashes"]["sha512"] = "bad".into();
        assert!(!valid(&v));
        v = sample();
        v["files"][0]["env"] = serde_json::json!({"client":"unsupported","server":"required"});
        assert!(valid(&v));
        v["files"][0]["env"]["client"] = "anything".into();
        assert!(!valid(&v));
    }
    #[test]
    fn modpack_paths() {
        for path in [
            "/a",
            "../a",
            "a/../b",
            "C:/a",
            "C:a",
            "a\\b",
            "a//b",
            "a/./b",
            "a/CON.txt",
            "NUL",
            "LPT1.jar",
            "COM¹.txt",
            "a:stream",
            "a.",
            "a ",
            "a/",
            "",
        ] {
            assert!(!safe_path(path), "{path}");
        }
        assert!(safe_path("config/测试.json"));
        let mut v = sample();
        let f = v["files"][0].clone();
        v["files"].as_array_mut().unwrap().push(f);
        assert!(!valid(&v));
        v["files"][1]["path"] = "MODS".into();
        assert!(!valid(&v));
    }
    #[test]
    fn modpack_urls_and_limits() {
        for url in [
            "http://cdn.modrinth.com/a",
            "https://cdn.modrinth.com.evil/a",
            "https://cdn.modrinth.com@evil/a",
            "https://cdn.modrinth.com:443/a",
            "https://github.com/u/r/blob/main/a",
            "https://gitlab.com/a%zz",
            "https://gitlab.com/a b",
        ] {
            assert!(!allowed_download(url), "{url}");
        }
        for url in [
            "https://github.com/u/r/releases/download/v/a",
            "https://github.com/u/r/raw/main/a",
            "https://raw.githubusercontent.com/u/r/main/a",
            "https://gitlab.com/u/r/-/raw/main/a",
        ] {
            assert!(allowed_download(url));
        }
        assert!(parse(&vec![b' '; MAX_INDEX_BYTES + 1]).is_err());
        assert!(parse(format!("{}0{}", "[".repeat(33), "]".repeat(33)).as_bytes()).is_err());
        let mut v = sample();
        v["files"][0]["fileSize"] = (MAX_FILE_BYTES + 1).into();
        assert!(!valid(&v));
    }
}
