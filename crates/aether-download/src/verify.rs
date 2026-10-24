//! Validate an already-downloaded artifact. No network access or file writes.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};
use thiserror::Error;

use crate::{DownloadArtifact, is_safe_relative_path};

#[derive(Debug, Error)]
pub enum ArtifactVerificationError {
    #[error("invalid artifact relative path: {0:?}")]
    UnsafePath(PathBuf),
    #[error("artifact resolved outside the game directory: {0:?}")]
    EscapedPath(PathBuf),
    #[error("artifact is not a regular file: {0:?}")]
    NotAFile(PathBuf),
    #[error("file size mismatch at {path:?}: expected {expected}, received {actual}")]
    SizeMismatch { path: PathBuf, expected: u64, actual: u64 },
    #[error("file SHA-1 mismatch: {0:?}")]
    HashMismatch(PathBuf),
    #[error("I/O error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Verify the declared size and SHA-1 before opening an artifact as a ZIP.
/// Return the canonical path to prevent following links outside the game tree.
pub fn verify_local_artifact(
    game_directory: &Path,
    artifact: &DownloadArtifact,
) -> Result<PathBuf, ArtifactVerificationError> {
    if !is_safe_relative_path(&artifact.destination) {
        return Err(ArtifactVerificationError::UnsafePath(artifact.destination.clone()));
    }
    let game_root = fs::canonicalize(game_directory).map_err(|source| ArtifactVerificationError::Io {
        path: game_directory.to_owned(),
        source,
    })?;
    let file_path = game_root.join(&artifact.destination);
    let file_path = fs::canonicalize(&file_path).map_err(|source| ArtifactVerificationError::Io {
        path: file_path,
        source,
    })?;
    if !file_path.starts_with(&game_root) {
        return Err(ArtifactVerificationError::EscapedPath(file_path));
    }
    let metadata = fs::metadata(&file_path).map_err(|source| ArtifactVerificationError::Io {
        path: file_path.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(ArtifactVerificationError::NotAFile(file_path));
    }
    if metadata.len() != artifact.size {
        return Err(ArtifactVerificationError::SizeMismatch {
            path: file_path,
            expected: artifact.size,
            actual: metadata.len(),
        });
    }
    let mut input = File::open(&file_path).map_err(|source| ArtifactVerificationError::Io {
        path: file_path.clone(),
        source,
    })?;
    let mut digest = Sha1::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = input.read(&mut buffer).map_err(|source| ArtifactVerificationError::Io {
            path: file_path.clone(),
            source,
        })?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    let actual = format!("{:x}", digest.finalize());
    if !actual.eq_ignore_ascii_case(&artifact.sha1) {
        return Err(ArtifactVerificationError::HashMismatch(file_path));
    }
    Ok(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArtifactSource;

    #[test]
    fn accepts_known_sha1_and_rejects_different_size_or_digest() {
        let root = std::env::temp_dir().join(format!("aether-verify-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let name = format!("test-{:?}.txt", std::thread::current().id());
        fs::write(root.join(&name), b"abc").unwrap();

        let mut artifact = DownloadArtifact {
            destination: PathBuf::from(&name),
            size: 3,
            sha1: "a9993e364706816aba3e25717850c26c9cd0d89d".to_owned(),
            sources: vec![ArtifactSource {
                provider: "fixture".to_owned(),
                url: "https://example.invalid".to_owned(),
            }],
        };
        assert!(verify_local_artifact(&root, &artifact).is_ok());
        artifact.size = 9;
        assert!(matches!(
            verify_local_artifact(&root, &artifact),
            Err(ArtifactVerificationError::SizeMismatch { .. })
        ));
        artifact.size = 3;
        artifact.sha1 = "0000000000000000000000000000000000000000".to_owned();
        assert!(matches!(
            verify_local_artifact(&root, &artifact),
            Err(ArtifactVerificationError::HashMismatch(_))
        ));
        fs::remove_file(root.join(name)).unwrap();
    }
}
