use clouddeck_extension_api::conformance::check_provider;
use clouddeck_extension_toy::ToyProvider;

#[test]
fn toy_provider_conforms_to_contract_v1() {
    let violations = check_provider(&ToyProvider);
    assert!(
        violations.is_empty(),
        "contract v1 violations:\n{}",
        violations.join("\n")
    );
}
