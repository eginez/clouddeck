use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::ids::{ConnectionId, FlowId};
use crate::settings::{SettingOption, folder_path_error, is_valid_setting_key};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMethod {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A key the host maps to one of its icons; unknown keys show no icon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Drawn before any call, so the host can show the form the method needs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_fields: Vec<PromptField>,
}

impl AuthMethod {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            icon: None,
            initial_fields: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthInput {
    pub connection: Option<ConnectionId>,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKind {
    #[default]
    Text,
    /// Masked on screen and never stored as a setting.
    Secret,
    Path,
    /// A directory, chosen with the native folder picker. Same rules as a Folder setting.
    Folder,
    Choice,
    /// Read-only text (the field's `help`), never submitted.
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptField {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub kind: PromptKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// Choice values with their labels; the default must be one of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SettingOption>,
    /// Changing this field calls `continue_auth` with `AuthAnswer::Change`, and the plugin answers with an updated prompt.
    #[serde(default)]
    pub refresh_on_change: bool,
    /// A plugin setting key the value is stored under as soon as it changes, so `Host::setting` sees it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persist_as: Option<String>,
    /// A message about this field alone, shown beside it after a failed attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PromptField {
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: PromptKind) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
            default: None,
            help: None,
            required: false,
            options: Vec::new(),
            refresh_on_change: false,
            persist_as: None,
            error: None,
        }
    }

    /// Checks the declaration itself: the id, the choice options and default, and the persisted key.
    pub fn check(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err(format!("field {:?} needs an id", self.label));
        }
        match self.kind {
            PromptKind::Choice => self.check_choice()?,
            PromptKind::Secret if self.persist_as.is_some() => {
                return Err(format!(
                    "field {} is a secret and cannot persist_as",
                    self.id
                ));
            }
            _ if !self.options.is_empty() => {
                return Err(format!("field {} has options but is not a choice", self.id));
            }
            _ => {}
        }
        if let Some(key) = &self.persist_as
            && !is_valid_setting_key(key)
        {
            return Err(format!(
                "field {} persists as an invalid setting key {key:?}",
                self.id
            ));
        }
        if let Some(default) = self.default.as_deref().filter(|d| !d.is_empty())
            && let Some(error) = self.value_error(default)
        {
            return Err(format!("field {} has an invalid default: {error}", self.id));
        }
        Ok(())
    }

    fn check_choice(&self) -> Result<(), String> {
        if self.options.is_empty() {
            return Err(format!("choice field {} needs options", self.id));
        }
        let values: BTreeSet<&str> = self.options.iter().map(|o| o.value.as_str()).collect();
        if values.len() != self.options.len() {
            return Err(format!("choice field {} lists an option twice", self.id));
        }
        match &self.default {
            Some(default) if values.contains(default.as_str()) => Ok(()),
            _ => Err(format!(
                "choice field {} needs a default that is one of its options",
                self.id
            )),
        }
    }

    /// Why `text` cannot be submitted for this field, if it cannot.
    pub fn value_error(&self, text: &str) -> Option<String> {
        match self.kind {
            PromptKind::Folder => folder_path_error(text),
            PromptKind::Choice if !self.options.iter().any(|o| o.value == text) => {
                Some(format!("{text:?} is not one of the options"))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthAnswer {
    Poll,
    Values(BTreeMap<String, String>),
    /// The user changed `field` in an open prompt; `values` holds every current value.
    Change {
        field: String,
        values: BTreeMap<String, String>,
    },
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStep {
    Done,
    Prompt {
        flow: FlowId,
        fields: Vec<PromptField>,
        /// A status line, such as "Found 3 profiles in /home/me/.aws".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// A form-level problem with the last attempt; the form stays up so the user can fix it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    OpenUrl {
        flow: FlowId,
        url: String,
        then_poll: bool,
    },
    DeviceCode {
        flow: FlowId,
        code: String,
        url: String,
        complete_url: Option<String>,
        expires_in: Duration,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStatus {
    SignedOut,
    SignedIn {
        expires: Option<SystemTime>,
    },
    Expired,
    /// The check itself failed (for example a network error), so the sign-in state is not known.
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choice() -> PromptField {
        PromptField {
            default: Some("eu".into()),
            options: vec![
                SettingOption {
                    value: "eu".into(),
                    label: "Europe".into(),
                },
                SettingOption {
                    value: "us".into(),
                    label: "United States".into(),
                },
            ],
            ..PromptField::new("region", "Region", PromptKind::Choice)
        }
    }

    #[test]
    fn a_choice_needs_options_and_a_default_among_them() {
        assert!(choice().check().is_ok());
        assert!(
            PromptField {
                default: None,
                ..choice()
            }
            .check()
            .is_err()
        );
        assert!(
            PromptField {
                default: Some("ap".into()),
                ..choice()
            }
            .check()
            .is_err()
        );
        assert!(
            PromptField {
                options: Vec::new(),
                ..choice()
            }
            .check()
            .is_err()
        );
    }

    #[test]
    fn a_choice_cannot_list_an_option_twice() {
        let mut field = choice();
        field.options.push(SettingOption {
            value: "eu".into(),
            label: "Again".into(),
        });
        assert!(field.check().is_err());
    }

    #[test]
    fn options_belong_only_to_choices() {
        let field = PromptField {
            options: choice().options,
            ..PromptField::new("token", "Token", PromptKind::Text)
        };
        assert!(field.check().is_err());
    }

    #[test]
    fn a_secret_cannot_persist() {
        let field = PromptField {
            persist_as: Some("token".into()),
            ..PromptField::new("token", "Token", PromptKind::Secret)
        };
        assert!(field.check().is_err());
    }

    #[test]
    fn persisted_keys_must_be_valid_setting_keys() {
        let folder = |key: &str| PromptField {
            persist_as: Some(key.into()),
            ..PromptField::new("folder", "Folder", PromptKind::Folder)
        };
        assert!(folder("config_dir").check().is_ok());
        assert!(folder("Not A Key").check().is_err());
    }

    #[test]
    fn a_field_needs_an_id() {
        assert!(
            PromptField::new("", "Nameless", PromptKind::Text)
                .check()
                .is_err()
        );
    }

    #[test]
    fn folder_values_follow_folder_setting_rules() {
        let field = PromptField::new("folder", "Folder", PromptKind::Folder);
        assert!(field.value_error("/srv/config").is_none());
        assert!(field.value_error("~/config").is_none());
        assert!(field.value_error("config").is_some());
    }

    #[test]
    fn choice_values_must_be_listed() {
        assert!(choice().value_error("eu").is_none());
        assert!(choice().value_error("ap").is_some());
    }

    #[test]
    fn defaults_must_pass_the_same_value_rules() {
        let folder = PromptField {
            default: Some("relative/path".into()),
            ..PromptField::new("folder", "Folder", PromptKind::Folder)
        };
        assert!(folder.check().is_err());
        let unset = PromptField {
            default: Some(String::new()),
            ..PromptField::new("folder", "Folder", PromptKind::Folder)
        };
        assert!(unset.check().is_ok());
    }
}
