use crate::release::source::{FetchedRelease, http_error, read_body, release_tag};

use super::index::INDEX_URL;

const SMALL_LIMIT: u64 = 64 * 1024;
const INDEX_LIMIT: u64 = 1024 * 1024;
const USER_AGENT: &str = "clouddeck";

/// Where extension index, release and asset bytes come from. Tests supply a fake.
pub trait ExtensionSource {
    /// The index.json bytes and their detached base64 signature.
    fn index(&self) -> Result<(Vec<u8>, String), String>;
    /// The latest release of the repository: its tag, manifest bytes and manifest signature.
    fn release(&self, owner: &str, repo: &str) -> Result<FetchedRelease, String>;
    fn download(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        asset: &str,
        max_bytes: u64,
    ) -> Result<Vec<u8>, String>;
}

/// Splits `https://github.com/<owner>/<repo>` into its owner and repository names.
pub fn parse_repository(repository: &str) -> Result<(String, String), String> {
    let refuse = || format!("{repository} is not a https://github.com/<owner>/<repo> URL");
    let parsed = url::Url::parse(repository).map_err(|_| refuse())?;
    let segments: Vec<&str> = parsed
        .path_segments()
        .map(Iterator::collect)
        .unwrap_or_default();
    let valid_host = parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed.port().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none();
    match segments.as_slice() {
        [owner, repo] if valid_host && is_name(owner) && is_name(repo) => {
            Ok((owner.to_string(), repo.to_string()))
        }
        _ => Err(refuse()),
    }
}

fn is_name(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub struct GithubExtensions {
    agent: ureq::Agent,
}

impl Default for GithubExtensions {
    fn default() -> Self {
        Self {
            agent: ureq::AgentBuilder::new().redirects(0).build(),
        }
    }
}

impl GithubExtensions {
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
        let response = self
            .agent
            .get(url)
            .set("User-Agent", USER_AGENT)
            .call()
            .map_err(http_error)?;
        let response = if (300..400).contains(&response.status()) {
            let location = response
                .header("Location")
                .ok_or_else(|| format!("redirect for {url} has no Location"))?
                .to_string();
            self.agent
                .get(&location)
                .set("User-Agent", USER_AGENT)
                .call()
                .map_err(http_error)?
        } else {
            response
        };
        read_body(response, max_bytes, &mut |_, _| {})
    }

    fn release_asset(owner: &str, repo: &str, tag: &str, asset: &str) -> String {
        format!("https://github.com/{owner}/{repo}/releases/download/{tag}/{asset}")
    }
}

impl ExtensionSource for GithubExtensions {
    fn index(&self) -> Result<(Vec<u8>, String), String> {
        let bytes = self.get(INDEX_URL, INDEX_LIMIT)?;
        let signature = self.get(&format!("{INDEX_URL}.sig"), SMALL_LIMIT)?;
        let signature = String::from_utf8(signature)
            .map_err(|error| format!("index.json.sig is not text: {error}"))?
            .trim()
            .to_string();
        Ok((bytes, signature))
    }

    fn release(&self, owner: &str, repo: &str) -> Result<FetchedRelease, String> {
        let latest = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
        let tag = release_tag(&self.get(&latest, SMALL_LIMIT)?)?;
        let manifest_bytes = self.get(
            &Self::release_asset(owner, repo, &tag, "manifest.json"),
            SMALL_LIMIT,
        )?;
        let signature = self.get(
            &Self::release_asset(owner, repo, &tag, "manifest.json.sig"),
            SMALL_LIMIT,
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
        owner: &str,
        repo: &str,
        tag: &str,
        asset: &str,
        max_bytes: u64,
    ) -> Result<Vec<u8>, String> {
        self.get(&Self::release_asset(owner, repo, tag, asset), max_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_github_repository_urls() {
        assert_eq!(
            parse_repository("https://github.com/eginez/clouddeck-toy").unwrap(),
            ("eginez".to_string(), "clouddeck-toy".to_string())
        );
    }

    #[test]
    fn refuses_other_hosts_paths_and_traversal() {
        for url in [
            "http://github.com/a/b",
            "https://evil.example/a/b",
            "https://github.com/a",
            "https://github.com/a/b/c",
            "https://github.com/a/..",
            "https://github.com/a/b?x=1",
            "https://github.com:444/a/b",
            "not a url",
        ] {
            assert!(parse_repository(url).is_err(), "{url} should be refused");
        }
    }
}
