//! Prism Launcher / MultiMC instance detection.
//!
//! Both launchers share one on-disk layout, so a single parser covers them:
//!
//! ```text
//! <launcher>/prismlauncher.cfg      (or multimc.cfg)   InstanceDir=instances
//! <launcher>/instances/<id>/instance.cfg               name=<display name>
//! <launcher>/instances/<id>/mmc-pack.json              components[].uid/version
//! <launcher>/instances/<id>/.minecraft/                mods, config, saves
//! ```
//!
//! The important difference from PCL2 and HMCL is what is **not** there: an
//! instance has no `versions/<id>/<id>.json`. The version is described by
//! `mmc-pack.json` — a base `net.minecraft` component plus optional loader
//! components — and the launcher assembles the real version metadata at launch
//! time. Importing therefore has to recreate the version from its identifier
//! instead of copying a file, which is why [`super::execute`] has a separate
//! entry point for these instances.
//!
//! Everything here is a pure function over text or a bounded directory read, so
//! the parsers are unit tested without touching a real launcher.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::ExistingFiles;

/// Config files a Prism/MultiMC installation keeps at its root.
const CONFIG_NAMES: [&str; 2] = ["prismlauncher.cfg", "multimc.cfg"];

/// Instance directory used when the config does not override it.
const DEFAULT_INSTANCE_DIR: &str = "instances";

/// Upper bound for one `mmc-pack.json`.
const MAX_PACK_BYTES: u64 = 1024 * 1024;

/// Upper bound for one `instance.cfg` or launcher config.
const MAX_CONFIG_BYTES: u64 = 256 * 1024;

/// Loader components, mapped to the label shown in the UI.
const LOADERS: [(&str, &str); 5] = [
    ("net.fabricmc.fabric-loader", "Fabric"),
    ("org.quiltmc.quilt-loader", "Quilt"),
    ("net.minecraftforge", "Forge"),
    ("net.neoforged", "NeoForge"),
    ("net.liteloader", "LiteLoader"),
];

/// One instance folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrismInstance {
    /// Folder name under the instance directory; the import identifier.
    pub id: String,
    /// `name=` from `instance.cfg`, falling back to the folder name.
    pub name: String,
    /// Base Minecraft version from the `net.minecraft` component.
    pub minecraft_version: String,
    /// Loader summary such as `Fabric 0.18.4`; `None` for a vanilla instance.
    pub loader: Option<String>,
}

impl PrismInstance {
    /// Label shown next to the instance in the import dialog.
    #[must_use]
    pub fn display_name(&self) -> String {
        match &self.loader {
            Some(loader) => format!("{}（{} + {}）", self.name, self.minecraft_version, loader),
            None => format!("{}（{}）", self.name, self.minecraft_version),
        }
    }
}

/// The parts of `mmc-pack.json` this module reads.
#[derive(Debug, Deserialize)]
struct PackFile {
    #[serde(default)]
    components: Vec<PackComponent>,
}

#[derive(Debug, Deserialize)]
struct PackComponent {
    uid: String,
    #[serde(default)]
    version: String,
}

/// Base version and loader resolved from one `mmc-pack.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrismPack {
    /// `version` of the `net.minecraft` component.
    pub minecraft_version: String,
    /// Loader summary, when the pack names one.
    pub loader: Option<String>,
}

/// Whether a listing looks like a Prism/MultiMC installation.
///
/// Either the launcher config sits at the root, or an instance folder with an
/// `mmc-pack.json` is present.
#[must_use]
pub fn matches(files: &ExistingFiles) -> bool {
    if CONFIG_NAMES.iter().any(|name| files.contains(name)) {
        return true;
    }
    files.entries.iter().any(|entry| {
        let segments: Vec<&str> = entry.split('/').collect();
        segments.len() == 3 && segments[0] == DEFAULT_INSTANCE_DIR && segments[2] == "mmc-pack.json"
    })
}

/// Whether `root` holds a Prism/MultiMC launcher configuration.
///
/// The command layer uses this to tell a launcher folder apart from a game
/// folder that merely happens to contain an `instances/` directory: only a
/// configuration file makes a folder a launcher.
#[must_use]
pub fn has_config(root: &Path) -> bool {
    CONFIG_NAMES.iter().any(|name| root.join(name).is_file())
}

/// Reads the `InstanceDir=` value out of a launcher config.
///
/// The value is a path relative to the launcher folder in every real
/// installation, but an absolute one is accepted too: it is only ever used to
/// locate instances, never as a write destination.
#[must_use]
pub fn instance_directory(config: &str) -> Option<String> {
    for line in config.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';', '[']) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "InstanceDir" {
            continue;
        }
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    None
}

/// Parses `mmc-pack.json` into the base version and a loader summary.
///
/// Returns `None` when the file names no `net.minecraft` component, which is
/// the only component an import cannot do without.
#[must_use]
pub fn parse_pack(json: &str) -> Option<PrismPack> {
    let body = json.strip_prefix('\u{feff}').unwrap_or(json);
    let pack: PackFile = serde_json::from_str(body).ok()?;

    let minecraft_version = pack
        .components
        .iter()
        .find(|component| component.uid == "net.minecraft")
        .map(|component| component.version.trim().to_owned())
        .filter(|version| !version.is_empty())?;

    let loader = LOADERS.iter().find_map(|(uid, label)| {
        pack.components
            .iter()
            .find(|component| component.uid == *uid)
            .map(|component| {
                let version = component.version.trim();
                if version.is_empty() {
                    (*label).to_owned()
                } else {
                    format!("{label} {version}")
                }
            })
    });

    Some(PrismPack {
        minecraft_version,
        loader,
    })
}

/// Reads `name=` out of an `instance.cfg`.
#[must_use]
pub fn instance_name(config: &str) -> Option<String> {
    for line in config.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "name" {
            continue;
        }
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    None
}

/// Reads one instance folder. Returns `None` when it is not a usable instance.
#[must_use]
pub fn read_instance(directory: &Path) -> Option<PrismInstance> {
    let id = directory.file_name()?.to_str()?.to_owned();
    let pack = parse_pack(&read_capped(
        &directory.join("mmc-pack.json"),
        MAX_PACK_BYTES,
    )?)?;
    let name = read_capped(&directory.join("instance.cfg"), MAX_CONFIG_BYTES)
        .and_then(|text| instance_name(&text))
        .unwrap_or_else(|| id.clone());

    Some(PrismInstance {
        id,
        name,
        minecraft_version: pack.minecraft_version,
        loader: pack.loader,
    })
}

/// Resolves the instance directory of a Prism/MultiMC installation.
///
/// Returns `None` when `root` holds no launcher config and no `instances/`
/// directory, so a plain game folder is never mistaken for a launcher.
#[must_use]
pub fn locate_instances_root(root: &Path) -> Option<PathBuf> {
    for name in CONFIG_NAMES {
        let Some(config) = read_capped(&root.join(name), MAX_CONFIG_BYTES) else {
            continue;
        };
        let directory = instance_directory(&config)
            .map(|value| root.join(value))
            .unwrap_or_else(|| root.join(DEFAULT_INSTANCE_DIR));
        if directory.is_dir() {
            return Some(directory);
        }
    }

    let fallback = root.join(DEFAULT_INSTANCE_DIR);
    fallback.is_dir().then_some(fallback)
}

/// Lists every readable instance under a launcher folder, sorted by name.
///
/// Symbolic links are skipped and the listing is bounded, so a hostile tree
/// cannot stall the caller.
#[must_use]
pub fn list_instances(root: &Path) -> Vec<PrismInstance> {
    let Some(directory) = locate_instances_root(root) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };

    let mut instances = Vec::new();
    for entry in entries.flatten().take(super::MAX_SCAN_ENTRIES) {
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        if let Some(instance) = read_instance(&entry.path()) {
            instances.push(instance);
        }
    }

    instances.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
    instances
}

/// Reads a file only when it is a regular file within `limit` bytes.
fn read_capped(path: &Path, limit: u64) -> Option<String> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    fs::read_to_string(path).ok()
}

#[cfg(test)]
#[path = "prism_tests.rs"]
mod tests;
