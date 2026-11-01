use super::*;
use std::io::Cursor;
use zip::write::SimpleFileOptions;

fn index() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"formatVersion":1,"game":"minecraft","versionId":"v1","name":"测试",
        "dependencies":{"minecraft":"1.21.1"},"files":[{"path":"mods/a.jar","fileSize":3,
        "hashes":{"sha1":format!("{:x}",Sha1::digest(b"abc")),"sha512":format!("{:x}",Sha512::digest(b"abc"))},
        "downloads":["https://cdn.modrinth.com/a"]}]})).unwrap()
}
fn archive(entries: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("modrinth.index.json", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(&index()).unwrap();
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap()
}
struct Net(&'static [u8]);
impl FileSource for Net {
    fn open(&self, _: &str) -> Result<Box<dyn Read>, String> {
        Ok(Box::new(Cursor::new(self.0)))
    }
}
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("aether-modpack-{}", aether_core::InstanceId::new()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn modpack_extracts_by_name_and_resolves_client_layer() {
    let mut pack = Pack::open(archive(&[
        ("overrides/config/a.txt", b"common"),
        ("client-overrides/config/a.txt", b"client"),
        ("server-overrides/server.txt", b"server"),
    ]))
    .unwrap();
    let tmp = Temp::new();
    let root = tmp.0.join("new");
    pack.materialize(&root, &Net(b"abc"), false).unwrap();
    assert_eq!(fs::read(root.join("mods/a.jar")).unwrap(), b"abc");
    assert_eq!(fs::read(root.join("config/a.txt")).unwrap(), b"client");
    assert!(!root.join("server.txt").exists());
    assert!(pack.materialize(&root, &Net(b"abc"), false).is_err());
    assert_eq!(fs::read(root.join("config/a.txt")).unwrap(), b"client");
}
#[test]
fn modpack_hash_size_failure_rolls_back_owned_directory() {
    for bytes in [b"abd".as_slice(), b"ab", b"abcd"] {
        let mut pack = Pack::open(archive(&[])).unwrap();
        let tmp = Temp::new();
        let root = tmp.0.join("new");
        assert!(pack.materialize(&root, &Net(bytes), false).is_err());
        assert!(!root.exists());
    }
}
#[test]
fn modpack_rejects_traversal_aliases_and_collisions() {
    for path in [
        "overrides/../escape",
        "overrides/CON",
        "server-overrides/C:/bad",
        "overrides/mods/a.jar",
    ] {
        assert!(Pack::open(archive(&[(path, b"x")])).is_err(), "{path}");
    }
    assert!(
        Pack::open(archive(&[
            ("overrides/config", b"a"),
            ("overrides/config/file", b"b")
        ]))
        .is_err()
    );
    assert!(Pack::open(archive(&[("overrides/A", b"a"), ("overrides/a", b"b")])).is_err());
}
#[test]
fn modpack_checks_both_hashes_independently() {
    for hash in ["sha1", "sha512"] {
        let mut value: serde_json::Value = serde_json::from_slice(&index()).unwrap();
        let len = if hash == "sha1" { 40 } else { 128 };
        value["files"][0]["hashes"][hash] = "0".repeat(len).into();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("modrinth.index.json", SimpleFileOptions::default())
            .unwrap();
        writer
            .write_all(&serde_json::to_vec(&value).unwrap())
            .unwrap();
        let mut pack = Pack::open(writer.finish().unwrap()).unwrap();
        let tmp = Temp::new();
        let root = tmp.0.join("new");
        assert!(
            pack.materialize(&root, &Net(b"abc"), false).is_err(),
            "{hash}"
        );
        assert!(!root.exists());
    }
}

#[test]
fn modpack_cancellation_during_read_rolls_back_and_stays_cancelled() {
    struct CancelSource(CancellationToken);
    struct CancelRead(CancellationToken);
    impl FileSource for CancelSource {
        fn open(&self, _: &str) -> Result<Box<dyn Read>, String> {
            Ok(Box::new(CancelRead(self.0.clone())))
        }
    }
    impl Read for CancelRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            self.0.cancel();
            Err(std::io::Error::other("cancelled"))
        }
    }
    let cancel = CancellationToken::new();
    let tmp = Temp::new();
    let root = tmp.0.join("new");
    let mut pack = Pack::open(archive(&[])).unwrap();
    let error = pack
        .materialize_with_cancel(&root, &CancelSource(cancel.clone()), false, &cancel)
        .unwrap_err();
    assert_eq!(error, crate::install::CANCELLED);
    assert!(!root.exists());
}

#[test]
fn modpack_rejects_zip_symlinks() {
    let mut writer = zip::ZipWriter::new(archive(&[]));
    writer
        .add_symlink(
            "overrides/link",
            "../../outside",
            SimpleFileOptions::default(),
        )
        .unwrap();
    assert!(Pack::open(writer.finish().unwrap()).is_err());
}
