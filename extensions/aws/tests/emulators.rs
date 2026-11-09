use clouddeck_provider_kit::{AwsFiles, ComputeService};
use std::path::PathBuf;

#[tokio::test]
#[ignore = "needs the floci emulator: CLOUDDECK_EMULATORS=1 AWS_SHARED_CREDENTIALS_FILE=testing/emulators/aws/credentials, after testing/emulators/run.sh start aws"]
async fn lists_seeded_instances_from_the_floci_emulator() {
    assert_eq!(
        std::env::var("CLOUDDECK_EMULATORS").as_deref(),
        Ok("1"),
        "set CLOUDDECK_EMULATORS=1 to run the emulator tests"
    );
    assert!(
        std::env::var_os("AWS_SHARED_CREDENTIALS_FILE").is_some(),
        "set AWS_SHARED_CREDENTIALS_FILE to testing/emulators/aws/credentials so the real ~/.aws is not read"
    );
    let config =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testing/emulators/aws/config");
    let service =
        clouddeck_aws::AwsSdkComputeService::new("local-dev", "us-east-1").with_files(AwsFiles {
            config,
            ..AwsFiles::default()
        });

    let result = service
        .list_instances(None)
        .await
        .expect("listing EC2 instances from the emulator");

    assert!(
        !result.instances.is_empty(),
        "the seeded emulator should list at least one instance"
    );
}
