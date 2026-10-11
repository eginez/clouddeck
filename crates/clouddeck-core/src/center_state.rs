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
}
