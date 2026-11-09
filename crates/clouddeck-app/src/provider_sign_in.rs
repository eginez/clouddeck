//! Drives a provider's generic sign-in flow (`continue_auth`) from the first step
//! `begin_auth` returned, reporting each step to the Connect window. The
//! step-to-action logic lives in clouddeck-core's `provider_sign_in`.

use std::sync::Arc;
use std::time::Instant;

use clouddeck_core::{
    AuthAnswer, AuthStep, ErrorKind, FlowId, NoAuth, Provider, ProviderError, SignInAction,
    sign_in_action,
};
use gpui::AsyncApp;

use crate::provider_call::{off_thread, with_call};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Progress {
    Action(SignInAction),
    Failed(ProviderError),
}

/// Returns whether the flow signed in. `report` returns `false` to stop the flow; the
/// driver then cancels it at the provider.
pub(crate) async fn drive(
    provider: Arc<dyn Provider>,
    first: AuthStep,
    mut report: impl FnMut(Progress, &mut AsyncApp) -> bool,
    cx: &mut AsyncApp,
) -> bool {
    let mut opened: Vec<FlowId> = Vec::new();
    let mut deadline: Option<Instant> = None;
    let mut step = first;
    loop {
        if let AuthStep::DeviceCode { expires_in, .. } = &step {
            deadline.get_or_insert_with(|| Instant::now() + *expires_in);
        }
        let action = sign_in_action(step);
        match action {
            SignInAction::Finish => {
                report(Progress::Action(action), cx);
                return true;
            }
            SignInAction::NeedsInput { flow } => {
                cancel_flow(provider.clone(), flow, cx).await;
                report(
                    Progress::Failed(ProviderError::new(
                        ErrorKind::Unsupported,
                        "the sign-in asks for input CloudDeck cannot show yet",
                    )),
                    cx,
                );
                return false;
            }
            SignInAction::OpenBrowser {
                ref flow,
                ref url,
                poll,
                ..
            } => {
                let flow = flow.clone();
                if !report(Progress::Action(action.clone()), cx) {
                    cancel_flow(provider.clone(), flow, cx).await;
                    return false;
                }
                if !opened.contains(&flow) {
                    opened.push(flow.clone());
                    let _ = cx.update(|cx| cx.open_url(url));
                }
                if !poll {
                    return false;
                }
                if deadline_passed(deadline) {
                    cancel_flow(provider.clone(), flow, cx).await;
                    report(Progress::Failed(expired_error()), cx);
                    return false;
                }
                let polled = off_thread(cx, {
                    let provider = provider.clone();
                    move || {
                        with_call(|call| {
                            provider.auth().unwrap_or(&NoAuth).continue_auth(
                                call,
                                &flow,
                                AuthAnswer::Poll,
                            )
                        })
                    }
                })
                .await;
                match polled {
                    Ok(next) => step = next,
                    Err(error) => {
                        report(Progress::Failed(error), cx);
                        return false;
                    }
                }
            }
        }
    }
}

pub(crate) fn deadline_passed(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|at| Instant::now() >= at)
}

pub(crate) fn expired_error() -> ProviderError {
    ProviderError::new(ErrorKind::Timeout, "the sign-in code expired")
}

pub(crate) async fn cancel_flow(provider: Arc<dyn Provider>, flow: FlowId, cx: &AsyncApp) {
    let _ = off_thread(cx, move || {
        with_call(|call| {
            provider
                .auth()
                .unwrap_or(&NoAuth)
                .continue_auth(call, &flow, AuthAnswer::Cancel)
        })
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::sync::Mutex;
    use std::time::Duration;

    use clouddeck_core::{
        Auth, AuthInput, AuthMethod, Browse, Call, Connection, ConnectionId, Dimension, FieldDef,
        Instance, KindId, KindInfo, ListResult, ProviderInfo, Selection,
    };
    use gpui::TestAppContext;

    struct FakeSignIn {
        polls: Mutex<VecDeque<AuthStep>>,
        answers: Mutex<Vec<AuthAnswer>>,
    }

    impl FakeSignIn {
        fn new(polls: Vec<AuthStep>) -> Arc<Self> {
            Arc::new(Self {
                polls: Mutex::new(polls.into()),
                answers: Mutex::new(Vec::new()),
            })
        }

        fn answers(&self) -> Vec<AuthAnswer> {
            self.answers.lock().unwrap().clone()
        }
    }

    impl Provider for FakeSignIn {
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

    impl Browse for FakeSignIn {
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

    impl Auth for FakeSignIn {
        fn auth_methods(&self) -> Vec<AuthMethod> {
            vec![AuthMethod::new(
                "browser".to_string(),
                "Browser".to_string(),
            )]
        }

        fn begin_auth(
            &self,
            _cx: &Call<'_>,
            _method: &AuthMethod,
            _input: AuthInput,
        ) -> Result<AuthStep, ProviderError> {
            Err(ProviderError::unsupported())
        }

        fn continue_auth(
            &self,
            _cx: &Call<'_>,
            _flow: &FlowId,
            answer: AuthAnswer,
        ) -> Result<AuthStep, ProviderError> {
            self.answers.lock().unwrap().push(answer);
            self.polls
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(ProviderError::unsupported)
        }
    }

    fn device_code(expires_in: Duration) -> AuthStep {
        AuthStep::DeviceCode {
            flow: FlowId::from("flow-1"),
            code: "ABCD".to_string(),
            url: "https://example.test/device".to_string(),
            complete_url: Some("https://example.test/complete".to_string()),
            expires_in,
        }
    }

    struct Run {
        reports: Rc<RefCell<Vec<Progress>>>,
        signed_in: Rc<Cell<Option<bool>>>,
    }

    fn run(
        cx: &mut TestAppContext,
        provider: &Arc<FakeSignIn>,
        first: AuthStep,
        keep_going: bool,
    ) -> Run {
        let reports = Rc::new(RefCell::new(Vec::new()));
        let signed_in = Rc::new(Cell::new(None));
        let provider: Arc<dyn Provider> = provider.clone();
        let sink = reports.clone();
        let result = signed_in.clone();
        cx.update(|cx| {
            cx.spawn(async move |cx| {
                let done = drive(
                    provider,
                    first,
                    move |progress, _| {
                        sink.borrow_mut().push(progress);
                        keep_going
                    },
                    cx,
                )
                .await;
                result.set(Some(done));
            })
            .detach();
        });
        cx.run_until_parked();
        Run { reports, signed_in }
    }

    #[gpui::test]
    fn device_code_opens_the_url_and_polls_until_done(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(vec![AuthStep::Done]);
        let run = run(cx, &provider, device_code(Duration::from_secs(900)), true);

        assert_eq!(run.signed_in.get(), Some(true));
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.test/complete")
        );
        assert_eq!(provider.answers(), vec![AuthAnswer::Poll]);
        assert_eq!(
            *run.reports.borrow(),
            vec![
                Progress::Action(SignInAction::OpenBrowser {
                    flow: FlowId::from("flow-1"),
                    url: "https://example.test/complete".to_string(),
                    code: Some("ABCD".to_string()),
                    poll: true,
                }),
                Progress::Action(SignInAction::Finish),
            ]
        );
    }

    #[gpui::test]
    fn device_code_past_its_expiry_is_cancelled_and_reported(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(vec![AuthStep::Done]);
        let run = run(cx, &provider, device_code(Duration::ZERO), true);

        assert_eq!(run.signed_in.get(), Some(false));
        assert_eq!(provider.answers(), vec![AuthAnswer::Cancel]);
        assert_eq!(
            run.reports.borrow().last(),
            Some(&Progress::Failed(expired_error()))
        );
    }

    #[gpui::test]
    fn a_report_that_stops_the_flow_cancels_it_at_the_provider(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(vec![AuthStep::Done]);
        let run = run(cx, &provider, device_code(Duration::from_secs(900)), false);

        assert_eq!(run.signed_in.get(), Some(false));
        assert_eq!(provider.answers(), vec![AuthAnswer::Cancel]);
    }

    #[gpui::test]
    fn prompt_flow_is_cancelled_and_reported(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(vec![]);
        let first = AuthStep::Prompt {
            flow: FlowId::from("flow-2"),
            fields: vec![],
        };
        let run = run(cx, &provider, first, true);

        assert_eq!(run.signed_in.get(), Some(false));
        assert_eq!(provider.answers(), vec![AuthAnswer::Cancel]);
        assert!(matches!(
            run.reports.borrow().as_slice(),
            [Progress::Failed(error)] if error.kind == ErrorKind::Unsupported
        ));
    }
}
