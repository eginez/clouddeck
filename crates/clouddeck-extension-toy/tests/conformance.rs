#[allow(dead_code)]
mod common;

use std::sync::Arc;

use clouddeck_core::extension_host::Manifest;
use clouddeck_extension_api::conformance::{check_manifest_settings, check_provider};
use clouddeck_extension_toy::ToyProvider;

#[test]
fn toy_provider_conforms_to_contract_v1() {
    let violations = check_provider(Arc::new(ToyProvider));
    assert!(
        violations.is_empty(),
        "contract v1 violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn toy_manifest_settings_match_the_code() {
    let manifest = Manifest::parse(&common::manifest_text()).unwrap();
    let violations = check_manifest_settings(&ToyProvider, &manifest.settings);
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}
