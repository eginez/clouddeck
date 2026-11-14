//! Application Default Credentials: locate, parse, refresh, and cache the access token.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use clouddeck_extension_api::{ErrorHint, ErrorKind, ProviderError};
use serde::Deserialize;

use crate::config::{AuthMode, Config, EMULATOR_TOKEN};
use crate::http::Http;
use crate::service_account::{self, JWT_BEARER_GRANT};

const ADC_FILE: &str = "application_default_credentials.json";
const ADVICE: &str = "run: gcloud auth application-default login, or choose a Config folder";
const SIGN_IN_COMMAND: &str = "gcloud auth application-default login";
/// Refresh this long before the token really expires.
const EXPIRY_SLACK: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdcCreds {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub quota_project: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub access: String,
    pub quota_project: Option<String>,
}

#[derive(Deserialize)]
struct AdcFile {
    #[serde(rename = "type")]
    kind: String,
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
    quota_project_id: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<u64>,
}

pub fn auth_error(message: &str) -> ProviderError {
    let mut error = ProviderError::new(ErrorKind::Auth, message);
    error.advice = Some(ADVICE.to_string());
    error
}

fn missing_adc(message: &str) -> ProviderError {
    let mut error = auth_error(message);
    error.hint = Some(Box::new(ErrorHint {
        title: "Google Cloud needs Application Default Credentials".into(),
        detail: Some(
            "Run this once in a terminal. It opens your browser to sign in and writes the file CloudDeck reads. Looked in $GOOGLE_APPLICATION_CREDENTIALS and ~/.config/gcloud.".into(),
        ),
        command: Some(SIGN_IN_COMMAND.into()),
        file: None,
    }));
    error
}

fn unsupported_adc(kind: &str) -> ProviderError {
    let mut error = auth_error(&format!(
        "ADC credential type {kind:?} is not supported yet"
    ));
    error.hint = Some(Box::new(ErrorHint {
        title: "Service-account credentials aren’t supported yet".into(),
        detail: Some(format!(
            "Your ADC file is a {kind} key. CloudDeck can use a user login for now, or the Service account key sign-in method."
        )),
        command: Some(SIGN_IN_COMMAND.into()),
        file: None,
    }));
    error
}

/// Pure: turn the ADC file text into credentials.
pub fn parse_adc(text: &str) -> Result<AdcCreds, ProviderError> {
    let file: AdcFile =
        serde_json::from_str(text).map_err(|e| auth_error(&format!("unreadable ADC file: {e}")))?;
    if file.kind != "authorized_user" {
        return Err(unsupported_adc(&file.kind));
    }
    let missing = || auth_error("ADC file lacks client_id, client_secret or refresh_token");
    Ok(AdcCreds {
        client_id: file.client_id.ok_or_else(missing)?,
        client_secret: file.client_secret.ok_or_else(missing)?,
        refresh_token: file.refresh_token.ok_or_else(missing)?,
        quota_project: file.quota_project_id,
    })
}

/// Pure: the path ADC is read from, given the relevant environment.
pub fn adc_path(explicit: Option<String>, home: Option<String>) -> Option<PathBuf> {
    if let Some(path) = explicit.filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    let home = home.filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".config/gcloud/application_default_credentials.json"))
}

/// Reads the credentials from the config folder when one is set, else from the environment default.
pub fn load_adc(folder: Option<&Path>) -> Result<AdcCreds, ProviderError> {
    let path = match folder {
        Some(dir) => dir.join(ADC_FILE),
        None => {
            let explicit = std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok();
            adc_path(explicit, std::env::var("HOME").ok())
                .ok_or_else(|| missing_adc("no Application Default Credentials found"))?
        }
    };
    let text = std::fs::read_to_string(&path).map_err(|e| {
        let looked_in = path.parent().unwrap_or(&path).display();
        missing_adc(&format!(
            "no Application Default Credentials in {looked_in}: {e}"
        ))
    })?;
    parse_adc(&text)
}

/// Pure: the token endpoint's sibling that reports the account behind a token.
pub fn tokeninfo_url(token_url: &str) -> String {
    let base = token_url.strip_suffix("/token").unwrap_or(token_url);
    format!("{base}/tokeninfo")
}

/// Pure: read the refresh response into a token and its lifetime.
pub fn parse_token_response(body: &str) -> Result<(String, Duration), ProviderError> {
    let parsed: TokenResponse =
        serde_json::from_str(body).map_err(|e| auth_error(&format!("bad token response: {e}")))?;
    let lifetime = Duration::from_secs(parsed.expires_in.unwrap_or(3600));
    Ok((parsed.access_token, lifetime))
}

type Loader = Box<dyn Fn(Option<&Path>) -> Result<AdcCreds, ProviderError> + Send + Sync>;

struct Cached {
    token_url: String,
    folder: Option<PathBuf>,
    key_file: Option<PathBuf>,
    token: Token,
    expiry: Instant,
}

/// Hands out access tokens, refreshing only when the cached one is about to expire.
pub struct TokenSource {
    http: Arc<dyn Http>,
    load: Loader,
    cache: Mutex<Option<Cached>>,
}

impl TokenSource {
    pub fn new(http: Arc<dyn Http>, load: Loader) -> Self {
        Self {
            http,
            load,
            cache: Mutex::new(None),
        }
    }

    pub fn token(&self, timeout: Duration, config: &Config) -> Result<Token, ProviderError> {
        if config.auth == AuthMode::Emulator {
            return Ok(Token {
                access: EMULATOR_TOKEN.to_string(),
                quota_project: None,
            });
        }
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.as_ref().filter(|c| {
            c.token_url == config.token
                && c.folder == config.folder
                && c.key_file == config.key_file
                && c.expiry > Instant::now()
        }) {
            return Ok(cached.token.clone());
        }
        let (token, lifetime) = match &config.key_file {
            Some(path) => self.key_token(path, &config.token, timeout)?,
            None => {
                let creds = (self.load)(config.folder.as_deref())?;
                self.refresh(&creds, &config.token, timeout)?
            }
        };
        *cache = Some(Cached {
            token_url: config.token.clone(),
            folder: config.folder.clone(),
            key_file: config.key_file.clone(),
            token: token.clone(),
            expiry: Instant::now() + lifetime.saturating_sub(EXPIRY_SLACK),
        });
        Ok(token)
    }

    fn key_token(
        &self,
        path: &Path,
        token_url: &str,
        timeout: Duration,
    ) -> Result<(Token, Duration), ProviderError> {
        let key = service_account::load_key(path)?;
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let assertion = service_account::sign_jwt(&key, now, token_url)?;
        let form = [
            ("grant_type", JWT_BEARER_GRANT),
            ("assertion", assertion.as_str()),
        ];
        let body = self
            .http
            .post_form(token_url, &form, timeout)
            .map_err(|e| e.into_provider())?;
        let (access, lifetime) = parse_token_response(&body)?;
        Ok((
            Token {
                access,
                quota_project: None,
            },
            lifetime,
        ))
    }

    fn refresh(
        &self,
        creds: &AdcCreds,
        token_url: &str,
        timeout: Duration,
    ) -> Result<(Token, Duration), ProviderError> {
        let form = [
            ("grant_type", "refresh_token"),
            ("client_id", creds.client_id.as_str()),
            ("client_secret", creds.client_secret.as_str()),
            ("refresh_token", creds.refresh_token.as_str()),
        ];
        let body = self
            .http
            .post_form(token_url, &form, timeout)
            .map_err(|e| {
                let mut error = e.into_provider();
                error.advice.get_or_insert_with(|| ADVICE.to_string());
                error
            })?;
        let (access, lifetime) = parse_token_response(&body)?;
        let quota_project = creds.quota_project.clone();
        Ok((
            Token {
                access,
                quota_project,
            },
            lifetime,
        ))
    }

    pub fn forget(&self) {
        *self.cache.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::tokeninfo_url;

    #[test]
    fn tokeninfo_sits_beside_the_token_endpoint() {
        assert_eq!(
            tokeninfo_url("https://oauth2.googleapis.com/token"),
            "https://oauth2.googleapis.com/tokeninfo"
        );
        assert_eq!(
            tokeninfo_url("http://localhost:4588/token"),
            "http://localhost:4588/tokeninfo"
        );
    }
}
