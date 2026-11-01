use super::*;
use std::io::Write;
fn dir() -> PathBuf { let p = std::env::temp_dir().join(format!("aether-drop-test-{}", InstanceId::new())); fs::create_dir(&p).unwrap(); p }
#[test]
fn stage_limits_and_single_use_expiry() {
    let root = dir(); let jar = root.join("a.jar"); fs::write(&jar, b"x").unwrap();
    let state = DropState::default(); let now = Instant::now();
    assert!(stage_at(&state, vec![jar.clone(); 21], now).is_err());
    let preview = stage_at(&state, vec![jar.clone()], now).unwrap();
    assert_eq!(preview.items[0].kind, Kind::Mod);
    assert!(take(&state, &preview.token, now).is_ok()); assert!(take(&state, &preview.token, now).is_err());
    let preview = stage_at(&state, vec![jar], now).unwrap();
    assert!(take(&state, &preview.token, now + TTL).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn classifies_zip_names_without_extracting() {
    let root = dir();
    for (name, entries, expected) in [("data.zip", vec!["pack.mcmeta", "data/a"], Kind::Datapack), ("resource.zip", vec!["pack.mcmeta", "assets/a"], Kind::Resourcepack), ("shader.zip", vec!["shaders/a"], Kind::Shader)] {
        let path = root.join(name); let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for entry in entries { zip.start_file(entry, zip::write::SimpleFileOptions::default()).unwrap(); zip.write_all(b"x").unwrap(); }
        zip.finish().unwrap(); assert_eq!(classify(&path).unwrap(), expected);
    }
    assert!(classify(&root.join("bad.mrpack")).is_err()); fs::remove_dir_all(root).unwrap();
}
#[test]
fn copy_never_overwrites() {
    let root = dir(); let source = root.join("a.jar"); fs::write(&source, b"new").unwrap();
    let destination = root.join("mods"); fs::create_dir(&destination).unwrap(); fs::write(destination.join("a.jar"), b"old").unwrap();
    let staged = Source { path: source, item: Item { name: "a.jar".into(), size: 3, kind: Kind::Mod } };
    assert!(copy_file(&staged, &destination).is_err()); assert_eq!(fs::read(destination.join("a.jar")).unwrap(), b"old");
    fs::remove_file(destination.join("a.jar")).unwrap(); copy_file(&staged, &destination).unwrap(); assert_eq!(fs::read(destination.join("a.jar")).unwrap(), b"new"); fs::remove_dir_all(root).unwrap();
}
