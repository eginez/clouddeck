#[test]
fn builtin_providers_are_aws_only() {
    let ids: Vec<String> = clouddeck_aws::builtin_providers()
        .iter()
        .map(|provider| provider.info().id)
        .collect();
    assert_eq!(ids, ["aws"]);
}

#[test]
fn app_registry_without_extensions_holds_aws_and_demo() {
    let root = tempfile::tempdir().unwrap();
    let state_root = tempfile::tempdir().unwrap();

    let registry = clouddeck_core::extension_host::app_registry(
        None,
        &root.path().join("absent"),
        state_root.path(),
        vec![clouddeck_aws::aws_provider()],
    );

    let ids: Vec<String> = registry
        .providers()
        .iter()
        .map(|provider| provider.info().id)
        .collect();
    assert_eq!(ids, ["aws", "demo"]);
    assert!(registry.refusals().is_empty());
}
