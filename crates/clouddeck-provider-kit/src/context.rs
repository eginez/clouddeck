use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
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

impl From<AwsProfileAuth> for AwsProfile {
    fn from(profile: AwsProfileAuth) -> Self {
        Self {
            name: profile.name,
            region: profile.region,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwsConfigFile {
    Config,
    Credentials,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsConfigParseError {
    pub file: AwsConfigFile,
    pub line: usize,
    pub message: &'static str,
}

impl AwsConfigParseError {
    fn into_io_error(self, config_path: &Path, credentials_path: &Path) -> std::io::Error {
        let path = match self.file {
            AwsConfigFile::Config => config_path,
            AwsConfigFile::Credentials => credentials_path,
        };
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{}: line {}: {}", path.display(), self.line, self.message),
        )
    }
}

impl fmt::Display for AwsConfigParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = match self.file {
            AwsConfigFile::Config => "config",
            AwsConfigFile::Credentials => "credentials",
        };
        write!(f, "AWS {file} file, line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for AwsConfigParseError {}

#[derive(Default)]
struct ProfileSection {
    values: BTreeMap<String, String>,
}

type Sections = BTreeMap<String, ProfileSection>;

/// Maps a section header to the key its settings are stored under. Config-file
/// `sso-session` blocks get their own namespace so they never surface as profiles.
fn section_key(header: &str, file: AwsConfigFile) -> Option<String> {
    let name = match file {
        AwsConfigFile::Config => {
            if header == "default" {
                return Some(header.to_owned());
            }
            if let Some(session) = header.strip_prefix("sso-session ") {
                let session = session.trim();
                return (!session.is_empty()).then(|| format!("sso-session {session}"));
            }
            header.strip_prefix("profile ")?.trim()
        }
        AwsConfigFile::Credentials => {
            if header.starts_with("sso-session ") || header.starts_with("services ") {
                return None;
            }
            header.strip_prefix("profile ").unwrap_or(header).trim()
        }
    };
    (!name.is_empty()).then(|| name.to_owned())
}

fn parse_sections(text: &str, file: AwsConfigFile) -> Result<Sections, AwsConfigParseError> {
    let mut sections = Sections::new();
    let mut current: Option<String> = None;
    for (index, raw_line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        parse_line(raw_line, index + 1, file, &mut sections, &mut current)?;
    }
    Ok(sections)
}

fn parse_line(
    raw_line: &str,
    number: usize,
    file: AwsConfigFile,
    sections: &mut Sections,
    current: &mut Option<String>,
) -> Result<(), AwsConfigParseError> {
    let error = |message: &'static str| AwsConfigParseError {
        file,
        line: number,
        message,
    };
    let line = raw_line.trim();
    if line.is_empty() || line.starts_with(['#', ';']) {
        return Ok(());
    }
    if let Some(header) = line.strip_prefix('[') {
        let name = parse_header(header, file).map_err(&error)?;
        if let Some(name) = name.as_ref() {
            sections.entry(name.clone()).or_default();
        }
        *current = name;
        return Ok(());
    }
    let Some((key, value)) = line.split_once('=') else {
        // Indented lines continue the previous setting's multi-line value.
        if raw_line.starts_with(char::is_whitespace) {
            return Ok(());
        }
        return Err(error("expected `key = value` or a [section] header"));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(error("setting has no name before '='"));
    }
    let Some(name) = current.as_ref() else {
        return Ok(());
    };
    let value = clean_value(value);
    if !value.is_empty() {
        sections
            .entry(name.clone())
            .or_default()
            .values
            .insert(key.to_ascii_lowercase(), value.to_owned());
    }
    Ok(())
}

fn parse_header(header: &str, file: AwsConfigFile) -> Result<Option<String>, &'static str> {
    let Some(header) = header.strip_suffix(']') else {
        return Err("section header is missing its closing ']'");
    };
    let header = header.trim();
    if header.is_empty() {
        return Err("section header has no name");
    }
    Ok(section_key(header, file))
}

fn clean_value(value: &str) -> &str {
    value
        .split_once('#')
        .map_or(value, |(value, _)| value)
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
}

struct ProfileDraft {
    region: Option<String>,
    credential_kind: AwsCredentialKind,
    source_profile: Option<String>,
}

fn profile_draft(name: &str, config: &Sections, credentials: &Sections) -> ProfileDraft {
    let config_values = config.get(name).map(|section| &section.values);
    let credential_values = credentials.get(name).map(|section| &section.values);
    let value = |key: &str| {
        config_values
            .and_then(|values| values.get(key))
            .or_else(|| credential_values.and_then(|values| values.get(key)))
            .cloned()
    };
    let session_name = value("sso_session");
    let session_values = session_name
        .as_deref()
        .and_then(|session| config.get(&format!("sso-session {session}")))
        .map(|section| &section.values);
    let start_url = value("sso_start_url")
        .or_else(|| session_values.and_then(|values| values.get("sso_start_url").cloned()));
    let sso_region = value("sso_region")
        .or_else(|| session_values.and_then(|values| values.get("sso_region").cloned()));
    let credential_kind = match (start_url, sso_region) {
        (Some(start_url), Some(region)) => AwsCredentialKind::Sso(AwsSsoConfiguration {
            start_url,
            region,
            account_id: value("sso_account_id"),
            role_name: value("sso_role_name"),
            session_name,
        }),
        _ if value("credential_process").is_some() => AwsCredentialKind::ExternalProvider,
        _ if value("aws_access_key_id").is_some() || value("aws_secret_access_key").is_some() => {
            AwsCredentialKind::SharedCredentials
        }
        _ => AwsCredentialKind::Unconfigured,
    };
    let source_profile = if value("role_arn").is_some() {
        value("source_profile")
    } else {
        None
    };
    ProfileDraft {
        region: value("region"),
        credential_kind,
        source_profile,
    }
}

/// A role profile without credentials of its own assumes its `source_profile`,
/// so it inherits that profile's credential kind. The visited list stops cycles.
fn resolve_credential_kind(
    name: &str,
    drafts: &BTreeMap<String, ProfileDraft>,
    visited: &mut Vec<String>,
) -> AwsCredentialKind {
    let Some(draft) = drafts.get(name) else {
        return AwsCredentialKind::Unconfigured;
    };
    if !matches!(draft.credential_kind, AwsCredentialKind::Unconfigured) {
        return draft.credential_kind.clone();
    }
    let Some(source) = draft.source_profile.as_deref() else {
        return AwsCredentialKind::Unconfigured;
    };
    if visited.iter().any(|seen| seen == name) {
        return AwsCredentialKind::Unconfigured;
    }
    visited.push(name.to_owned());
    resolve_credential_kind(source, drafts, visited)
}

/// Parses profile metadata and SSO configuration without retaining secrets.
pub fn parse_profile_auth(
    config: &str,
    credentials: &str,
) -> Result<Vec<AwsProfileAuth>, AwsConfigParseError> {
    let config_sections = parse_sections(config, AwsConfigFile::Config)?;
    let credential_sections = parse_sections(credentials, AwsConfigFile::Credentials)?;
    let names: BTreeSet<&String> = config_sections
        .keys()
        .filter(|name| !name.starts_with("sso-session "))
        .chain(credential_sections.keys())
        .collect();
    let drafts: BTreeMap<String, ProfileDraft> = names
        .into_iter()
        .map(|name| {
            (
                name.clone(),
                profile_draft(name, &config_sections, &credential_sections),
            )
        })
        .collect();
    Ok(drafts
        .keys()
        .map(|name| AwsProfileAuth {
            name: name.clone(),
            region: drafts[name].region.clone(),
            credential_kind: resolve_credential_kind(name, &drafts, &mut Vec::new()),
        })
        .collect())
}

fn read_aws_file(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(std::io::Error::new(
            e.kind(),
            format!("{}: {e}", path.display()),
        )),
    }
}

/// Reads the given config and credentials files. A missing file counts as empty;
/// a malformed one yields an `InvalidData` error naming the file and line.
pub fn read_profile_auth(
    config_path: &Path,
    credentials_path: &Path,
) -> std::io::Result<Vec<AwsProfileAuth>> {
    let config = read_aws_file(config_path)?;
    let credentials = read_aws_file(credentials_path)?;
    parse_profile_auth(&config, &credentials)
        .map_err(|error| error.into_io_error(config_path, credentials_path))
}

/// The `config_file` setting's default, meaning "use AWS_CONFIG_FILE, else ~/.aws/config".
pub const DEFAULT_AWS_CONFIG_FILE: &str = "~/.aws/config";

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_default()
}

pub fn aws_file_paths() -> (PathBuf, PathBuf) {
    let home = home_dir().join(".aws");
    let config = std::env::var_os("AWS_CONFIG_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("config"));
    let credentials = std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join("credentials"));
    (config, credentials)
}

/// The config file named by the `config_file` setting, or `None` when it is unset or the default.
pub fn custom_aws_config_file(setting: Option<&str>) -> Option<PathBuf> {
    let value = setting?.trim();
    if value.is_empty() || value == DEFAULT_AWS_CONFIG_FILE {
        return None;
    }
    match value.strip_prefix("~/") {
        Some(rest) => {
            let home = std::env::var_os("HOME").filter(|value| !value.is_empty())?;
            Some(PathBuf::from(home).join(rest))
        }
        None => Some(PathBuf::from(value)),
    }
}

/// Config and credentials paths for the `config_file` setting; credentials always come from the environment.
pub fn aws_file_paths_for(setting: Option<&str>) -> (PathBuf, PathBuf) {
    let (config, credentials) = aws_file_paths();
    (
        custom_aws_config_file(setting).unwrap_or(config),
        credentials,
    )
}

pub fn load_profile_auth_for(setting: Option<&str>) -> std::io::Result<Vec<AwsProfileAuth>> {
    let (config, credentials) = aws_file_paths_for(setting);
    read_profile_auth(&config, &credentials)
}

/// Only profile names and region metadata are retained; secret values are ignored.
pub fn parse_profiles(
    config: &str,
    credentials: &str,
) -> Result<Vec<AwsProfile>, AwsConfigParseError> {
    Ok(parse_profile_auth(config, credentials)?
        .into_iter()
        .map(AwsProfile::from)
        .collect())
}

pub fn load_profiles_for(setting: Option<&str>) -> std::io::Result<Vec<AwsProfile>> {
    Ok(load_profile_auth_for(setting)?
        .into_iter()
        .map(AwsProfile::from)
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
enum StoredContext {
    Current {
        provider: String,
        connection: String,
    },
    Legacy {
        profile: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceContext {
    pub profile: String,
}
impl WorkspaceContext {
    pub fn load(path: &Path) -> Option<Self> {
        Self::decode(&std::fs::read(path).ok()?)
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        let profile = match serde_json::from_slice::<StoredContext>(bytes).ok()? {
            StoredContext::Current {
                provider,
                connection,
            } if provider == "aws" => connection,
            StoredContext::Current { .. } => return None,
            StoredContext::Legacy { profile } => profile,
        };
        let context = Self { profile };
        context.is_usable().then_some(context)
    }

    pub fn is_usable(&self) -> bool {
        !self.profile.trim().is_empty()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if !self.is_usable() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "AWS profile must be non-empty",
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
        let stored = StoredContext::Current {
            provider: "aws".into(),
            connection: self.profile.clone(),
        };
        file.write_all(&serde_json::to_vec(&stored).map_err(std::io::Error::other)?)?;
        file.sync_all()?;
        std::fs::rename(temp, path)
    }
}

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".config")
        })
}

pub fn context_path() -> PathBuf {
    config_home().join("clouddeck/context.json")
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        )
        .unwrap();
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
        };
        context.save(&path).unwrap();
        assert_eq!(WorkspaceContext::load(&path), Some(context));
        assert!(
            WorkspaceContext {
                profile: String::new(),
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
        std::fs::write(&path, br#"{"profile":""}"#).unwrap();
        assert!(WorkspaceContext::load(&path).is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn legacy_context_fixture_loads_as_aws_profile() {
        let legacy = include_bytes!("../tests/fixtures/context/legacy-context.json");
        assert_eq!(
            WorkspaceContext::decode(legacy),
            Some(WorkspaceContext {
                profile: "team".into(),
            })
        );
    }

    #[test]
    fn workspace_context_writes_provider_and_connection_only() {
        let path = std::env::temp_dir().join(format!(
            "clouddeck-context-format-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        WorkspaceContext {
            profile: "team".into(),
        }
        .save(&path)
        .unwrap();
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            written,
            serde_json::json!({
                "provider": "aws",
                "connection": "team",
            })
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn workspace_context_rejects_non_aws_contexts() {
        assert!(WorkspaceContext::decode(br#"{"provider":"gcp","connection":"team"}"#).is_none());
    }

    #[test]
    fn workspace_context_ignores_scopes_written_by_older_builds() {
        assert_eq!(
            WorkspaceContext::decode(
                br#"{"provider":"aws","connection":"team","scopes":["eu-west-1","us-east-1"]}"#
            ),
            Some(WorkspaceContext {
                profile: "team".into(),
            })
        );
    }

    #[test]
    fn default_or_blank_config_setting_uses_the_environment_paths() {
        assert_eq!(custom_aws_config_file(None), None);
        assert_eq!(custom_aws_config_file(Some("  ")), None);
        assert_eq!(custom_aws_config_file(Some(DEFAULT_AWS_CONFIG_FILE)), None);
        assert_eq!(aws_file_paths_for(None), aws_file_paths());
        assert_eq!(aws_file_paths_for(Some("")), aws_file_paths());
    }

    #[test]
    fn custom_config_setting_replaces_only_the_config_path() {
        assert_eq!(
            custom_aws_config_file(Some("/srv/aws/profiles")),
            Some(PathBuf::from("/srv/aws/profiles"))
        );
        assert_eq!(
            custom_aws_config_file(Some("~/work/aws-config")),
            Some(home_dir().join("work/aws-config"))
        );
        let (config, credentials) = aws_file_paths_for(Some("/srv/aws/profiles"));
        assert_eq!(config, PathBuf::from("/srv/aws/profiles"));
        assert_eq!(credentials, aws_file_paths().1);
    }
}
