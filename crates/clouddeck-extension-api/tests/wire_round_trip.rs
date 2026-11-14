use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::{Duration, UNIX_EPOCH};

use clouddeck_extension_api::wire::{decode, encode};
use clouddeck_extension_api::{
    Address, AddressKind, AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, CellFailure,
    Connection, ConnectionId, CoreField, DimValue, Dimension, ErrorKind, ExtrasSection, FieldDef,
    FieldKind, FieldSource, FlowId, InspectorRowDef, InspectorSectionDef, InspectorValue, Instance,
    KindId, KindInfo, Label, ListResult, MatchMode, PromptField, ProviderError, ProviderInfo,
    ResourceId, Selection, SettingDef, SettingKind, SettingOption, SettingValue, State, StateKind,
    ValueId,
};
use serde::Serialize;
use serde::de::DeserializeOwned;

const UNICODE: &str = "Ünïcødé 名前 🚀";

fn round_trip<T: Serialize + DeserializeOwned + PartialEq + Debug>(value: T) {
    let bytes = encode(&value).expect("encodes");
    let decoded: T = decode(&bytes).expect("decodes");
    assert_eq!(decoded, value);
}

#[test]
fn instances_round_trip_with_empty_none_and_unicode_values() {
    round_trip(Instance::default());
    round_trip(Instance {
        id: UNICODE.into(),
        name: Some(String::new()),
        group: None,
        state: State {
            kind: StateKind::Terminated,
            raw: UNICODE.into(),
        },
        state_reason: Some("stopped by 管理者".into()),
        instance_type: None,
        region: Some("eu-west-1".into()),
        location: None,
        created: Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        addresses: vec![Address {
            kind: AddressKind::Public,
            value: "203.0.113.7".into(),
        }],
        labels: vec![
            Label {
                key: "owner".into(),
                value: None,
            },
            Label {
                key: UNICODE.into(),
                value: Some(String::new()),
            },
        ],
        extras: vec![
            ExtrasSection {
                title: "Details".into(),
                entries: Vec::new(),
            },
            ExtrasSection {
                title: UNICODE.into(),
                entries: vec![(String::new(), UNICODE.into())],
            },
        ],
    });
}

#[test]
fn selections_and_dimensions_round_trip() {
    round_trip(Selection::new());
    round_trip(
        Selection::new()
            .with("region", Vec::new())
            .with(UNICODE, vec![ValueId::from("eu"), ValueId::from(UNICODE)]),
    );
    round_trip(Dimension {
        id: "region".into(),
        label: "Region".into(),
        plural_label: None,
        multi: true,
        optional: false,
        values: Vec::new(),
    });
    round_trip(Dimension {
        id: UNICODE.into(),
        label: UNICODE.into(),
        plural_label: Some(String::new()),
        multi: false,
        optional: true,
        values: vec![
            DimValue {
                id: ValueId::from("us"),
                label: "US".into(),
                parent: None,
                default: true,
            },
            DimValue {
                id: ValueId::from("us-east"),
                label: UNICODE.into(),
                parent: Some(ValueId::from("us")),
                default: false,
            },
        ],
    });
}

#[test]
fn list_results_and_errors_round_trip() {
    round_trip(ListResult::default());
    round_trip(ListResult {
        resources: vec![Instance::default()],
        failures: vec![CellFailure {
            cell: Selection::new().with("region", Vec::new()),
            error: ProviderError {
                kind: ErrorKind::Throttled,
                message: UNICODE.into(),
                advice: Some(String::new()),
                retry_after: Some(Duration::from_millis(1500)),
                hint: None,
            },
        }],
    });
    for kind in [
        ErrorKind::Auth,
        ErrorKind::Network,
        ErrorKind::Timeout,
        ErrorKind::Permission,
        ErrorKind::Throttled,
        ErrorKind::NotFound,
        ErrorKind::Unsupported,
        ErrorKind::Other,
    ] {
        round_trip(ProviderError::of(kind));
    }
    round_trip(ProviderError {
        kind: ErrorKind::Network,
        message: String::new(),
        advice: Some(UNICODE.into()),
        retry_after: None,
        hint: None,
    });
}

#[test]
fn settings_round_trip_every_kind_and_value() {
    round_trip(SettingDef {
        key: "region".into(),
        label: "Region".into(),
        description: None,
        group: None,
        kind: SettingKind::Text,
        default: None,
        secret: false,
    });
    let kinds = vec![
        SettingKind::Text,
        SettingKind::Path,
        SettingKind::Bool,
        SettingKind::Number {
            min: None,
            max: Some(1.5),
        },
        SettingKind::DurationSecs {
            min: Some(1),
            max: None,
        },
        SettingKind::Choice(vec![SettingOption {
            value: "eu".into(),
            label: UNICODE.into(),
        }]),
    ];
    for kind in kinds {
        round_trip(SettingDef {
            key: UNICODE.into(),
            label: String::new(),
            description: Some(String::new()),
            group: Some(UNICODE.into()),
            kind,
            default: None,
            secret: true,
        });
    }
    let values = vec![
        SettingValue::Text(UNICODE.into()),
        SettingValue::Bool(true),
        SettingValue::Number(2.5),
        SettingValue::DurationSecs(30),
    ];
    for value in values {
        round_trip(SettingDef {
            key: "timeout".into(),
            label: "Timeout".into(),
            description: None,
            group: None,
            kind: SettingKind::Text,
            default: Some(value),
            secret: false,
        });
    }
}

#[test]
fn auth_round_trips_every_variant() {
    round_trip(AuthMethod::new(UNICODE.into(), String::new()));
    round_trip(AuthMethod {
        id: "token".into(),
        label: "API token".into(),
        header_button: None,
        description: Some("Paste a token from the console".into()),
        settings: vec!["config_dir".into(), "region".into()],
    });
    round_trip(AuthInput {
        connection: None,
        values: BTreeMap::new(),
    });
    round_trip(AuthInput {
        connection: Some(ConnectionId::from(UNICODE)),
        values: BTreeMap::from([(String::new(), UNICODE.into())]),
    });
    for answer in [
        AuthAnswer::Poll,
        AuthAnswer::Values(BTreeMap::new()),
        AuthAnswer::Values(BTreeMap::from([("code".into(), UNICODE.into())])),
        AuthAnswer::Cancel,
    ] {
        round_trip(answer);
    }
    let flow = FlowId::from("flow-1");
    for step in [
        AuthStep::Done,
        AuthStep::Prompt {
            flow: flow.clone(),
            fields: Vec::new(),
        },
        AuthStep::Prompt {
            flow: flow.clone(),
            fields: vec![PromptField {
                id: "token".into(),
                label: UNICODE.into(),
                secret: true,
            }],
        },
        AuthStep::OpenUrl {
            flow: flow.clone(),
            url: UNICODE.into(),
            then_poll: false,
        },
        AuthStep::DeviceCode {
            flow: flow.clone(),
            code: "ABCD-1234".into(),
            url: "https://example.com/device".into(),
            complete_url: None,
            expires_in: Duration::from_secs(900),
        },
        AuthStep::DeviceCode {
            flow,
            code: String::new(),
            url: String::new(),
            complete_url: Some(UNICODE.into()),
            expires_in: Duration::ZERO,
        },
    ] {
        round_trip(step);
    }
    for status in [
        AuthStatus::SignedOut,
        AuthStatus::SignedIn { expires: None },
        AuthStatus::SignedIn {
            expires: Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        },
        AuthStatus::Expired,
        AuthStatus::Unknown,
    ] {
        round_trip(status);
    }
}

#[test]
fn field_and_inspector_definitions_round_trip() {
    let sources = [
        FieldSource::Core(CoreField::Name),
        FieldSource::Extra {
            section: String::new(),
            key: UNICODE.into(),
        },
        FieldSource::Section {
            section: "Network".into(),
        },
        FieldSource::Labels,
        FieldSource::Concat(vec![
            FieldSource::Labels,
            FieldSource::Core(CoreField::Addresses),
        ]),
    ];
    for source in sources {
        round_trip(FieldDef {
            name: "name".into(),
            label: UNICODE.into(),
            hint: String::new(),
            kind: FieldKind::Text,
            match_mode: MatchMode::PrefixOrGlob,
            query: true,
            search: false,
            facet: true,
            column: false,
            source,
        });
    }

    round_trip(InspectorSectionDef {
        title: String::new(),
        rows: Vec::new(),
    });
    let values = [
        InspectorValue::State,
        InspectorValue::Type,
        InspectorValue::Location,
        InspectorValue::Created,
        InspectorValue::PrivateAddress,
        InspectorValue::PublicAddress,
        InspectorValue::Extra {
            section: UNICODE.into(),
            key: String::new(),
        },
    ];
    for value in values {
        round_trip(InspectorSectionDef {
            title: UNICODE.into(),
            rows: vec![InspectorRowDef {
                label: String::new(),
                value,
                monospace: true,
                copyable: false,
            }],
        });
    }
}

#[test]
fn provider_metadata_and_identifiers_round_trip() {
    round_trip(ProviderInfo {
        id: "toy".into(),
        name: UNICODE.into(),
        version: String::new(),
        connection_label: "Profile".into(),
        onboarding: Default::default(),
    });
    round_trip(KindInfo {
        id: KindId::instances(),
        label: UNICODE.into(),
    });
    round_trip(Connection {
        id: ConnectionId::from("default"),
        display_name: String::new(),
        detail: None,
        tags: Vec::new(),
    });
    round_trip(ResourceId::from(UNICODE));
    round_trip(None::<ConnectionId>);
    round_trip(Some(ConnectionId::from(UNICODE)));
    round_trip(());
    round_trip(Vec::<String>::new());
    round_trip(vec![UNICODE.to_string(), String::new()]);
    round_trip(Ok::<Vec<Connection>, ProviderError>(Vec::new()));
}

#[test]
fn unset_plural_label_is_left_out_of_the_messagepack_map() {
    let dimension = |plural_label: Option<&str>| Dimension {
        id: "d".into(),
        label: "Region".into(),
        plural_label: plural_label.map(str::to_string),
        multi: true,
        optional: false,
        values: Vec::new(),
    };
    let unset = rmp_serde::to_vec_named(&dimension(None)).unwrap();
    assert!(!contains(&unset, b"plural_label"));
    let set = rmp_serde::to_vec_named(&dimension(Some("Regions"))).unwrap();
    assert!(contains(&set, b"plural_label"));
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
