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
    let left: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        left,
        vec![std::ffi::OsString::from("a.png")],
        "no temporary file is left"
    );
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn writers_of_the_same_file_never_share_a_temporary_file() {
    let dir = temp_dir();
    let path = dir.join("icon");
    let first = vec![1u8; 64 * 1024];
    let second = vec![2u8; 48 * 1024];

    // a rename onto a file another writer is renaming over can fail on Windows; a failed
    // write is fine (icons are fetched again), a mixed or leftover file is not
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..50 {
                    let _ = write_replacing(&path, &first);
                }
            });
            scope.spawn(|| {
                for _ in 0..50 {
                    let _ = write_replacing(&path, &second);
                }
            });
        }
    });
    write_replacing(&path, &first).unwrap();

    // the file is always one writer's whole payload, never a mix, and nothing is left behind
    let bytes = fs::read(&path).unwrap();
    assert!(
        bytes == first || bytes == second,
        "the file is one whole payload"
    );
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn a_link_in_the_way_is_replaced_and_its_target_is_left_alone() {
    let dir = temp_dir();
    let victim = dir.join("victim.txt");
    fs::write(&victim, b"keep me").unwrap();
    let path = dir.join("icon");
    // a link to the victim at the final name and at the name the temporary file used to have;
    // where links cannot be made (no privilege on Windows) there is nothing to test
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
