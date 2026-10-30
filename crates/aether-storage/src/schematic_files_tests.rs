use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("aether-schem-{}-{n}", std::process::id()));
        fs::create_dir_all(path.join("schematics")).unwrap();
        Self(path)
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_schematic_is_read_by_plain_name() {
    let t = Temp::new();
    t.write("schematics/House.litematic", b"bytes");

    assert_eq!(
        read_schematic_file(&t.0, "House.litematic").unwrap(),
        b"bytes"
    );
    assert_eq!(
        read_schematic_file(&t.0, "HOUSE.LITEMATIC").is_ok(),
        cfg!(windows)
    );
}

#[test]
fn names_that_are_paths_or_not_schematics_are_refused_before_touching_the_disk() {
    let t = Temp::new();
    t.write("secret.litematic", b"secret");
    t.write("schematics/notes.txt", b"x");

    for name in [
        "",
        "../secret.litematic",
        "..\\secret.litematic",
        "a/b.nbt",
        "C:\\x.nbt",
        "notes.txt",
        "House",
        "x.schematic",
    ] {
        assert!(
            matches!(
                read_schematic_file(&t.0, name),
                Err(ManageError::InvalidName)
            ),
            "{name:?}"
        );
    }
    assert!(matches!(
        read_schematic_file(&t.0, "ghost.nbt"),
        Err(ManageError::NotFound)
    ));
}

#[test]
fn a_folder_or_an_oversized_file_is_not_read() {
    let t = Temp::new();
    fs::create_dir_all(t.0.join("schematics/dir.nbt")).unwrap();
    assert!(matches!(
        read_schematic_file(&t.0, "dir.nbt"),
        Err(ManageError::NotAllowed)
    ));

    // A sparse file larger than the cap is refused on its length, without reading it.
    let path = t.0.join("schematics/big.litematic");
    let file = fs::File::create(&path).unwrap();
    file.set_len(MAX_SCHEMATIC_FILE_BYTES + 1).unwrap();
    drop(file);
    assert!(matches!(
        read_schematic_file(&t.0, "big.litematic"),
        Err(ManageError::NotAllowed)
    ));
}

#[cfg(unix)]
#[test]
fn a_link_is_not_followed() {
    let t = Temp::new();
    t.write("outside.litematic", b"outside");
    std::os::unix::fs::symlink(
        t.0.join("outside.litematic"),
        t.0.join("schematics/link.litematic"),
    )
    .unwrap();
    assert!(matches!(
        read_schematic_file(&t.0, "link.litematic"),
        Err(ManageError::NotAllowed)
    ));
}

#[test]
fn stems_are_made_safe_and_never_empty() {
    assert_eq!(safe_stem("My House"), "My House");
    assert_eq!(safe_stem("a/b\\c:d*e?"), "a_b_c_d_e_");
    assert_eq!(safe_stem("  .. "), "schematic");
    assert_eq!(safe_stem(""), "schematic");
    assert_eq!(safe_stem("住宅 (v2)"), "住宅 (v2)");
    assert_eq!(safe_stem(&"x".repeat(500)).len(), 80);
    assert_eq!(safe_stem("trailing. "), "trailing");
}

#[test]
fn a_new_file_takes_a_free_name_and_never_overwrites() {
    let t = Temp::new();
    t.write("schematics/House.schem", b"mine");

    let first = write_new_schematic(&t.0, "House", "schem", b"one").unwrap();
    let second = write_new_schematic(&t.0, "House", "schem", b"two").unwrap();

    assert_eq!(first, "House 2.schem");
    assert_eq!(second, "House 3.schem");
    assert_eq!(
        fs::read(t.0.join("schematics/House.schem")).unwrap(),
        b"mine"
    );
    assert_eq!(
        fs::read(t.0.join("schematics/House 2.schem")).unwrap(),
        b"one"
    );
    assert_eq!(
        fs::read(t.0.join("schematics/House 3.schem")).unwrap(),
        b"two"
    );
}

#[test]
fn the_first_save_uses_the_plain_name_and_creates_the_folder() {
    let t = Temp::new();
    fs::remove_dir_all(t.0.join("schematics")).unwrap();

    assert_eq!(
        write_new_schematic(&t.0, "Tower", "nbt", b"x").unwrap(),
        "Tower.nbt"
    );
    assert!(t.0.join("schematics/Tower.nbt").is_file());
}

#[test]
fn only_schematic_extensions_can_be_written() {
    let t = Temp::new();
    for extension in ["exe", "jar", "txt", "", "../x", "LITEMATIC"] {
        assert!(
            matches!(
                write_new_schematic(&t.0, "x", extension, b"x"),
                Err(ManageError::InvalidName)
            ),
            "{extension:?}"
        );
    }
    assert_eq!(fs::read_dir(t.0.join("schematics")).unwrap().count(), 0);
}

#[test]
fn a_hostile_stem_cannot_leave_the_folder() {
    let t = Temp::new();

    let name = write_new_schematic(&t.0, "../../escape", "nbt", b"x").unwrap();

    assert!(is_plain_name(&name));
    assert!(t.0.join("schematics").join(&name).is_file());
    assert!(!t.0.join("escape.nbt").exists());
}
