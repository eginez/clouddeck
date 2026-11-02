use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "extension.toml";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub schema_version: u32,
    pub api_version: u32,
    #[serde(default)]
    pub authors: Vec<String>,
    pub repository: Option<String>,
    /// File inside the extension directory; defaults to `lib<id>` with the platform's library suffix.
    pub library: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderDecl>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProviderDecl {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Capability {
    #[serde(rename = "network:http")]
    NetworkHttp { hosts: Vec<String> },
    #[serde(rename = "process:exec")]
    ProcessExec {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    #[serde(rename = "download_file")]
    DownloadFile { url: Vec<String> },
    #[serde(rename = "credentials")]
    Credentials,
    #[serde(rename = "secrets")]
    Secrets,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let manifest: Self =
            toml::from_str(text).map_err(|error| format!("invalid {MANIFEST_FILE}: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), String> {
        if !is_valid_id(&self.id) {
            return Err(format!(
                "extension id {:?} may only contain lowercase letters, digits, '-' and '_'",
                self.id
            ));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "{} uses manifest schema {}; this CloudDeck reads schema {SCHEMA_VERSION}",
                self.id, self.schema_version
            ));
        }
        Ok(())
    }

    pub fn check_api(&self, host_api: u32, host_version: &str) -> Result<(), String> {
        if self.api_version == host_api {
            return Ok(());
        }
        Err(format!(
            "{} was built for extension API {} but this CloudDeck {host_version} uses API {host_api}. Rebuild {} for CloudDeck {host_version}.",
            self.id, self.api_version, self.id
        ))
    }

    pub fn library_path(&self, dir: &Path) -> Result<PathBuf, String> {
        let file = self
            .library
            .clone()
            .unwrap_or_else(|| default_library_file(&self.id));
        let relative = Path::new(&file);
        let stays_inside = !file.is_empty()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !stays_inside {
            return Err(format!(
                "library {file:?} must be a file name inside the extension directory"
            ));
        }
        let dir = dir
            .canonicalize()
            .map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
        if fs::symlink_metadata(dir.join(relative)).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!(
                "library {file:?} is a symlink; copy the library into the extension directory instead"
            ));
        }
        let path = dir
            .join(relative)
            .canonicalize()
            .map_err(|error| format!("cannot find library {file:?}: {error}"))?;
        if !path.starts_with(&dir) {
            return Err(format!(
                "library {file:?} resolves outside the extension directory"
            ));
        }
        Ok(path)
    }
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn default_library_file(id: &str) -> String {
    let stem = format!("lib{}", id.replace('-', "_"));
    if cfg!(target_os = "macos") {
        format!("{stem}.dylib")
    } else {
        format!("{stem}.so")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
id = "toy"
name = "Toy"
version = "0.1.0"
schema_version = 1
api_version = 1
authors = ["Esteban"]

[providers.toy]
name = "Toy cloud"

[[capabilities]]
kind = "network:http"
hosts = ["api.example.com"]

[[capabilities]]
kind = "secrets"
"#;

    #[test]
    fn parses_providers_and_capabilities() {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        assert_eq!(manifest.id, "toy");
        assert_eq!(manifest.providers["toy"].name, "Toy cloud");
        assert_eq!(
            manifest.capabilities,
            vec![
                Capability::NetworkHttp {
                    hosts: vec!["api.example.com".into()]
                },
                Capability::Secrets,
            ]
        );
    }

    #[test]
    fn unknown_capability_kind_is_refused() {
        let text = MANIFEST.replace("kind = \"secrets\"", "kind = \"root:everything\"");
        assert!(Manifest::parse(&text).is_err());
    }

    #[test]
    fn api_mismatch_asks_for_rebuild() {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        let message = manifest.check_api(2, "0.3.0").unwrap_err();
        assert!(
            message.contains("Rebuild toy for CloudDeck 0.3.0"),
            "{message}"
        );
    }

    #[test]
    fn library_path_stays_inside_extension_directory() {
        let manifest = Manifest::parse(&MANIFEST.replace(
            "[providers.toy]",
            "library = \"../escape.so\"\n[providers.toy]",
        ))
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(manifest.library_path(dir.path()).is_err());
    }

    #[test]
    fn default_library_name_follows_id() {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(default_library_file("toy")), b"").unwrap();
        let path = manifest.library_path(dir.path()).unwrap();
        assert!(path.ends_with(default_library_file("toy")));
    }

    #[test]
    fn rejects_ids_that_are_not_plain_names() {
        let text = MANIFEST.replace("id = \"toy\"", "id = \"../toy\"");
        assert!(Manifest::parse(&text).is_err());
    }
}
