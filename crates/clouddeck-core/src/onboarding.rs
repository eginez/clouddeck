//! Runs a provider's onboarding method as a state machine: the form the method declares, then
//! `begin_auth` and `continue_auth`. Windows draw `OnboardingView` and forward edits; nothing here draws.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AuthAnswer, AuthInput, AuthMethod, AuthStep, Call, ConnectionId, FlowId, PromptField,
    PromptKind, Provider, ProviderError, SettingOption,
};

/// Stores a value under a plugin setting key; an error is shown beside the field.
pub type SettingStore<'a> = &'a dyn Fn(&str, &str) -> Result<(), String>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    Editing,
    Browser {
        url: String,
        code: Option<String>,
        poll: bool,
    },
    Finished,
}

pub struct Onboarding {
    method: AuthMethod,
    connection: Option<ConnectionId>,
    fields: Vec<PromptField>,
    values: BTreeMap<String, String>,
    flow: Option<FlowId>,
    phase: Phase,
    message: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardingView {
    pub title: String,
    pub description: Option<String>,
    pub fields: Vec<FieldView>,
    pub message: Option<String>,
    pub error: Option<String>,
    pub action: ActionView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldView {
    pub id: String,
    pub label: String,
    pub kind: PromptKind,
    pub value: String,
    pub options: Vec<SettingOption>,
    pub help: Option<String>,
    pub required: bool,
    pub read_only: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionView {
    Connect,
    Continue,
    Waiting { url: String, code: Option<String> },
    Finished,
}

impl Onboarding {
    /// Fails when the method declares a form that cannot be shown or submitted.
    pub fn new(method: AuthMethod, connection: Option<ConnectionId>) -> Result<Self, String> {
        check_form(&method.initial_fields)?;
        let fields = method.initial_fields.clone();
        let values = seed(&fields, &BTreeMap::new());
        Ok(Self {
            method,
            connection,
            fields,
            values,
            flow: None,
            phase: Phase::Editing,
            message: None,
            error: None,
        })
    }

    /// Records an edit. A field that `refresh_on_change` asks the provider again while a prompt is open.
    pub fn change(
        &mut self,
        provider: &dyn Provider,
        call: &Call<'_>,
        store: SettingStore<'_>,
        id: &str,
        text: &str,
    ) {
        let Some(index) = self.fields.iter().position(|f| f.id == id) else {
            return;
        };
        let field = self.fields[index].clone();
        if field.kind == PromptKind::Note || self.phase != Phase::Editing {
            return;
        }
        self.values.insert(id.to_string(), text.to_string());
        self.fields[index].error = field.value_error(text);
        if self.fields[index].error.is_some() {
            return;
        }
        if let Some(key) = &field.persist_as
            && let Err(reason) = store(key, text)
        {
            self.fields[index].error = Some(reason);
            return;
        }
        if let Some(flow) = self.flow.clone().filter(|_| field.refresh_on_change) {
            let answer = AuthAnswer::Change {
                field: id.to_string(),
                values: self.values.clone(),
            };
            let result = provider.continue_auth(call, &flow, answer);
            self.apply(result);
        }
    }

    /// Begins the method when no prompt is open yet, otherwise answers the open prompt with the current values.
    pub fn submit(&mut self, provider: &dyn Provider, call: &Call<'_>) {
        if self.phase != Phase::Editing || !self.check_values() {
            return;
        }
        self.error = None;
        let result = match self.flow.clone() {
            None => provider.begin_auth(
                call,
                &self.method,
                AuthInput {
                    connection: self.connection.clone(),
                    values: self.submitted(),
                },
            ),
            Some(flow) => provider.continue_auth(call, &flow, AuthAnswer::Values(self.submitted())),
        };
        self.apply(result);
    }

    /// Asks the provider whether a browser sign-in has completed.
    pub fn poll(&mut self, provider: &dyn Provider, call: &Call<'_>) {
        let Phase::Browser { poll: true, .. } = self.phase else {
            return;
        };
        let Some(flow) = self.flow.clone() else {
            return;
        };
        let result = provider.continue_auth(call, &flow, AuthAnswer::Poll);
        self.apply(result);
    }

    /// Abandons the flow, if one is open, and returns the form to its first state.
    pub fn cancel(&mut self, provider: &dyn Provider, call: &Call<'_>) {
        if let Some(flow) = self.flow.take() {
            let _ = provider.continue_auth(call, &flow, AuthAnswer::Cancel);
        }
        self.phase = Phase::Editing;
        self.message = None;
        self.error = None;
    }

    pub fn view(&self) -> OnboardingView {
        OnboardingView {
            title: self.method.label.clone(),
            description: self.method.description.clone(),
            fields: self.fields.iter().map(|f| self.field_view(f)).collect(),
            message: self.message.clone(),
            error: self.error.clone(),
            action: match &self.phase {
                Phase::Editing if self.flow.is_none() => ActionView::Connect,
                Phase::Editing => ActionView::Continue,
                Phase::Browser { url, code, .. } => ActionView::Waiting {
                    url: url.clone(),
                    code: code.clone(),
                },
                Phase::Finished => ActionView::Finished,
            },
        }
    }

    fn field_view(&self, field: &PromptField) -> FieldView {
        let read_only = field.kind == PromptKind::Note;
        FieldView {
            id: field.id.clone(),
            label: field.label.clone(),
            kind: field.kind,
            value: if read_only {
                String::new()
            } else {
                self.values.get(&field.id).cloned().unwrap_or_default()
            },
            options: field.options.clone(),
            help: field.help.clone(),
            required: field.required,
            read_only,
            error: field.error.clone(),
        }
    }

    fn submitted(&self) -> BTreeMap<String, String> {
        self.values
            .iter()
            .filter(|(id, _)| {
                self.fields
                    .iter()
                    .any(|f| &f.id == *id && f.kind != PromptKind::Note)
            })
            .map(|(id, value)| (id.clone(), value.clone()))
            .collect()
    }

    fn check_values(&mut self) -> bool {
        let mut complete = true;
        for field in &mut self.fields {
            if field.kind == PromptKind::Note {
                continue;
            }
            let text = self.values.get(&field.id).map_or("", String::as_str);
            let error = if text.trim().is_empty() {
                field
                    .required
                    .then(|| format!("{} is required", field.label))
            } else {
                field.value_error(text)
            };
            complete &= error.is_none();
            field.error = error;
        }
        complete
    }

    fn apply(&mut self, result: Result<AuthStep, ProviderError>) {
        match result {
            Err(error) => self.error = Some(error.to_string()),
            Ok(AuthStep::Done) => self.finish(),
            Ok(AuthStep::Prompt {
                flow,
                fields,
                message,
                error,
            }) => match check_form(&fields) {
                Err(reason) => self.error = Some(reason),
                Ok(()) => {
                    self.values = seed(&fields, &self.values);
                    self.fields = fields;
                    self.flow = Some(flow);
                    self.phase = Phase::Editing;
                    self.message = message;
                    self.error = error;
                }
            },
            Ok(AuthStep::OpenUrl {
                flow,
                url,
                then_poll,
            }) => self.wait(
                flow,
                Phase::Browser {
                    url,
                    code: None,
                    poll: then_poll,
                },
            ),
            Ok(AuthStep::DeviceCode {
                flow,
                code,
                url,
                complete_url,
                ..
            }) => self.wait(
                flow,
                Phase::Browser {
                    url: complete_url.unwrap_or(url),
                    code: Some(code),
                    poll: true,
                },
            ),
        }
    }

    fn finish(&mut self) {
        self.flow = None;
        self.phase = Phase::Finished;
        self.message = None;
        self.error = None;
    }

    fn wait(&mut self, flow: FlowId, phase: Phase) {
        self.flow = Some(flow);
        self.phase = phase;
        self.message = None;
        self.error = None;
    }
}

/// Values for every field that takes input, keeping what the user already typed where a field survives.
fn seed(fields: &[PromptField], existing: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    fields
        .iter()
        .filter(|f| f.kind != PromptKind::Note)
        .map(|f| {
            let value = existing
                .get(&f.id)
                .or(f.default.as_ref())
                .cloned()
                .unwrap_or_default();
            (f.id.clone(), value)
        })
        .collect()
}

fn check_form(fields: &[PromptField]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for field in fields {
        if !ids.insert(field.id.as_str()) {
            return Err(format!("the form lists field {} twice", field.id));
        }
        field.check()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    use super::*;
    use crate::{
        Connection, Dimension, ErrorKind, Host, Instance, KindId, KindInfo, ListResult, LogLevel,
        ProviderInfo, Selection,
    };

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

    #[derive(Default)]
    struct FormProvider {
        begins: Mutex<Vec<AuthInput>>,
        answers: Mutex<Vec<AuthAnswer>>,
        begin: Mutex<Option<Result<AuthStep, ProviderError>>>,
        continues: Mutex<VecDeque<Result<AuthStep, ProviderError>>>,
    }

    impl FormProvider {
        fn answering(
            begin: Result<AuthStep, ProviderError>,
            continues: Vec<Result<AuthStep, ProviderError>>,
        ) -> Self {
            Self {
                begin: Mutex::new(Some(begin)),
                continues: Mutex::new(continues.into()),
                ..Self::default()
            }
        }
    }

    impl Provider for FormProvider {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "form".to_string(),
                name: "Form".to_string(),
                version: "0".to_string(),
                connection_label: "Profile".to_string(),
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
        fn fields(&self, _kind: &KindId) -> Vec<crate::FieldDef> {
            vec![]
        }
        fn begin_auth(
            &self,
            _cx: &Call<'_>,
            _method: &AuthMethod,
            input: AuthInput,
        ) -> Result<AuthStep, ProviderError> {
            self.begins.lock().unwrap().push(input);
            self.begin
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Err(ProviderError::unsupported()))
        }
        fn continue_auth(
            &self,
            _cx: &Call<'_>,
            _flow: &FlowId,
            answer: AuthAnswer,
        ) -> Result<AuthStep, ProviderError> {
            self.answers.lock().unwrap().push(answer);
            self.continues
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| Err(ProviderError::unsupported()))
        }
    }

    fn folder(id: &str) -> PromptField {
        PromptField::new(id, "Config folder", PromptKind::Folder)
    }

    fn profile_choice() -> PromptField {
        PromptField {
            default: Some("alpha".into()),
            options: vec![
                SettingOption {
                    value: "alpha".into(),
                    label: "Alpha".into(),
                },
                SettingOption {
                    value: "beta".into(),
                    label: "Beta".into(),
                },
            ],
            ..PromptField::new("profile", "Profile", PromptKind::Choice)
        }
    }

    fn method(fields: Vec<PromptField>) -> AuthMethod {
        AuthMethod {
            initial_fields: fields,
            ..AuthMethod::new("profile", "Profile in a folder")
        }
    }

    fn flow() -> FlowId {
        FlowId::from("flow-1")
    }

    fn prompt(fields: Vec<PromptField>) -> AuthStep {
        AuthStep::Prompt {
            flow: flow(),
            fields,
            message: None,
            error: None,
        }
    }

    fn with_call<T>(work: impl FnOnce(&Call<'_>) -> T) -> T {
        let host = NoopHost;
        work(&Call {
            deadline: None,
            host: &host,
        })
    }

    fn no_store(_: &str, _: &str) -> Result<(), String> {
        Ok(())
    }

    fn field_value(view: &OnboardingView, id: &str) -> String {
        view.fields
            .iter()
            .find(|f| f.id == id)
            .unwrap()
            .value
            .clone()
    }

    #[test]
    fn form_starts_with_defaults_and_offers_connect() {
        let onboarding =
            Onboarding::new(method(vec![folder("config_dir"), profile_choice()]), None).unwrap();
        let view = onboarding.view();
        assert_eq!(view.title, "Profile in a folder");
        assert_eq!(field_value(&view, "config_dir"), "");
        assert_eq!(field_value(&view, "profile"), "alpha");
        assert_eq!(view.action, ActionView::Connect);
    }

    #[test]
    fn a_method_with_a_choice_missing_its_default_is_refused() {
        let choice = PromptField {
            default: None,
            ..profile_choice()
        };
        assert!(Onboarding::new(method(vec![choice]), None).is_err());
    }

    #[test]
    fn submit_begins_with_the_values_and_shows_the_prompt() {
        let provider = FormProvider::answering(
            Ok(AuthStep::Prompt {
                flow: flow(),
                fields: vec![folder("config_dir"), profile_choice()],
                message: Some("Found 2 profiles".into()),
                error: None,
            }),
            vec![],
        );
        let mut onboarding =
            Onboarding::new(method(vec![folder("config_dir"), profile_choice()]), None).unwrap();
        with_call(|call| {
            onboarding.change(&provider, call, &no_store, "config_dir", "/work/aws");
            onboarding.submit(&provider, call);
        });
        let begins = provider.begins.lock().unwrap();
        assert_eq!(begins.len(), 1);
        assert_eq!(
            begins[0].values.get("config_dir").map(String::as_str),
            Some("/work/aws")
        );
        drop(begins);
        let view = onboarding.view();
        assert_eq!(view.action, ActionView::Continue);
        assert_eq!(view.message.as_deref(), Some("Found 2 profiles"));
    }

    #[test]
    fn required_fields_are_checked_before_anything_is_sent() {
        let provider = FormProvider::default();
        let mut onboarding = Onboarding::new(
            method(vec![PromptField {
                required: true,
                ..folder("config_dir")
            }]),
            None,
        )
        .unwrap();
        with_call(|call| onboarding.submit(&provider, call));
        assert!(provider.begins.lock().unwrap().is_empty());
        let view = onboarding.view();
        assert_eq!(
            view.fields[0].error.as_deref(),
            Some("Config folder is required")
        );
    }

    #[test]
    fn changing_a_refresh_field_asks_the_provider_and_replaces_the_prompt() {
        let refresh = PromptField {
            refresh_on_change: true,
            persist_as: Some("config_dir".into()),
            ..folder("config_dir")
        };
        let provider = FormProvider::answering(
            Ok(prompt(vec![refresh.clone(), profile_choice()])),
            vec![Ok(AuthStep::Prompt {
                flow: flow(),
                fields: vec![
                    refresh,
                    PromptField {
                        options: vec![SettingOption {
                            value: "gamma".into(),
                            label: "Gamma".into(),
                        }],
                        default: Some("gamma".into()),
                        ..profile_choice()
                    },
                ],
                message: Some("Found 1 profile".into()),
                error: None,
            })],
        );
        let stored = RefCell::new(Vec::new());
        let store = |key: &str, value: &str| {
            stored
                .borrow_mut()
                .push((key.to_string(), value.to_string()));
            Ok(())
        };
        let mut onboarding = Onboarding::new(method(vec![]), None).unwrap();
        with_call(|call| {
            onboarding.submit(&provider, call);
            onboarding.change(&provider, call, &store, "config_dir", "/work/aws");
        });
        assert_eq!(
            *stored.borrow(),
            vec![("config_dir".to_string(), "/work/aws".to_string())]
        );
        let answers = provider.answers.lock().unwrap();
        assert_eq!(
            answers.first(),
            Some(&AuthAnswer::Change {
                field: "config_dir".into(),
                values: BTreeMap::from([
                    ("config_dir".into(), "/work/aws".into()),
                    ("profile".into(), "alpha".into()),
                ]),
            })
        );
        drop(answers);
        let view = onboarding.view();
        assert_eq!(view.message.as_deref(), Some("Found 1 profile"));
        assert_eq!(view.fields[1].options[0].value, "gamma");
    }

    #[test]
    fn an_invalid_folder_is_refused_without_storing_or_asking() {
        let provider = FormProvider::default();
        let stored = RefCell::new(0);
        let store = |_: &str, _: &str| {
            *stored.borrow_mut() += 1;
            Ok(())
        };
        let mut onboarding = Onboarding::new(
            method(vec![PromptField {
                refresh_on_change: true,
                persist_as: Some("config_dir".into()),
                ..folder("config_dir")
            }]),
            None,
        )
        .unwrap();
        with_call(|call| {
            onboarding.change(&provider, call, &store, "config_dir", "relative/path");
        });
        assert_eq!(*stored.borrow(), 0);
        assert!(provider.answers.lock().unwrap().is_empty());
        assert!(
            onboarding.view().fields[0]
                .error
                .as_deref()
                .unwrap()
                .contains("absolute")
        );
    }

    #[test]
    fn submit_refuses_a_value_that_failed_validation() {
        let provider = FormProvider::default();
        let mut onboarding = Onboarding::new(method(vec![folder("config_dir")]), None).unwrap();
        with_call(|call| {
            onboarding.change(
                &provider,
                call,
                &|_, _| Ok(()),
                "config_dir",
                "relative/path",
            );
            onboarding.submit(&provider, call);
        });
        assert!(provider.begins.lock().unwrap().is_empty());
        assert!(provider.answers.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failed_store_is_shown_beside_the_field() {
        let provider = FormProvider::default();
        let mut onboarding = Onboarding::new(
            method(vec![PromptField {
                persist_as: Some("config_dir".into()),
                ..folder("config_dir")
            }]),
            None,
        )
        .unwrap();
        let store = |_: &str, _: &str| Err("cannot save settings".to_string());
        with_call(|call| onboarding.change(&provider, call, &store, "config_dir", "/work"));
        assert_eq!(
            onboarding.view().fields[0].error.as_deref(),
            Some("cannot save settings")
        );
    }

    #[test]
    fn notes_are_read_only_and_never_submitted() {
        let note = PromptField {
            help: Some("Read the docs first".into()),
            ..PromptField::new("note", "Note", PromptKind::Note)
        };
        let provider = FormProvider::answering(Ok(prompt(vec![note.clone()])), vec![]);
        let mut onboarding = Onboarding::new(method(vec![note]), None).unwrap();
        with_call(|call| onboarding.submit(&provider, call));
        assert!(provider.begins.lock().unwrap()[0].values.is_empty());
        let view = onboarding.view();
        assert!(view.fields[0].read_only);
        assert_eq!(view.fields[0].help.as_deref(), Some("Read the docs first"));
    }

    #[test]
    fn a_device_code_waits_in_the_browser_until_polling_completes() {
        let provider = FormProvider::answering(
            Ok(AuthStep::DeviceCode {
                flow: flow(),
                code: "ABCD".into(),
                url: "https://example.test/device".into(),
                complete_url: Some("https://example.test/complete".into()),
                expires_in: Duration::from_secs(900),
            }),
            vec![Ok(AuthStep::Done)],
        );
        let mut onboarding = Onboarding::new(method(vec![]), None).unwrap();
        with_call(|call| onboarding.submit(&provider, call));
        assert_eq!(
            onboarding.view().action,
            ActionView::Waiting {
                url: "https://example.test/complete".into(),
                code: Some("ABCD".into()),
            }
        );
        with_call(|call| onboarding.poll(&provider, call));
        assert_eq!(onboarding.view().action, ActionView::Finished);
        assert_eq!(*provider.answers.lock().unwrap(), vec![AuthAnswer::Poll]);
    }

    #[test]
    fn a_provider_error_keeps_the_form_and_shows_the_reason() {
        let provider = FormProvider::answering(
            Err(ProviderError::new(ErrorKind::Auth, "token expired")),
            vec![],
        );
        let mut onboarding = Onboarding::new(method(vec![folder("config_dir")]), None).unwrap();
        with_call(|call| onboarding.submit(&provider, call));
        let view = onboarding.view();
        assert_eq!(view.error.as_deref(), Some("token expired"));
        assert_eq!(view.action, ActionView::Connect);
        assert_eq!(view.fields.len(), 1);
    }

    #[test]
    fn cancel_tells_the_provider_and_returns_to_connect() {
        let provider = FormProvider::answering(Ok(prompt(vec![])), vec![Ok(AuthStep::Done)]);
        let mut onboarding = Onboarding::new(method(vec![]), None).unwrap();
        with_call(|call| {
            onboarding.submit(&provider, call);
            onboarding.cancel(&provider, call);
        });
        assert_eq!(*provider.answers.lock().unwrap(), vec![AuthAnswer::Cancel]);
        assert_eq!(onboarding.view().action, ActionView::Connect);
    }

    #[test]
    fn a_prompt_with_an_invalid_form_is_not_shown() {
        let provider = FormProvider::answering(
            Ok(prompt(vec![
                profile_choice(),
                PromptField {
                    default: None,
                    options: vec![],
                    ..PromptField::new("broken", "Broken", PromptKind::Choice)
                },
            ])),
            vec![],
        );
        let mut onboarding = Onboarding::new(method(vec![]), None).unwrap();
        with_call(|call| onboarding.submit(&provider, call));
        let view = onboarding.view();
        assert!(view.fields.is_empty());
        assert_eq!(view.action, ActionView::Connect);
        assert!(view.error.is_some());
    }
}
