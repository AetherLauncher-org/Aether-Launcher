pub mod arguments;
pub mod catalog;
mod inherit;
pub mod loader;
pub mod rules;

pub use loader::{
    ResolvedVersionManifest, VersionLoadError, load_resolved_version_manifest,
    load_version_manifest,
};

use std::collections::BTreeMap;

use aether_core::VersionId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::arguments::Arguments;
use crate::rules::{Environment, Rule, rules_allow};

/// 版本清单里的单个可下载文件（客户端 jar 等）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDownload {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VersionDownloads {
    #[serde(default)]
    pub client: Option<FileDownload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersionRequirement {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryArtifact {
    /// 相对于 libraries 目录的路径
    pub path: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<LibraryArtifact>,
    #[serde(default)]
    pub classifiers: BTreeMap<String, LibraryArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LibraryExtract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    /// Maven 坐标，例如 `group:artifact:version`
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    /// 没有 `downloads` 的加载器库只给 Maven 仓库地址
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Operating system to native classifier mapping, e.g. windows -> natives-windows-${arch}.
    #[serde(default)]
    pub natives: BTreeMap<String, String>,
    #[serde(default)]
    pub extract: Option<LibraryExtract>,
}

impl Library {
    pub fn is_allowed(&self, environment: &Environment) -> bool {
        rules_allow(&self.rules, environment)
    }
}

/// 版本清单。继承合并见 [`VersionManifest::merge_with_parent`]。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    pub id: String,
    #[serde(default)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub main_class: Option<String>,
    #[serde(default)]
    pub assets: Option<String>,
    #[serde(default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub downloads: Option<VersionDownloads>,
    #[serde(default)]
    pub java_version: Option<JavaVersionRequirement>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    /// 1.13 之后的结构化参数
    #[serde(default)]
    pub arguments: Option<Arguments>,
    /// 1.12.2 及更早的单行游戏参数
    #[serde(default)]
    pub minecraft_arguments: Option<String>,
}

impl VersionManifest {
    pub fn version_id(&self) -> Result<VersionId, ManifestError> {
        VersionId::parse(self.id.clone()).map_err(|_| ManifestError::InvalidVersionId)
    }

    /// 当前环境下需要的库，保持清单里的顺序。
    pub fn active_libraries(&self, environment: &Environment) -> Vec<&Library> {
        self.libraries
            .iter()
            .filter(|library| library.is_allowed(environment))
            .collect()
    }
}

pub fn parse_version_manifest(input: &str) -> Result<VersionManifest, ManifestError> {
    serde_json::from_str(input).map_err(ManifestError::InvalidJson)
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("version manifest contains invalid JSON")]
    InvalidJson(#[source] serde_json::Error),
    #[error("version manifest contains an invalid version identifier")]
    InvalidVersionId,
}

#[cfg(test)]
mod tests {
    use crate::rules::Environment;

    use super::{Library, parse_version_manifest};

    const VANILLA: &str = r#"
    {
      "id": "1.21.1",
      "type": "release",
      "mainClass": "net.minecraft.client.main.Main",
      "assets": "17",
      "assetIndex": {
        "id": "17",
        "sha1": "1111111111111111111111111111111111111111",
        "size": 450000,
        "totalSize": 800000000,
        "url": "https://example.invalid/assets/17.json"
      },
      "downloads": {
        "client": {
          "sha1": "2222222222222222222222222222222222222222",
          "size": 26000000,
          "url": "https://example.invalid/client.jar"
        },
        "server": {
          "sha1": "3333333333333333333333333333333333333333",
          "size": 1,
          "url": "https://example.invalid/server.jar"
        }
      },
      "javaVersion": {"component": "java-runtime-delta", "majorVersion": 21},
      "libraries": [
        {
          "name": "com.example:common:1.0",
          "downloads": {
            "artifact": {
              "path": "com/example/common/1.0/common-1.0.jar",
              "sha1": "4444444444444444444444444444444444444444",
              "size": 10,
              "url": "https://example.invalid/common-1.0.jar"
            }
          }
        },
        {
          "name": "com.example:native:1.0:natives-windows",
          "rules": [{"action": "allow", "os": {"name": "windows"}}],
          "downloads": {
            "artifact": {
              "path": "com/example/native/1.0/native-1.0-natives-windows.jar",
              "sha1": "5555555555555555555555555555555555555555",
              "size": 20,
              "url": "https://example.invalid/native-windows.jar"
            }
          }
        }
      ],
      "arguments": {
        "game": ["--version", "${version_name}"],
        "jvm": ["-cp", "${classpath}"]
      }
    }
    "#;

    fn environment(os_name: &str) -> Environment {
        Environment {
            os_name: os_name.to_owned(),
            ..Environment::default()
        }
    }

    fn names(libraries: &[&Library]) -> Vec<String> {
        let mut names = Vec::new();
        for library in libraries {
            names.push(library.name.clone());
        }
        names
    }

    #[test]
    fn parses_minimal_manifest() {
        let manifest = parse_version_manifest(r#"{"id":"1.21.1"}"#).unwrap();
        assert_eq!(manifest.id, "1.21.1");
        assert!(manifest.libraries.is_empty());
        assert!(manifest.arguments.is_none());
    }

    #[test]
    fn parses_a_modern_vanilla_manifest() {
        let manifest = parse_version_manifest(VANILLA).unwrap();

        let main_class = manifest.main_class.as_deref();
        assert_eq!(main_class, Some("net.minecraft.client.main.Main"));
        assert_eq!(manifest.assets.as_deref(), Some("17"));
        assert_eq!(manifest.libraries.len(), 2);

        let index = manifest.asset_index.unwrap();
        assert_eq!(index.id, "17");
        assert_eq!(index.size, 450_000);

        let java = manifest.java_version.unwrap();
        assert_eq!(java.component, "java-runtime-delta");
        assert_eq!(java.major_version, 21);

        let client = manifest.downloads.unwrap().client.unwrap();
        assert_eq!(client.url, "https://example.invalid/client.jar");

        let arguments = manifest.arguments.unwrap();
        assert_eq!(arguments.game.len(), 2);
        assert_eq!(arguments.jvm.len(), 2);
    }

    #[test]
    fn libraries_are_filtered_by_their_rules() {
        let manifest = parse_version_manifest(VANILLA).unwrap();

        let windows = manifest.active_libraries(&environment("windows"));
        let expected = [
            "com.example:common:1.0",
            "com.example:native:1.0:natives-windows",
        ];
        assert_eq!(names(&windows), expected);

        let linux = manifest.active_libraries(&environment("linux"));
        assert_eq!(names(&linux), ["com.example:common:1.0"]);
    }

    #[test]
    fn parses_loader_libraries_that_only_have_a_repository_url() {
        let json = r#"
        {
          "id": "fabric-loader-0.16.0-1.21.1",
          "inheritsFrom": "1.21.1",
          "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
          "libraries": [
            {"name": "net.fabricmc:fabric-loader:0.16.0", "url": "https://maven.example.invalid/"}
          ]
        }
        "#;

        let manifest = parse_version_manifest(json).unwrap();

        assert_eq!(manifest.inherits_from.as_deref(), Some("1.21.1"));
        let library = &manifest.libraries[0];
        assert!(library.downloads.is_none());
        let repository = library.url.as_deref();
        assert_eq!(repository, Some("https://maven.example.invalid/"));
    }

    #[test]
    fn parses_legacy_single_line_arguments() {
        let json = r#"{"id":"1.12.2","minecraftArguments":"--username ${auth_player_name}"}"#;

        let manifest = parse_version_manifest(json).unwrap();

        let legacy = manifest.minecraft_arguments.as_deref();
        assert_eq!(legacy, Some("--username ${auth_player_name}"));
        assert!(manifest.arguments.is_none());
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(parse_version_manifest("{").is_err());
    }
}
