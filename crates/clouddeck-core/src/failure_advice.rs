use crate::CloudDeckError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advice {
    pub cause: String,
    pub action: Option<String>,
}

impl Advice {
    pub fn text(&self) -> String {
        match &self.action {
            Some(action) => format!("{} {}", self.cause, action),
            None => self.cause.clone(),
        }
    }
}

/// The empty-region message comes from the SDK, CLI and identity paths, all of which
/// report it as an unclassified error.
pub fn failure_advice(error: &CloudDeckError) -> Advice {
    match error {
        CloudDeckError::Auth => Advice {
            cause: "Your AWS sign-in has expired or is no longer valid.".to_string(),
            action: Some("Sign in again, then retry.".to_string()),
        },
        CloudDeckError::Permission => Advice {
            cause: "The selected profile or role is not allowed to make this request.".to_string(),
            action: Some(
                "Check the IAM permissions for that profile or role, then retry.".to_string(),
            ),
        },
        CloudDeckError::Throttled => Advice {
            cause: "AWS is limiting requests from this account.".to_string(),
            action: Some("Wait a moment, then retry.".to_string()),
        },
        CloudDeckError::Network => Advice {
            cause: "CloudDeck could not reach AWS.".to_string(),
            action: Some("Check your network connection, then retry.".to_string()),
        },
        CloudDeckError::Timeout => Advice {
            cause: "AWS did not respond in time.".to_string(),
            action: Some("Retry the load.".to_string()),
        },
        CloudDeckError::NotFound => Advice {
            cause: error.user_message(),
            action: None,
        },
        CloudDeckError::Unknown(message) if message == "AWS region cannot be empty" => Advice {
            cause: "No AWS region is set for this profile.".to_string(),
            action: Some("Choose a region, then retry.".to_string()),
        },
        CloudDeckError::Unknown(message) => Advice {
            cause: message.clone(),
            action: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_sign_in_asks_the_user_to_sign_in_again() {
        let advice = failure_advice(&CloudDeckError::Auth);
        assert_eq!(
            advice.cause,
            "Your AWS sign-in has expired or is no longer valid."
        );
        assert_eq!(advice.action.as_deref(), Some("Sign in again, then retry."));
    }

    #[test]
    fn permission_failures_name_the_profile_or_role() {
        let advice = failure_advice(&CloudDeckError::Permission);
        assert_eq!(
            advice.cause,
            "The selected profile or role is not allowed to make this request."
        );
        assert_eq!(
            advice.action.as_deref(),
            Some("Check the IAM permissions for that profile or role, then retry.")
        );
    }

    #[test]
    fn throttling_asks_the_user_to_retry_later() {
        let advice = failure_advice(&CloudDeckError::Throttled);
        assert_eq!(advice.cause, "AWS is limiting requests from this account.");
        assert_eq!(advice.action.as_deref(), Some("Wait a moment, then retry."));
    }

    #[test]
    fn network_failures_ask_the_user_to_check_connectivity() {
        let advice = failure_advice(&CloudDeckError::Network);
        assert_eq!(advice.cause, "CloudDeck could not reach AWS.");
        assert_eq!(
            advice.action.as_deref(),
            Some("Check your network connection, then retry.")
        );
    }

    #[test]
    fn timeouts_ask_the_user_to_retry() {
        let advice = failure_advice(&CloudDeckError::Timeout);
        assert_eq!(advice.cause, "AWS did not respond in time.");
        assert_eq!(advice.action.as_deref(), Some("Retry the load."));
    }

    #[test]
    fn not_found_uses_the_standard_user_message() {
        let advice = failure_advice(&CloudDeckError::NotFound);
        assert_eq!(advice.cause, CloudDeckError::NotFound.user_message());
        assert_eq!(advice.action, None);
    }

    #[test]
    fn missing_region_asks_the_user_to_choose_one() {
        let advice = failure_advice(&CloudDeckError::Unknown(
            "AWS region cannot be empty".to_string(),
        ));
        assert_eq!(advice.cause, "No AWS region is set for this profile.");
        assert_eq!(
            advice.action.as_deref(),
            Some("Choose a region, then retry.")
        );
    }

    #[test]
    fn unknown_errors_fall_back_to_the_raw_message() {
        let advice = failure_advice(&CloudDeckError::Unknown("something weird".to_string()));
        assert_eq!(advice.cause, "something weird");
        assert_eq!(advice.action, None);
    }

    #[test]
    fn text_joins_cause_and_action_with_a_space() {
        let advice = failure_advice(&CloudDeckError::Throttled);
        assert_eq!(
            advice.text(),
            "AWS is limiting requests from this account. Wait a moment, then retry."
        );
        assert_eq!(
            failure_advice(&CloudDeckError::NotFound).text(),
            CloudDeckError::NotFound.user_message()
        );
    }
}
