//! Tests for the modpack install transaction, without any real network.
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use aether_core::InstanceId;
use aether_download::CancellationToken;
use sha1::{Digest, Sha1};
use sha2::Sha512;

use super::*;
use crate::identify::ContentNet;
use crate::install::CANCELLED;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct FakeHost {
    dir: PathBuf,
    pick: Mutex<Option<PathBuf>>,
}

impl FakeHost {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("aether-modpack-install-{}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        Self { dir, pick: Mutex::new(None) }
    }
}

impl Drop for FakeHost {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl crate::Host for FakeHost {
    fn emit_import_item(&self, _event: &crate::import::ImportItemEvent) {}
    fn pick_folder(&self, _title: &str) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
    fn pick_file(&self, _filter_name: &str, _extensions: &[&str]) -> Result<Option<PathBuf>, String> {
        Ok(self.pick.lock().unwrap().clone())
    }
    fn known_folders(&self) -> crate::host::KnownFolders {
        crate::host::KnownFolders { desktop: None, downloads: None, documents: None, home: None }
    }
    fn data_dir(&self) -> Result<PathBuf, String> {
        Ok(self.dir.clone())
    }
    fn is_instance_running(&self, _id: InstanceId) -> Result<bool, String> {
        Ok(false)
    }
    fn emit_login(&self, _event: &crate::microsoft::LoginEvent) {}
    fn emit_install(&self, _progress: &crate::install::InstallProgress) {}
}

/// A `ContentNet` that answers one versions listing.
struct FakeNet {
    versions: String,
}

impl ContentNet for FakeNet {
    fn post_json(&self, _url: &str, _body: &str) -> Result<String, String> {
        Err("not used".to_owned())
    }
    fn get_text(&self, _url: &str) -> Result<String, String> {
        Ok(self.versions.clone())
    }
    fn get_bytes(&self, _url: &str, _limit: usize) -> Result<Vec<u8>, String> {
        Err("not used".to_owned())
    }
}

/// A `FileSource` that serves fixed bytes for any URL.
struct FakeSource(Vec<u8>);

impl FileSource for FakeSource {
    fn open(&self, _url: &str) -> Result<Box<dyn Read>, String> {
        Ok(Box::new(Cursor::new(self.0.clone())))
    }
}

fn versions_json(sha1: &str, sha512: &str, size: u64) -> String {
    format!(
        r#"[{{"id":"pack","version_number":"1.0","version_type":"release","game_versions":["1.21.1"],
        "loaders":["fabric"],"date_published":"2026-01-01T00:00:00Z",
        "files":[{{"url":"https://cdn.modrinth.com/data/pack.mrpack","filename":"pack.mrpack",
        "size":{size},"primary":true,"hashes":{{"sha1":"{sha1}","sha512":"{sha512}"}}}}]}}]"#
    )
}

#[test]
fn local_pick_dismissal_is_cancelled() {
    let host = FakeHost::new();
    let source = FakeSource(Vec::new());
    let net = FakeNet { versions: String::new() };
    let cancel = CancellationToken::new();
    let error = install_with(&host, &net, &source, None, "New Pack", false, &cancel).unwrap_err();
    assert_eq!(error, CANCELLED);
}

#[test]
fn fetch_archive_verifies_against_the_platform_hashes() {
    let host = FakeHost::new();
    let bytes = b"not really a zip but the hash must match".to_vec();
    let sha1 = format!("{:x}", Sha1::digest(&bytes));
    let sha512 = format!("{:x}", Sha512::digest(&bytes));
    let net = FakeNet { versions: versions_json(&sha1, &sha512, bytes.len() as u64) };
    let source = FakeSource(bytes.clone());
    let cancel = CancellationToken::new();

    let archive = fetch_archive(&net, &source, "AAAA0000", &host.dir, &cancel).unwrap();
    let mut out = Vec::new();
    archive.take(64 * 1024).read_to_end(&mut out).unwrap();
    assert_eq!(out, bytes);
}

#[test]
fn fetch_archive_rejects_a_hash_mismatch() {
    let host = FakeHost::new();
    let bytes = b"the real bytes".to_vec();
    let sha1 = format!("{:x}", Sha1::digest(&bytes));
    let sha512 = format!("{:x}", Sha512::digest(&bytes));
    let net = FakeNet { versions: versions_json(&sha1, &sha512, bytes.len() as u64) };
    let source = FakeSource(b"tampered bytes".to_vec());
    let cancel = CancellationToken::new();

    assert!(fetch_archive(&net, &source, "AAAA0000", &host.dir, &cancel).is_err());
}

#[test]
fn fetch_archive_rejects_an_oversized_archive() {
    let host = FakeHost::new();
    let bytes = b"x".to_vec();
    let sha1 = format!("{:x}", Sha1::digest(&bytes));
    let sha512 = format!("{:x}", Sha512::digest(&bytes));
    let net = FakeNet { versions: versions_json(&sha1, &sha512, crate::modpack::MAX_ARCHIVE_BYTES + 1) };
    let source = FakeSource(bytes);
    let cancel = CancellationToken::new();

    assert!(fetch_archive(&net, &source, "AAAA0000", &host.dir, &cancel).is_err());
}
