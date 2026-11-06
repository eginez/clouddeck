use std::fmt;

/// Mirrors Swift's `CloudDeckError`: a small, closed set of classification
/// buckets used to turn arbitrary service errors into user-facing messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudDeckError {
    Auth,
    Network,
    Timeout,
    Permission,
    Throttled,
    NotFound,
    Unknown(String),
}

impl CloudDeckError {
    pub const THROTTLED_RETRY_MESSAGE: &'static str =
        "The service throttled this request. Retry the load after a short delay.";

    pub fn user_message(&self) -> String {
        match self {
            CloudDeckError::Auth => {
                "Credentials are invalid or expired. Switch to a valid AWS profile.".to_string()
            }
            CloudDeckError::Network => {
                "Network request failed. Check connectivity and try again.".to_string()
            }
            CloudDeckError::Timeout => {
                "The request timed out. Try again or reduce the scope of the load.".to_string()
            }
            CloudDeckError::Permission => {
                "You do not have permission for this action. Verify IAM permissions.".to_string()
            }
            CloudDeckError::Throttled => Self::THROTTLED_RETRY_MESSAGE.to_string(),
            CloudDeckError::NotFound => "The requested resource was not found.".to_string(),
            CloudDeckError::Unknown(message) => message.clone(),
        }
    }

    /// Prefixes the user message with "Could not {operation}: ", unless it
    /// already carries that prefix (matches Swift's idempotent `message(for:)`).
    pub fn message_for(&self, operation: &str) -> String {
        let prefix = format!("Could not {operation}:");
        let user_message = self.user_message();
        if user_message.starts_with(&prefix) {
            user_message
        } else {
            format!("{prefix} {user_message}")
        }
    }

    /// Classifies an arbitrary error by lowercased substring match on its
    /// `Display` output. Modeled AWS errors should be classified before they
    /// reach this fallback (see `from_aws_error_code`), because messages such
    /// as "not authorized" are permission failures rather than auth expiry.
    pub fn from_service_error(error: &(dyn std::error::Error + 'static)) -> CloudDeckError {
        let description = error.to_string();
        let normalized = description.to_lowercase();

        if normalized.contains("forbidden")
            || normalized.contains("access denied")
            || normalized.contains("not authorized")
            || normalized.contains("permission")
        {
            return CloudDeckError::Permission;
        }

        if normalized.contains("credential")
            || normalized.contains("token")
            || normalized.contains("auth")
        {
            return CloudDeckError::Auth;
        }

        if normalized.contains("timed out") || normalized.contains("timeout") {
            return CloudDeckError::Timeout;
        }

        if normalized.contains("network")
            || normalized.contains("connection")
            || normalized.contains("could not connect")
            || normalized.contains("offline")
        {
            return CloudDeckError::Network;
        }

        if normalized.contains("throttl")
            || normalized.contains("requestlimitexceeded")
            || normalized.contains("toomanyrequests")
            || normalized.contains("rate exceeded")
            || normalized.contains("too many requests")
        {
            return CloudDeckError::Throttled;
        }

        if normalized.contains("not found")
            || normalized.contains("no such")
            || normalized.contains("does not exist")
        {
            return CloudDeckError::NotFound;
        }

        CloudDeckError::Unknown(description)
    }

    /// Classifies a modeled AWS service error using its error code first, then
    /// falls back to the display text for transport and provider failures.
    pub fn from_aws_error_code(code: Option<&str>, description: &str) -> CloudDeckError {
        match code.unwrap_or_default() {
            "AccessDenied"
            | "AccessDeniedException"
            | "UnauthorizedOperation"
            | "UnauthorizedClientException"
            | "ForbiddenException" => CloudDeckError::Permission,
            "ExpiredToken"
            | "ExpiredTokenException"
            | "InvalidClientException"
            | "InvalidGrantException"
            | "InvalidIdentityToken"
            | "InvalidIdentityTokenException"
            | "UnrecognizedClientException" => CloudDeckError::Auth,
            "RequestLimitExceeded"
            | "Throttling"
            | "ThrottlingException"
            | "TooManyRequestsException" => CloudDeckError::Throttled,
            "InvalidInstanceID.NotFound" | "InstanceNotFound" => CloudDeckError::NotFound,
            _ => {
                let error = std::io::Error::other(description.to_string());
                Self::from_service_error(&error)
            }
        }
    }
}

impl fmt::Display for CloudDeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_message())
    }
}

impl std::error::Error for CloudDeckError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_auth_errors() {
        let err = std::io::Error::other("invalid auth token");
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::Auth
        );
    }

    #[test]
    fn classifies_timeout_errors() {
        let err = std::io::Error::other("request timed out");
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::Timeout
        );
    }

    #[test]
    fn classifies_network_errors() {
        let err = std::io::Error::other("network connection lost");
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::Network
        );
    }

    #[test]
    fn classifies_could_not_connect_as_network() {
        let err = std::io::Error::other(
            "Could not connect to the endpoint URL: \"https://ec2.us-east-1.amazonaws.com/\"",
        );
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::Network
        );
    }

    #[test]
    fn classifies_throttled_errors() {
        let err = std::io::Error::other("RequestLimitExceeded");
        assert_eq!(
            CloudDeckError::from_service_error(&err)
                .to_string()
                .to_lowercase(),
            CloudDeckError::Throttled.to_string().to_lowercase()
        );
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::Throttled
        );
    }

    #[test]
    fn classifies_permission_errors() {
        let err = std::io::Error::other("access denied");
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::Permission
        );
    }

    #[test]
    fn permission_text_wins_over_auth_wording() {
        let err = std::io::Error::other("the caller is not authorized to use this credential");
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::Permission
        );
    }

    #[test]
    fn modeled_aws_error_codes_are_classified_without_substring_ambiguity() {
        assert_eq!(
            CloudDeckError::from_aws_error_code(Some("UnauthorizedOperation"), "not authorized"),
            CloudDeckError::Permission
        );
        assert_eq!(
            CloudDeckError::from_aws_error_code(Some("ExpiredToken"), "access denied"),
            CloudDeckError::Auth
        );
    }

    #[test]
    fn classifies_not_found_errors() {
        let err = std::io::Error::other("no such instance");
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::NotFound
        );
    }

    #[test]
    fn classifies_unrecognized_errors_as_unknown() {
        let err = std::io::Error::other("something weird happened");
        assert_eq!(
            CloudDeckError::from_service_error(&err),
            CloudDeckError::Unknown("something weird happened".to_string())
        );
    }

    #[test]
    fn message_for_adds_prefix_when_absent() {
        assert_eq!(
            CloudDeckError::Timeout.message_for("list instances"),
            "Could not list instances: The request timed out. Try again or reduce the scope of the load."
        );
    }

    #[test]
    fn message_for_is_idempotent_when_prefix_already_present() {
        let already_prefixed =
            CloudDeckError::Unknown("Could not list instances: something failed".to_string());
        assert_eq!(
            already_prefixed.message_for("list instances"),
            "Could not list instances: something failed"
        );
    }
}
