use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::create::create_instance;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Temp(PathBuf);

impl Temp {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("aether-manage-{}-{n}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
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

fn names(entries: &[ContentEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

#[test]
fn only_one_ordinary_name_is_plain() {
    for good in ["sodium.jar", "My World", "a.b.c", "世界", "x.jar.disabled"] {
        assert!(is_plain_name(good), "{good}");
    }
    for bad in [
        "",
        ".",
        "..",
        "a/b",
        "a\\b",
        "../x",
        "/abs",
        "C:",
        "C:\\x",
        "name:stream",
        "trail.",
        "trail ",
        "ctl\u{7}",
        "a\nb",
    ] {
        assert!(!is_plain_name(bad), "{bad:?}");
    }
    assert!(!is_plain_name(&"x".repeat(256)));
    assert!(is_plain_name(&"x".repeat(255)));
}

#[test]
fn content_kinds_round_trip_through_their_labels() {
    for kind in ContentKind::ALL {
        assert_eq!(ContentKind::parse(kind.label()), Some(kind));
    }
    assert_eq!(ContentKind::parse("../mods"), None);
    assert_eq!(ContentKind::parse(""), None);
}

#[test]
fn a_missing_folder_lists_as_empty() {
    let t = Temp::new();
    for kind in ContentKind::ALL {
        assert!(list_content(&t.0, kind).unwrap().is_empty());
        assert_eq!(count_content(&t.0, kind), 0);
    }
}

#[test]
fn mods_are_jars_and_a_disabled_jar_says_so() {
    let t = Temp::new();
    t.write("mods/Sodium.jar", b"12345");
    t.write("mods/lithium.jar.disabled", b"123");
    t.write("mods/readme.txt", b"no");
    t.write("mods/folder.jar/inner.class", b"no");
    t.write("mods/odd.disabled", b"no");

    let mods = list_content(&t.0, ContentKind::Mods).unwrap();

    assert_eq!(names(&mods), ["lithium.jar.disabled", "Sodium.jar"]);
    assert!(!mods[0].enabled);
    assert!(mods[1].enabled);
    assert_eq!(mods[1].size, 5);
    assert_eq!(count_content(&t.0, ContentKind::Mods), 2);
}

#[test]
fn packs_shaders_worlds_and_schematics_follow_their_own_rules() {
    let t = Temp::new();
    t.write("resourcepacks/Faithful.zip", b"zip");
    t.write("resourcepacks/Unpacked/pack.mcmeta", b"{}");
    t.write("resourcepacks/notes.txt", b"no");
    t.write("shaderpacks/BSL.zip", b"zip");
    t.write("saves/Survival/level.dat", b"world");
    t.write("saves/Survival/region/r.0.0.mca", &[0u8; 100]);
    t.write("saves/Not a world/readme.txt", b"no");
    t.write("saves/loose-file.txt", b"no");
    t.write("schematics/House.litematic", b"1");
    t.write("schematics/Tower.SCHEM", b"1");
    t.write("schematics/readme.md", b"no");

    let packs = list_content(&t.0, ContentKind::ResourcePacks).unwrap();
    assert_eq!(names(&packs), ["Faithful.zip", "Unpacked"]);
    assert!(packs[1].is_folder && !packs[0].is_folder);
    assert_eq!(
        names(&list_content(&t.0, ContentKind::ShaderPacks).unwrap()),
        ["BSL.zip"]
    );
    let worlds = list_content(&t.0, ContentKind::Saves).unwrap();
    assert_eq!(names(&worlds), ["Survival"]);
    assert_eq!(worlds[0].size, 5 + 100);
    assert_eq!(
        names(&list_content(&t.0, ContentKind::Schematics).unwrap()),
        ["House.litematic", "Tower.SCHEM"]
    );
}

#[cfg(unix)]
#[test]
fn links_are_neither_listed_nor_followed() {
    let t = Temp::new();
    t.write("outside/secret.jar", b"secret");
    t.write("mods/real.jar", b"1");
    std::os::unix::fs::symlink(t.0.join("outside/secret.jar"), t.0.join("mods/link.jar")).unwrap();
    std::os::unix::fs::symlink(t.0.join("outside"), t.0.join("saves")).unwrap();

    assert_eq!(
        names(&list_content(&t.0, ContentKind::Mods).unwrap()),
        ["real.jar"]
    );
    assert!(matches!(
        remove_content(&t.0, ContentKind::Mods, "link.jar"),
        Err(ManageError::NotAllowed)
    ));
    assert!(t.0.join("outside/secret.jar").is_file());
}

#[test]
fn switching_a_mod_renames_it_and_refuses_to_overwrite() {
    let t = Temp::new();
    t.write("mods/a.jar", b"a");

    set_mod_enabled(&t.0, "a.jar", false).unwrap();
    assert!(!t.0.join("mods/a.jar").exists());
    assert!(t.0.join("mods/a.jar.disabled").is_file());
    // Already in that state: nothing to do.
    set_mod_enabled(&t.0, "a.jar.disabled", false).unwrap();

    set_mod_enabled(&t.0, "a.jar.disabled", true).unwrap();
    assert!(t.0.join("mods/a.jar").is_file());

    t.write("mods/b.jar", b"b");
    t.write("mods/b.jar.disabled", b"old");
    assert!(matches!(
        set_mod_enabled(&t.0, "b.jar", false),
        Err(ManageError::AlreadyExists)
    ));
    assert_eq!(fs::read(t.0.join("mods/b.jar.disabled")).unwrap(), b"old");
}

#[test]
fn only_jars_can_be_switched_and_only_by_plain_names() {
    let t = Temp::new();
    t.write("mods/notes.txt", b"x");
    t.write("outside.jar", b"x");

    assert!(matches!(
        set_mod_enabled(&t.0, "notes.txt", false),
        Err(ManageError::NotAllowed)
    ));
    assert!(matches!(
        set_mod_enabled(&t.0, "../outside.jar", false),
        Err(ManageError::InvalidName)
    ));
    assert!(matches!(
        set_mod_enabled(&t.0, "ghost.jar", false),
        Err(ManageError::NotFound)
    ));
    assert!(t.0.join("outside.jar").is_file());
}

#[test]
fn removing_deletes_a_file_or_a_whole_world_and_nothing_else() {
    let t = Temp::new();
    t.write("mods/a.jar", b"a");
    t.write("mods/b.jar", b"b");
    t.write("saves/Survival/level.dat", b"w");
    t.write("saves/Survival/region/r.0.0.mca", b"r");
    t.write("saves/Creative/level.dat", b"w");

    remove_content(&t.0, ContentKind::Mods, "a.jar").unwrap();
    remove_content(&t.0, ContentKind::Saves, "Survival").unwrap();

    assert!(!t.0.join("mods/a.jar").exists());
    assert!(t.0.join("mods/b.jar").is_file());
    assert!(!t.0.join("saves/Survival").exists());
    assert!(t.0.join("saves/Creative/level.dat").is_file());
}

#[test]
fn removing_refuses_names_that_could_leave_the_folder() {
    let t = Temp::new();
    t.write("mods/a.jar", b"a");
    t.write("keep.txt", b"keep");

    for name in [
        "",
        ".",
        "..",
        "../keep.txt",
        "..\\keep.txt",
        "mods/../keep.txt",
        "C:\\x",
    ] {
        assert!(
            matches!(
                remove_content(&t.0, ContentKind::Mods, name),
                Err(ManageError::InvalidName)
            ),
            "{name:?}"
        );
    }
    assert!(matches!(
        remove_content(&t.0, ContentKind::Mods, "ghost.jar"),
        Err(ManageError::NotFound)
    ));
    assert!(t.0.join("keep.txt").is_file());
    assert!(t.0.join("mods/a.jar").is_file());
}

#[test]
fn directory_size_counts_files_and_ignores_missing_paths() {
    let t = Temp::new();
    t.write("d/a", &[0u8; 10]);
    t.write("d/sub/b", &[0u8; 5]);
    assert_eq!(directory_size(&t.0.join("d")), 15);
    assert_eq!(directory_size(&t.0.join("absent")), 0);
}

#[test]
fn deleting_an_instance_removes_only_its_directory() {
    let t = Temp::new();
    t.write("game/versions/1.21.1/1.21.1.json", b"{}");
    let first = create_instance(&t.0, "First", "1.21.1").unwrap();
    let second = create_instance(&t.0, "Second", "1.21.1").unwrap();
    fs::write(first.root.join("minecraft/options.txt"), b"x").unwrap();

    delete_instance(&t.0, first.id).unwrap();

    assert!(!first.root.exists());
    assert!(second.root.join("instance.json").is_file());
    assert!(t.0.join("game/versions/1.21.1/1.21.1.json").is_file());
    // Gone: a second delete fails instead of doing something else.
    assert!(delete_instance(&t.0, first.id).is_err());
}

#[test]
fn deleting_an_unknown_instance_touches_nothing() {
    let t = Temp::new();
    let kept = create_instance(&t.0, "Kept", "1.21.1").unwrap();

    assert!(delete_instance(&t.0, InstanceId::new()).is_err());
    assert!(kept.root.join("instance.json").is_file());
}

#[test]
fn logs_list_the_latest_and_the_crash_reports_newest_first() {
    let t = Temp::new();
    t.write("logs/latest.log", b"hello");
    t.write("logs/debug.log", b"no");
    t.write("crash-reports/crash-a.txt", b"a");
    t.write("crash-reports/crash-b.txt", b"b");
    t.write("crash-reports/other.dat", b"no");

    let files = list_logs(&t.0).unwrap();

    assert_eq!(files[0].source, LogSource::Latest);
    assert_eq!(files[0].size, 5);
    let crashes: Vec<&str> = files[1..].iter().map(|file| file.name.as_str()).collect();
    assert_eq!(crashes.len(), 2);
    assert!(crashes.contains(&"crash-a.txt") && crashes.contains(&"crash-b.txt"));
    assert!(list_logs(&Temp::new().0).unwrap().is_empty());
}

#[test]
fn a_short_log_is_returned_whole() {
    let t = Temp::new();
    t.write("logs/latest.log", "line one\nline two\n".as_bytes());

    let log = read_log(&t.0, &LogSource::Latest).unwrap();

    assert_eq!(log.text, "line one\nline two\n");
    assert!(!log.truncated);
}

#[test]
fn a_long_log_is_cut_from_the_start_at_a_line_boundary() {
    let t = Temp::new();
    let mut text = String::new();
    let mut index = 0;
    while (text.len() as u64) < MAX_LOG_BYTES * 2 {
        text.push_str(&format!("line {index:08}\n"));
        index += 1;
    }
    t.write("logs/latest.log", text.as_bytes());

    let log = read_log(&t.0, &LogSource::Latest).unwrap();

    assert!(log.truncated);
    assert!(log.text.len() as u64 <= MAX_LOG_BYTES);
    assert!(
        log.text.starts_with("line "),
        "starts mid-line: {:?}",
        &log.text[..20]
    );
    assert!(log.text.ends_with(&format!("line {:08}\n", index - 1)));
}

#[test]
fn log_bytes_that_are_not_utf8_are_replaced_not_fatal() {
    let t = Temp::new();
    t.write("logs/latest.log", &[b'o', b'k', 0xff, 0xfe, b'\n']);

    let log = read_log(&t.0, &LogSource::Latest).unwrap();

    assert!(log.text.starts_with("ok"));
}

#[test]
fn a_crash_report_is_read_by_plain_name_only() {
    let t = Temp::new();
    t.write("crash-reports/crash-a.txt", b"boom");
    t.write("secret.txt", b"secret");

    let report = read_log(&t.0, &LogSource::CrashReport("crash-a.txt".to_owned())).unwrap();
    assert_eq!(report.text, "boom");

    for name in [
        "../secret.txt",
        "..\\secret.txt",
        "crash-a.dat",
        "",
        "sub/crash-a.txt",
    ] {
        assert!(
            matches!(
                read_log(&t.0, &LogSource::CrashReport(name.to_owned())),
                Err(ManageError::InvalidName)
            ),
            "{name:?}"
        );
    }
    assert!(matches!(
        read_log(&t.0, &LogSource::CrashReport("ghost.txt".to_owned())),
        Err(ManageError::NotFound)
    ));
    assert!(matches!(
        read_log(&t.0, &LogSource::Latest),
        Err(ManageError::NotFound)
    ));
}
