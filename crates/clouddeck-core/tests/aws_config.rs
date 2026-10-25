use clouddeck_core::{
    AwsConfigFile, AwsCredentialKind, AwsProfile, AwsProfileAuth, AwsSsoConfiguration,
    parse_profile_auth, parse_profiles, read_profile_auth,
};

const LEGACY_SSO: &str = include_str!("fixtures/aws/legacy-sso-start-url.config");
const SSO_SESSION_SHARED: &str = include_str!("fixtures/aws/sso-session-shared.config");
const ROLE_SOURCE: &str = include_str!("fixtures/aws/role-source-profile.config");
const CREDENTIAL_PROCESS: &str = include_str!("fixtures/aws/credential-process.config");
const NO_REGION: &str = include_str!("fixtures/aws/no-region.config");
const DUPLICATE_PROFILES: &str = include_str!("fixtures/aws/duplicate-profiles.config");
const MALFORMED_CONFIG: &str = include_str!("fixtures/aws/malformed.config");
const MALFORMED_CREDENTIALS: &str = include_str!("fixtures/aws/malformed.credentials");
const SHARED_CREDENTIALS: &str = include_str!("fixtures/aws/shared.credentials");

fn auth(config: &str, credentials: &str) -> Vec<AwsProfileAuth> {
    parse_profile_auth(config, credentials).expect("fixture should parse")
}

fn profile<'a>(profiles: &'a [AwsProfileAuth], name: &str) -> &'a AwsProfileAuth {
    profiles
        .iter()
        .find(|profile| profile.name == name)
        .unwrap_or_else(|| panic!("profile {name} missing"))
}

fn sso(start_url: &str, region: &str, session: Option<&str>) -> AwsCredentialKind {
    AwsCredentialKind::Sso(AwsSsoConfiguration {
        start_url: start_url.into(),
        region: region.into(),
        account_id: None,
        role_name: None,
        session_name: session.map(Into::into),
    })
}

#[test]
fn legacy_sso_start_url_profile_resolves_without_session() {
    let profiles = auth(LEGACY_SSO, "");

    let legacy = profile(&profiles, "legacy");
    assert_eq!(legacy.region.as_deref(), Some("eu-west-1"));
    assert_eq!(
        legacy.credential_kind,
        AwsCredentialKind::Sso(AwsSsoConfiguration {
            start_url: "https://legacy.awsapps.com/start".into(),
            region: "us-west-2".into(),
            account_id: Some("123456789012".into()),
            role_name: Some("Developer".into()),
            session_name: None,
        })
    );
}

#[test]
fn legacy_default_profile_without_credentials_is_unconfigured() {
    let profiles = auth(LEGACY_SSO, "");

    let default = profile(&profiles, "default");
    assert_eq!(default.region.as_deref(), Some("us-east-1"));
    assert_eq!(default.credential_kind, AwsCredentialKind::Unconfigured);
}

#[test]
fn sso_session_shared_by_several_profiles_resolves_each_profile() {
    let profiles = auth(SSO_SESSION_SHARED, "");

    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["dev", "orphan", "prod"]);
    assert_eq!(
        profile(&profiles, "dev").credential_kind,
        AwsCredentialKind::Sso(AwsSsoConfiguration {
            start_url: "https://corp.awsapps.com/start".into(),
            region: "us-east-1".into(),
            account_id: Some("111111111111".into()),
            role_name: Some("Developer".into()),
            session_name: Some("corp".into()),
        })
    );
    assert_eq!(
        profile(&profiles, "prod").credential_kind,
        AwsCredentialKind::Sso(AwsSsoConfiguration {
            start_url: "https://corp.awsapps.com/start".into(),
            region: "us-east-1".into(),
            account_id: Some("222222222222".into()),
            role_name: Some("ReadOnly".into()),
            session_name: Some("corp".into()),
        })
    );
}

#[test]
fn sso_session_referenced_but_not_defined_is_unconfigured() {
    let profiles = auth(SSO_SESSION_SHARED, "");

    let orphan = profile(&profiles, "orphan");
    assert_eq!(orphan.region.as_deref(), Some("us-west-1"));
    assert_eq!(orphan.credential_kind, AwsCredentialKind::Unconfigured);
}

#[test]
fn role_arn_with_source_profile_inherits_source_credentials() {
    let profiles = auth(ROLE_SOURCE, SHARED_CREDENTIALS);

    let base = profile(&profiles, "base");
    assert_eq!(
        profile(&profiles, "base-dev").credential_kind,
        base.credential_kind
    );
    assert_eq!(
        profile(&profiles, "base-dev").region.as_deref(),
        Some("us-west-2")
    );
    assert_eq!(
        profile(&profiles, "shared-dev").credential_kind,
        AwsCredentialKind::SharedCredentials
    );
    assert!(!format!("{profiles:?}").contains("do-not-retain"));
}

#[test]
fn role_arn_with_missing_source_profile_is_unconfigured() {
    let profiles = auth(ROLE_SOURCE, SHARED_CREDENTIALS);

    assert_eq!(
        profile(&profiles, "missing-source").credential_kind,
        AwsCredentialKind::Unconfigured
    );
}

#[test]
fn role_arn_source_profile_cycle_is_unconfigured_without_looping() {
    let profiles = auth(ROLE_SOURCE, SHARED_CREDENTIALS);

    assert_eq!(
        profile(&profiles, "chain-a").credential_kind,
        AwsCredentialKind::Unconfigured
    );
    assert_eq!(
        profile(&profiles, "chain-b").credential_kind,
        AwsCredentialKind::Unconfigured
    );
}

#[test]
fn credential_process_profile_is_external_provider() {
    let profiles = auth(CREDENTIAL_PROCESS, "");

    let external = profile(&profiles, "external");
    assert_eq!(external.region.as_deref(), Some("us-east-1"));
    assert_eq!(
        external.credential_kind,
        AwsCredentialKind::ExternalProvider
    );
}

#[test]
fn role_arn_sourced_from_credential_process_is_external_provider() {
    let profiles = auth(CREDENTIAL_PROCESS, "");

    assert_eq!(
        profile(&profiles, "external-role").credential_kind,
        AwsCredentialKind::ExternalProvider
    );
}

#[test]
fn credential_process_in_credentials_file_is_external_provider() {
    let profiles = auth(
        "",
        "[provider]\ncredential_process = /usr/local/bin/provider\nregion = ap-south-1\n",
    );

    assert_eq!(
        profile(&profiles, "provider").credential_kind,
        AwsCredentialKind::ExternalProvider
    );
}

#[test]
fn profile_without_region_has_no_region() {
    let profiles = auth(NO_REGION, SHARED_CREDENTIALS);

    let no_region = profile(&profiles, "no-region");
    assert_eq!(no_region.region, None);
    assert_eq!(
        no_region.credential_kind,
        AwsCredentialKind::SharedCredentials
    );
    let bare = profile(&profiles, "bare");
    assert_eq!(bare.region, None);
    assert_eq!(bare.credential_kind, AwsCredentialKind::Unconfigured);
}

#[test]
fn duplicate_profile_names_merge_with_last_value_winning() {
    let profiles = auth(DUPLICATE_PROFILES, "");

    let names: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["default", "team"]);
    assert_eq!(
        profile(&profiles, "default").region.as_deref(),
        Some("us-east-2")
    );
    let team = profile(&profiles, "team");
    assert_eq!(team.region.as_deref(), Some("eu-west-1"));
    assert_eq!(
        team.credential_kind,
        sso("https://team.awsapps.com/start", "eu-west-1", None)
    );
}

#[test]
fn duplicate_profile_region_in_credentials_does_not_override_config() {
    let profiles = auth(
        "[profile team]\nregion = eu-west-1\n",
        "[team]\nregion = ap-southeast-1\n",
    );

    assert_eq!(profiles[0].region.as_deref(), Some("eu-west-1"));
}

#[test]
fn malformed_config_returns_error_naming_file_and_line() {
    let error = parse_profile_auth(MALFORMED_CONFIG, "").unwrap_err();

    assert_eq!(error.file, AwsConfigFile::Config);
    assert_eq!(error.line, 4);
    assert!(error.to_string().contains("AWS config file, line 4"));
}

#[test]
fn malformed_credentials_returns_error_naming_file_and_line() {
    let error = parse_profile_auth("", MALFORMED_CREDENTIALS).unwrap_err();

    assert_eq!(error.file, AwsConfigFile::Credentials);
    assert_eq!(error.line, 4);
    assert!(!error.to_string().contains("do-not-retain"));
}

#[test]
fn malformed_ini_inputs_return_errors_without_panicking() {
    let cases = [
        "[profile unterminated",
        "[]",
        "[ ]",
        "[profile x]\nno equals sign here",
        "[profile x]\n = value-without-key",
        "[profile x]\nregion = us-east-1\n]",
    ];
    for input in cases {
        assert!(
            parse_profile_auth(input, "").is_err(),
            "expected error for {input:?}"
        );
        assert!(
            parse_profiles(input, "").is_err(),
            "expected error for {input:?}"
        );
    }
}

#[test]
fn non_ascii_and_comment_lines_parse_without_panicking() {
    let profiles = auth(
        "# comment with no equals\n; another comment\n[profile é]\nregion = 日本\n",
        "",
    );

    assert_eq!(profiles[0].name, "é");
    assert_eq!(profiles[0].region.as_deref(), Some("日本"));
}

#[test]
fn read_profile_auth_error_names_malformed_file_path_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config");
    let credentials = dir.path().join("credentials");
    std::fs::write(&config, "[profile ok]\nregion = us-east-1\nbroken line\n").unwrap();
    std::fs::write(&credentials, "").unwrap();

    let error = read_profile_auth(&config, &credentials).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    let message = error.to_string();
    assert!(message.contains(&config.display().to_string()), "{message}");
    assert!(message.contains("line 3"), "{message}");
}

#[test]
fn read_profile_auth_treats_missing_files_as_empty() {
    let dir = tempfile::tempdir().unwrap();

    let profiles = read_profile_auth(&dir.path().join("config"), &dir.path().join("credentials"))
        .expect("missing files are not an error");

    assert!(profiles.is_empty());
}

#[test]
fn profile_auth_converts_to_secret_free_profile() {
    let profiles: Vec<AwsProfile> = parse_profiles(
        "[profile team]\nregion = eu-west-1\nsso_start_url = https://team.awsapps.com/start\nsso_region = eu-west-1\n",
        "",
    )
    .unwrap();

    assert_eq!(
        profiles,
        vec![AwsProfile {
            name: "team".into(),
            region: Some("eu-west-1".into()),
        }]
    );
}

#[test]
fn indented_continuation_lines_are_accepted() {
    let profiles = auth(
        "[profile s3]\ns3 =\n  max_concurrent_requests = 20\n  multipart_threshold\nregion = us-east-1\n",
        "",
    );

    assert_eq!(profiles[0].region.as_deref(), Some("us-east-1"));
}

#[test]
fn utf8_byte_order_mark_is_ignored() {
    let profiles = auth("\u{feff}[profile team]\nregion = eu-west-1\n", "");

    assert_eq!(profiles[0].name, "team");
    assert_eq!(profiles[0].region.as_deref(), Some("eu-west-1"));
}

#[test]
fn region_only_credentials_section_is_not_shared_credentials() {
    let profiles = auth("", "[regional]\nregion = us-east-1\n");

    assert_eq!(
        profile(&profiles, "regional").credential_kind,
        AwsCredentialKind::Unconfigured
    );
}
