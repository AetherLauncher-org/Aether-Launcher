use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;
use sha1::{Digest, Sha1};

use super::{AssetError, verify_installed_assets};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("aether-assets-{}-{n}", std::process::id()));
        fs::create_dir_all(root.join("assets/indexes")).unwrap();
        fs::create_dir_all(root.join("assets/objects")).unwrap();
        Self { root }
    }

    fn put_object(&self, bytes: &[u8]) -> String {
        let hash = format!("{:x}", Sha1::digest(bytes));
        let path = self.root.join("assets/objects").join(&hash[..2]).join(&hash);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        hash
    }

    fn put_index(&self, data: serde_json::Value) -> (String, u64) {
        let bytes = serde_json::to_vec(&data).unwrap();
        fs::write(self.root.join("assets/indexes/17.json"), &bytes).unwrap();
        (format!("{:x}", Sha1::digest(&bytes)), bytes.len() as u64)
    }

    fn verify(&self, digest: &str, size: u64) -> Result<super::VerifiedAssets, AssetError> {
        verify_installed_assets(&self.root, "17", digest, size)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn validates_index_and_deduplicated_content_addressed_objects() {
    let fixture = Fixture::new();
    let hash = fixture.put_object(b"abc");
    let (digest, size) = fixture.put_index(json!({"objects": {
        "minecraft/lang/en_us.json": {"hash":hash,"size":3},
        "minecraft/lang/en_gb.json": {"hash":hash,"size":3}
    }}));
    let result = fixture.verify(&digest, size).unwrap();
    assert_eq!(result.objects_verified, 1);
    assert_eq!(result.bytes_verified, 3);
    assert!(result.index_file.ends_with("assets/indexes/17.json"));
}

#[test]
fn rejects_modified_index_hash_and_length() {
    let fixture = Fixture::new();
    let (digest, size) = fixture.put_index(json!({"objects": {}}));
    assert!(matches!(
        fixture.verify(&"0".repeat(40), size),
        Err(AssetError::Verification(_))
    ));
    assert!(matches!(
        fixture.verify(&digest, size + 1),
        Err(AssetError::Verification(_))
    ));
    assert!(matches!(
        verify_installed_assets(&fixture.root, "../escape", &digest, size),
        Err(AssetError::UnsafeIndexId)
    ));
}

#[test]
fn rejects_missing_and_tampered_object_files() {
    let fixture = Fixture::new();
    let hash = fixture.put_object(b"abc");
    let (digest, size) = fixture.put_index(json!({"objects": {
        "one": {"hash":hash,"size":3}
    }}));
    fs::write(fixture.root.join("assets/objects").join(&hash[..2]).join(&hash), b"bad").unwrap();
    assert!(matches!(fixture.verify(&digest, size), Err(AssetError::Verification(_))));
    fs::remove_file(fixture.root.join("assets/objects").join(&hash[..2]).join(&hash)).unwrap();
    assert!(matches!(fixture.verify(&digest, size), Err(AssetError::Verification(_))));
}

#[test]
fn rejects_bad_hashes_and_conflicting_object_sizes() {
    let fixture = Fixture::new();
    let (digest, size) = fixture.put_index(json!({"objects": {
        "bad": {"hash":"../invalid","size":3}
    }}));
    assert!(matches!(fixture.verify(&digest, size), Err(AssetError::InvalidHash(_))));

    let hash = fixture.put_object(b"abc");
    let (digest, size) = fixture.put_index(json!({"objects": {
        "first": {"hash":hash,"size":3},
        "second": {"hash":hash,"size":4}
    }}));
    assert!(matches!(fixture.verify(&digest, size), Err(AssetError::ConflictingObjectSizes)));
}

#[test]
fn rejects_oversized_index_and_objects_before_verification() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.verify(&"0".repeat(40), 32 * 1024 * 1024 + 1),
        Err(AssetError::IndexTooLarge)
    ));
    let (digest, size) = fixture.put_index(json!({"objects": {
        "huge": {"hash":"0123456789abcdef0123456789abcdef01234567","size":1073741825_u64}
    }}));
    assert!(matches!(fixture.verify(&digest, size), Err(AssetError::ObjectTooLarge)));
}

#[cfg(unix)]
#[test]
fn rejects_object_symlink_outside_game_directory() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let data = b"abc";
    let hash = format!("{:x}", Sha1::digest(data));
    let outside = fixture.root.parent().unwrap().join(format!("aether-object-outside-{}", std::process::id()));
    fs::write(&outside, data).unwrap();
    let path = fixture.root.join("assets/objects").join(&hash[..2]).join(&hash);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    symlink(&outside, path).unwrap();
    let (digest, size) = fixture.put_index(json!({"objects": {
        "one": {"hash":hash,"size":3}
    }}));
    assert!(matches!(fixture.verify(&digest, size), Err(AssetError::Verification(_))));
    fs::remove_file(outside).unwrap();
}
