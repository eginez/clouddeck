//! The only IO seam: a tiny blocking HTTP trait, with a ureq implementation.

use std::io::Read;
use std::time::Duration;

use clouddeck_extension_api::{ErrorKind, ProviderError};

/// Cap on a response body, so a misbehaving server cannot exhaust memory.
const MAX_BODY: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpError {
    /// `None` for transport failures (DNS, connect, timeout).
    pub status: Option<u16>,
    pub timed_out: bool,
    pub message: String,
}

pub struct Request<'a> {
    pub url: &'a str,
    pub token: Option<&'a str>,
    pub quota_project: Option<&'a str>,
    pub timeout: Duration,
}

pub trait Http: Send + Sync {
    fn get(&self, request: &Request<'_>) -> Result<String, HttpError>;
    fn post_form(
        &self,
        url: &str,
        form: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<String, HttpError>;
}

impl HttpError {
    pub fn into_provider(self) -> ProviderError {
        let kind = match self.status {
            _ if self.timed_out => ErrorKind::Timeout,
            Some(400 | 401) => ErrorKind::Auth,
            Some(403) => ErrorKind::Permission,
            Some(404) => ErrorKind::NotFound,
            Some(429) => ErrorKind::Throttled,
            Some(_) => ErrorKind::Other,
            None => ErrorKind::Network,
        };
        ProviderError::new(kind, self.message)
    }
}

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> Self {
        Self {
            agent: ureq::AgentBuilder::new().redirects(0).build(),
        }
    }
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

fn read_body(response: ureq::Response) -> Result<String, HttpError> {
    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_BODY)
        .read_to_string(&mut body)
        .map_err(|e| transport(&e.to_string(), e.kind() == std::io::ErrorKind::TimedOut))?;
    Ok(body)
}

fn transport(message: &str, timed_out: bool) -> HttpError {
    HttpError {
        status: None,
        timed_out,
        message: message.to_string(),
    }
}

fn convert(error: ureq::Error) -> HttpError {
    match error {
        ureq::Error::Status(code, response) => HttpError {
            status: Some(code),
            timed_out: false,
            message: format!("HTTP {code}: {}", api_message(response)),
        },
        ureq::Error::Transport(t) => {
            let timed_out = t.kind() == ureq::ErrorKind::Io && t.to_string().contains("timed out");
            transport(&t.to_string(), timed_out)
        }
    }
}

/// Google error bodies look like `{"error":{"message":"..."}}`; fall back to the status text.
fn api_message(response: ureq::Response) -> String {
    let text = response.status_text().to_string();
    let body = read_body(response).unwrap_or_default();
    let parsed: Option<serde_json::Value> = serde_json::from_str(&body).ok();
    parsed
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or(text)
}

impl Http for UreqHttp {
    fn get(&self, request: &Request<'_>) -> Result<String, HttpError> {
        let mut call = self.agent.get(request.url).timeout(request.timeout);
        if let Some(token) = request.token {
            call = call.set("Authorization", &format!("Bearer {token}"));
        }
        if let Some(project) = request.quota_project {
            call = call.set("x-goog-user-project", project);
        }
        read_body(call.call().map_err(convert)?)
    }

    fn post_form(
        &self,
        url: &str,
        form: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<String, HttpError> {
        let call = self.agent.post(url).timeout(timeout);
        read_body(call.send_form(form).map_err(convert)?)
    }
}
