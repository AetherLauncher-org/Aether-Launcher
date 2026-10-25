use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha1::{Digest, Sha1};
use zip::write::SimpleFileOptions;

use super::{
    NativeInstallArchive, NativeInstallError, install_native_archives,
    verify_installed_native_archives,
};
use crate::{ArtifactSource, DownloadArtifact};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    game: PathBuf,
    output: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let game =
            std::env::temp_dir().join(format!("aether-native-install-{}-{id}", std::process::id()));
        fs::create_dir_all(game.join("libraries")).unwrap();
        fs::create_dir_all(game.join("instances/one")).unwrap();
        let output = game.join("instances/one/natives");
        Self { game, output }
    }

    fn jar(&self, name: &str, files: &[(&str, &[u8])]) -> DownloadArtifact {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for &(entry, bytes) in files {
            writer
                .start_file(
                    entry,
                    SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        let digest = format!("{:x}", Sha1::digest(&bytes));
        let relative = PathBuf::from("libraries").join(name);
        fs::write(self.game.join(&relative), &bytes).unwrap();
        DownloadArtifact {
            destination: relative,
            size: bytes.len() as u64,
            sha1: digest,
            sources: vec![ArtifactSource {
                provider: "test".to_owned(),
                url: "https://example.invalid".to_owned(),
            }],
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.game);
    }
}

fn install(
    fixture: &Fixture,
    artifact: &DownloadArtifact,
    exclusions: &[String],
) -> Result<PathBuf, NativeInstallError> {
    install_native_archives(
        &fixture.game,
        &fixture.output,
        &[NativeInstallArchive {
            artifact,
            exclusions,
        }],
    )
}

fn assert_not_published(fixture: &Fixture) {
    assert!(!fixture.output.exists());
    let parent = fixture.output.parent().unwrap();
    assert!(fs::read_dir(parent).unwrap().flatten().all(|entry| {
        !entry
            .file_name()
            .to_string_lossy()
            .starts_with(".aether-native-stage-")
    }));
}

#[test]
fn verifies_archive_and_atomically_publishes_native_files() {
    let f = Fixture::new();
    let jar = f.jar(
        "native.jar",
        &[
            ("META-INF/MANIFEST.MF", b"metadata"),
            ("linux/liblwjgl.so", b"native"),
            ("windows/lwjgl.dll", b"windows"),
        ],
    );
    let path = install(&f, &jar, &["META-INF/".to_owned()]).unwrap();
    // Both sides must use the same Windows canonical path representation.
    // Some volumes return the verbatim `\\\\?\\` prefix from canonicalize.
    assert_eq!(path, fs::canonicalize(&f.output).unwrap());
    assert_eq!(fs::read(path.join("linux/liblwjgl.so")).unwrap(), b"native");
    assert_eq!(
        fs::read(path.join("windows/lwjgl.dll")).unwrap(),
        b"windows"
    );
    assert!(!path.join("META-INF/MANIFEST.MF").exists());
}

#[test]
fn refuses_bad_hash_or_size_without_creating_output() {
    let f = Fixture::new();
    let mut jar = f.jar("native.jar", &[("native.dll", b"good")]);
    jar.sha1 = "0000000000000000000000000000000000000000".to_owned();
    assert!(matches!(
        install(&f, &jar, &[]),
        Err(NativeInstallError::Verification(_))
    ));
    assert_not_published(&f);
    jar.size += 1;
    assert!(matches!(
        install(&f, &jar, &[]),
        Err(NativeInstallError::Verification(_))
    ));
    assert_not_published(&f);
}

#[test]
fn blocks_traversal_and_rejects_existing_native_directory() {
    let f = Fixture::new();
    let bad = f.jar("bad.jar", &[("../outside.dll", b"escape")]);
    assert!(matches!(
        install(&f, &bad, &[]),
        Err(NativeInstallError::Preflight(_))
    ));
    assert_not_published(&f);
    assert!(!f.game.join("instances/one/outside.dll").exists());

    fs::create_dir_all(&f.output).unwrap();
    fs::write(f.output.join("keep.txt"), b"unchanged").unwrap();
    let good = f.jar("good.jar", &[("native.dll", b"ok")]);
    assert!(matches!(
        install(&f, &good, &[]),
        Err(NativeInstallError::DestinationExists(_))
    ));
    assert_eq!(fs::read(f.output.join("keep.txt")).unwrap(), b"unchanged");
}

#[test]
fn rejects_duplicate_files_across_native_archives() {
    let f = Fixture::new();
    let first = f.jar("one.jar", &[("lwjgl.dll", b"one")]);
    let second = f.jar("two.jar", &[("LWJGL.dll", b"two")]);
    let result = install_native_archives(
        &f.game,
        &f.output,
        &[
            NativeInstallArchive {
                artifact: &first,
                exclusions: &[],
            },
            NativeInstallArchive {
                artifact: &second,
                exclusions: &[],
            },
        ],
    );
    assert!(matches!(
        result,
        Err(NativeInstallError::DuplicateOutput(_))
    ));
    assert_not_published(&f);
}

#[test]
fn refuses_destination_outside_the_game_directory() {
    let f = Fixture::new();
    let jar = f.jar("native.jar", &[("native.dll", b"good")]);
    let result = install_native_archives(
        &f.game,
        Path::new("/"),
        &[NativeInstallArchive {
            artifact: &jar,
            exclusions: &[],
        }],
    );
    assert!(matches!(result, Err(NativeInstallError::UnsafeDestination)));
    assert_not_published(&f);
}

#[cfg(unix)]
#[test]
fn refuses_external_symlinked_archive() {
    use std::os::unix::fs::symlink;

    let f = Fixture::new();
    let outside = f
        .game
        .parent()
        .unwrap()
        .join(format!("aether-external-{}", std::process::id()));
    fs::write(&outside, b"external").unwrap();
    let original = f.jar("native.jar", &[("native.dll", b"good")]);
    fs::remove_file(f.game.join(&original.destination)).unwrap();
    symlink(&outside, f.game.join(&original.destination)).unwrap();
    assert!(matches!(
        install(&f, &original, &[]),
        Err(NativeInstallError::Verification(_))
    ));
    assert_not_published(&f);
    fs::remove_file(outside).unwrap();
}

#[test]
fn verifies_existing_natives_and_rejects_tampering_and_extras() {
    let f = Fixture::new();
    let artifact = f.jar("native.jar", &[("native.dll", b"good")]);
    let archives = [NativeInstallArchive {
        artifact: &artifact,
        exclusions: &[],
    }];
    install_native_archives(&f.game, &f.output, &archives).unwrap();
    verify_installed_native_archives(&f.game, &f.output, &archives).unwrap();

    fs::write(f.output.join("native.dll"), b"evil").unwrap();
    assert!(matches!(
        verify_installed_native_archives(&f.game, &f.output, &archives),
        Err(NativeInstallError::InvalidInstalledFile(_))
    ));
    fs::write(f.output.join("native.dll"), b"good").unwrap();
    fs::write(f.output.join("unknown.dll"), b"untrusted").unwrap();
    assert!(matches!(
        verify_installed_native_archives(&f.game, &f.output, &archives),
        Err(NativeInstallError::UnexpectedInstalledFile(_))
    ));
}

#[cfg(unix)]
#[test]
fn rejects_symlinked_native_directory_members() {
    use std::os::unix::fs::symlink;

    let f = Fixture::new();
    let artifact = f.jar("native.jar", &[("native.dll", b"good")]);
    let archives = [NativeInstallArchive {
        artifact: &artifact,
        exclusions: &[],
    }];
    install_native_archives(&f.game, &f.output, &archives).unwrap();
    fs::remove_file(f.output.join("native.dll")).unwrap();
    symlink(
        f.game.join("libraries/native.jar"),
        f.output.join("native.dll"),
    )
    .unwrap();
    assert!(matches!(
        verify_installed_native_archives(&f.game, &f.output, &archives),
        Err(NativeInstallError::InvalidInstalledFile(_))
    ));
}
