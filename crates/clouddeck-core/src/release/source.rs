//! Release source location on GitHub Releases.

use std::io::Read;

use serde::Deserialize;

/// GitHub owner that publishes releases; changing it is a one-line move to a public releases repo.
pub const RELEASE_OWNER: &str = "eginez";
/// GitHub repository that publishes releases; see RELEASE_OWNER.
pub const RELEASE_REPO: &str = "clouddeck";
/// The only host the GitHub token is ever sent to.
const TOKEN_HOST: &str = "api.github.com";
/// Shown when no GitHub token can be found for the private repository.
pub const NO_TOKEN_MESSAGE: &str = "Updates need GitHub access while the repository is private: run gh auth login or set CLOUDDECK_UPDATE_TOKEN";

/// Manifest bytes, their detached base64 ed25519 signature, and the tag of the release they came from.
#[derive(Debug)]
pub struct FetchedRelease {
    pub tag: String,
    pub manifest_bytes: Vec<u8>,
    pub signature_b64: String,
}

/// Upper bound for release JSON, manifest and signature responses.
const SMALL_BODY_LIMIT: u64 = 64 * 1024;

pub trait UpdateSource: Send + Sync {
    /// Fetches the latest release manifest and its signature.
    fn latest(&self) -> Result<FetchedRelease, String>;
    /// Downloads the named release asset, failing if it exceeds `max_bytes`, and reports (received, total) bytes.
    fn download(
        &self,
        asset_name: &str,
        max_bytes: u64,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Vec<u8>, String>;
}

pub fn latest_release_url(owner: &str, repo: &str) -> String {
    format!("https://api.github.com/repos/{owner}/{repo}/releases/latest")
}

#[derive(Deserialize)]
struct ReleaseJson {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    url: String,
}

/// Returns the tag name of a GitHub release JSON document.
pub fn release_tag(release_json: &[u8]) -> Result<String, String> {
    let release: ReleaseJson = serde_json::from_slice(release_json)
        .map_err(|error| format!("invalid release JSON: {error}"))?;
    Ok(release.tag_name)
}

/// Returns the API url of the named asset in a GitHub release JSON document.
pub fn asset_api_url(release_json: &[u8], name: &str) -> Result<String, String> {
    let release: ReleaseJson = serde_json::from_slice(release_json)
        .map_err(|error| format!("invalid release JSON: {error}"))?;
    release
        .assets
        .into_iter()
        .find(|asset| asset.name == name)
        .map(|asset| asset.url)
        .ok_or_else(|| format!("release has no asset {name}"))
}

/// Picks the first non-empty token from the environment, then from the gh CLI.
pub fn resolve_token(env: Option<String>, gh: impl FnOnce() -> Option<String>) -> Option<String> {
    non_empty(env).or_else(|| non_empty(gh()))
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

pub struct GithubSource {
    token: String,
    agent: ureq::Agent,
}

impl GithubSource {
    fn new(token: String) -> Self {
        let agent = ureq::AgentBuilder::new().redirects(0).build();
        Self { token, agent }
    }

    fn release_json(&self) -> Result<Vec<u8>, String> {
        let response = self
            .agent
            .get(&latest_release_url(RELEASE_OWNER, RELEASE_REPO))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/vnd.github+json")
            .set("User-Agent", "clouddeck")
            .call()
            .map_err(latest_release_error)?;
        read_body(response, SMALL_BODY_LIMIT, &mut |_, _| {})
    }

    fn asset(
        &self,
        release_json: &[u8],
        name: &str,
        max_bytes: u64,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Vec<u8>, String> {
        let url = asset_api_url(release_json, name)?;
        ensure_token_host(&url)?;
        let response = self
            .agent
            .get(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Accept", "application/octet-stream")
            .set("User-Agent", "clouddeck")
            .call()
            .map_err(http_error)?;
        let response = if (300..400).contains(&response.status()) {
            let location = response
                .header("Location")
                .ok_or_else(|| format!("redirect for asset {name} has no Location"))?
                .to_string();
            self.agent
                .get(&location)
                .set("Accept", "application/octet-stream")
                .set("User-Agent", "clouddeck")
                .call()
                .map_err(http_error)?
        } else {
            response
        };
        read_body(response, max_bytes, progress)
    }
}

impl UpdateSource for GithubSource {
    fn latest(&self) -> Result<FetchedRelease, String> {
        let release = self.release_json()?;
        let tag = release_tag(&release)?;
        let manifest_bytes =
            self.asset(&release, "manifest.json", SMALL_BODY_LIMIT, &mut |_, _| {})?;
        let signature = self.asset(
            &release,
            "manifest.json.sig",
            SMALL_BODY_LIMIT,
            &mut |_, _| {},
        )?;
        let signature_b64 = String::from_utf8(signature)
            .map_err(|error| format!("manifest.json.sig is not text: {error}"))?
            .trim()
            .to_string();
        Ok(FetchedRelease {
            tag,
            manifest_bytes,
            signature_b64,
        })
    }

    fn download(
        &self,
        asset_name: &str,
        max_bytes: u64,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Vec<u8>, String> {
        let release = self.release_json()?;
        self.asset(&release, asset_name, max_bytes, progress)
    }
}

fn read_body(
    response: ureq::Response,
    max_bytes: u64,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    if response.status() != 200 {
        return Err(format!("unexpected HTTP status {}", response.status()));
    }
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let too_large = || format!("response is larger than the expected {max_bytes} bytes");
    if total > max_bytes {
        return Err(too_large());
    }
    let mut reader = response.into_reader();
    let mut body = Vec::new();
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| format!("download interrupted: {error}"))?;
        if read == 0 {
            break;
        }
        if body.len() as u64 + read as u64 > max_bytes {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk[..read]);
        progress(body.len() as u64, total);
    }
    Ok(body)
}

fn ensure_token_host(asset_url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(asset_url)
        .map_err(|error| format!("release asset url is invalid: {error}"))?;
    if parsed.scheme() == "https"
        && parsed.host_str() == Some(TOKEN_HOST)
        && parsed.port().is_none()
    {
        Ok(())
    } else {
        Err(format!(
            "refusing to send the update token to {}",
            parsed.host_str().unwrap_or("an unknown host")
        ))
    }
}

fn http_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("GitHub request failed with HTTP {code}"),
        ureq::Error::Transport(transport) => format!("GitHub request failed: {transport}"),
    }
}

fn latest_release_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => latest_release_status_message(code)
            .map(str::to_string)
            .unwrap_or_else(|| format!("GitHub request failed with HTTP {code}")),
        ureq::Error::Transport(transport) => format!("GitHub request failed: {transport}"),
    }
}

fn latest_release_status_message(code: u16) -> Option<&'static str> {
    match code {
        401 => Some(
            "GitHub rejected the update token (bad or expired): run gh auth login or set a valid CLOUDDECK_UPDATE_TOKEN",
        ),
        403 => Some(
            "GitHub refused the request (rate limit reached, or the token lacks access): try again later or check that CLOUDDECK_UPDATE_TOKEN has scope repo",
        ),
        404 => Some(
            "No release has been published yet, or your GitHub token cannot see the private repository (check gh auth status or CLOUDDECK_UPDATE_TOKEN scope: repo)",
        ),
        _ => None,
    }
}

/// Builds a GitHub source using the update token from the environment or the gh CLI.
pub fn github_source_from_env() -> Result<GithubSource, String> {
    let token = resolve_token(std::env::var("CLOUDDECK_UPDATE_TOKEN").ok(), gh_auth_token)
        .ok_or_else(|| NO_TOKEN_MESSAGE.to_string())?;
    Ok(GithubSource::new(token))
}

fn gh_auth_token() -> Option<String> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_JSON: &str = r#"{"tag_name":"v0.2.0","assets":[{"name":"manifest.json","url":"https://api.github.com/repos/eginez/clouddeck/releases/assets/1"},{"name":"manifest.json.sig","url":"https://api.github.com/repos/eginez/clouddeck/releases/assets/2"}]}"#;

    struct FakeSource;

    impl UpdateSource for FakeSource {
        fn latest(&self) -> Result<FetchedRelease, String> {
            Ok(FetchedRelease {
                tag: "v0.2.0".to_string(),
                manifest_bytes: b"{}".to_vec(),
                signature_b64: "c2ln".to_string(),
            })
        }

        fn download(
            &self,
            _asset_name: &str,
            _max_bytes: u64,
            progress: &mut dyn FnMut(u64, u64),
        ) -> Result<Vec<u8>, String> {
            progress(3, 3);
            Ok(b"abc".to_vec())
        }
    }

    #[test]
    fn token_host_accepts_https_github_api() {
        assert!(
            ensure_token_host("https://api.github.com/repos/eginez/clouddeck/releases/assets/1")
                .is_ok()
        );
    }

    #[test]
    fn token_host_rejects_foreign_host() {
        let error = ensure_token_host("https://evil.example.com/asset").unwrap_err();
        assert_eq!(
            error,
            "refusing to send the update token to evil.example.com"
        );
    }

    #[test]
    fn token_host_rejects_userinfo_trick() {
        assert!(ensure_token_host("https://api.github.com@evil.example.com/asset").is_err());
    }

    #[test]
    fn token_host_rejects_non_default_port() {
        assert!(
            ensure_token_host(
                "https://api.github.com:8443/repos/eginez/clouddeck/releases/assets/1"
            )
            .is_err()
        );
    }

    #[test]
    fn token_host_rejects_plain_http() {
        assert!(
            ensure_token_host("http://api.github.com/repos/eginez/clouddeck/releases/assets/1")
                .is_err()
        );
    }

    #[test]
    fn asset_refuses_foreign_host_before_sending_token() {
        let release = br#"{"tag_name":"v0.2.0","assets":[{"name":"manifest.json","url":"https://evil.example.com/asset"}]}"#;
        let source = GithubSource::new("secret-token".to_string());
        let error = source
            .asset(release, "manifest.json", SMALL_BODY_LIMIT, &mut |_, _| {})
            .unwrap_err();
        assert_eq!(
            error,
            "refusing to send the update token to evil.example.com"
        );
    }

    #[test]
    fn latest_release_url_points_at_eginez_clouddeck() {
        assert_eq!(
            latest_release_url("eginez", "clouddeck"),
            "https://api.github.com/repos/eginez/clouddeck/releases/latest"
        );
    }

    #[test]
    fn asset_api_url_returns_url_of_named_asset() {
        assert_eq!(
            asset_api_url(RELEASE_JSON.as_bytes(), "manifest.json.sig").unwrap(),
            "https://api.github.com/repos/eginez/clouddeck/releases/assets/2"
        );
    }

    #[test]
    fn release_tag_returns_tag_name() {
        assert_eq!(release_tag(RELEASE_JSON.as_bytes()).unwrap(), "v0.2.0");
    }

    #[test]
    fn release_tag_requires_tag_name() {
        assert!(release_tag(br#"{"assets":[]}"#).is_err());
    }

    #[test]
    fn asset_api_url_reports_unknown_asset() {
        assert_eq!(
            asset_api_url(RELEASE_JSON.as_bytes(), "x").unwrap_err(),
            "release has no asset x"
        );
    }

    #[test]
    fn asset_api_url_rejects_invalid_json() {
        assert!(asset_api_url(b"not json", "manifest.json").is_err());
    }

    #[test]
    fn resolve_token_trims_env_and_skips_gh() {
        let mut gh_called = false;
        let token = resolve_token(Some(" abc ".to_string()), || {
            gh_called = true;
            Some("gh1\n".to_string())
        });
        assert_eq!(token, Some("abc".to_string()));
        assert!(!gh_called);
    }

    #[test]
    fn resolve_token_falls_back_to_gh_when_env_blank() {
        let token = resolve_token(Some("  ".to_string()), || Some("gh1\n".to_string()));
        assert_eq!(token, Some("gh1".to_string()));
    }

    #[test]
    fn resolve_token_is_none_without_any_source() {
        assert_eq!(resolve_token(None, || None), None);
    }

    #[test]
    fn no_token_message_is_exact() {
        assert_eq!(
            NO_TOKEN_MESSAGE,
            "Updates need GitHub access while the repository is private: run gh auth login or set CLOUDDECK_UPDATE_TOKEN"
        );
    }

    #[test]
    fn latest_release_404_explains_missing_release_or_token_access() {
        let message = latest_release_status_message(404).unwrap();
        assert!(message.contains("No release has been published yet"));
        assert!(message.contains("cannot see the private repository"));
    }

    #[test]
    fn latest_release_401_names_the_token() {
        let message = latest_release_status_message(401).unwrap();
        assert!(message.contains("rejected the update token"));
    }

    #[test]
    fn latest_release_403_is_distinct_from_401() {
        let forbidden = latest_release_status_message(403).unwrap();
        assert!(forbidden.contains("rate limit"));
        assert_ne!(forbidden, latest_release_status_message(401).unwrap());
    }

    #[test]
    fn latest_release_other_statuses_have_no_special_message() {
        assert_eq!(latest_release_status_message(500), None);
    }

    fn response(body: &str) -> ureq::Response {
        ureq::Response::new(200, "OK", body).unwrap()
    }

    #[test]
    fn read_body_accepts_body_within_limit() {
        let body = read_body(response("abcdef"), 6, &mut |_, _| {}).unwrap();
        assert_eq!(body, b"abcdef".to_vec());
    }

    #[test]
    fn read_body_rejects_body_over_limit() {
        let error = read_body(response("abcdefg"), 6, &mut |_, _| {}).unwrap_err();
        assert!(
            error.contains("larger than the expected 6 bytes"),
            "{error}"
        );
    }

    #[test]
    fn fake_source_serves_fixed_bytes() {
        let source: &dyn UpdateSource = &FakeSource;
        let release = source.latest().unwrap();
        assert_eq!(release.manifest_bytes, b"{}".to_vec());
        assert_eq!(release.signature_b64, "c2ln");

        let mut seen = Vec::new();
        let mut progress = |received: u64, total: u64| seen.push((received, total));
        let bytes = source.download("manifest.json", 3, &mut progress).unwrap();
        assert_eq!(bytes, b"abc".to_vec());
        assert_eq!(seen, vec![(3, 3)]);
    }
}
