pub mod assets;
pub mod native_install;
pub mod native_zip;
pub mod verify;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod engine;
pub mod hash;
pub mod mirrors;
pub mod selector;
pub mod transport;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSource {
    pub provider: String,
    pub url: String,
}

/// 一个待下载的文件。`destination` 是相对于实例或游戏目录的路径，不能是绝对路径，
/// 也不能含 `..`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadArtifact {
    pub destination: PathBuf,
    pub size: u64,
    pub sha1: String,
    pub sources: Vec<ArtifactSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DownloadPlan {
    pub artifacts: Vec<DownloadArtifact>,
}

impl DownloadPlan {
    pub fn validate(&self) -> Result<(), DownloadPlanError> {
        let mut destinations = HashSet::new();
        for artifact in &self.artifacts {
            let destination = &artifact.destination;
            if !is_safe_relative_path(destination) {
                return Err(DownloadPlanError::UnsafeDestination(destination.clone()));
            }
            if !destinations.insert(destination_key(destination)) {
                return Err(DownloadPlanError::DuplicateDestination(destination.clone()));
            }
            if artifact.sources.is_empty() {
                return Err(DownloadPlanError::NoSource(destination.clone()));
            }
            if !is_sha1(&artifact.sha1) {
                return Err(DownloadPlanError::InvalidSha1(destination.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DownloadPlanError {
    #[error("artifact destination {0:?} is not a safe relative path")]
    UnsafeDestination(PathBuf),
    #[error("more than one artifact is written to {0:?}")]
    DuplicateDestination(PathBuf),
    #[error("artifact {0:?} has no download source")]
    NoSource(PathBuf),
    #[error("artifact {0:?} has an invalid SHA-1 digest")]
    InvalidSha1(PathBuf),
}

pub(crate) fn is_sha1(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|c| c.is_ascii_hexdigit())
}

/// Windows 上路径不区分大小写，`/` 和 `\` 等价，比较重复目标时统一处理。
fn destination_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}

/// 与平台无关的检查：同时把 `/` 和 `\` 当作分隔符，
/// 这样在 Linux 上通过的路径换到 Windows 上也不会越出目录。
pub fn is_safe_relative_path(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    !text.is_empty() && text.split(['/', '\\']).all(is_safe_segment)
}

fn is_safe_segment(segment: &str) -> bool {
    if segment.is_empty() || segment == "." || segment == ".." {
        return false;
    }
    // Windows 会去掉末尾的点和空格，冒号表示盘符或数据流
    if segment.contains([':', '<', '>', '"', '|', '?', '*']) || segment.ends_with(['.', ' ']) {
        return false;
    }
    if segment.chars().any(char::is_control) {
        return false;
    }
    let stem = segment.split('.').next().unwrap_or(segment);
    !is_windows_reserved(stem)
}

fn is_windows_reserved(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let numbered = upper.strip_prefix("COM").or(upper.strip_prefix("LPT"));
    numbered.is_some_and(|digit| matches!(digit.as_bytes(), [b'1'..=b'9']))
}

// 末尾的 re-export 属于集成要求，触发 items_after_test_module 时在此豁免。
#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use std::path::PathBuf;

    use super::{ArtifactSource, DownloadArtifact, DownloadPlan, DownloadPlanError};

    const SHA1: &str = "0123456789abcdef0123456789abcdef01234567";

    fn artifact(destination: &str) -> DownloadArtifact {
        DownloadArtifact {
            destination: PathBuf::from(destination),
            size: 1,
            sha1: SHA1.to_owned(),
            sources: vec![ArtifactSource {
                provider: "mojang".to_owned(),
                url: "https://example.invalid/file".to_owned(),
            }],
        }
    }

    fn plan(destinations: &[&str]) -> DownloadPlan {
        DownloadPlan {
            artifacts: destinations.iter().copied().map(artifact).collect(),
        }
    }

    #[test]
    fn accepts_relative_destinations() {
        let valid = plan(&["libraries/a.jar", "versions/b.jar"]);
        assert_eq!(valid.validate(), Ok(()));
    }

    #[test]
    fn accepts_an_empty_plan() {
        assert_eq!(DownloadPlan::default().validate(), Ok(()));
    }

    #[test]
    fn rejects_unsafe_destinations() {
        for destination in [
            "",
            "/etc/passwd",
            "\\windows\\system32",
            "C:\\Users\\a.jar",
            "../outside.jar",
            "libraries/../../outside.jar",
            "libraries\\..\\..\\outside.jar",
            "libraries//a.jar",
            "libraries/",
            "./a.jar",
            "a.jar:stream",
            "libraries/CON.jar",
            "libraries/com1",
            "trailing-dot./a.jar",
        ] {
            let result = plan(&[destination]).validate();
            let expected = DownloadPlanError::UnsafeDestination(PathBuf::from(destination));
            assert_eq!(result, Err(expected), "{destination:?}");
        }
    }

    #[test]
    fn rejects_duplicate_destinations_ignoring_case_and_separators() {
        let result = plan(&["libraries/a.jar", "Libraries\\A.jar"]).validate();
        let duplicate = PathBuf::from("Libraries\\A.jar");
        let expected = DownloadPlanError::DuplicateDestination(duplicate);
        assert_eq!(result, Err(expected));
    }

    #[test]
    fn rejects_artifacts_without_sources() {
        let mut broken = plan(&["libraries/a.jar"]);
        broken.artifacts[0].sources.clear();
        let expected = DownloadPlanError::NoSource(PathBuf::from("libraries/a.jar"));
        assert_eq!(broken.validate(), Err(expected));
    }

    #[test]
    fn rejects_malformed_sha1() {
        let mut broken = plan(&["libraries/a.jar"]);
        broken.artifacts[0].sha1 = "not-a-digest".to_owned();
        let expected = DownloadPlanError::InvalidSha1(PathBuf::from("libraries/a.jar"));
        assert_eq!(broken.validate(), Err(expected));
    }
}

// 按“集成要求”固定在文件末尾；`items_after_test_module` 在此显式豁免。
pub use engine::{
    ArtifactResult, AttemptError, CancellationToken, DownloadEngine, DownloadError, NoopProgress,
    ProgressSink, RetryPolicy, SourceFailure,
};
pub use hash::{HashMismatch, HashingWriter, VerifyError, verify_reader};
pub use mirrors::{
    BMCLAPI_BASE, BMCLAPI_PROVIDER, bmclapi_mirror, expand_plan_with_mirrors,
    fetch_first_available, with_mirrors,
};
pub use selector::{ProviderStat, degraded_weight, effective_weight, order_sources};
pub use transport::{HttpTransport, Range, TransportError, TransportRequest, TransportResponse};
