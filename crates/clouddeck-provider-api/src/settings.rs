use std::fmt;

use serde::{Deserialize, Serialize};

/// A setting a provider reads through `Host::setting`. Declared in `extension.toml` and by `Provider::settings`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingDef {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
    pub kind: SettingKind,
    #[serde(default)]
    pub default: Option<SettingValue>,
    /// Stored through host secret storage, never in the settings file. Secrets are text.
    #[serde(default)]
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Text,
    Path,
    Bool,
    Number {
        #[serde(default)]
        min: Option<f64>,
        #[serde(default)]
        max: Option<f64>,
    },
    DurationSecs {
        #[serde(default)]
        min: Option<u64>,
        #[serde(default)]
        max: Option<u64>,
    },
    Choice(Vec<SettingOption>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingValue {
    Text(String),
    Bool(bool),
    Number(f64),
    DurationSecs(u64),
}

impl SettingValue {
    /// The text an extension receives from `Host::setting`.
    pub fn host_text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Bool(value) => value.to_string(),
            Self::Number(value) => value.to_string(),
            Self::DurationSecs(seconds) => seconds.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingError(pub String);

impl fmt::Display for SettingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SettingError {}

/// Keys are lowercase `[a-z0-9_.-]`. The `clouddeck.` prefix belongs to host-internal keys.
pub fn is_valid_setting_key(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with("clouddeck.")
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-'))
}

/// Differences between settings declared in code and the manifest's copy, one message each.
pub fn settings_mismatches(in_code: &[SettingDef], declared: &[SettingDef]) -> Vec<String> {
    let mut mismatches = Vec::new();
    for def in in_code {
        match declared.iter().find(|other| other.key == def.key) {
            None => mismatches.push(format!("setting {} is not in the manifest", def.key)),
            Some(other) if other != def => {
                mismatches.push(format!("setting {} differs from the manifest", def.key));
            }
            Some(_) => {}
        }
    }
    for other in declared {
        if !in_code.iter().any(|def| def.key == other.key) {
            mismatches.push(format!(
                "manifest declares setting {} the provider lacks",
                other.key
            ));
        }
    }
    mismatches
}

impl SettingDef {
    /// Checks the declaration itself, including that its default is a valid value.
    pub fn check(&self) -> Result<(), SettingError> {
        self.check_declaration()?;
        self.check_kind()?;
        self.check_secret()?;
        if let Some(default) = &self.default {
            self.validate(default)
                .map_err(|error| SettingError(format!("default: {error}")))?;
        }
        Ok(())
    }

    fn reject<T>(&self, reason: &str) -> Result<T, SettingError> {
        Err(SettingError(format!("setting {}: {reason}", self.key)))
    }

    fn check_declaration(&self) -> Result<(), SettingError> {
        if !is_valid_setting_key(&self.key) {
            return Err(SettingError(format!(
                "setting key {:?} may only contain lowercase letters, digits, '.', '_' and '-', and may not start with clouddeck.",
                self.key
            )));
        }
        if self.label.trim().is_empty() {
            return self.reject("needs a label");
        }
        for text in [&self.description, &self.group].into_iter().flatten() {
            if text.contains('\n') {
                return self.reject("description and group must be one line");
            }
        }
        Ok(())
    }

    fn check_kind(&self) -> Result<(), SettingError> {
        match &self.kind {
            SettingKind::Number { min, max } => {
                if min.is_some_and(|v| !v.is_finite()) || max.is_some_and(|v| !v.is_finite()) {
                    return self.reject("min and max must be finite");
                }
                check_bounds(&self.key, min.as_ref(), max.as_ref())
            }
            SettingKind::DurationSecs { min, max } => {
                check_bounds(&self.key, min.as_ref(), max.as_ref())
            }
            SettingKind::Choice(options) => {
                if options.is_empty() {
                    return self.reject("a choice needs at least one option");
                }
                for (index, option) in options.iter().enumerate() {
                    if options[..index].iter().any(|o| o.value == option.value) {
                        return self.reject(&format!("option {:?} is listed twice", option.value));
                    }
                }
                Ok(())
            }
            SettingKind::Text | SettingKind::Path | SettingKind::Bool => Ok(()),
        }
    }

    fn check_secret(&self) -> Result<(), SettingError> {
        if !self.secret {
            return Ok(());
        }
        if !matches!(self.kind, SettingKind::Text) {
            return self.reject("a secret must have kind text");
        }
        if self.default.is_some() {
            return self
                .reject("a secret cannot have a default, since defaults live in the manifest");
        }
        Ok(())
    }

    pub fn validate(&self, value: &SettingValue) -> Result<(), SettingError> {
        match (&self.kind, value) {
            (SettingKind::Text | SettingKind::Path, SettingValue::Text(_))
            | (SettingKind::Bool, SettingValue::Bool(_)) => Ok(()),
            (SettingKind::Number { min, max }, SettingValue::Number(number)) => {
                if !number.is_finite() {
                    return self.reject(&format!("{number} is not a finite number"));
                }
                if min.is_some_and(|min| *number < min) || max.is_some_and(|max| *number > max) {
                    return self.reject(&format!("{number} is outside the allowed range"));
                }
                Ok(())
            }
            (SettingKind::DurationSecs { min, max }, SettingValue::DurationSecs(seconds)) => {
                if min.is_some_and(|min| *seconds < min) || max.is_some_and(|max| *seconds > max) {
                    return self.reject(&format!("{seconds} seconds is outside the allowed range"));
                }
                Ok(())
            }
            (SettingKind::Choice(options), SettingValue::Text(text)) => {
                if options.iter().any(|option| option.value == *text) {
                    Ok(())
                } else {
                    self.reject(&format!("{text:?} is not one of the options"))
                }
            }
            _ => self.reject(&format!("{value:?} does not match kind {:?}", self.kind)),
        }
    }

    /// Reads host text (as passed to extensions or typed by a user) into a value of this kind, then validates it.
    pub fn parse(&self, text: &str) -> Result<SettingValue, SettingError> {
        let value = match &self.kind {
            SettingKind::Text | SettingKind::Path | SettingKind::Choice(_) => {
                SettingValue::Text(text.to_string())
            }
            SettingKind::Bool => match text {
                "true" => SettingValue::Bool(true),
                "false" => SettingValue::Bool(false),
                _ => return self.reject(&format!("{text:?} is not true or false")),
            },
            SettingKind::Number { .. } => match text.parse::<f64>() {
                Ok(number) => SettingValue::Number(number),
                Err(_) => return self.reject(&format!("{text:?} is not a number")),
            },
            SettingKind::DurationSecs { .. } => match text.parse::<u64>() {
                Ok(seconds) => SettingValue::DurationSecs(seconds),
                Err(_) => {
                    return self.reject(&format!("{text:?} is not a whole number of seconds"));
                }
            },
        };
        self.validate(&value)?;
        Ok(value)
    }
}

fn check_bounds<T: PartialOrd + fmt::Display>(
    key: &str,
    min: Option<&T>,
    max: Option<&T>,
) -> Result<(), SettingError> {
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        return Err(SettingError(format!(
            "setting {key}: min {min} is greater than max {max}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(key: &str, kind: SettingKind) -> SettingDef {
        SettingDef {
            key: key.into(),
            label: "Label".into(),
            description: None,
            group: None,
            kind,
            default: None,
            secret: false,
        }
    }

    fn timeout() -> SettingDef {
        def(
            "timeout",
            SettingKind::DurationSecs {
                min: Some(1),
                max: Some(60),
            },
        )
    }

    fn region() -> SettingDef {
        def(
            "region",
            SettingKind::Choice(vec![
                SettingOption {
                    value: "eu".into(),
                    label: "Europe".into(),
                },
                SettingOption {
                    value: "us".into(),
                    label: "United States".into(),
                },
            ]),
        )
    }

    #[test]
    fn ranges_bound_numbers_and_durations() {
        let timeout = timeout();
        assert!(timeout.validate(&SettingValue::DurationSecs(30)).is_ok());
        assert!(timeout.validate(&SettingValue::DurationSecs(0)).is_err());
        assert!(timeout.validate(&SettingValue::DurationSecs(61)).is_err());

        let ratio = def(
            "ratio",
            SettingKind::Number {
                min: Some(0.0),
                max: Some(1.0),
            },
        );
        assert!(ratio.validate(&SettingValue::Number(0.5)).is_ok());
        assert!(ratio.validate(&SettingValue::Number(1.5)).is_err());
        assert!(ratio.validate(&SettingValue::Number(f64::NAN)).is_err());
    }

    #[test]
    fn choice_accepts_only_listed_values() {
        let region = region();
        assert!(region.validate(&SettingValue::Text("eu".into())).is_ok());
        assert!(region.validate(&SettingValue::Text("ap".into())).is_err());
    }

    #[test]
    fn value_must_match_the_kind() {
        assert!(timeout().validate(&SettingValue::Bool(true)).is_err());
        assert!(
            def("flag", SettingKind::Bool)
                .validate(&SettingValue::Text("true".into()))
                .is_err()
        );
    }

    #[test]
    fn parse_reads_host_text_by_kind() {
        assert_eq!(
            def("flag", SettingKind::Bool).parse("true"),
            Ok(SettingValue::Bool(true))
        );
        assert!(def("flag", SettingKind::Bool).parse("yes").is_err());
        assert_eq!(timeout().parse("30"), Ok(SettingValue::DurationSecs(30)));
        assert!(timeout().parse("30.5").is_err());
        assert_eq!(
            def(
                "ratio",
                SettingKind::Number {
                    min: None,
                    max: None
                }
            )
            .parse("0.25"),
            Ok(SettingValue::Number(0.25))
        );
        assert_eq!(region().parse("us"), Ok(SettingValue::Text("us".into())));
    }

    #[test]
    fn host_text_round_trips_through_parse() {
        let cases = [
            (SettingValue::Bool(false), def("flag", SettingKind::Bool)),
            (SettingValue::DurationSecs(45), timeout()),
            (
                SettingValue::Number(2.5),
                def(
                    "n",
                    SettingKind::Number {
                        min: None,
                        max: None,
                    },
                ),
            ),
        ];
        for (value, definition) in cases {
            assert_eq!(definition.parse(&value.host_text()), Ok(value));
        }
    }

    #[test]
    fn check_rejects_bad_declarations() {
        assert!(def("Bad Key", SettingKind::Text).check().is_err());
        assert!(def("clouddeck.fleet", SettingKind::Text).check().is_err());
        assert!(
            def(
                "inverted",
                SettingKind::DurationSecs {
                    min: Some(9),
                    max: Some(3)
                }
            )
            .check()
            .is_err()
        );
        assert!(def("empty", SettingKind::Choice(vec![])).check().is_err());
        let mut duplicate = region();
        if let SettingKind::Choice(options) = &mut duplicate.kind {
            options.push(options[0].clone());
        }
        assert!(duplicate.check().is_err());
        let mut bad_default = timeout();
        bad_default.default = Some(SettingValue::DurationSecs(0));
        assert!(bad_default.check().is_err());
        let mut off_choice = region();
        off_choice.default = Some(SettingValue::Text("ap".into()));
        assert!(off_choice.check().is_err());
    }

    #[test]
    fn secrets_are_text_without_defaults() {
        let mut token = def("token", SettingKind::Text);
        token.secret = true;
        assert!(token.check().is_ok());
        token.default = Some(SettingValue::Text("x".into()));
        assert!(token.check().is_err());
        let mut number = def(
            "n",
            SettingKind::Number {
                min: None,
                max: None,
            },
        );
        number.secret = true;
        assert!(number.check().is_err());
    }

    #[test]
    fn keys_allow_only_the_manifest_alphabet() {
        assert!(is_valid_setting_key("aws.config-file_2"));
        assert!(!is_valid_setting_key(""));
        assert!(!is_valid_setting_key("Upper"));
        assert!(!is_valid_setting_key("a b"));
        assert!(!is_valid_setting_key("clouddeck.demo_scenario"));
    }
}
