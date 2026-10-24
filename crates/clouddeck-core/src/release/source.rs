//! Release source location on GitHub Releases.

use std::io::Read;

use serde::Deserialize;

/// GitHub owner that publishes releases; changing it is a one-line move to a public releases repo.
pub const RELEASE_OWNER: &str = "eginez";
/// GitHub repository that publishes releases; see RELEASE_OWNER.
pub const RELEASE_REPO: &str = "clouddeck-rs";
/// Shown when no GitHub token can be found for the private repository.
pub const NO_TOKEN_MESSAGE: &str = "Updates need GitHub access while the repository is private: run gh auth login or set CLOUDDECK_UPDATE_TOKEN";

/// Manifest bytes and their detached base64 ed25519 signature for the latest release.
#[derive(Debug)]
pub struct FetchedRelease {
    pub manifest_bytes: Vec<u8>,
    pub signature_b64: String,
}

pub trait UpdateSource: Send + Sync {
    /// Fetches the latest release manifest and its signature.
    fn latest(&self) -> Result<FetchedRelease, String>;
    /// Downloads the named release asset, reporting (received, total) bytes.
    fn download(
        &self,
        asset_name: &str,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Vec<u8>, String>;
}

pub fn latest_release_url(owner: &str, repo: &str) -> String {
    format!("https://api.github.com/repos/{owner}/{repo}/releases/latest")
}

#[derive(Deserialize)]
struct ReleaseJson {
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    url: String,
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
            .map_err(http_error)?;
        read_body(response, &mut |_, _| {})
    }

    fn asset(
        &self,
        release_json: &[u8],
        name: &str,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Vec<u8>, String> {
        let url = asset_api_url(release_json, name)?;
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
        read_body(response, progress)
    }
}

impl UpdateSource for GithubSource {
    fn latest(&self) -> Result<FetchedRelease, String> {
        let release = self.release_json()?;
        let manifest_bytes = self.asset(&release, "manifest.json", &mut |_, _| {})?;
        let signature = self.asset(&release, "manifest.json.sig", &mut |_, _| {})?;
        let signature_b64 = String::from_utf8(signature)
            .map_err(|error| format!("manifest.json.sig is not text: {error}"))?
            .trim()
            .to_string();
        Ok(FetchedRelease {
            manifest_bytes,
            signature_b64,
        })
    }

    fn download(
        &self,
        asset_name: &str,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<Vec<u8>, String> {
        let release = self.release_json()?;
        self.asset(&release, asset_name, progress)
    }
}

fn read_body(
    response: ureq::Response,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Vec<u8>, String> {
    if response.status() != 200 {
        return Err(format!("unexpected HTTP status {}", response.status()));
    }
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
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
        body.extend_from_slice(&chunk[..read]);
        progress(body.len() as u64, total);
    }
    Ok(body)
}

fn http_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("GitHub request failed with HTTP {code}"),
        ureq::Error::Transport(transport) => format!("GitHub request failed: {transport}"),
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

    const RELEASE_JSON: &str = r#"{"tag_name":"v0.2.0","assets":[{"name":"manifest.json","url":"https://api.github.com/repos/eginez/clouddeck-rs/releases/assets/1"},{"name":"manifest.json.sig","url":"https://api.github.com/repos/eginez/clouddeck-rs/releases/assets/2"}]}"#;

    struct FakeSource;

    impl UpdateSource for FakeSource {
        fn latest(&self) -> Result<FetchedRelease, String> {
            Ok(FetchedRelease {
                manifest_bytes: b"{}".to_vec(),
                signature_b64: "c2ln".to_string(),
            })
        }

        fn download(
            &self,
            _asset_name: &str,
            progress: &mut dyn FnMut(u64, u64),
        ) -> Result<Vec<u8>, String> {
            progress(3, 3);
            Ok(b"abc".to_vec())
        }
    }

    #[test]
    fn latest_release_url_points_at_eginez_clouddeck_rs() {
        assert_eq!(
            latest_release_url("eginez", "clouddeck-rs"),
            "https://api.github.com/repos/eginez/clouddeck-rs/releases/latest"
        );
    }

    #[test]
    fn asset_api_url_returns_url_of_named_asset() {
        assert_eq!(
            asset_api_url(RELEASE_JSON.as_bytes(), "manifest.json.sig").unwrap(),
            "https://api.github.com/repos/eginez/clouddeck-rs/releases/assets/2"
        );
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
    fn fake_source_serves_fixed_bytes() {
        let source: &dyn UpdateSource = &FakeSource;
        let release = source.latest().unwrap();
        assert_eq!(release.manifest_bytes, b"{}".to_vec());
        assert_eq!(release.signature_b64, "c2ln");

        let mut seen = Vec::new();
        let mut progress = |received: u64, total: u64| seen.push((received, total));
        let bytes = source.download("manifest.json", &mut progress).unwrap();
        assert_eq!(bytes, b"abc".to_vec());
        assert_eq!(seen, vec![(3, 3)]);
    }
}
