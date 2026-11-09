use std::collections::BTreeMap;

use clouddeck_provider_api::{SettingError, SettingKind};

use crate::{
    AuthInput, AuthMethod, AuthStep, Call, ConnectionId, Provider, ProviderInfo, SettingDef,
    SettingValue, start_sign_in,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectError {
    pub message: String,
    pub advice: Option<String>,
}

impl From<crate::ProviderError> for ConnectError {
    fn from(error: crate::ProviderError) -> Self {
        Self {
            message: error.message,
            advice: error.advice,
        }
    }
}

impl From<SettingError> for ConnectError {
    fn from(error: SettingError) -> Self {
        Self {
            message: error.0,
            advice: None,
        }
    }
}

/// The Connect window's model: the provider's auth methods, the selected one, and the values of its settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectForm {
    pub title: String,
    pub methods: Vec<AuthMethod>,
    pub selected: usize,
    settings: Vec<SettingDef>,
    values: BTreeMap<String, SettingValue>,
    pub error: Option<ConnectError>,
}

/// The auth methods a provider offers for connecting; empty when it has no sign-in.
pub fn connect_methods(provider: &dyn Provider) -> Vec<AuthMethod> {
    provider
        .auth()
        .map(|auth| auth.auth_methods())
        .unwrap_or_default()
}

/// The settings the provider declares; the Connect form draws rows from these.
pub fn connect_settings(provider: &dyn Provider) -> Vec<SettingDef> {
    provider
        .config()
        .map(|config| config.settings())
        .unwrap_or_default()
}

impl ConnectForm {
    /// `saved` holds the stored values of the provider's settings; keys not in it take the default or a blank value.
    pub fn new(
        info: &ProviderInfo,
        methods: Vec<AuthMethod>,
        settings: Vec<SettingDef>,
        saved: BTreeMap<String, SettingValue>,
    ) -> Self {
        let values = settings
            .iter()
            .map(|def| {
                let value = saved
                    .get(&def.key)
                    .or(def.default.as_ref())
                    .cloned()
                    .unwrap_or_else(|| blank(def));
                (def.key.clone(), value)
            })
            .collect();
        Self {
            title: format!("Connect to {}", info.name),
            methods,
            selected: 0,
            settings,
            values,
            error: None,
        }
    }

    pub fn selected_method(&self) -> Option<&AuthMethod> {
        self.methods.get(self.selected)
    }

    /// One row per key of the selected method, in the method's order; keys the provider does not declare are skipped.
    pub fn rows(&self) -> Vec<&SettingDef> {
        let Some(method) = self.selected_method() else {
            return Vec::new();
        };
        method
            .settings
            .iter()
            .filter_map(|key| self.settings.iter().find(|def| &def.key == key))
            .collect()
    }

    pub fn value(&self, key: &str) -> Option<&SettingValue> {
        self.values.get(key)
    }

    pub fn select(&mut self, index: usize) {
        if index < self.methods.len() && index != self.selected {
            self.selected = index;
            self.error = None;
        }
    }

    pub fn set_value(&mut self, key: &str, value: SettingValue) {
        if self.settings.iter().any(|def| def.key == key) {
            self.values.insert(key.to_string(), value);
            self.error = None;
        }
    }

    /// The selected method's rows, each validated against its declaration.
    pub fn writes(&self) -> Result<Vec<(String, SettingValue)>, ConnectError> {
        self.rows()
            .into_iter()
            .map(|def| -> Result<(String, SettingValue), ConnectError> {
                let value = self
                    .values
                    .get(&def.key)
                    .cloned()
                    .unwrap_or_else(|| blank(def));
                def.validate(&value)?;
                Ok((def.key.clone(), value))
            })
            .collect()
    }
}

/// Saves each value through `save` first, then begins the method, so the flow sees what the form saved.
pub fn submit(
    provider: &dyn Provider,
    call: &Call<'_>,
    method: &AuthMethod,
    connection: Option<ConnectionId>,
    writes: Vec<(String, SettingValue)>,
    mut save: impl FnMut(&str, SettingValue) -> Result<(), SettingError>,
) -> Result<AuthStep, ConnectError> {
    for (key, value) in writes {
        save(&key, value)?;
    }
    start_sign_in(
        provider,
        call,
        method,
        AuthInput {
            connection,
            values: BTreeMap::new(),
        },
    )
    .map_err(ConnectError::from)
}

fn blank(def: &SettingDef) -> SettingValue {
    match &def.kind {
        SettingKind::Bool => SettingValue::Bool(false),
        SettingKind::Number { min, max } => {
            SettingValue::Number(min.unwrap_or_else(|| max.unwrap_or(0.0).min(0.0)))
        }
        SettingKind::DurationSecs { min, .. } => SettingValue::DurationSecs(min.unwrap_or(0)),
        SettingKind::Choice(options) => SettingValue::Text(
            options
                .first()
                .map(|option| option.value.clone())
                .unwrap_or_default(),
        ),
        SettingKind::Text | SettingKind::Path | SettingKind::Folder => {
            SettingValue::Text(String::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use clouddeck_provider_api::{
        Auth, Config, ErrorKind, FieldDef, Host, Instance, KindId, KindInfo, ListResult, LogLevel,
        SettingOption,
    };

    use super::*;
    use crate::{Browse, Connection, Dimension, ProviderError, Selection};

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

    fn with_call<T>(work: impl FnOnce(&Call<'_>) -> T) -> T {
        let host = NoopHost;
        work(&Call {
            deadline: None,
            host: &host,
        })
    }

    fn def(key: &str, kind: SettingKind) -> SettingDef {
        SettingDef {
            key: key.into(),
            label: key.into(),
            description: None,
            group: None,
            kind,
            default: None,
            secret: false,
        }
    }

    fn region_choice() -> SettingKind {
        SettingKind::Choice(vec![
            SettingOption {
                value: "us".into(),
                label: "US".into(),
            },
            SettingOption {
                value: "eu".into(),
                label: "EU".into(),
            },
        ])
    }

    fn all_kinds() -> Vec<SettingDef> {
        vec![
            def("name", SettingKind::Text),
            def("config_dir", SettingKind::Folder),
            def("flag", SettingKind::Bool),
            SettingDef {
                default: Some(SettingValue::Text("eu".into())),
                ..def("region", region_choice())
            },
            SettingDef {
                secret: true,
                ..def("token", SettingKind::Text)
            },
            def(
                "timeout",
                SettingKind::DurationSecs {
                    min: Some(5),
                    max: Some(60),
                },
            ),
        ]
    }

    fn method(id: &str, settings: &[&str]) -> AuthMethod {
        AuthMethod {
            settings: settings.iter().map(|key| key.to_string()).collect(),
            ..AuthMethod::new(id.into(), id.into())
        }
    }

    fn info() -> ProviderInfo {
        ProviderInfo {
            id: "fake".into(),
            name: "Fake Cloud".into(),
            version: "0".into(),
            connection_label: "Project".into(),
        }
    }

    fn form(methods: Vec<AuthMethod>) -> ConnectForm {
        ConnectForm::new(&info(), methods, all_kinds(), BTreeMap::new())
    }

    struct FakeCloud {
        methods: Vec<AuthMethod>,
        settings: Vec<SettingDef>,
        begin_error: Option<ProviderError>,
        begins: Mutex<Vec<(String, Option<String>)>>,
    }

    impl FakeCloud {
        fn new(methods: Vec<AuthMethod>) -> Self {
            Self {
                methods,
                settings: all_kinds(),
                begin_error: None,
                begins: Mutex::new(Vec::new()),
            }
        }
    }

    impl Provider for FakeCloud {
        fn info(&self) -> ProviderInfo {
            info()
        }

        fn auth(&self) -> Option<&dyn Auth> {
            Some(self)
        }

        fn config(&self) -> Option<&dyn Config> {
            Some(self)
        }
    }

    impl Browse for FakeCloud {
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

    impl Auth for FakeCloud {
        fn auth_methods(&self) -> Vec<AuthMethod> {
            self.methods.clone()
        }

        fn begin_auth(
            &self,
            _cx: &Call<'_>,
            method: &AuthMethod,
            input: AuthInput,
        ) -> Result<AuthStep, ProviderError> {
            self.begins.lock().unwrap().push((
                method.id.clone(),
                input.connection.map(|id| id.as_str().to_string()),
            ));
            match &self.begin_error {
                Some(error) => Err(error.clone()),
                None => Ok(AuthStep::Done),
            }
        }
    }

    impl Config for FakeCloud {
        fn settings(&self) -> Vec<SettingDef> {
            self.settings.clone()
        }
    }

    #[test]
    fn title_names_the_provider() {
        assert_eq!(form(vec![]).title, "Connect to Fake Cloud");
    }

    #[test]
    fn rows_are_the_selected_methods_settings_in_its_order() {
        let mut form = form(vec![
            method("a", &["config_dir", "region", "missing"]),
            method("b", &["token", "timeout"]),
        ]);
        let keys = |form: &ConnectForm| -> Vec<String> {
            form.rows().into_iter().map(|def| def.key.clone()).collect()
        };
        assert_eq!(keys(&form), ["config_dir", "region"]);

        form.select(1);
        assert_eq!(keys(&form), ["token", "timeout"]);
    }

    #[test]
    fn every_kind_starts_with_its_stored_default_or_a_blank_value() {
        let mut stored = BTreeMap::new();
        stored.insert("name".to_string(), SettingValue::Text("prod".into()));
        let form = ConnectForm::new(
            &info(),
            vec![method(
                "a",
                &["name", "config_dir", "flag", "region", "token", "timeout"],
            )],
            all_kinds(),
            stored,
        );

        assert_eq!(form.value("name"), Some(&SettingValue::Text("prod".into())));
        assert_eq!(
            form.value("config_dir"),
            Some(&SettingValue::Text(String::new()))
        );
        assert_eq!(form.value("flag"), Some(&SettingValue::Bool(false)));
        assert_eq!(form.value("region"), Some(&SettingValue::Text("eu".into())));
        assert_eq!(
            form.value("token"),
            Some(&SettingValue::Text(String::new()))
        );
        assert_eq!(form.value("timeout"), Some(&SettingValue::DurationSecs(5)));
    }

    #[test]
    fn writes_are_validated_against_each_declaration() {
        let mut form = form(vec![method("a", &["config_dir", "timeout"])]);
        form.set_value("config_dir", SettingValue::Text("relative/dir".into()));
        form.set_value("timeout", SettingValue::DurationSecs(30));

        let error = form.writes().unwrap_err();
        assert!(error.message.contains("config_dir"), "{}", error.message);

        form.set_value("config_dir", SettingValue::Text("~/configs".into()));
        assert_eq!(
            form.writes().unwrap(),
            vec![
                (
                    "config_dir".to_string(),
                    SettingValue::Text("~/configs".into())
                ),
                ("timeout".to_string(), SettingValue::DurationSecs(30)),
            ]
        );
    }

    #[test]
    fn a_choice_outside_its_options_is_refused() {
        let mut form = form(vec![method("a", &["region"])]);
        form.set_value("region", SettingValue::Text("ap".into()));
        assert!(form.writes().is_err());
    }

    #[test]
    fn selecting_a_method_clears_the_error() {
        let mut form = form(vec![method("a", &[]), method("b", &[])]);
        form.error = Some(ConnectError {
            message: "nope".into(),
            advice: None,
        });
        form.select(1);
        assert_eq!(form.selected, 1);
        assert!(form.error.is_none());
    }

    #[test]
    fn values_are_saved_before_the_method_begins() {
        let provider = FakeCloud::new(vec![method("a", &[])]);
        let order = RefCell::new(Vec::new());
        let writes = vec![
            ("config_dir".to_string(), SettingValue::Text("/srv".into())),
            ("flag".to_string(), SettingValue::Bool(true)),
        ];

        let step = with_call(|call| {
            submit(
                &provider,
                call,
                &method("a", &[]),
                Some(ConnectionId::from("prod")),
                writes,
                |key, _| {
                    order.borrow_mut().push(format!("save {key}"));
                    Ok(())
                },
            )
        });

        assert_eq!(step, Ok(AuthStep::Done));
        assert_eq!(*order.borrow(), ["save config_dir", "save flag"]);
        assert_eq!(
            *provider.begins.lock().unwrap(),
            vec![("a".to_string(), Some("prod".to_string()))]
        );
    }

    #[test]
    fn a_failed_save_stops_before_the_method_begins() {
        let provider = FakeCloud::new(vec![method("a", &[])]);
        let writes = vec![
            ("config_dir".to_string(), SettingValue::Text("/srv".into())),
            ("flag".to_string(), SettingValue::Bool(true)),
        ];

        let error = with_call(|call| {
            submit(
                &provider,
                call,
                &method("a", &[]),
                None,
                writes,
                |key, _| {
                    if key == "flag" {
                        Err(SettingError("cannot save settings: disk full".into()))
                    } else {
                        Ok(())
                    }
                },
            )
        })
        .unwrap_err();

        assert_eq!(error.message, "cannot save settings: disk full");
        assert!(provider.begins.lock().unwrap().is_empty());
    }

    #[test]
    fn a_begin_error_keeps_its_message_and_advice() {
        let mut provider = FakeCloud::new(vec![method("a", &[])]);
        provider.begin_error = Some(ProviderError {
            kind: ErrorKind::Auth,
            message: "Choose a profile to sign in to.".into(),
            advice: Some("Pick a profile in the context bar.".into()),
            retry_after: None,
        });

        let error = with_call(|call| {
            submit(&provider, call, &method("a", &[]), None, vec![], |_, _| {
                Ok(())
            })
        })
        .unwrap_err();

        assert_eq!(
            error,
            ConnectError {
                message: "Choose a profile to sign in to.".into(),
                advice: Some("Pick a profile in the context bar.".into()),
            }
        );
    }

    #[test]
    fn a_provider_without_sign_in_offers_no_methods_and_no_settings() {
        struct NoSignIn;
        impl Provider for NoSignIn {
            fn info(&self) -> ProviderInfo {
                info()
            }
        }
        impl Browse for NoSignIn {
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

        assert!(connect_methods(&NoSignIn).is_empty());
        assert!(connect_settings(&NoSignIn).is_empty());
    }
}
