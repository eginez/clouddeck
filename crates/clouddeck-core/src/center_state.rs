use crate::CloudDeckError;

/// The centered state shown in the main area when the table has nothing to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CenterState {
    /// Instances are loading and none are available yet.
    Loading,
    /// The fleet has no instances in the current scope.
    EmptyFleet,
    /// Instances are loaded but the query filter hides all of them.
    NoMatch,
    /// The load failed because credentials are invalid or expired.
    SignedOut,
    /// The load failed because the network is unreachable.
    Offline,
    /// The load failed because the service throttled the request.
    Throttled,
    /// The load failed for another reason; carries the user-facing message.
    Error { message: String },
}

/// Inputs that decide which centered state applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CenterInput {
    /// Whether a load is in progress.
    pub is_loading: bool,
    /// Instances loaded for the current scope, before the query filter.
    pub loaded_count: usize,
    /// Instances left after the query filter.
    pub shown_count: usize,
    /// The last load's error, if the whole load failed.
    pub error: Option<CloudDeckError>,
}

/// Returns the centered state to show, or `None` when the table should be shown.
pub fn center_state(input: &CenterInput) -> Option<CenterState> {
    if input.loaded_count > 0 {
        return if input.shown_count > 0 {
            None
        } else {
            Some(CenterState::NoMatch)
        };
    }

    if input.is_loading {
        return Some(CenterState::Loading);
    }

    match &input.error {
        Some(CloudDeckError::Auth) => Some(CenterState::SignedOut),
        Some(CloudDeckError::Network) => Some(CenterState::Offline),
        Some(CloudDeckError::Throttled) => Some(CenterState::Throttled),
        Some(error) => Some(CenterState::Error {
            message: error.user_message(),
        }),
        None => Some(CenterState::EmptyFleet),
    }
}

/// Profile, scope and last-good data used to word a centered state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CenterContext {
    /// The active AWS profile name.
    pub profile: String,
    /// The label for the current region scope.
    pub scope_label: String,
    /// Whether the scope covers all selected regions.
    pub scope_is_all: bool,
    /// The current search query, empty when none.
    pub query: String,
    /// Time of the last good load, formatted as "HH:MM".
    pub last_good: Option<String>,
    /// Seconds until the next automatic retry, if one is scheduled.
    pub retry_in_seconds: Option<u32>,
}

/// Title, body and optional action for a centered state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CenterText {
    /// Headline of the centered state.
    pub title: String,
    /// Explanatory text under the headline.
    pub body: String,
    /// Label of the offered button, if any.
    pub action: Option<String>,
    /// Shell command shown with the action, if any.
    pub command: Option<String>,
}

fn text(title: &str, body: String) -> CenterText {
    CenterText {
        title: title.to_string(),
        body,
        action: None,
        command: None,
    }
}

/// Returns the title, body, action and command for a centered state.
pub fn center_text(state: &CenterState, ctx: &CenterContext) -> CenterText {
    match state {
        CenterState::Loading => text(
            "Loading instances",
            format!("{} · {}", ctx.profile, ctx.scope_label),
        ),
        CenterState::EmptyFleet => {
            if ctx.scope_is_all {
                text(
                    "No instances",
                    format!(
                        "{} has no EC2 instances in the selected regions.",
                        ctx.profile
                    ),
                )
            } else {
                CenterText {
                    action: Some("Select all regions".to_string()),
                    ..text(
                        &format!("No instances in {}", ctx.scope_label),
                        format!(
                            "{} has no EC2 instances in this region. Try other regions.",
                            ctx.profile
                        ),
                    )
                }
            }
        }
        CenterState::NoMatch => {
            let body = if ctx.query.is_empty() {
                "No instances match these filters.".to_string()
            } else {
                format!(
                    "No instances match “{}”. Press Esc to clear.",
                    ctx.query
                )
            };
            CenterText {
                action: Some("Clear filters".to_string()),
                ..text("No instances match", body)
            }
        }
        CenterState::SignedOut => CenterText {
            action: Some("Retry".to_string()),
            ..text(
                &format!("Credentials for {} need attention", ctx.profile),
                "AWS rejected this profile’s credentials. Renew them with the AWS CLI, then retry."
                    .to_string(),
            )
        },
        CenterState::Offline => text(
            "You’re offline",
            match &ctx.last_good {
                Some(time) => format!(
                    "Showing the last data from {time}. CloudDeck will refresh when the connection returns."
                ),
                None => "CloudDeck will refresh when the connection returns.".to_string(),
            },
        ),
        CenterState::Throttled => CenterText {
            action: Some("Retry now".to_string()),
            ..text(
                "AWS is rate limiting requests",
                match &ctx.last_good {
                    Some(time) => format!(
                        "CloudDeck will retry on its own, with a growing delay. The data shown is from {time}."
                    ),
                    None => {
                        "CloudDeck will retry on its own, with a growing delay.".to_string()
                    }
                },
            )
        },
        CenterState::Error { message } => CenterText {
            action: Some("Retry".to_string()),
            ..text("Couldn’t load instances", message.clone())
        },
    }
}

/// Returns the short status-bar text for an error state, or `None` for other states.
pub fn error_strip_text(state: &CenterState, ctx: &CenterContext) -> Option<String> {
    match state {
        CenterState::Offline => Some("Offline".to_string()),
        CenterState::Throttled => Some(match ctx.retry_in_seconds {
            Some(seconds) => format!("Throttled · retry in {seconds}s"),
            None => "Throttled".to_string(),
        }),
        CenterState::SignedOut => Some("Check credentials".to_string()),
        CenterState::Error { .. } => Some("Couldn’t load".to_string()),
        CenterState::Loading | CenterState::EmptyFleet | CenterState::NoMatch => None,
    }
}

/// Returns the short reason shown for a region that failed to load.
pub fn region_failure_reason(error: &CloudDeckError, retry_in_seconds: Option<u32>) -> String {
    match error {
        CloudDeckError::Throttled => match retry_in_seconds {
            Some(seconds) => format!("Throttled · retrying in {seconds}s"),
            None => "Throttled".to_string(),
        },
        CloudDeckError::Auth => "Signed out".to_string(),
        CloudDeckError::Network => "Offline".to_string(),
        CloudDeckError::Timeout => "Timed out".to_string(),
        CloudDeckError::Permission => "Access denied".to_string(),
        CloudDeckError::NotFound => "Not found".to_string(),
        CloudDeckError::Unknown(_) => "Failed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> CenterInput {
        CenterInput {
            is_loading: false,
            loaded_count: 0,
            shown_count: 0,
            error: None,
        }
    }

    #[test]
    fn shows_table_when_rows_are_visible() {
        let input = CenterInput {
            loaded_count: 20,
            shown_count: 20,
            ..input()
        };
        assert_eq!(center_state(&input), None);
    }

    #[test]
    fn shows_table_when_rows_are_visible_while_loading() {
        let input = CenterInput {
            is_loading: true,
            loaded_count: 20,
            shown_count: 20,
            ..input()
        };
        assert_eq!(center_state(&input), None);
    }

    #[test]
    fn shows_no_match_when_filter_hides_all_loaded_rows() {
        let input = CenterInput {
            loaded_count: 20,
            shown_count: 0,
            ..input()
        };
        assert_eq!(center_state(&input), Some(CenterState::NoMatch));
    }

    #[test]
    fn shows_no_match_even_when_error_is_present() {
        let input = CenterInput {
            loaded_count: 20,
            shown_count: 0,
            error: Some(CloudDeckError::Network),
            ..input()
        };
        assert_eq!(center_state(&input), Some(CenterState::NoMatch));
    }

    #[test]
    fn shows_loading_while_loading() {
        let input = CenterInput {
            is_loading: true,
            ..input()
        };
        assert_eq!(center_state(&input), Some(CenterState::Loading));
    }

    #[test]
    fn shows_loading_over_auth_error() {
        let input = CenterInput {
            is_loading: true,
            error: Some(CloudDeckError::Auth),
            ..input()
        };
        assert_eq!(center_state(&input), Some(CenterState::Loading));
    }

    #[test]
    fn auth_error_shows_signed_out() {
        let input = CenterInput {
            error: Some(CloudDeckError::Auth),
            ..input()
        };
        assert_eq!(center_state(&input), Some(CenterState::SignedOut));
    }

    #[test]
    fn network_error_shows_offline() {
        let input = CenterInput {
            error: Some(CloudDeckError::Network),
            ..input()
        };
        assert_eq!(center_state(&input), Some(CenterState::Offline));
    }

    #[test]
    fn throttled_error_shows_throttled() {
        let input = CenterInput {
            error: Some(CloudDeckError::Throttled),
            ..input()
        };
        assert_eq!(center_state(&input), Some(CenterState::Throttled));
    }

    #[test]
    fn permission_error_shows_error_with_user_message() {
        let input = CenterInput {
            error: Some(CloudDeckError::Permission),
            ..input()
        };
        assert_eq!(
            center_state(&input),
            Some(CenterState::Error {
                message: CloudDeckError::Permission.user_message(),
            })
        );
    }

    #[test]
    fn unknown_error_shows_error_with_its_message() {
        let input = CenterInput {
            error: Some(CloudDeckError::Unknown("boom".to_string())),
            ..input()
        };
        assert_eq!(
            center_state(&input),
            Some(CenterState::Error {
                message: "boom".to_string(),
            })
        );
    }

    #[test]
    fn timeout_error_shows_error_with_user_message() {
        let input = CenterInput {
            error: Some(CloudDeckError::Timeout),
            ..input()
        };
        assert_eq!(
            center_state(&input),
            Some(CenterState::Error {
                message: CloudDeckError::Timeout.user_message(),
            })
        );
    }

    #[test]
    fn empty_input_shows_empty_fleet() {
        assert_eq!(center_state(&input()), Some(CenterState::EmptyFleet));
    }

    fn ctx() -> CenterContext {
        CenterContext {
            profile: "prod-readonly".to_string(),
            scope_label: "us-east-1".to_string(),
            scope_is_all: false,
            query: String::new(),
            last_good: Some("12:04".to_string()),
            retry_in_seconds: Some(8),
        }
    }

    fn expected(
        title: &str,
        body: &str,
        action: Option<&str>,
        command: Option<&str>,
    ) -> CenterText {
        CenterText {
            title: title.to_string(),
            body: body.to_string(),
            action: action.map(str::to_string),
            command: command.map(str::to_string),
        }
    }

    #[test]
    fn loading_text_names_profile_and_scope() {
        assert_eq!(
            center_text(&CenterState::Loading, &ctx()),
            expected("Loading instances", "prod-readonly · us-east-1", None, None)
        );
    }

    #[test]
    fn empty_fleet_text_offers_all_regions_when_scope_is_one_region() {
        assert_eq!(
            center_text(&CenterState::EmptyFleet, &ctx()),
            expected(
                "No instances in us-east-1",
                "prod-readonly has no EC2 instances in this region. Try other regions.",
                Some("Select all regions"),
                None
            )
        );
    }

    #[test]
    fn empty_fleet_text_for_all_regions_has_no_action() {
        let ctx = CenterContext {
            scope_label: "All regions".to_string(),
            scope_is_all: true,
            ..ctx()
        };
        assert_eq!(
            center_text(&CenterState::EmptyFleet, &ctx),
            expected(
                "No instances",
                "prod-readonly has no EC2 instances in the selected regions.",
                None,
                None
            )
        );
    }

    #[test]
    fn no_match_text_quotes_query() {
        let ctx = CenterContext {
            query: "bastion".to_string(),
            ..ctx()
        };
        assert_eq!(
            center_text(&CenterState::NoMatch, &ctx),
            expected(
                "No instances match",
                "No instances match “bastion”. Press Esc to clear.",
                Some("Clear filters"),
                None
            )
        );
    }

    #[test]
    fn no_match_text_without_query_describes_filters() {
        assert_eq!(
            center_text(&CenterState::NoMatch, &ctx()),
            expected(
                "No instances match",
                "No instances match these filters.",
                Some("Clear filters"),
                None
            )
        );
    }

    #[test]
    fn signed_out_text_is_neutral_across_credential_types() {
        assert_eq!(
            center_text(&CenterState::SignedOut, &ctx()),
            expected(
                "Credentials for prod-readonly need attention",
                "AWS rejected this profile’s credentials. Renew them with the AWS CLI, then retry.",
                Some("Retry"),
                None
            )
        );
    }

    #[test]
    fn offline_text_shows_last_good_time() {
        assert_eq!(
            center_text(&CenterState::Offline, &ctx()),
            expected(
                "You’re offline",
                "Showing the last data from 12:04. CloudDeck will refresh when the connection returns.",
                None,
                None
            )
        );
    }

    #[test]
    fn offline_text_without_last_good_time() {
        let ctx = CenterContext {
            last_good: None,
            ..ctx()
        };
        assert_eq!(
            center_text(&CenterState::Offline, &ctx),
            expected(
                "You’re offline",
                "CloudDeck will refresh when the connection returns.",
                None,
                None
            )
        );
    }

    #[test]
    fn throttled_text_shows_last_good_time_and_retry_action() {
        assert_eq!(
            center_text(&CenterState::Throttled, &ctx()),
            expected(
                "AWS is rate limiting requests",
                "CloudDeck will retry on its own, with a growing delay. The data shown is from 12:04.",
                Some("Retry now"),
                None
            )
        );
    }

    #[test]
    fn error_text_shows_message_and_retry_action() {
        assert_eq!(
            center_text(
                &CenterState::Error {
                    message: "boom".to_string()
                },
                &ctx()
            ),
            expected("Couldn’t load instances", "boom", Some("Retry"), None)
        );
    }

    #[test]
    fn error_strip_text_for_each_state() {
        assert_eq!(
            error_strip_text(&CenterState::Throttled, &ctx()),
            Some("Throttled · retry in 8s".to_string())
        );
        assert_eq!(
            error_strip_text(
                &CenterState::Throttled,
                &CenterContext {
                    retry_in_seconds: None,
                    ..ctx()
                }
            ),
            Some("Throttled".to_string())
        );
        assert_eq!(
            error_strip_text(&CenterState::Offline, &ctx()),
            Some("Offline".to_string())
        );
        assert_eq!(
            error_strip_text(&CenterState::SignedOut, &ctx()),
            Some("Check credentials".to_string())
        );
        assert_eq!(
            error_strip_text(
                &CenterState::Error {
                    message: "boom".to_string()
                },
                &ctx()
            ),
            Some("Couldn’t load".to_string())
        );
        assert_eq!(error_strip_text(&CenterState::Loading, &ctx()), None);
    }

    #[test]
    fn region_failure_reasons_are_short_labels() {
        assert_eq!(
            region_failure_reason(&CloudDeckError::Throttled, Some(8)),
            "Throttled · retrying in 8s"
        );
        assert_eq!(
            region_failure_reason(&CloudDeckError::Throttled, None),
            "Throttled"
        );
        assert_eq!(
            region_failure_reason(&CloudDeckError::Permission, None),
            "Access denied"
        );
        assert_eq!(
            region_failure_reason(&CloudDeckError::Unknown("x".to_string()), None),
            "Failed"
        );
    }
}
