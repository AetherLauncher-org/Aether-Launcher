//! PCL2 (Plain Craft Launcher 2) layout recognition.
//!
//! PCL2 keeps its own configuration under `PCL/` next to the game directories,
//! and version metadata in `<game>/versions/<id>/<id>.json`. The two facts this
//! module relies on:
//!
//! * a strong marker file exists (`PCL/Setup.ini`, or the older flat `PCL.ini`);
//! * for every version folder, `<id>.json` sits beside the folder's `jar/`
//!   and `natives/` subdirectories.
//!
//! `Setup.ini` parsing is deliberately shallow: only `key=value` lines are
//! understood, the first `=` splits, surrounding quotes are stripped, and
//! unknown keys are ignored. Real PCL2 configuration has nested structures and
//! comment conventions this module does not attempt to model; anything richer
//! than a flat key/value map would not change the import plan.
//!
//! # The one configuration value that is used
//!
//! [`LAUNCH_FOLDER_SELECT`] is the only key whose value reaches the import plan.
//! PCL2 rewrites its own base directory to `$` before saving it, so a stored
//! `$\.minecraft` means `<launcher folder>\.minecraft` and an absolute value is
//! used verbatim. [`resolve_configured_path`] performs that substitution and
//! refuses anything that walks upwards, because the value is user-editable text
//! that must not be able to point the scan outside the folder it names.
//!
//! Everything else in the file stays advisory: the version plan is still built
//! from the directory layout, never from a configuration value.

use std::fs;
use std::path::{Component, Path, PathBuf};

use super::{ExistingFiles, ImportCandidate, ImportSourceKind};

/// Marker files that identify a PCL2 installation.
pub const MARKERS: [&str; 2] = ["PCL/Setup.ini", "PCL.ini"];

/// Keys of `Setup.ini` this module understands.
///
/// Every recognised key is advisory only; the plan is built from the directory
/// layout, never from a configuration value, because configuration values are
/// Keys of `Setup.ini` this module understands.
///
/// Every recognised key except [`LAUNCH_FOLDER_SELECT`] is advisory only; the
/// plan is built from the directory layout, never from a configuration value,
/// because configuration values are user-editable and may point outside the
/// tree.
pub const KNOWN_KEYS: [&str; 7] = [
    "setup_version",
    "launch_folder_index",
    LAUNCH_FOLDER_SELECT,
    LAUNCH_FOLDERS,
    "launcher_visibility",
    "update_channel",
    "language",
];

/// `Setup.ini` key holding the currently selected game folder.
///
/// PCL2 stores the path with its own base directory replaced by `$`; see the
/// module documentation.
pub const LAUNCH_FOLDER_SELECT: &str = "LaunchFolderSelect";

/// `Setup.ini` key holding the custom folder list, `Name>Path|Name>Path`.
///
/// PCL2 normally keeps this list in the registry, so the key is often absent
/// from `Setup.ini`. When it is present it is read as a best-effort addition to
/// [`LAUNCH_FOLDER_SELECT`], never as a replacement.
pub const LAUNCH_FOLDERS: &str = "LaunchFolders";

/// Upper bound on the game directories honoured from one configuration file.
pub const MAX_CONFIGURED_FOLDERS: usize = 16;

/// Whether the listing carries a PCL2 marker.
#[must_use]
pub fn matches(files: &ExistingFiles) -> bool {
    MARKERS.iter().any(|marker| files.contains(marker))
}

/// Parsed, flat view of a `Setup.ini` file.
#[must_use]
pub fn parse_setup_ini(content: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with(';')
            || line.starts_with('#')
            || line.starts_with('[')
        {
            continue;
        }

        let Some((key, value)) = split_entry(line) else {
            continue;
        };

        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        let value = value.trim().trim_matches('"').trim().to_owned();
        pairs.push((key.to_owned(), value));
    }

    pairs
}

/// Splits one `key<sep>value` line on whichever separator comes first.
///
/// PCL2 writes `key:value` — its own writer rejects a key containing `:` — while
/// hand-edited files and older builds use `key=value`. A Windows path value
/// contains `:` as well, so only the *first* separator may split, and the earlier
/// of the two wins: `LaunchFolderSelect:E:\mc` keeps the whole path, and
/// `LaunchArgumentTitle:foo=bar` keeps `foo=bar`.
fn split_entry(line: &str) -> Option<(&str, &str)> {
    match (line.find('='), line.find(':')) {
        (Some(equals), Some(colon)) if colon < equals => Some((&line[..colon], &line[colon + 1..])),
        (Some(equals), _) => Some((&line[..equals], &line[equals + 1..])),
        (None, Some(colon)) => Some((&line[..colon], &line[colon + 1..])),
        (None, None) => None,
    }
}

/// Looks up one key of an already parsed `Setup.ini`.
#[must_use]
pub fn setup_value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

/// Builds the PCL2 candidates for a listing.
#[must_use]
pub fn plan(files: &ExistingFiles) -> Vec<ImportCandidate> {
    super::plan_candidates(ImportSourceKind::Pcl2, files)
}

/// Resolves one configured folder value against the launcher folder.
///
/// Returns `None` for an empty value, for a value that would walk upwards with
/// `..`, and for a value that cannot be represented as a path. A leading `$` is
/// replaced by `launcher_root`; a relative value is resolved against it as well,
/// which is what PCL2 itself does.
#[must_use]
pub fn resolve_configured_path(value: &str, launcher_root: &Path) -> Option<PathBuf> {
    let trimmed = value.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return None;
    }

    let resolved = match trimmed.strip_prefix('$') {
        Some(rest) => launcher_root.join(rest.trim_start_matches(['\\', '/'])),
        None => {
            let path = Path::new(trimmed);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                launcher_root.join(trimmed)
            }
        }
    };

    if resolved
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return None;
    }

    Some(resolved)
}

/// Game directories a PCL2 configuration points at, in priority order.
///
/// The selected folder comes first, then every entry of [`LAUNCH_FOLDERS`].
/// Duplicates are dropped case-insensitively because PCL2 runs mostly on
/// Windows, and the list is capped at [`MAX_CONFIGURED_FOLDERS`] so a hostile or
/// hand-edited file cannot make the caller walk an unbounded number of paths.
#[must_use]
pub fn configured_game_directories(
    pairs: &[(String, String)],
    launcher_root: &Path,
) -> Vec<PathBuf> {
    let mut directories = Vec::new();

    if let Some(value) = setup_value(pairs, LAUNCH_FOLDER_SELECT)
        && let Some(path) = resolve_configured_path(value, launcher_root)
    {
        push_unique(&mut directories, path);
    }

    if let Some(value) = setup_value(pairs, LAUNCH_FOLDERS) {
        for entry in value.split('|') {
            let Some((_, path)) = entry.split_once('>') else {
                continue;
            };
            if let Some(path) = resolve_configured_path(path, launcher_root) {
                push_unique(&mut directories, path);
            }
        }
    }

    directories
}

/// Appends `path` unless it is already present or the cap is reached.
fn push_unique(directories: &mut Vec<PathBuf>, path: PathBuf) {
    if directories.len() >= MAX_CONFIGURED_FOLDERS {
        return;
    }
    let key = path.to_string_lossy().to_ascii_lowercase();
    if directories
        .iter()
        .any(|existing| existing.to_string_lossy().to_ascii_lowercase() == key)
    {
        return;
    }
    directories.push(path);
}

/// 从一个 PCL2 启动器目录里读出的游戏目录。
///
/// 找不到配置、或配置里没记游戏目录时返回空列表。这只是个提示：调用方仍要
/// 用 [`crate::import::fallback::has_version`] 确认该目录里真有想找的版本。
#[must_use]
pub fn configured_game_directories_for_launcher(launcher_root: &Path) -> Vec<PathBuf> {
    let Some(config) = locate_config(launcher_root) else {
        return Vec::new();
    };
    configured_game_directories(&config.pairs, &config.launcher_root)
        .into_iter()
        .filter(|directory| directory.is_dir())
        .collect()
}

/// A PCL2 configuration file and the folder `$` in it refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pcl2Config {
    /// Folder PCL2 treats as its own; `$` in a configured path resolves here.
    pub launcher_root: PathBuf,
    /// Parsed `key<sep>value` pairs.
    pub pairs: Vec<(String, String)>,
}

/// Upper bound on a configuration file this module reads.
pub const MAX_CONFIG_BYTES: u64 = 256 * 1024;

/// Locates the PCL2 configuration that belongs to `directory`.
///
/// PCL2 is portable and gets repackaged, so three layouts are accepted:
///
/// * `<dir>/PCL/Setup.ini` — the documented layout; `dir` is the launcher folder;
/// * `<dir>/PCL.ini` — the older flat file; `dir` is the launcher folder;
/// * `<dir>/Setup.ini` — either a build that keeps the file at its root, or the
///   user pointing straight at the `PCL` configuration folder, in which case the
///   launcher folder is the parent.
///
/// A `Setup.ini` in an arbitrary folder is only trusted when it actually looks
/// like PCL2's configuration, so an unrelated installer file cannot turn a
/// random directory into a launcher.
#[must_use]
pub fn locate_config(directory: &Path) -> Option<Pcl2Config> {
    if let Some(pairs) = read_config(&directory.join("PCL").join("Setup.ini")) {
        return Some(Pcl2Config {
            launcher_root: directory.to_path_buf(),
            pairs,
        });
    }
    if let Some(pairs) = read_config(&directory.join("PCL.ini")) {
        return Some(Pcl2Config {
            launcher_root: directory.to_path_buf(),
            pairs,
        });
    }

    let pairs = read_config(&directory.join("Setup.ini"))?;
    if is_config_folder(directory) {
        return Some(Pcl2Config {
            launcher_root: launcher_root(directory, &pairs),
            pairs,
        });
    }
    if pairs.iter().any(|(key, _)| looks_like_key(key)) {
        return Some(Pcl2Config {
            launcher_root: directory.to_path_buf(),
            pairs,
        });
    }
    None
}

/// Whether `directory` is PCL2's own `PCL` configuration folder.
fn is_config_folder(directory: &Path) -> bool {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("pcl"))
}

/// Picks the folder `$` refers to for a `Setup.ini` that sits in a `PCL` folder.
///
/// The folder is either PCL2's configuration directory (launcher folder is the
/// parent) or a launcher that happens to be called `PCL` (launcher folder is the
/// folder itself). Only a `$`-relative value depends on that choice — an absolute
/// one resolves the same way from either candidate — so the parent is used only
/// when a relative value is present and actually resolves there.
fn launcher_root(directory: &Path, pairs: &[(String, String)]) -> PathBuf {
    let Some(parent) = directory.parent() else {
        return directory.to_path_buf();
    };
    if has_relative_value(pairs) && resolves_to_versions(pairs, parent) {
        parent.to_path_buf()
    } else {
        directory.to_path_buf()
    }
}

/// Whether a configured folder is written relative to the launcher (`$`).
fn has_relative_value(pairs: &[(String, String)]) -> bool {
    setup_value(pairs, LAUNCH_FOLDER_SELECT)
        .is_some_and(|value| value.trim_start().starts_with('$'))
        || setup_value(pairs, LAUNCH_FOLDERS).is_some_and(|value| value.contains(">$"))
}

/// Whether the configured game folders of `pairs` exist below `root`.
fn resolves_to_versions(pairs: &[(String, String)], root: &Path) -> bool {
    configured_game_directories(pairs, root)
        .iter()
        .any(|directory| directory.join("versions").is_dir())
}

/// Whether `key` is one PCL2 writes into `Setup.ini`.
fn looks_like_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    KNOWN_KEYS
        .iter()
        .any(|known| known.eq_ignore_ascii_case(key))
        || ["launch", "system", "ui", "setup", "language", "update"]
            .iter()
            .any(|prefix| lowered.starts_with(prefix))
}

/// Reads one bounded configuration file, refusing links and oversized files.
fn read_config(path: &Path) -> Option<Vec<(String, String)>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return None;
    }
    if metadata.len() > MAX_CONFIG_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return None;
    }
    Some(parse_setup_ini(&String::from_utf8_lossy(&bytes)))
}

/// Game directories a PCL2 tree may keep its versions in, in priority order.
///
/// Used only for diagnostics; [`plan`] discovers the actual directories from the
/// listing instead of assuming one of these exists.
#[must_use]
pub fn candidate_game_directories() -> Vec<PathBuf> {
    vec![PathBuf::new(), PathBuf::from(".minecraft")]
}

#[cfg(test)]
mod inline_tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::{
        configured_game_directories, locate_config, parse_setup_ini, resolve_configured_path,
        setup_value,
    };

    #[test]
    fn ignores_comments_and_sections() {
        let pairs = parse_setup_ini("[Setup]\n; comment\n# other\nkey=value\n");
        assert_eq!(pairs, vec![("key".to_owned(), "value".to_owned())]);
    }

    #[test]
    fn parses_the_colon_separator_pcl2_writes() {
        let pairs = parse_setup_ini(
            "UiLauncherSat:100\nLaunchFolderSelect:E:\\a\\.minecraft\\\nLaunchVersionSelect:1.21.9\n",
        );

        assert_eq!(
            setup_value(&pairs, "LaunchFolderSelect"),
            Some("E:\\a\\.minecraft\\")
        );
        assert_eq!(setup_value(&pairs, "UiLauncherSat"), Some("100"));
        assert_eq!(setup_value(&pairs, "LaunchVersionSelect"), Some("1.21.9"));
    }

    #[test]
    fn keeps_equals_signs_inside_a_colon_separated_value() {
        let pairs = parse_setup_ini("LaunchArgumentTitle:foo=bar\n");
        assert_eq!(setup_value(&pairs, "LaunchArgumentTitle"), Some("foo=bar"));
    }

    #[test]
    fn keeps_colons_inside_an_equals_separated_value() {
        let pairs = parse_setup_ini("LaunchFolderSelect=E:\\a\\.minecraft\\\n");
        assert_eq!(
            setup_value(&pairs, "LaunchFolderSelect"),
            Some("E:\\a\\.minecraft\\")
        );
    }

    #[test]
    fn resolves_the_launcher_relative_prefix() {
        let root = Path::new("launcher");
        assert_eq!(
            resolve_configured_path("$/.minecraft", root),
            Some(root.join(".minecraft"))
        );
        assert_eq!(resolve_configured_path("$", root), Some(root.to_path_buf()));
    }

    #[test]
    fn keeps_absolute_values_and_resolves_relative_ones() {
        let root = Path::new("launcher");
        let absolute = std::env::temp_dir().join("aether-pcl2-absolute");
        assert_eq!(
            resolve_configured_path(&absolute.to_string_lossy(), root),
            Some(absolute)
        );
        assert_eq!(resolve_configured_path("mc", root), Some(root.join("mc")));
    }

    #[test]
    fn rejects_empty_and_upward_walking_values() {
        let root = Path::new("launcher");
        assert_eq!(resolve_configured_path("   ", root), None);
        assert_eq!(resolve_configured_path("$/../Windows", root), None);
        assert_eq!(resolve_configured_path("../Windows", root), None);
    }

    #[test]
    fn reads_the_selected_folder_before_the_custom_list() {
        let root = Path::new("launcher");
        let pairs =
            parse_setup_ini("LaunchFolderSelect=$/mc\nLaunchFolders=Second>other|Third>$/mc\n");

        assert_eq!(
            configured_game_directories(&pairs, root),
            vec![root.join("mc"), root.join("other")]
        );
    }

    #[test]
    fn an_empty_configuration_yields_no_directory() {
        let root = Path::new("launcher");
        let pairs = parse_setup_ini("LaunchFolderSelect=\n");
        assert!(configured_game_directories(&pairs, root).is_empty());
        assert_eq!(setup_value(&pairs, "LaunchFolderSelect"), Some(""));
    }

    /// A temporary directory removed when it goes out of scope.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "aether-pcl2-{label}-{}-{counter}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }

        fn mkdir(&self, relative: &str) {
            std::fs::create_dir_all(self.0.join(relative)).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn reads_the_documented_setup_ini_layout() {
        let temp = TempDir::new("documented");
        temp.write("PCL/Setup.ini", "LaunchFolderSelect=$/mc\n");

        let config = locate_config(temp.path()).expect("PCL/Setup.ini has to be read");

        assert_eq!(config.launcher_root, temp.path());
        assert_eq!(
            config.pairs,
            vec![("LaunchFolderSelect".to_owned(), "$/mc".to_owned())]
        );
    }

    #[test]
    fn accepts_the_flat_pcl_ini_layout() {
        let temp = TempDir::new("flat");
        temp.write("PCL.ini", "LaunchFolderSelect=$/mc\n");

        let config = locate_config(temp.path()).expect("PCL.ini has to be read");

        assert_eq!(config.launcher_root, temp.path());
    }

    #[test]
    fn accepts_a_setup_ini_at_the_root_of_a_launcher() {
        let temp = TempDir::new("root-ini");
        temp.write("Setup.ini", "LaunchFolderSelect=$/.minecraft\n");
        temp.mkdir(".minecraft/versions/1.20.1");

        let config = locate_config(temp.path()).expect("Setup.ini has to be read");

        assert_eq!(config.launcher_root, temp.path());
    }

    #[test]
    fn pointing_at_the_configuration_folder_uses_its_parent() {
        let temp = TempDir::new("config-folder");
        temp.mkdir(".minecraft/versions/1.20.1");
        temp.write("PCL/Setup.ini", "LaunchFolderSelect=$/.minecraft\n");

        let config =
            locate_config(&temp.path().join("PCL")).expect("the PCL folder has to be recognised");

        assert_eq!(config.launcher_root, temp.path());
    }

    #[test]
    fn an_absolute_folder_keeps_the_selected_folder_as_root() {
        let temp = TempDir::new("absolute");
        temp.write("PCL/Setup.ini", "LaunchFolderSelect:E:\\a\\.minecraft\\\n");

        let config =
            locate_config(&temp.path().join("PCL")).expect("the PCL folder has to be recognised");

        assert_eq!(config.launcher_root, temp.path().join("PCL"));
    }

    #[test]
    fn refuses_a_setup_ini_that_is_not_pcl2() {
        let temp = TempDir::new("not-pcl2");
        temp.write("Setup.ini", "AppName=SomethingElse\nInstallDir=C:\\x\n");

        assert!(locate_config(temp.path()).is_none());
    }

    #[test]
    fn refuses_a_folder_without_any_configuration() {
        let temp = TempDir::new("no-config");
        temp.mkdir("versions/1.20.1");

        assert!(locate_config(temp.path()).is_none());
    }
}
