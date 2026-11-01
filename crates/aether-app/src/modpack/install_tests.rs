//! Real storage transactions with in-memory archives and injected game lifecycle.
use super::*;
use sha1::{Digest, Sha1};
use sha2::Sha512;
use std::io::{Cursor, Write};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use zip::write::SimpleFileOptions;

struct FakeHost {
    dir: PathBuf,
    pick: Mutex<Option<PathBuf>>,
}
impl FakeHost {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("aether-pack-install-{}", InstanceId::new()));
        fs::create_dir(&dir).unwrap();
        Self {
            dir,
            pick: Mutex::new(None),
        }
    }
    fn select(&self, bytes: &[u8]) {
        let path = self.dir.join("selected.mrpack");
        fs::write(&path, bytes).unwrap();
        *self.pick.lock().unwrap() = Some(path);
    }
    fn clean(&self, instances: usize) {
        assert!(!fs::read_dir(&self.dir).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".mrpack-")
        }));
        let root = self.dir.join("instances");
        assert_eq!(
            if root.exists() {
                fs::read_dir(root).unwrap().count()
            } else {
                0
            },
            instances
        );
    }
}
impl Drop for FakeHost {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
impl Host for FakeHost {
    fn emit_import_item(&self, _: &crate::import::ImportItemEvent) {}
    fn pick_folder(&self, _: &str) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    fn pick_file(&self, _: &str, _: &[&str]) -> Result<Option<PathBuf>, String> {
        Ok(self.pick.lock().unwrap().clone())
    }
    fn known_folders(&self) -> crate::host::KnownFolders {
        crate::host::KnownFolders {
            desktop: None,
            downloads: None,
            documents: None,
            home: None,
        }
    }
    fn data_dir(&self) -> Result<PathBuf, String> {
        Ok(self.dir.clone())
    }
    fn is_instance_running(&self, _: InstanceId) -> Result<bool, String> {
        Ok(false)
    }
    fn emit_login(&self, _: &crate::microsoft::LoginEvent) {}
    fn emit_install(&self, _: &crate::install::InstallProgress) {}
}
struct FakeNet(String);
impl ContentNet for FakeNet {
    fn post_json(&self, _: &str, _: &str) -> Result<String, String> {
        panic!("unexpected POST")
    }
    fn get_text(&self, url: &str) -> Result<String, String> {
        assert!(url.contains("/project/AAAA0000/version"));
        Ok(self.0.clone())
    }
    fn get_bytes(&self, _: &str, _: usize) -> Result<Vec<u8>, String> {
        panic!("unexpected bytes")
    }
}
struct Source {
    archive: Vec<u8>,
    payload: Vec<u8>,
    cancel_read: Option<CancellationToken>,
}
impl FileSource for Source {
    fn open(&self, url: &str) -> Result<Box<dyn Read>, String> {
        if url.ends_with("pack.mrpack") {
            return Ok(Box::new(Cursor::new(self.archive.clone())));
        }
        assert!(url.ends_with("/mod.jar"));
        Ok(Box::new(PayloadReader {
            data: Cursor::new(self.payload.clone()),
            cancel: self.cancel_read.clone(),
        }))
    }
}
struct PayloadReader {
    data: Cursor<Vec<u8>>,
    cancel: Option<CancellationToken>,
}
impl Read for PayloadReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let count = self.data.read(out)?;
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        Ok(count)
    }
}
#[derive(Default)]
struct FakeGame {
    prepared: AtomicUsize,
    activated: AtomicUsize,
    fail: bool,
    cancel: Option<CancellationToken>,
}
impl GameLifecycle for FakeGame {
    fn prepare(
        &self,
        _: &dyn Host,
        base: &Path,
        index: &Index,
        _: &str,
        cancel: &CancellationToken,
    ) -> Result<String, String> {
        assert!(!cancel.is_cancelled());
        self.prepared.fetch_add(1, Ordering::SeqCst);
        fs::create_dir_all(base.join("game")).unwrap();
        fs::write(base.join("game/shared"), b"keep cached game").unwrap();
        Ok(index.dependencies["minecraft"].clone())
    }
    fn activate(&self, store: &Path, game: &Path, id: InstanceId) -> Result<(), String> {
        self.activated.fetch_add(1, Ordering::SeqCst);
        let instance = aether_storage::InstanceStore::open(store)
            .unwrap()
            .load(id)
            .unwrap();
        assert_eq!(instance.state, InstanceState::Incomplete);
        assert_eq!(
            fs::read(instance.root.join("minecraft/mods/a.jar")).unwrap(),
            b"abc"
        );
        let visible = game
            .parent()
            .unwrap()
            .join("instances")
            .join(id.to_string());
        assert!(!visible.exists(), "must not publish before activation");
        assert_eq!(fs::read(game.join("shared")).unwrap(), b"keep cached game");
        if self.fail {
            return Err("activation failed".to_owned());
        }
        aether_storage::set_instance_state(&instance.root, &instance, InstanceState::Ready)
            .unwrap();
        if let Some(cancel) = &self.cancel {
            cancel.cancel();
        }
        Ok(())
    }
}
fn archive(options: bool) -> Vec<u8> {
    let index = serde_json::json!({"formatVersion":1,"game":"minecraft","versionId":"v1","name":"Pack",
        "dependencies":{"minecraft":"1.21.1"},"files":[{"path":"mods/a.jar","fileSize":3,
        "hashes":{"sha1":format!("{:x}",Sha1::digest(b"abc")),"sha512":format!("{:x}",Sha512::digest(b"abc"))},
        "downloads":["https://cdn.modrinth.com/mod.jar"]}]});
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("modrinth.index.json", SimpleFileOptions::default())
        .unwrap();
    writer
        .write_all(&serde_json::to_vec(&index).unwrap())
        .unwrap();
    writer
        .start_file("overrides/config/test.txt", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"pack config").unwrap();
    if options {
        writer
            .start_file("overrides/options.txt", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"lang:en_us\n").unwrap();
    }
    writer.finish().unwrap().into_inner()
}
fn versions(bytes: &[u8], size: u64) -> String {
    serde_json::json!([{"id":"pack","project_id":"AAAA0000","version_number":"1.0","version_type":"release",
        "game_versions":["1.21.1"],"loaders":["fabric"],"date_published":"2026-01-01T00:00:00Z",
        "files":[{"url":"https://cdn.modrinth.com/data/pack.mrpack","filename":"pack.mrpack","size":size,
        "primary":true,"hashes":{"sha1":format!("{:x}",Sha1::digest(bytes)),"sha512":format!("{:x}",Sha512::digest(bytes))}}]}]).to_string()
}
fn run(
    host: &FakeHost,
    source: &Source,
    game: &FakeGame,
    remote: bool,
    cancel: &CancellationToken,
) -> Result<InstanceSummary, String> {
    let net = FakeNet(versions(&source.archive, source.archive.len() as u64));
    install_with(
        &Services {
            host,
            net: &net,
            source,
            game,
        },
        remote.then_some("AAAA0000"),
        "New Pack",
        false,
        cancel,
    )
}
fn source(options: bool) -> Source {
    Source {
        archive: archive(options),
        payload: b"abc".to_vec(),
        cancel_read: None,
    }
}
#[test]
fn local_install_publishes_ready_instance_and_preserves_defaults_or_pack_options() {
    for options in [false, true] {
        let host = FakeHost::new();
        let source = source(options);
        host.select(&source.archive);
        let game = FakeGame::default();
        let result = run(&host, &source, &game, false, &CancellationToken::new()).unwrap();
        assert_eq!(result.state(), "ready");
        assert_eq!(result.name(), "New Pack");
        assert_eq!(result.version(), "1.21.1");
        let ready = aether_storage::InstanceStore::open(&host.dir)
            .unwrap()
            .load(InstanceId::parse(result.id()).unwrap())
            .unwrap();
        assert_eq!(
            ready.root,
            fs::canonicalize(host.dir.join("instances").join(result.id())).unwrap()
        );
        let minecraft = ready.root.join("minecraft");
        assert_eq!(
            fs::read(minecraft.join("config/test.txt")).unwrap(),
            b"pack config"
        );
        let settings = fs::read_to_string(minecraft.join("options.txt")).unwrap();
        if options {
            assert_eq!(settings, "lang:en_us\n");
        } else {
            assert!(settings.contains("lang:zh_cn"));
        }
        assert_eq!(game.activated.load(Ordering::SeqCst), 1);
        host.clean(1);
    }
}
#[test]
fn remote_install_verifies_archive_and_cleans_spool() {
    let host = FakeHost::new();
    let result = run(
        &host,
        &source(false),
        &FakeGame::default(),
        true,
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(result.state(), "ready");
    host.clean(1);
}
#[test]
fn invalid_remote_zip_cleans_spool_before_preparation() {
    let host = FakeHost::new();
    let mut source = source(false);
    source.archive = b"not a zip".to_vec();
    let game = FakeGame::default();
    assert!(run(&host, &source, &game, true, &CancellationToken::new()).is_err());
    assert_eq!(game.prepared.load(Ordering::SeqCst), 0);
    host.clean(0);
}
#[test]
fn payload_hash_failure_rolls_back_without_touching_existing_instance() {
    let host = FakeHost::new();
    let existing = aether_storage::create_instance(&host.dir, "Existing", "1.21.1").unwrap();
    let original = fs::read(existing.root.join("minecraft/options.txt")).unwrap();
    let mut source = source(false);
    source.payload = b"abd".to_vec();
    let game = FakeGame::default();
    assert!(
        run(&host, &source, &game, true, &CancellationToken::new())
            .unwrap_err()
            .contains("SHA")
    );
    assert_eq!(game.activated.load(Ordering::SeqCst), 0);
    assert_eq!(
        fs::read(existing.root.join("minecraft/options.txt")).unwrap(),
        original
    );
    assert!(host.dir.join("game/shared").exists());
    host.clean(1);
}
#[test]
fn activation_failure_rolls_back_materialized_instance() {
    let host = FakeHost::new();
    let game = FakeGame {
        fail: true,
        ..Default::default()
    };
    assert_eq!(
        run(
            &host,
            &source(false),
            &game,
            true,
            &CancellationToken::new()
        )
        .unwrap_err(),
        "activation failed"
    );
    assert_eq!(game.activated.load(Ordering::SeqCst), 1);
    host.clean(0);
}
#[test]
fn cancellation_during_payload_read_rolls_back_without_activation() {
    let host = FakeHost::new();
    let cancel = CancellationToken::new();
    let mut source = source(false);
    source.cancel_read = Some(cancel.clone());
    let game = FakeGame::default();
    assert_eq!(
        run(&host, &source, &game, true, &cancel).unwrap_err(),
        CANCELLED
    );
    assert_eq!(game.activated.load(Ordering::SeqCst), 0);
    host.clean(0);
}
#[test]
fn cancellation_after_activation_does_not_publish_ready_record() {
    let host = FakeHost::new();
    let cancel = CancellationToken::new();
    let game = FakeGame {
        cancel: Some(cancel.clone()),
        ..Default::default()
    };
    assert_eq!(
        run(&host, &source(false), &game, true, &cancel).unwrap_err(),
        CANCELLED
    );
    assert_eq!(game.activated.load(Ordering::SeqCst), 1);
    host.clean(0);
}
#[test]
fn picker_dismissal_and_pre_cancel_never_prepare_game() {
    let host = FakeHost::new();
    let game = FakeGame::default();
    let cancel = CancellationToken::new();
    assert_eq!(
        run(&host, &source(false), &game, false, &cancel).unwrap_err(),
        CANCELLED
    );
    cancel.cancel();
    assert_eq!(
        run(&host, &source(false), &game, true, &cancel).unwrap_err(),
        CANCELLED
    );
    assert_eq!(game.prepared.load(Ordering::SeqCst), 0);
    host.clean(0);
}
#[test]
fn archive_hash_mismatch_and_size_limit_leave_no_spool() {
    let host = FakeHost::new();
    let source = source(false);
    let cancel = CancellationToken::new();
    for net in [
        FakeNet(versions(b"bad", source.archive.len() as u64)),
        FakeNet(versions(
            &source.archive,
            super::super::MAX_ARCHIVE_BYTES + 1,
        )),
    ] {
        assert!(fetch_archive(&net, &source, "AAAA0000", &host.dir, &cancel).is_err());
        host.clean(0);
    }
}
