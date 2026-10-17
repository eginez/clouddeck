use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsProfile {
    pub name: String,
    pub region: Option<String>,
}

/// The non-secret portion of an IAM Identity Center profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsSsoConfiguration {
    pub start_url: String,
    pub region: String,
    pub account_id: Option<String>,
    pub role_name: Option<String>,
    pub session_name: Option<String>,
}

impl AwsSsoConfiguration {
    pub fn is_configured(&self) -> bool {
        !self.start_url.trim().is_empty() && !self.region.trim().is_empty()
    }
}

/// How the AWS SDK is expected to resolve credentials for a profile. Secret
/// values are deliberately not represented by this type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AwsCredentialKind {
    SharedCredentials,
    Sso(AwsSsoConfiguration),
    /// `credential_process` is reported so the app can explain that an
    /// external provider is configured. CloudDeck does not invoke a process.
    ExternalProvider,
    Unconfigured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsProfileAuth {
    pub name: String,
    pub region: Option<String>,
    pub credential_kind: AwsCredentialKind,
}

#[derive(Default)]
struct ProfileSection {
    values: BTreeMap<String, String>,
}

fn parse_sections(text: &str, config_file: bool) -> BTreeMap<String, ProfileSection> {
    let mut sections: BTreeMap<String, ProfileSection> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            let section = line[1..line.len() - 1].trim();
            let name = if config_file {
                if section == "default" {
                    Some(section.to_owned())
                } else {
                    section
                        .strip_prefix("profile ")
                        .map(str::trim)
                        .map(str::to_owned)
                }
            } else if section == "default"
                || (!section.starts_with("sso-session ") && !section.starts_with("services "))
            {
                Some(
                    section
                        .strip_prefix("profile ")
                        .unwrap_or(section)
                        .trim()
                        .to_owned(),
                )
            } else {
                None
            };
            current = name.filter(|name| !name.is_empty());
            if let Some(name) = current.as_ref() {
                sections.entry(name.clone()).or_default();
            }
            continue;
        }
        let Some(name) = current.as_ref() else {
            continue;
        };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value
            .split_once('#')
            .map_or(value, |(value, _)| value)
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim();
        if !value.is_empty() {
            sections
                .entry(name.clone())
                .or_default()
                .values
                .insert(key.trim().to_ascii_lowercase(), value.to_owned());
        }
    }

    // Config-only SSO sessions have names that intentionally do not appear as
    // profiles in the output. Keep them in a separate namespace by reparsing
    // the section headers here.
    if config_file {
        let mut session = None;
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') && line.ends_with(']') {
                let section = line[1..line.len() - 1].trim();
                session = section
                    .strip_prefix("sso-session ")
                    .map(str::trim)
                    .filter(|name| !name.is_empty())
                    .map(|name| format!("sso-session {name}"));
                if let Some(name) = session.as_ref() {
                    sections.entry(name.clone()).or_default();
                }
                continue;
            }
            let Some(name) = session.as_ref() else {
                continue;
            };
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value
                .split_once('#')
                .map_or(value, |(value, _)| value)
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            if !value.is_empty() {
                sections
                    .entry(name.clone())
                    .or_default()
                    .values
                    .insert(key.trim().to_ascii_lowercase(), value.to_owned());
            }
        }
    }
    sections
}

/// Parses profile metadata and SSO configuration without retaining secrets.
pub fn parse_profile_auth(config: &str, credentials: &str) -> Vec<AwsProfileAuth> {
    let config_sections = parse_sections(config, true);
    let credential_sections = parse_sections(credentials, false);
    let mut names: BTreeMap<String, ()> = BTreeMap::new();
    names.extend(
        config_sections
            .keys()
            .filter(|name| !name.starts_with("sso-session "))
            .map(|name| (name.clone(), ())),
    );
    names.extend(credential_sections.keys().map(|name| (name.clone(), ())));

    names
        .into_keys()
        .map(|name| {
            let config_values = config_sections.get(&name).map(|s| &s.values);
            let credential_values = credential_sections.get(&name).map(|s| &s.values);
            let value = |key: &str| {
                config_values
                    .and_then(|values| values.get(key))
                    .or_else(|| credential_values.and_then(|values| values.get(key)))
                    .cloned()
            };
            let region = value("region");
            let external = value("credential_process").is_some();
            let direct_start_url = value("sso_start_url");
            let direct_sso_region = value("sso_region");
            let session_name = value("sso_session");
            let session_values = session_name
                .as_deref()
                .and_then(|session| config_sections.get(&format!("sso-session {session}")))
                .map(|section| &section.values);
            let start_url = direct_start_url
                .or_else(|| session_values.and_then(|values| values.get("sso_start_url").cloned()));
            let sso_region = direct_sso_region
                .or_else(|| session_values.and_then(|values| values.get("sso_region").cloned()));
            let account_id = value("sso_account_id");
            let role_name = value("sso_role_name");
            let credential_kind = match (start_url, sso_region) {
                (Some(start_url), Some(sso_region)) => {
                    AwsCredentialKind::Sso(AwsSsoConfiguration {
                        start_url,
                        region: sso_region,
                        account_id,
                        role_name,
                        session_name,
                    })
                }
                _ if external => AwsCredentialKind::ExternalProvider,
                _ if credential_values.is_some() => AwsCredentialKind::SharedCredentials,
                _ => AwsCredentialKind::Unconfigured,
            };
            AwsProfileAuth {
                name,
                region,
                credential_kind,
            }
        })
        .collect()
}

pub fn load_profile_auth() -> std::io::Result<Vec<AwsProfileAuth>> {
    fn read(path: PathBuf) -> std::io::Result<String> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e),
        }
    }
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".aws");
    let config = std::env::var_os("AWS_CONFIG_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("config"));
    let credentials = std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("credentials"));
    Ok(parse_profile_auth(&read(config)?, &read(credentials)?))
}

/// Only profile names and region metadata are retained; secret values are ignored.
pub fn parse_profiles(config: &str, credentials: &str) -> Vec<AwsProfile> {
    let mut profiles: BTreeMap<String, Option<String>> = BTreeMap::new();
    for (text, is_config) in [(credentials, false), (config, true)] {
        let mut current = None;
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') && line.ends_with(']') {
                let section = line[1..line.len() - 1].trim();
                current = if is_config {
                    if section == "default" {
                        Some(section)
                    } else {
                        section.strip_prefix("profile ").map(str::trim)
                    }
                } else if section == "default"
                    || (!section.starts_with("sso-session ") && !section.starts_with("services "))
                {
                    // Credentials files use bare profile names. Accept the
                    // optional `profile ` spelling too, while ignoring
                    // config-only metadata sections if a caller passes a
                    // combined or unusual file here.
                    Some(section.strip_prefix("profile ").unwrap_or(section).trim())
                } else {
                    None
                }
                .filter(|s| !s.is_empty());
                if let Some(name) = current {
                    profiles.entry(name.into()).or_default();
                }
            } else if let (Some(name), Some((key, value))) = (current, line.split_once('='))
                && key.trim() == "region"
                && !value.trim().is_empty()
            {
                let value = value
                    .split_once('#')
                    .map_or(value, |(value, _)| value)
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !value.is_empty() {
                    profiles.insert(name.into(), Some(value.into()));
                }
            }
        }
    }
    profiles
        .into_iter()
        .map(|(name, region)| AwsProfile { name, region })
        .collect()
}

pub fn load_profiles() -> std::io::Result<Vec<AwsProfile>> {
    fn read(path: PathBuf) -> std::io::Result<String> {
        match std::fs::read_to_string(path) {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(e),
        }
    }
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".aws");
    let config = std::env::var_os("AWS_CONFIG_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("config"));
    let credentials = std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("credentials"));
    Ok(parse_profiles(&read(config)?, &read(credentials)?))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceContext {
    pub profile: String,
    pub region: String,
}
impl WorkspaceContext {
    pub fn load(path: &Path) -> Option<Self> {
        let context: Self = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        context.is_usable().then_some(context)
    }

    pub fn is_usable(&self) -> bool {
        !self.profile.trim().is_empty() && !self.region.trim().is_empty()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if !self.is_usable() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "AWS profile and region must be non-empty",
            ));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("context.json");
        let temp = path.with_file_name(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::File::create(&temp)?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec(self).map_err(std::io::Error::other)?)?;
        file.sync_all()?;
        std::fs::rename(temp, path)
    }
}

pub fn context_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".config")
        });
    base.join("clouddeck/context.json")
}

pub const FLEET_REGIONS: &[&str] = &["us-east-1", "us-west-2", "eu-west-1", "eu-central-1"];
pub const REGIONS: &[&str] = &[
    "fleet",
    "us-east-1",
    "us-east-2",
    "us-west-1",
    "us-west-2",
    "ca-central-1",
    "eu-west-1",
    "eu-west-2",
    "eu-west-3",
    "eu-central-1",
    "eu-north-1",
    "ap-southeast-1",
    "ap-southeast-2",
    "ap-northeast-1",
    "ap-northeast-2",
    "ap-south-1",
    "sa-east-1",
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_merge_without_secrets_and_ignore_sso_sessions() {
        let profiles = parse_profiles(
            "[profile team]\nregion=eu-west-1\nsso_session=corp\n[sso-session corp]\nsso_region=us-east-1\n[default]\nregion=us-west-2",
            "[team]\naws_secret_access_key=never-retain\nregion=us-east-1",
        );
        assert_eq!(
            profiles,
            vec![
                AwsProfile {
                    name: "default".into(),
                    region: Some("us-west-2".into())
                },
                AwsProfile {
                    name: "team".into(),
                    region: Some("eu-west-1".into())
                }
            ]
        );
        assert!(!format!("{profiles:?}").contains("never-retain"));
    }

    #[test]
    fn profile_parser_ignores_non_profile_sections_and_inline_comments() {
        let profiles = parse_profiles(
            "[profile team]\nregion = eu-west-1 # primary\n[sso-session corp]\nregion = should-not-appear\n[services custom]\nregion = should-not-appear",
            "[default]\nregion=us-east-1\n[profile team]\nregion=wrong\n[sso-session corp]\nregion=also-wrong",
        );
        assert_eq!(
            profiles,
            vec![
                AwsProfile {
                    name: "default".into(),
                    region: Some("us-east-1".into())
                },
                AwsProfile {
                    name: "team".into(),
                    region: Some("eu-west-1".into())
                },
            ]
        );
    }

    #[test]
    fn auth_parser_resolves_sso_session_without_retaining_secrets() {
        let profiles = parse_profile_auth(
            "[profile team]\nregion = eu-west-1\nsso_session = corp\n\n[sso-session corp]\nsso_start_url = https://example.awsapps.com/start\nsso_region = us-east-1\n",
            "[team]\naws_access_key_id = AKIAEXAMPLE\naws_secret_access_key = do-not-retain\n",
        );
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "team");
        assert_eq!(profiles[0].region.as_deref(), Some("eu-west-1"));
        assert!(matches!(
            &profiles[0].credential_kind,
            AwsCredentialKind::Sso(AwsSsoConfiguration {
                start_url,
                region,
                session_name: Some(session),
                ..
            }) if start_url == "https://example.awsapps.com/start"
                && region == "us-east-1"
                && session == "corp"
        ));
        assert!(!format!("{profiles:?}").contains("do-not-retain"));
    }

    #[test]
    fn auth_parser_surfaces_external_provider_without_invoking_it() {
        let profiles = parse_profile_auth(
            "[profile external]\nregion=us-east-1\n",
            "[external]\ncredential_process = /usr/local/bin/provider --profile external\n",
        );
        assert_eq!(
            profiles[0].credential_kind,
            AwsCredentialKind::ExternalProvider
        );
    }

    #[test]
    fn workspace_context_save_is_atomic_and_rejects_empty_values() {
        let path = std::env::temp_dir().join(format!(
            "clouddeck-context-test-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let context = WorkspaceContext {
            profile: "team".into(),
            region: "eu-west-1".into(),
        };
        context.save(&path).unwrap();
        assert_eq!(WorkspaceContext::load(&path), Some(context));
        assert!(
            WorkspaceContext {
                profile: String::new(),
                region: "eu-west-1".into()
            }
            .save(&path)
            .is_err()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn workspace_context_load_rejects_empty_values() {
        let path = std::env::temp_dir().join(format!(
            "clouddeck-context-invalid-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, br#"{"profile":"","region":"us-east-1"}"#).unwrap();
        assert!(WorkspaceContext::load(&path).is_none());
        let _ = std::fs::remove_file(path);
    }
}
