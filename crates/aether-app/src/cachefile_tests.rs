use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> std::path::PathBuf {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("aether-cachefile-{}-{n}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_file_is_written_whole_and_replaced() {
    let dir = temp_dir();
    let path = dir.join("a.png");

    write_replacing(&path, b"one").unwrap();
    write_replacing(&path, b"two").unwrap();

    assert_eq!(fs::read(&path).unwrap(), b"two");
    assert!(!dir.join("a.png.tmp").exists());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_link_in_the_way_is_replaced_and_its_target_is_left_alone() {
    let dir = temp_dir();
    let victim = dir.join("victim.txt");
    fs::write(&victim, b"keep me").unwrap();
    let path = dir.join("icon");
    // a link to the victim at the final name and at the temporary name; where links cannot be
    // made (no privilege on Windows) there is nothing to test
    let linked =
        make_link(&victim, &path).is_ok() && make_link(&victim, &dir.join("icon.tmp")).is_ok();

    write_replacing(&path, b"image").unwrap();

    assert_eq!(fs::read(&victim).unwrap(), b"keep me");
    assert_eq!(fs::read(&path).unwrap(), b"image");
    assert!(
        linked || cfg!(windows),
        "links should be possible on this platform"
    );
    let _ = fs::remove_dir_all(dir);
}

#[cfg(unix)]
fn make_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_link(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[test]
fn a_file_over_the_limit_is_not_returned() {
    let dir = temp_dir();
    let path = dir.join("big");
    fs::write(&path, vec![7u8; 11]).unwrap();

    assert_eq!(read_capped(&path, 11).map(|b| b.len()), Some(11));
    assert_eq!(read_capped(&path, 10), None);
    assert_eq!(read_capped(&dir.join("missing"), 10), None);
    let _ = fs::remove_dir_all(dir);
}
