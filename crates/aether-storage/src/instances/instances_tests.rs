use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::{GameInstance, InstanceId, InstanceState, VersionId};

use super::{InstanceLoadError, InstanceStore};
use crate::encode_instance;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    base: PathBuf,
    id: InstanceId,
    directory: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let number = NEXT.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("aether-store-{}-{number}", std::process::id()));
        let id = InstanceId::new();
        let directory = base.join("instances").join(id.to_string());
        fs::create_dir_all(&directory).unwrap();
        Self {
            base,
            id,
            directory,
        }
    }

    fn record(&self) -> GameInstance {
        GameInstance {
            id: self.id,
            name: "Aether Test".to_owned(),
            version: VersionId::parse("1.21.1").unwrap(),
            root: self.directory.clone(),
            state: InstanceState::Ready,
        }
    }

    fn write(&self, instance: &GameInstance) {
        let contents = encode_instance(instance).unwrap();
        fs::write(self.directory.join("instance.json"), contents).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[test]
fn loads_only_the_requested_instance_from_managed_directory() {
    let fixture = Fixture::new();
    let instance = fixture.record();
    fixture.write(&instance);

    let store = InstanceStore::open(&fixture.base).unwrap();
    let loaded = store.load(fixture.id).unwrap();
    assert_eq!(loaded, instance);
    assert!(store.load(InstanceId::new()).is_err());
}

#[test]
fn rejects_mismatched_instance_id_and_root() {
    let fixture = Fixture::new();
    let mut instance = fixture.record();
    instance.id = InstanceId::new();
    fixture.write(&instance);
    let store = InstanceStore::open(&fixture.base).unwrap();
    assert!(matches!(
        store.load(fixture.id),
        Err(InstanceLoadError::IdMismatch)
    ));

    instance.id = fixture.id;
    instance.root = fixture.base.clone();
    fixture.write(&instance);
    assert!(matches!(
        store.load(fixture.id),
        Err(InstanceLoadError::RootMismatch)
    ));
}

#[test]
fn rejects_oversized_and_malformed_records() {
    let fixture = Fixture::new();
    let file = fixture.directory.join("instance.json");
    fs::write(&file, vec![b'a'; 128 * 1024 + 1]).unwrap();
    let store = InstanceStore::open(&fixture.base).unwrap();
    assert!(matches!(
        store.load(fixture.id),
        Err(InstanceLoadError::TooLarge)
    ));
    fs::write(file, b"{bad json").unwrap();
    assert!(matches!(
        store.load(fixture.id),
        Err(InstanceLoadError::Decode(_))
    ));
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_records_and_directories() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    fixture.write(&fixture.record());
    let file = fixture.directory.join("instance.json");
    let target = fixture.base.join("outside.json");
    fs::rename(&file, &target).unwrap();
    symlink(&target, &file).unwrap();
    let store = InstanceStore::open(&fixture.base).unwrap();
    assert!(matches!(
        store.load(fixture.id),
        Err(InstanceLoadError::UnsafeRecord)
    ));

    fs::remove_file(&file).unwrap();
    fs::remove_dir(&fixture.directory).unwrap();
    symlink(&fixture.base, &fixture.directory).unwrap();
    assert!(matches!(
        store.load(fixture.id),
        Err(InstanceLoadError::UnsafeDirectory)
    ));
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_instances_root() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    let instances = fixture.base.join("instances");
    let real = fixture.base.join("real");
    fs::rename(&instances, &real).unwrap();
    symlink(&real, &instances).unwrap();
    assert!(matches!(
        InstanceStore::open(&fixture.base),
        Err(InstanceLoadError::InvalidStore)
    ));
}
