//! Unit tests for the import commands.
//!
//! The file is pulled in with `#[path = "import_tests.rs"] mod tests;` from
//! `commands/import.rs`, so it is the *body* of the `tests` module and `super::*`
//! refers to `commands::import`. Do not wrap the contents in another
//! `mod tests { ... }`.
//!
//! Only the pure helpers are covered here: the Tauri commands themselves need a
//! running application handle and a native folder picker. Every test builds its
//! own directory below `std::env::temp_dir()` and removes it on drop.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use aether_core::{GameInstance, InstanceId, InstanceState, VersionId};
use aether_download::CancellationToken;
use aether_download::transport::HttpTransport;
use aether_storage::{ImportPlan, ImportSkipReason, ImportSourceKind};

use super::{
    ImportCandidateDto, ImportResultDto, ImportSkippedDto, ImportSourceDto, ImportState,
    LauncherInstallationDto, PendingSource, Reporter, describe_scan_error,
    drop_repeated_game_directories, installation_at, listing_of, pcl2_game_directories,
    plan_source, run_import,
};
use crate::commands::instances::InstanceSummary;

/// Monotonic counter that keeps temporary directory names unique per process.
static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A temporary directory removed when it goes out of scope.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "aether-import-command-{label}-{}-{counter}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn touch(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, contents).unwrap();
        path
    }

    fn mkdir(&self, relative: &str) -> PathBuf {
        let path = self.path.join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn a_plain_game_folder_is_labelled_folder() {
    let temp = TempDir::new("plain");
    temp.touch("versions/1.20.1/1.20.1.json", r#"{"id":"1.20.1"}"#);

    let (kind, candidates) = plan_source(temp.path(), None).unwrap();

    assert_eq!(kind, ImportSourceKind::Folder);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].version_id, "1.20.1");
}

#[test]
fn a_hmcl_marker_is_detected() {
    let temp = TempDir::new("hmcl");
    temp.touch(".hmcl.json", "{}");
    temp.touch("versions/1.20.1/1.20.1.json", r#"{"id":"1.20.1"}"#);

    let (kind, _) = plan_source(temp.path(), None).unwrap();

    assert_eq!(kind, ImportSourceKind::Hmcl);
}

#[test]
fn a_forced_kind_overrides_detection() {
    let temp = TempDir::new("forced");
    temp.touch(".hmcl.json", "{}");
    temp.touch("versions/1.20.1/1.20.1.json", r#"{"id":"1.20.1"}"#);

    let (kind, _) = plan_source(temp.path(), Some(ImportSourceKind::Pcl2)).unwrap();

    assert_eq!(kind, ImportSourceKind::Pcl2);
}

#[test]
fn a_missing_folder_is_reported_without_a_path() {
    let temp = TempDir::new("missing");
    let error = plan_source(&temp.path().join("nope"), None).unwrap_err();

    assert!(!error.contains(temp.path().to_string_lossy().as_ref()));
    assert!(error.contains("not a readable directory"));
}

#[test]
fn scan_errors_never_echo_a_path() {
    let temp = TempDir::new("scan-error");
    let file = temp.touch("file", "{}");
    let error = plan_source(&file, None).unwrap_err();

    assert!(!error.contains(file.to_string_lossy().as_ref()));
}
#[test]
fn only_game_folders_with_versions_survive() {
    let temp = TempDir::new("pcl2-dirs");
    temp.mkdir("configured/versions/1.20.1");
    temp.mkdir("empty");
    temp.mkdir(".minecraft/versions/1.21");

    let config = vec![
        ("LaunchFolderSelect".to_owned(), "$/configured".to_owned()),
        ("LaunchFolderSelect".to_owned(), "$/empty".to_owned()),
    ];
    let directories = pcl2_game_directories(temp.path(), &config);

    assert_eq!(
        directories,
        vec![
            temp.path().join("configured"),
            temp.path().join(".minecraft"),
        ]
    );
}

#[test]
fn the_launcher_folder_is_offered_when_it_holds_versions() {
    let temp = TempDir::new("pcl2-self");
    temp.mkdir("versions/1.20.1");

    let directories = pcl2_game_directories(temp.path(), &[]);

    assert_eq!(directories, vec![temp.path().to_path_buf()]);
}

#[test]
fn import_state_hands_out_single_use_tokens() {
    let temp = TempDir::new("tokens");
    let state = ImportState::default();

    let token = state
        .remember(PendingSource::GameDirectory {
            root: temp.path().to_path_buf(),
        })
        .unwrap();
    assert!(state.take(&token).is_ok());
    assert!(state.take(&token).is_err(), "a token must not be reusable");
}

#[test]
fn an_unknown_token_is_rejected() {
    let state = ImportState::default();
    assert!(state.take("not-a-token").is_err());
}

#[test]
fn source_dto_uses_camel_case_and_hides_no_fields() {
    let dto = ImportSourceDto {
        token: "t".to_owned(),
        source_kind: "pcl2",
        source_path: "D:\\MC".to_owned(),
        candidates: vec![ImportCandidateDto {
            id: "1.20.1".to_owned(),
            display_name: "1.20.1".to_owned(),
        }],
    };

    let value = serde_json::to_value(&dto).unwrap();

    assert_eq!(value["token"], "t");
    assert_eq!(value["sourceKind"], "pcl2");
    assert_eq!(value["sourcePath"], "D:\\MC");
    assert_eq!(value["candidates"][0]["id"], "1.20.1");
    assert_eq!(value["candidates"][0]["displayName"], "1.20.1");
    assert!(value.get("source_kind").is_none());
}

#[test]
fn installation_dto_nests_its_sources() {
    let dto = LauncherInstallationDto {
        kind: "pcl2",
        launcher_root: "D:\\PCL2".to_owned(),
        sources: vec![ImportSourceDto {
            token: "t".to_owned(),
            source_kind: "pcl2",
            source_path: "D:\\PCL2\\.minecraft".to_owned(),
            candidates: Vec::new(),
        }],
    };

    let value = serde_json::to_value(&dto).unwrap();

    assert_eq!(value["kind"], "pcl2");
    assert_eq!(value["launcherRoot"], "D:\\PCL2");
    assert_eq!(value["sources"][0]["sourcePath"], "D:\\PCL2\\.minecraft");
}

#[test]
fn result_dto_reports_imported_instances_and_skips() {
    let instance = GameInstance {
        id: InstanceId::new(),
        name: "1.20.1".to_owned(),
        version: VersionId::parse("1.20.1").unwrap(),
        root: PathBuf::from("instances"),
        state: InstanceState::Incomplete,
    };
    let dto = ImportResultDto {
        imported: vec![InstanceSummary::from_instance(&instance)],
        skipped: vec![ImportSkippedDto {
            id: "1.21".to_owned(),
            reason: "missing-version-json",
        }],
    };

    let value = serde_json::to_value(&dto).unwrap();

    assert_eq!(value["imported"][0]["name"], "1.20.1");
    assert_eq!(value["imported"][0]["state"], "incomplete");
    assert_eq!(value["skipped"][0]["id"], "1.21");
    assert_eq!(value["skipped"][0]["reason"], "missing-version-json");
    assert!(value["imported"][0].get("root").is_none());
}

/// Fails every request, like an offline machine.
struct OfflineTransport;

impl HttpTransport for OfflineTransport {
    fn get(
        &self,
        request: &aether_download::transport::TransportRequest,
    ) -> Result<
        aether_download::transport::TransportResponse,
        aether_download::transport::TransportError,
    > {
        Err(aether_download::transport::TransportError::Timeout {
            url: request.url.clone(),
            timeout_secs: 1,
        })
    }
}

struct SilentReporter;

impl Reporter for SilentReporter {
    fn report(&self, _progress: &aether_app::install::InstallProgress) {}
}

fn plan_of(ids: &[&str], seed: &Path) -> ImportPlan {
    ImportPlan {
        requests: ids
            .iter()
            .map(|id| aether_storage::ImportRequest {
                id: (*id).to_owned(),
                version: (*id).to_owned(),
                name: (*id).to_owned(),
            })
            .collect(),
        skipped: vec![("1.12.2-forge".to_owned(), ImportSkipReason::NeedsLoader)],
        seed: seed.to_path_buf(),
    }
}

#[test]
fn planner_skips_are_carried_into_the_result() {
    let temp = TempDir::new("run-skips");
    let result = run_import(
        &OfflineTransport,
        temp.path(),
        plan_of(&[], temp.path()),
        &CancellationToken::new(),
        &SilentReporter,
        &|_| {},
    );

    assert!(result.imported.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].id, "1.12.2-forge");
    assert_eq!(result.skipped[0].reason, "needs-loader");
}

#[test]
fn an_unreachable_catalog_fails_each_version_without_creating_anything() {
    let temp = TempDir::new("run-offline");
    let result = run_import(
        &OfflineTransport,
        temp.path(),
        plan_of(&["1.20.1", "1.21"], temp.path()),
        &CancellationToken::new(),
        &SilentReporter,
        &|_| panic!("nothing should start"),
    );

    assert!(result.imported.is_empty());
    let failed: Vec<_> = result
        .skipped
        .iter()
        .filter(|item| item.reason == "install-failed")
        .collect();
    assert_eq!(failed.len(), 2);
    assert!(!temp.path().join("game").exists());
}

#[test]
fn a_cancelled_import_marks_the_rest_cancelled() {
    let temp = TempDir::new("run-cancel");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = run_import(
        &OfflineTransport,
        temp.path(),
        plan_of(&["1.20.1"], temp.path()),
        &cancel,
        &SilentReporter,
        &|_| {},
    );

    assert!(result.skipped.iter().any(|item| item.reason == "cancelled"));
}

#[test]
fn describe_scan_error_covers_every_variant() {
    for error in [
        aether_storage::ImportError::InvalidRoot,
        aether_storage::ImportError::TooManyEntries,
        aether_storage::ImportError::Io(std::io::Error::other("boom")),
    ] {
        let message = describe_scan_error(&error);
        assert!(!message.is_empty());
        assert!(!message.contains("boom"));
    }
}

#[test]
fn a_bakaxl_folder_is_recognised_by_its_executable() {
    let temp = TempDir::new("bakaxl");
    temp.touch("BakaXL.exe", "MZ");
    temp.touch(
        ".minecraft/versions/1.20.1/1.20.1.json",
        r#"{"id":"1.20.1"}"#,
    );

    let state = ImportState::default();
    let installation = installation_at(&state, temp.path(), &listing_of(temp.path()))
        .expect("a BakaXL folder has to be recognised");

    assert_eq!(installation.kind, "bakaxl");
    assert_eq!(installation.sources.len(), 1);
    assert_eq!(installation.sources[0].source_kind, "bakaxl");
    assert_eq!(installation.sources[0].candidates.len(), 1);
}

#[test]
fn a_prism_folder_offers_one_source_with_every_instance() {
    let temp = TempDir::new("prism");
    temp.touch("multimc.cfg", "InstanceDir=instances\n");
    temp.touch(
        "instances/fabric-1.16.1/mmc-pack.json",
        r#"{"components":[{"uid":"net.minecraft","version":"1.16.1"},
            {"uid":"net.fabricmc.fabric-loader","version":"0.18.4"}]}"#,
    );
    temp.touch("instances/fabric-1.16.1/instance.cfg", "name=Ranked\n");

    let state = ImportState::default();
    let installation = installation_at(&state, temp.path(), &listing_of(temp.path()))
        .expect("a Prism folder has to be recognised");

    assert_eq!(installation.kind, "prism");
    assert_eq!(installation.sources.len(), 1);
    let source = &installation.sources[0];
    assert_eq!(source.source_kind, "prism");
    assert_eq!(source.candidates.len(), 1);
    assert_eq!(source.candidates[0].id, "fabric-1.16.1");
    assert!(source.candidates[0].display_name.contains("Ranked"));
}

#[test]
fn a_plain_game_folder_is_not_a_launcher() {
    let temp = TempDir::new("plain-not-launcher");
    temp.touch("versions/1.20.1/1.20.1.json", r#"{"id":"1.20.1"}"#);

    let state = ImportState::default();

    assert!(installation_at(&state, temp.path(), &listing_of(temp.path())).is_none());
}

#[test]
fn an_hmcl_game_folder_is_recognised_by_its_config() {
    let temp = TempDir::new("hmcl");
    temp.touch(".hmcl.json", r#"{"version":3}"#);
    temp.touch("versions/1.20.1/1.20.1.json", r#"{"id":"1.20.1"}"#);

    let state = ImportState::default();
    let installation = installation_at(&state, temp.path(), &listing_of(temp.path()))
        .expect("an HMCL game folder has to be recognised");

    assert_eq!(installation.kind, "hmcl");
    assert_eq!(installation.sources.len(), 1);
    assert_eq!(installation.sources[0].source_kind, "hmcl");
    assert_eq!(installation.sources[0].candidates[0].id, "1.20.1");
}

/// 造一个只有路径和版本数的安装记录，用来验证去重。
fn installation(kind: &'static str, game_directories: &[&Path]) -> LauncherInstallationDto {
    LauncherInstallationDto {
        kind,
        launcher_root: "unused".to_owned(),
        sources: game_directories
            .iter()
            .map(|directory| ImportSourceDto {
                token: format!("{kind}-{}", directory.display()),
                source_kind: kind,
                source_path: directory.display().to_string(),
                candidates: Vec::new(),
            })
            .collect(),
    }
}

#[test]
fn the_same_game_directory_is_only_listed_once_across_launchers() {
    let temp = TempDir::new("dedup-across-launchers");
    let game = temp.path().join(".minecraft");
    fs::create_dir_all(&game).unwrap();

    // PCL2 和 HMCL 记录了同一个 .minecraft，这正是截图里重复三次的那一幕。
    let kept = drop_repeated_game_directories(vec![
        installation("pcl2", &[&game]),
        installation("hmcl", &[&game]),
    ]);

    assert_eq!(kept.len(), 1, "第二个启动器只剩重复条目时整条都要去掉");
    assert_eq!(kept[0].kind, "pcl2");
    assert_eq!(kept[0].sources.len(), 1);
}

#[test]
fn a_parent_folder_is_not_merged_into_its_minecraft_subfolder() {
    let temp = TempDir::new("dedup-parent-child");
    let parent = temp.path().to_path_buf();
    let nested = parent.join(".minecraft");
    fs::create_dir_all(&nested).unwrap();

    // `E:\a` 和 `E:\a\.minecraft` 是两个不同的目录，各自可能装不同的版本，
    // 所以都要留下。只有指向同一目录的重复条目才合并。
    let kept = drop_repeated_game_directories(vec![
        installation("pcl2", &[&parent, &nested]),
        installation("hmcl", &[&parent]),
    ]);

    // PCL2 的两条目录都保留；HMCL 的 parent 已被 PCL2 占用，整条去掉。
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].kind, "pcl2");
    assert_eq!(kept[0].sources.len(), 2, "父子目录是两个不同的来源");
}

#[test]
fn distinct_game_directories_are_all_kept() {
    let temp = TempDir::new("dedup-distinct");
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();

    let kept = drop_repeated_game_directories(vec![
        installation("pcl2", &[&first]),
        installation("hmcl", &[&second]),
    ]);

    assert_eq!(kept.len(), 2, "两个不同的目录都要留下");
    assert_eq!(kept[0].sources[0].source_path, first.display().to_string());
    assert_eq!(kept[1].sources[0].source_path, second.display().to_string());
}
