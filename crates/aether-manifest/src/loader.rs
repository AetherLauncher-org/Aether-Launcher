//! Resolve local Minecraft version manifests, including multi-level inheritsFrom.
//! File access stays under the caller-provided game's versions directory.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::{ManifestError, VersionManifest, parse_version_manifest};

const MAX_INHERITANCE_DEPTH: usize = 32;
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum VersionLoadError {
    #[error("unsafe Minecraft version identifier: {0:?}")]
    UnsafeVersionId(String),
    #[error("cannot access version file {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("version file escapes the versions directory: {0:?}")]
    PathEscape(PathBuf),
    #[error("version manifest is not a regular file: {0:?}")]
    NotAFile(PathBuf),
    #[error("version file exceeds the 8 MiB limit: {0:?}")]
    TooLarge(PathBuf),
    #[error("invalid version manifest at {path:?}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: ManifestError,
    },
    #[error("version file {path:?} declares id {actual:?}, expected {expected:?}")]
    IdMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("cyclic inheritsFrom chain: {0:?}")]
    InheritanceCycle(Vec<String>),
    #[error("inheritsFrom exceeds the {MAX_INHERITANCE_DEPTH}-version limit")]
    InheritanceTooDeep,
}

/// A fully resolved version and the source version of its client download.
#[derive(Debug, Clone)]
pub struct ResolvedVersionManifest {
    pub manifest: VersionManifest,
    pub client_jar_version: Option<String>,
}

/// Read `<game_directory>/versions/<id>/<id>.json` and resolve all local parents.
///
/// Parent manifests are resolved before `merge_with_parent`. The resulting
/// manifest retains the requested child's ID and contains no `inheritsFrom`.
/// Missing parents and malformed files are errors, not silent fallbacks.
///
/// Only the game's own `versions` tree can be read: identifiers are validated
/// and canonical paths are checked before opening each file.
pub fn load_version_manifest(
    game_directory: &Path,
    version_id: &str,
) -> Result<VersionManifest, VersionLoadError> {
    Ok(load_resolved_version_manifest(game_directory, version_id)?.manifest)
}

/// Also preserve the origin of downloads.client, since Fabric/Forge may
/// inherit the actual client JAR from a parent version.
pub fn load_resolved_version_manifest(
    game_directory: &Path,
    version_id: &str,
) -> Result<ResolvedVersionManifest, VersionLoadError> {
    validate_version_id(version_id)?;
    let versions = game_directory.join("versions");
    let root = fs::canonicalize(&versions).map_err(|source| VersionLoadError::Io {
        path: versions,
        source,
    })?;
    resolve_version(&root, version_id, &mut Vec::new())
}

fn resolve_version(
    root: &Path,
    id: &str,
    chain: &mut Vec<String>,
) -> Result<ResolvedVersionManifest, VersionLoadError> {
    validate_version_id(id)?;
    if chain.iter().any(|seen| seen == id) {
        let mut cycle = chain.clone();
        cycle.push(id.to_owned());
        return Err(VersionLoadError::InheritanceCycle(cycle));
    }
    if chain.len() >= MAX_INHERITANCE_DEPTH {
        return Err(VersionLoadError::InheritanceTooDeep);
    }

    chain.push(id.to_owned());
    let result = resolve_entry(root, id, chain);
    chain.pop();
    result
}

fn resolve_entry(
    root: &Path,
    id: &str,
    chain: &mut Vec<String>,
) -> Result<ResolvedVersionManifest, VersionLoadError> {
    let path = root.join(id).join(format!("{id}.json"));
    let canonical = fs::canonicalize(&path).map_err(|source| VersionLoadError::Io {
        path: path.clone(),
        source,
    })?;
    if !canonical.starts_with(root) {
        return Err(VersionLoadError::PathEscape(path));
    }

    let metadata = fs::metadata(&canonical).map_err(|source| VersionLoadError::Io {
        path: path.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(VersionLoadError::NotAFile(path));
    }
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(VersionLoadError::TooLarge(path));
    }

    let json = fs::read_to_string(&canonical).map_err(|source| VersionLoadError::Io {
        path: path.clone(),
        source,
    })?;
    if json.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(VersionLoadError::TooLarge(path));
    }
    let manifest = parse_version_manifest(&json).map_err(|source| VersionLoadError::Parse {
        path: path.clone(),
        source,
    })?;
    if manifest.id != id {
        return Err(VersionLoadError::IdMismatch {
            path,
            expected: id.to_owned(),
            actual: manifest.id,
        });
    }

    let local_has_client = manifest
        .downloads
        .as_ref()
        .and_then(|downloads| downloads.client.as_ref())
        .is_some();
    match manifest.inherits_from.as_deref() {
        Some(parent_id) => {
            let parent = resolve_version(root, parent_id, chain)?;
            let merged = manifest.merge_with_parent(&parent.manifest);
            let client_jar_version = if local_has_client {
                Some(id.to_owned())
            } else if manifest.downloads.is_some() {
                // A child downloads object replaces the parent's object.
                None
            } else {
                parent.client_jar_version
            };
            Ok(ResolvedVersionManifest {
                manifest: merged,
                client_jar_version,
            })
        }
        None => Ok(ResolvedVersionManifest {
            manifest,
            client_jar_version: local_has_client.then(|| id.to_owned()),
        }),
    }
}

pub(crate) fn validate_version_id(id: &str) -> Result<(), VersionLoadError> {
    let unsafe_name = id.trim().is_empty()
        || matches!(id, "." | "..")
        || id.ends_with([' ', '.'])
        || id.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        });
    // Windows device names are reserved even when followed by an extension.
    let stem = id.split('.').next().unwrap_or(id).to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');

    if unsafe_name || reserved {
        Err(VersionLoadError::UnsafeVersionId(id.to_owned()))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{VersionLoadError, load_version_manifest};
    use crate::arguments::expand_arguments;
    use crate::rules::Environment;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let suffix = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir()
                .join(format!("aether-manifest-{}-{suffix}", std::process::id()));
            std::fs::create_dir_all(root.join("versions")).unwrap();
            Self(root)
        }

        fn root(&self) -> &Path {
            &self.0
        }

        fn write(&self, id: &str, json: &str) {
            let dir = self.0.join("versions").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{id}.json")), json).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolves_three_levels_with_existing_merge_semantics() {
        let f = Fixture::new();
        f.write(
            "1.21.1",
            r#"{
            "id":"1.21.1",
            "mainClass":"net.minecraft.client.main.Main",
            "javaVersion":{"component":"java-runtime-delta","majorVersion":21},
            "libraries":[{"name":"org.ow2.asm:asm:9.6"}],
            "arguments":{"game":["--parent"],"jvm":["-cp","parent"]}
        }"#,
        );
        f.write(
            "middle",
            r#"{
            "id":"middle","inheritsFrom":"1.21.1",
            "libraries":[{"name":"org.ow2.asm:asm:9.7"}],
            "arguments":{"game":["--middle"]}
        }"#,
        );
        f.write(
            "fabric",
            r#"{
            "id":"fabric","inheritsFrom":"middle",
            "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
            "libraries":[{"name":"net.fabricmc:fabric-loader:0.16.0"}],
            "arguments":{"game":["--child"],"jvm":["-DFabric=true"]}
        }"#,
        );

        let merged = load_version_manifest(f.root(), "fabric").unwrap();
        assert_eq!(merged.id, "fabric");
        assert_eq!(merged.inherits_from, None);
        assert_eq!(
            merged.main_class.as_deref(),
            Some("net.fabricmc.loader.impl.launch.knot.KnotClient")
        );
        assert_eq!(merged.java_version.unwrap().major_version, 21);
        let libraries: Vec<&str> = merged
            .libraries
            .iter()
            .map(|lib| lib.name.as_str())
            .collect();
        assert_eq!(
            libraries,
            ["net.fabricmc:fabric-loader:0.16.0", "org.ow2.asm:asm:9.7"]
        );
        let arguments = merged.arguments.unwrap();
        let environment = Environment::default();
        assert_eq!(
            expand_arguments(&arguments.game, &environment),
            ["--parent", "--middle", "--child"]
        );
        assert_eq!(
            expand_arguments(&arguments.jvm, &environment),
            ["-cp", "parent", "-DFabric=true"]
        );
    }

    #[test]
    fn remembers_exact_ancestor_that_supplied_client_jar() {
        let fixture = Fixture::new();
        fixture.write(
            "base",
            r#"{
            "id":"base",
            "downloads":{"client":{"sha1":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "size":3,"url":"https://example.invalid/base.jar"}}
        }"#,
        );
        fixture.write("middle", r#"{"id":"middle","inheritsFrom":"base"}"#);
        fixture.write("child", r#"{"id":"child","inheritsFrom":"middle"}"#);
        let resolved = super::load_resolved_version_manifest(fixture.root(), "child").unwrap();
        assert_eq!(resolved.manifest.id, "child");
        assert_eq!(resolved.client_jar_version.as_deref(), Some("base"));

        fixture.write(
            "child",
            r#"{
            "id":"child","inheritsFrom":"middle",
            "downloads":{"client":{"sha1":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "size":4,"url":"https://example.invalid/child.jar"}}
        }"#,
        );
        let resolved = super::load_resolved_version_manifest(fixture.root(), "child").unwrap();
        assert_eq!(resolved.client_jar_version.as_deref(), Some("child"));
    }

    #[test]
    fn loads_a_standalone_manifest() {
        let f = Fixture::new();
        f.write("1.20.1", r#"{"id":"1.20.1"}"#);
        assert_eq!(
            load_version_manifest(f.root(), "1.20.1").unwrap().id,
            "1.20.1"
        );
    }

    #[test]
    fn missing_parent_is_an_error() {
        let f = Fixture::new();
        f.write("fabric", r#"{"id":"fabric","inheritsFrom":"missing"}"#);
        assert!(matches!(
            load_version_manifest(f.root(), "fabric"),
            Err(VersionLoadError::Io { .. })
        ));
    }

    #[test]
    fn bad_json_and_mismatched_id_are_rejected() {
        let f = Fixture::new();
        f.write("bad", "{");
        assert!(matches!(
            load_version_manifest(f.root(), "bad"),
            Err(VersionLoadError::Parse { .. })
        ));
        f.write("alias", r#"{"id":"different"}"#);
        assert!(matches!(
            load_version_manifest(f.root(), "alias"),
            Err(VersionLoadError::IdMismatch { .. })
        ));
    }

    #[test]
    fn rejects_unsafe_child_and_parent_identifiers() {
        let f = Fixture::new();
        for id in [
            "../other",
            "..",
            "C:escape",
            "folder\\other",
            "CON.json",
            "bad.",
        ] {
            assert!(matches!(
                load_version_manifest(f.root(), id),
                Err(VersionLoadError::UnsafeVersionId(_))
            ));
        }
        f.write("fabric", r#"{"id":"fabric","inheritsFrom":"../escape"}"#);
        assert!(matches!(
            load_version_manifest(f.root(), "fabric"),
            Err(VersionLoadError::UnsafeVersionId(_))
        ));
    }

    #[test]
    fn detects_self_and_multi_version_cycles() {
        let f = Fixture::new();
        f.write("self", r#"{"id":"self","inheritsFrom":"self"}"#);
        assert!(matches!(
            load_version_manifest(f.root(), "self"),
            Err(VersionLoadError::InheritanceCycle(_))
        ));
        f.write("one", r#"{"id":"one","inheritsFrom":"two"}"#);
        f.write("two", r#"{"id":"two","inheritsFrom":"one"}"#);
        assert!(matches!(
            load_version_manifest(f.root(), "one"),
            Err(VersionLoadError::InheritanceCycle(_))
        ));
    }

    #[test]
    fn bounds_deep_inheritance_chains() {
        let f = Fixture::new();
        for n in 0..34 {
            f.write(
                &format!("v{n}"),
                &format!(r#"{{"id":"v{n}","inheritsFrom":"v{}"}}"#, n + 1),
            );
        }
        assert!(matches!(
            load_version_manifest(f.root(), "v0"),
            Err(VersionLoadError::InheritanceTooDeep)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks_pointing_outside_versions() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let outside = f.root().join("outside.json");
        std::fs::write(&outside, r#"{"id":"escape"}"#).unwrap();
        let dir = f.root().join("versions").join("escape");
        std::fs::create_dir_all(&dir).unwrap();
        symlink(&outside, dir.join("escape.json")).unwrap();

        assert!(matches!(
            load_version_manifest(f.root(), "escape"),
            Err(VersionLoadError::PathEscape(_))
        ));
    }
}
