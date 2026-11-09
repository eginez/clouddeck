use std::collections::BTreeMap;

use crate::{AuthInput, AuthStep, Call, FlowId, NoAuth, Provider, ProviderError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInAction {
    Finish,
    OpenBrowser {
        flow: FlowId,
        url: String,
        code: Option<String>,
        poll: bool,
    },
    NeedsInput {
        flow: FlowId,
    },
}

pub fn sign_in_action(step: AuthStep) -> SignInAction {
    match step {
        AuthStep::Done => SignInAction::Finish,
        AuthStep::DeviceCode {
            flow,
            code,
            url,
            complete_url,
            ..
        } => SignInAction::OpenBrowser {
            flow,
            url: complete_url.unwrap_or(url),
            code: Some(code),
            poll: true,
        },
        AuthStep::OpenUrl {
            flow,
            url,
            then_poll,
        } => SignInAction::OpenBrowser {
            flow,
            url,
            code: None,
            poll: then_poll,
        },
        AuthStep::Prompt { flow, .. } => SignInAction::NeedsInput { flow },
    }
}

pub fn start_sign_in(provider: &dyn Provider, call: &Call<'_>) -> Result<AuthStep, ProviderError> {
    let auth = provider.auth().unwrap_or(&NoAuth);
    let method = auth
        .auth_methods()
        .into_iter()
        .next()
        .ok_or_else(ProviderError::unsupported)?;
    auth.begin_auth(
        call,
        &method,
        AuthInput {
            connection: None,
            values: BTreeMap::new(),
        },
    )
}

pub fn sign_in_message(provider_name: &str, action: &SignInAction) -> String {
    match action {
        SignInAction::Finish => format!("Signed in to {provider_name}"),
        SignInAction::OpenBrowser {
            code: Some(code), ..
        } => format!("Enter code {code} in the browser to sign in to {provider_name}"),
        SignInAction::OpenBrowser { code: None, .. } => {
            format!("Finish signing in to {provider_name} in the browser")
        }
        SignInAction::NeedsInput { .. } => {
            format!("{provider_name} sign-in needs input CloudDeck cannot show yet")
        }
    }
}

pub fn sign_in_failed_message(provider_name: &str, error: &ProviderError) -> String {
    format!("{provider_name} sign-in failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Auth, AuthMethod, Browse, Connection, ConnectionId, Dimension, ErrorKind, FieldDef, Host,
        Instance, KindId, KindInfo, ListResult, LogLevel, ProviderInfo, Selection,
    };
    use std::path::PathBuf;
    use std::time::Duration;

    struct NoopHost;

    impl Host for NoopHost {
        fn log(&self, _level: LogLevel, _message: &str) {}
        fn data_dir(&self) -> PathBuf {
            PathBuf::new()
        }
        fn setting(&self, _key: &str) -> Option<String> {
            None
        }
        fn secret(&self, _key: &str) -> Option<String> {
            None
        }
        fn set_secret(&self, _key: &str, _value: Option<&str>) -> Result<(), ProviderError> {
            Ok(())
        }
        fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
            Ok(())
        }
        fn cancelled(&self) -> bool {
            false
        }
    }

    fn with_test_call<T>(work: impl FnOnce(&Call<'_>) -> T) -> T {
        let host = NoopHost;
        work(&Call {
            deadline: None,
            host: &host,
        })
    }

    struct SignInProvider {
        methods: bool,
    }

    impl Provider for SignInProvider {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "fake".to_string(),
                name: "Fake Cloud".to_string(),
                version: "0".to_string(),
                connection_label: "Project".to_string(),
            }
        }

        fn auth(&self) -> Option<&dyn Auth> {
            Some(self)
        }
    }

    impl Browse for SignInProvider {
        fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
            Ok(vec![])
        }

        fn dimensions(
            &self,
            _cx: &Call<'_>,
            _connection: &ConnectionId,
        ) -> Result<Vec<Dimension>, ProviderError> {
            Ok(vec![])
        }

        fn kinds(&self) -> Vec<KindInfo> {
            vec![]
        }

        fn list(
            &self,
            _cx: &Call<'_>,
            _connection: &ConnectionId,
            _kind: &KindId,
            _selection: &Selection,
        ) -> ListResult {
            ListResult::default()
        }

        fn console_url(
            &self,
            _connection: &ConnectionId,
            _kind: &KindId,
            _resource: &Instance,
        ) -> Option<String> {
            None
        }

        fn fields(&self, _kind: &KindId) -> Vec<FieldDef> {
            vec![]
        }
    }

    impl Auth for SignInProvider {
        fn auth_methods(&self) -> Vec<AuthMethod> {
            if self.methods {
                vec![AuthMethod::new(
                    "browser".to_string(),
                    "Browser".to_string(),
                )]
            } else {
                vec![]
            }
        }

        fn begin_auth(
            &self,
            _cx: &Call<'_>,
            _method: &AuthMethod,
            input: AuthInput,
        ) -> Result<AuthStep, ProviderError> {
            if input.connection.is_some() || !input.values.is_empty() {
                return Err(ProviderError::new(ErrorKind::Other, "unexpected input"));
            }
            Ok(device_code("flow-1", Some("https://example.test/complete")))
        }
    }

    fn device_code(flow: &str, complete_url: Option<&str>) -> AuthStep {
        AuthStep::DeviceCode {
            flow: FlowId::from(flow),
            code: "ABCD".to_string(),
            url: "https://example.test/device".to_string(),
            complete_url: complete_url.map(str::to_string),
            expires_in: Duration::from_secs(900),
        }
    }

    #[test]
    fn done_finishes_sign_in() {
        assert_eq!(sign_in_action(AuthStep::Done), SignInAction::Finish);
    }

    #[test]
    fn device_code_opens_the_complete_url_with_its_code_and_polls() {
        assert_eq!(
            sign_in_action(device_code("flow-1", Some("https://example.test/complete"))),
            SignInAction::OpenBrowser {
                flow: FlowId::from("flow-1"),
                url: "https://example.test/complete".to_string(),
                code: Some("ABCD".to_string()),
                poll: true,
            }
        );
    }

    #[test]
    fn device_code_without_a_complete_url_opens_the_plain_url() {
        assert_eq!(
            sign_in_action(device_code("flow-1", None)),
            SignInAction::OpenBrowser {
                flow: FlowId::from("flow-1"),
                url: "https://example.test/device".to_string(),
                code: Some("ABCD".to_string()),
                poll: true,
            }
        );
    }

    #[test]
    fn open_url_keeps_its_poll_flag_and_has_no_code() {
        let step = AuthStep::OpenUrl {
            flow: FlowId::from("flow-2"),
            url: "https://example.test/open".to_string(),
            then_poll: false,
        };
        assert_eq!(
            sign_in_action(step),
            SignInAction::OpenBrowser {
                flow: FlowId::from("flow-2"),
                url: "https://example.test/open".to_string(),
                code: None,
                poll: false,
            }
        );
    }

    #[test]
    fn prompt_needs_input() {
        let step = AuthStep::Prompt {
            flow: FlowId::from("flow-3"),
            fields: vec![],
        };
        assert_eq!(
            sign_in_action(step),
            SignInAction::NeedsInput {
                flow: FlowId::from("flow-3"),
            }
        );
    }

    #[test]
    fn start_sign_in_without_auth_methods_is_unsupported() {
        let provider = SignInProvider { methods: false };
        let error = with_test_call(|call| start_sign_in(&provider, call)).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unsupported);
    }

    #[test]
    fn start_sign_in_begins_the_first_method_without_a_connection() {
        let provider = SignInProvider { methods: true };
        let step = with_test_call(|call| start_sign_in(&provider, call)).unwrap();
        assert_eq!(
            step,
            device_code("flow-1", Some("https://example.test/complete"))
        );
    }

    #[test]
    fn messages_name_the_provider_and_the_code() {
        let browser = SignInAction::OpenBrowser {
            flow: FlowId::from("flow-1"),
            url: "https://example.test".to_string(),
            code: Some("ABCD".to_string()),
            poll: true,
        };
        assert_eq!(
            sign_in_message("Fake Cloud", &browser),
            "Enter code ABCD in the browser to sign in to Fake Cloud"
        );
        assert_eq!(
            sign_in_message("Fake Cloud", &SignInAction::Finish),
            "Signed in to Fake Cloud"
        );
        assert_eq!(
            sign_in_message(
                "Fake Cloud",
                &SignInAction::OpenBrowser {
                    flow: FlowId::from("flow-2"),
                    url: "https://example.test".to_string(),
                    code: None,
                    poll: false,
                }
            ),
            "Finish signing in to Fake Cloud in the browser"
        );
        assert_eq!(
            sign_in_message(
                "Fake Cloud",
                &SignInAction::NeedsInput {
                    flow: FlowId::from("flow-3"),
                }
            ),
            "Fake Cloud sign-in needs input CloudDeck cannot show yet"
        );
    }

    #[test]
    fn failure_message_includes_the_provider_error() {
        let error = ProviderError::new(ErrorKind::Auth, "token expired");
        assert_eq!(
            sign_in_failed_message("Fake Cloud", &error),
            "Fake Cloud sign-in failed: token expired"
        );
    }
}
