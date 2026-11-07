//! Runs a provider's generic sign-in flow (`begin_auth` / `continue_auth`) and
//! reports progress in the fleet's status flash. The pure step-to-message logic
//! lives in clouddeck-core's `provider_sign_in`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use clouddeck_core::{
    AuthAnswer, AuthStep, ErrorKind, FlowId, Provider, ProviderError, SignInAction,
    sign_in_action, sign_in_failed_message, sign_in_message, start_sign_in,
};
use gpui::{App, AsyncApp, Entity};

use crate::account_connect::{off_thread, with_call};
use crate::fleet_state::FleetState;

/// Starts `provider`'s sign-in unless one is already running for it. Returns
/// whether a new flow started.
pub fn begin_provider_sign_in(
    provider: Arc<dyn Provider>,
    fleet: Entity<FleetState>,
    in_flight: Rc<RefCell<HashSet<String>>>,
    cx: &mut App,
) -> bool {
    let id = provider.info().id;
    if !in_flight.borrow_mut().insert(id.clone()) {
        return false;
    }
    cx.spawn(async move |cx| {
        drive(provider, &fleet, cx).await;
        in_flight.borrow_mut().remove(&id);
    })
    .detach();
    true
}

async fn drive(provider: Arc<dyn Provider>, fleet: &Entity<FleetState>, cx: &mut AsyncApp) {
    let name = provider.info().name;
    let mut opened: Vec<FlowId> = Vec::new();
    let mut deadline: Option<Instant> = None;
    let mut step = match off_thread(cx, {
        let provider = provider.clone();
        move || with_call(|call| start_sign_in(provider.as_ref(), call))
    })
    .await
    {
        Ok(step) => step,
        Err(error) => {
            show(fleet, sign_in_failed_message(&name, &error), cx);
            return;
        }
    };
    loop {
        if let AuthStep::DeviceCode { expires_in, .. } = &step {
            deadline.get_or_insert_with(|| Instant::now() + *expires_in);
        }
        let action = sign_in_action(step);
        let message = sign_in_message(&name, &action);
        match action {
            SignInAction::Finish => {
                show(fleet, message, cx);
                return;
            }
            SignInAction::NeedsInput { flow } => {
                cancel_flow(provider.clone(), flow, cx).await;
                show(fleet, message, cx);
                return;
            }
            SignInAction::OpenBrowser {
                flow, url, poll, ..
            } => {
                if !opened.contains(&flow) {
                    opened.push(flow.clone());
                    let _ = cx.update(|cx| cx.open_url(&url));
                }
                if !show(fleet, message, cx) || !poll {
                    return;
                }
                if deadline_passed(deadline) {
                    cancel_flow(provider.clone(), flow, cx).await;
                    show(
                        fleet,
                        sign_in_failed_message(&name, &expired_error()),
                        cx,
                    );
                    return;
                }
                let polled = off_thread(cx, {
                    let provider = provider.clone();
                    move || with_call(|call| provider.continue_auth(call, &flow, AuthAnswer::Poll))
                })
                .await;
                match polled {
                    Ok(next) => step = next,
                    Err(error) => {
                        show(fleet, sign_in_failed_message(&name, &error), cx);
                        return;
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
        with_call(|call| provider.continue_auth(call, &flow, AuthAnswer::Cancel))
    })
    .await;
}

/// Returns `false` once the fleet has been dropped, so the flow can stop.
fn show(fleet: &Entity<FleetState>, text: String, cx: &mut AsyncApp) -> bool {
    fleet
        .update(cx, |fleet, cx| fleet.set_flash(text, cx))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use clouddeck_core::{
        AuthInput, AuthMethod, AuthStep, Call, Connection, ConnectionId, Dimension, ErrorKind,
        FieldDef, Instance, InstanceListViewModel, KindId, KindInfo, ListResult, ProviderError,
        ProviderInfo, Selection,
    };
    use gpui::{AppContext as _, TestAppContext};

    struct FakeSignIn {
        begin: Result<AuthStep, ProviderError>,
        polls: Mutex<VecDeque<AuthStep>>,
        answers: Mutex<Vec<AuthAnswer>>,
        begin_calls: AtomicUsize,
    }

    impl FakeSignIn {
        fn new(begin: Result<AuthStep, ProviderError>, polls: Vec<AuthStep>) -> Arc<Self> {
            Arc::new(Self {
                begin,
                polls: Mutex::new(polls.into()),
                answers: Mutex::new(Vec::new()),
                begin_calls: AtomicUsize::new(0),
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

        fn auth_methods(&self) -> Vec<AuthMethod> {
            vec![AuthMethod {
                id: "browser".to_string(),
                label: "Browser".to_string(),
            }]
        }

        fn begin_auth(
            &self,
            _cx: &Call<'_>,
            _method: &AuthMethod,
            _input: AuthInput,
        ) -> Result<AuthStep, ProviderError> {
            self.begin_calls.fetch_add(1, Ordering::SeqCst);
            self.begin.clone()
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

    fn device_code(complete_url: Option<&str>) -> AuthStep {
        AuthStep::DeviceCode {
            flow: FlowId::from("flow-1"),
            code: "ABCD".to_string(),
            url: "https://example.test/device".to_string(),
            complete_url: complete_url.map(str::to_string),
            expires_in: Duration::from_secs(900),
        }
    }

    fn fleet_for(cx: &mut TestAppContext, provider: Arc<dyn Provider>) -> Entity<FleetState> {
        cx.new(|_| FleetState::new(InstanceListViewModel::new(provider), "p".into(), false))
    }

    fn start(
        cx: &mut TestAppContext,
        provider: &Arc<FakeSignIn>,
        fleet: &Entity<FleetState>,
        in_flight: &Rc<RefCell<HashSet<String>>>,
    ) -> bool {
        let provider: Arc<dyn Provider> = provider.clone();
        cx.update(|cx| begin_provider_sign_in(provider, fleet.clone(), in_flight.clone(), cx))
    }

    fn flash(cx: &mut TestAppContext, fleet: &Entity<FleetState>) -> Option<String> {
        fleet.read_with(cx, |fleet, _| fleet.flash.clone())
    }

    #[gpui::test]
    fn device_code_flow_opens_the_url_polls_until_done(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(
            Ok(device_code(Some("https://example.test/complete"))),
            vec![device_code(None), AuthStep::Done],
        );
        let fleet = fleet_for(cx, provider.clone());
        let in_flight = Rc::default();

        assert!(start(cx, &provider, &fleet, &in_flight));
        cx.run_until_parked();

        assert_eq!(
            flash(cx, &fleet).as_deref(),
            Some("Signed in to Fake Cloud")
        );
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.test/complete")
        );
        assert_eq!(provider.answers(), vec![AuthAnswer::Poll, AuthAnswer::Poll]);
        assert!(in_flight.borrow().is_empty());
    }

    #[gpui::test]
    fn device_code_past_its_expiry_is_cancelled_and_reported(cx: &mut TestAppContext) {
        let expired = AuthStep::DeviceCode {
            flow: FlowId::from("flow-1"),
            code: "ABCD".to_string(),
            url: "https://example.test/device".to_string(),
            complete_url: None,
            expires_in: Duration::ZERO,
        };
        let provider = FakeSignIn::new(Ok(expired), vec![AuthStep::Done]);
        let fleet = fleet_for(cx, provider.clone());
        let in_flight = Rc::default();

        assert!(start(cx, &provider, &fleet, &in_flight));
        cx.run_until_parked();

        assert_eq!(provider.answers(), vec![AuthAnswer::Cancel]);
        assert_eq!(
            flash(cx, &fleet).as_deref(),
            Some("Fake Cloud sign-in failed: the sign-in code expired")
        );
        assert!(in_flight.borrow().is_empty());
    }

    #[gpui::test]
    fn prompt_flow_is_cancelled_and_reported(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(
            Ok(AuthStep::Prompt {
                flow: FlowId::from("flow-2"),
                fields: vec![],
            }),
            vec![],
        );
        let fleet = fleet_for(cx, provider.clone());
        let in_flight = Rc::default();

        assert!(start(cx, &provider, &fleet, &in_flight));
        cx.run_until_parked();

        assert_eq!(provider.answers(), vec![AuthAnswer::Cancel]);
        assert_eq!(
            flash(cx, &fleet).as_deref(),
            Some("Fake Cloud sign-in needs input CloudDeck cannot show yet")
        );
        assert!(in_flight.borrow().is_empty());
    }

    #[gpui::test]
    fn begin_error_is_reported_as_a_failed_sign_in(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(
            Err(ProviderError::new(ErrorKind::Auth, "token expired")),
            vec![],
        );
        let fleet = fleet_for(cx, provider.clone());
        let in_flight = Rc::default();

        assert!(start(cx, &provider, &fleet, &in_flight));
        cx.run_until_parked();

        assert_eq!(
            flash(cx, &fleet).as_deref(),
            Some("Fake Cloud sign-in failed: token expired")
        );
        assert!(provider.answers().is_empty());
        assert!(in_flight.borrow().is_empty());
    }

    #[gpui::test]
    fn a_second_start_is_ignored_while_a_flow_is_in_flight(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(Ok(AuthStep::Done), vec![]);
        let fleet = fleet_for(cx, provider.clone());
        let in_flight: Rc<RefCell<HashSet<String>>> = Rc::default();
        in_flight.borrow_mut().insert("fake".to_string());

        assert!(!start(cx, &provider, &fleet, &in_flight));
        cx.run_until_parked();

        assert_eq!(provider.begin_calls.load(Ordering::SeqCst), 0);
        assert!(flash(cx, &fleet).is_none());
    }

    #[gpui::test]
    fn the_flow_id_is_released_once_it_finishes(cx: &mut TestAppContext) {
        let provider = FakeSignIn::new(Ok(AuthStep::Done), vec![]);
        let fleet = fleet_for(cx, provider.clone());
        let in_flight = Rc::default();

        assert!(start(cx, &provider, &fleet, &in_flight));
        cx.run_until_parked();
        assert!(!in_flight.borrow().contains("fake"));
        assert!(start(cx, &provider, &fleet, &in_flight));
    }
}
