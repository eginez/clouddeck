//! Provider contract for CloudDeck extensions.
//!
//! 0.3.0 adds optional fields that describe onboarding: `Connection::detail` and `tags`,
//! `ProviderInfo::onboarding` (type `Onboarding`), `AuthMethod::header_button` and
//! `ProviderError::hint` (type `ErrorHint`). Each has a
//! serde default, and the wire format names its fields, so a payload from a 0.2 provider still
//! decodes and `API_VERSION` is unchanged.

mod auth;
mod error;
mod field;
mod host;
mod ids;
mod inspector;
mod instance;
mod provider;
mod selection;
mod settings;

pub use auth::{AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, PromptField};
pub use error::{ErrorHint, ErrorKind, ProviderError};
pub use field::{CoreField, FieldDef, FieldKind, FieldSource, MatchMode};
pub use host::{Call, Host, LogLevel};
pub use ids::{ConnectionId, DimensionId, FlowId, INSTANCES, KindId, ResourceId, ValueId};
pub use inspector::{InspectorRowDef, InspectorSectionDef, InspectorValue};
pub use instance::{Address, AddressKind, ExtrasSection, Instance, Label, State, StateKind};
pub use provider::{
    Auth, Browse, CellFailure, Config, Connection, KindInfo, ListResult, NoAuth, NoConfig,
    Onboarding, Provider, ProviderInfo,
};
pub use selection::{DimValue, Dimension, Selection};
pub use settings::{
    SettingDef, SettingError, SettingKind, SettingOption, SettingValue, is_valid_setting_key,
    settings_mismatches,
};
