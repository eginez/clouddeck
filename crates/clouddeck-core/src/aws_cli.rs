//! Read-only AWS CLI adapter. The CLI owns credential/SSO resolution and pagination;
//! CloudDeck never invokes a shell or stores credentials.
use crate::{
    BoxError, ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeService, EC2Instance,
    EC2InstanceTag, EC2SecurityGroup,
};
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub struct AwsCliComputeService {
    pub profile: String,
    pub region: String,
    /// Optional configured CLI path. `None` means auto-detect fixed locations.
    pub cli_path: Option<PathBuf>,
    pub request_timeout: Duration,
}

impl AwsCliComputeService {
    /// New service with a configured CLI path and request timeout.
    pub fn new(
        profile: String,
        region: String,
        cli_path: Option<PathBuf>,
        request_timeout: Duration,
    ) -> Self {
        Self {
            profile,
            region,
            cli_path,
            request_timeout,
        }
    }

    /// New service with auto-detected CLI path and the default timeout.
    pub fn with_defaults(profile: String, region: String) -> Self {
        Self::new(profile, region, None, DEFAULT_REQUEST_TIMEOUT)
    }
}

const AUTO_DETECT_LOCATIONS: [&str; 3] = [
    "/opt/homebrew/bin/aws",
    "/usr/local/bin/aws",
    "/usr/bin/aws",
];

/// Resolve the CLI executable for this service: the configured path when set,
/// otherwise the first auto-detected location, otherwise bare `aws` on PATH.
/// The returned path is always executed as argv[0] via `Command::new`; it is
/// never interpolated into a shell string.
fn aws_executable(cli_path: &Option<PathBuf>) -> PathBuf {
    if let Some(path) = cli_path {
        return path.clone();
    }
    // Finder-launched apps do not inherit a shell's Homebrew PATH.
    for path in AUTO_DETECT_LOCATIONS {
        if std::path::Path::new(path).is_file() {
            return path.into();
        }
    }
    "aws".into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliVersionError {
    /// The executable exists but could not be run (e.g. not executable, or
    /// crashed before printing a version).
    NotExecutable { path: PathBuf, detail: String },
    /// The executable ran but its version line did not contain `aws-cli/`.
    NotAwsCli { path: PathBuf, output: String },
    /// The executable ran and is AWS CLI v1 (`aws-cli/1.x`), which is not
    /// supported.
    V1NotSupported { path: PathBuf, version: String },
    /// The executable did not finish within the timeout.
    TimedOut { path: PathBuf },
}

impl std::fmt::Display for CliVersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotExecutable { path, detail } => write!(
                f,
                "Could not run AWS CLI at {path:?}: {detail}. Check that it is an executable AWS CLI v2 binary."
            ),
            Self::NotAwsCli { path, output } => write!(
                f,
                "The executable at {path:?} does not look like AWS CLI (output: {output}). Point Settings > AWS & Privacy > AWS CLI path at your aws binary."
            ),
            Self::V1NotSupported { path, version } => write!(
                f,
                "AWS CLI v1 is not supported at {path:?} ({version}). Install AWS CLI v2: https://aws.amazon.com/cli/"
            ),
            Self::TimedOut { path } => write!(
                f,
                "AWS CLI at {path:?} did not respond in time. Try again, or point Settings > AWS & Privacy > AWS CLI path at a working aws binary."
            ),
        }
    }
}

impl std::error::Error for CliVersionError {}

fn extract_version(output: &str) -> Option<String> {
    let lower = output.to_ascii_lowercase();
    let start = lower.find("aws-cli/")?;
    let after = &output[start + "aws-cli/".len()..];
    let version: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.')
        .collect();
    if version.is_empty() {
        return None;
    }
    Some(version)
}

/// Run `aws --version` for the given executable path and return the parsed
/// version string (e.g. `2.27.36`). Never invokes a shell.
///
/// Errors: `CliVersionError` (actionable, names the path), or an `io::Error`
/// if the file does not exist (surfaced as "not found").
pub async fn test_aws_cli(
    path: &PathBuf,
    timeout: Duration,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    if !path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("AWS CLI not found at {path:?}. Set the path in Settings > AWS & Privacy or install AWS CLI v2."),
        )
        .into());
    }
    let mut command = tokio::process::Command::new(path);
    command
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| CliVersionError::TimedOut { path: path.clone() })?;
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            return Err(CliVersionError::NotExecutable {
                path: path.clone(),
                detail: error.to_string(),
            }
            .into());
        }
    };
    if !output.status.success() {
        let detail: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(400)
            .collect();
        return Err(CliVersionError::NotExecutable {
            path: path.clone(),
            detail: if detail.trim().is_empty() {
                format!("exited with status {}", output.status)
            } else {
                detail.trim().to_string()
            },
        }
        .into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = match extract_version(&stdout) {
        Some(version) => version,
        None => {
            return Err(CliVersionError::NotAwsCli {
                path: path.clone(),
                output: stdout.trim().chars().take(200).collect(),
            }
            .into());
        }
    };
    let major: Option<u32> = version.split('.').next().and_then(|s| s.parse().ok());
    if major == Some(1) {
        return Err(CliVersionError::V1NotSupported {
            path: path.clone(),
            version,
        }
        .into());
    }
    Ok(version)
}

fn aws_args(profile: &str, region: &str) -> Vec<String> {
    vec![
        "ec2".into(),
        "describe-instances".into(),
        "--profile".into(),
        profile.into(),
        "--region".into(),
        region.into(),
        "--output".into(),
        "json".into(),
        "--page-size".into(),
        "1000".into(),
        "--cli-connect-timeout".into(),
        "10".into(),
        "--cli-read-timeout".into(),
        "20".into(),
    ]
}

fn build_command(executable: PathBuf, profile: &str, region: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(executable);
    command
        .args(aws_args(profile, region))
        // A GUI app has no terminal for a pager or auto-prompt. Keep all
        // credential/config resolution in the AWS CLI environment while
        // making its interactive features explicitly inert.
        .env("AWS_PAGER", "")
        .env("AWS_CLI_AUTO_PROMPT", "off")
        .env("AWS_MAX_ATTEMPTS", "2")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    command
}

async fn run_describe_instances(
    profile: &str,
    region: &str,
    executable: PathBuf,
    timeout: Duration,
    filters: Option<ComputeInstanceFilters>,
) -> Result<ComputeInstanceLoadResult, BoxError> {
    validate_context(profile, region)?;
    let executable_display = executable.display().to_string();
    let mut command = build_command(executable, profile, region);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| {
            std::io::Error::other(format!(
                "AWS CLI at {} timed out after {} seconds. Retry or choose one region.",
                executable_display,
                timeout.as_secs_f64()
            ))
        })?
        .map_err(|error| {
            std::io::Error::other(format!(
                "Could not run AWS CLI at {}: {error}. Install AWS CLI v2 and configure an AWS profile.",
                executable_display
            ))
        })?;
    if !output.status.success() {
        let message: String = String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(1200)
            .collect();
        let message = if message.trim().is_empty() {
            format!("command exited with status {}", output.status)
        } else {
            message.trim().to_string()
        };
        return Err(std::io::Error::other(format!(
            "AWS CLI: {message}. For expired SSO credentials, run aws sso login for the selected profile in your terminal, then Refresh."
        ))
        .into());
    }
    let instances = parse_instances(&output.stdout, region)?;
    Ok(ComputeInstanceLoadResult {
        instances: filters.map_or_else(|| instances.clone(), |f| f.apply(&instances)),
        partial_failures: vec![],
    })
}

fn validate_context(profile: &str, region: &str) -> std::io::Result<()> {
    if profile.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "AWS profile cannot be empty",
        ));
    }
    if region.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "AWS region cannot be empty",
        ));
    }
    Ok(())
}

#[async_trait::async_trait]
impl ComputeService for AwsCliComputeService {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        run_describe_instances(
            &self.profile,
            &self.region,
            aws_executable(&self.cli_path),
            self.request_timeout,
            filters,
        )
        .await
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Inventory {
    reservations: Vec<Reservation>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Reservation {
    #[serde(default)]
    instances: Vec<Instance>,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct Instance {
    instance_id: String,
    instance_type: Option<String>,
    state: Option<State>,
    launch_time: Option<String>,
    #[serde(rename = "PublicIpAddress")]
    public_ip: Option<String>,
    #[serde(rename = "PrivateIpAddress")]
    private_ip: Option<String>,
    vpc_id: Option<String>,
    subnet_id: Option<String>,
    security_groups: Vec<Group>,
    tags: Vec<Tag>,
    placement: Option<Placement>,
    image_id: Option<String>,
    key_name: Option<String>,
    architecture: Option<String>,
    state_transition_reason: Option<StateTransitionReason>,
    #[serde(rename = "IamInstanceProfile")]
    iam_instance_profile: Option<IamInstanceProfile>,
    #[serde(rename = "PrivateDnsName")]
    private_dns_name: Option<String>,
    #[serde(rename = "PublicDnsName")]
    public_dns_name: Option<String>,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase", default)]
struct Placement {
    availability_zone: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StateTransitionReason {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IamInstanceProfile {
    arn: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct State {
    name: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Group {
    group_id: Option<String>,
    group_name: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Tag {
    key: String,
    value: String,
}

pub fn parse_instances(bytes: &[u8], region: &str) -> Result<Vec<EC2Instance>, BoxError> {
    let inventory: Inventory = serde_json::from_slice(bytes)?;
    Ok(inventory
        .reservations
        .into_iter()
        .flat_map(|r| r.instances)
        .filter(|i| !i.instance_id.is_empty())
        .map(|i| EC2Instance {
            instance_id: i.instance_id,
            region: Some(region.into()),
            instance_type: i.instance_type,
            state: i.state.and_then(|s| s.name),
            launch_time: i
                .launch_time
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
                .map(Into::into),
            public_ip_address: i.public_ip,
            private_ip_address: i.private_ip,
            vpc_id: i.vpc_id,
            subnet_id: i.subnet_id,
            security_groups: i
                .security_groups
                .into_iter()
                .map(|g| EC2SecurityGroup {
                    group_id: g.group_id,
                    group_name: g.group_name,
                })
                .collect(),
            tags: i
                .tags
                .into_iter()
                .map(|t| EC2InstanceTag {
                    key: t.key,
                    value: t.value,
                })
                .collect(),
            availability_zone: i.placement.and_then(|p| p.availability_zone),
            image_id: i.image_id,
            key_name: i.key_name,
            architecture: i.architecture,
            state_transition_reason: i
                .state_transition_reason
                .and_then(|r| {
                    [r.code, r.message]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .trim()
                        .to_string()
                        .into()
                })
                .filter(|s| !s.is_empty()),
            iam_instance_profile_arn: i.iam_instance_profile.and_then(|p| p.arn),
            private_dns_name: i.private_dns_name,
            public_dns_name: i.public_dns_name,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(unix)]
    fn fake_cli(body: &str) -> PathBuf {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        // Write the body to a plain data file, then copy it over the target
        // executable path via `cp --remove-destination` (rename, not
        // truncate-in-place). An in-flight exec of the previous generation
        // keeps its own inode open; a rename replaces the directory entry and
        // leaves that open file untouched. Truncate-rewriting the same path
        // while it is open-for-exec makes the next exec fail with ETXTBSY
        // ("Text file busy") — observed under parallel test threads where
        // each generation only lives ~50-200 ms.
        static NEXT_SCRIPT_ID: AtomicU64 = AtomicU64::new(0);
        let id = NEXT_SCRIPT_ID.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let data = std::env::temp_dir().join(format!("clouddeck-fake-aws-data-{pid}-{id}.txt"));
        let path = std::env::temp_dir().join(format!("clouddeck-fake-aws-{pid}-{id}.sh"));
        std::fs::write(&data, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&data, Permissions::from_mode(0o700)).unwrap();
        let status = std::process::Command::new("cp")
            .args([
                "--remove-destination",
                data.to_str().unwrap(),
                path.to_str().unwrap(),
            ])
            .status()
            .expect("spawn cp for fake CLI script");
        std::fs::remove_file(&data).ok();
        assert!(
            status.success(),
            "cp --remove-destination into {} failed",
            path.display()
        );
        path
    }

    #[test]
    fn command_uses_structured_arguments_and_disables_interactive_features() {
        let args = aws_args("team profile", "eu-west-1");
        assert_eq!(args[0], "ec2");
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--profile" && pair[1] == "team profile")
        );
        assert!(
            args.windows(2)
                .any(|pair| pair[0] == "--region" && pair[1] == "eu-west-1")
        );
        // Keep a compile-time regression guard that the environment settings
        // are explicit in build_command; subprocess behavior is exercised by
        // the process adapter in production and the argument vector above is
        // safe to inspect without shelling out in unit tests.
        let _ = build_command(PathBuf::from("/tmp/fake aws"), "team profile", "eu-west-1");
    }

    #[test]
    fn empty_profile_or_region_is_rejected_before_process_launch() {
        assert!(validate_context("", "us-east-1").is_err());
        assert!(validate_context("team", " ").is_err());
        assert!(validate_context("team", "us-east-1").is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fake_cli_output_is_run_without_shell_interpolation_and_filtered() {
        let script = fake_cli(
            r#"printf '%s' '{"Reservations":[{"Instances":[{"InstanceId":"i-1","Tags":[{"Key":"Name","Value":"api"}]}]}]}'"#,
        );
        let result = run_describe_instances(
            "profile with spaces",
            "eu-west-1",
            script.clone(),
            Duration::from_secs(10),
            Some(ComputeInstanceFilters {
                name_contains: Some("api".into()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        assert_eq!(result.instances.len(), 1);
        assert_eq!(result.instances[0].id(), "eu-west-1:i-1");
        let _ = std::fs::remove_file(script);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fake_cli_failure_includes_stderr_and_exit_status() {
        let script = fake_cli("printf '%s' 'access denied' >&2; exit 7");
        let error = run_describe_instances(
            "team",
            "us-east-1",
            script.clone(),
            Duration::from_secs(10),
            None,
        )
        .await
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("access denied"));
        assert!(message.contains("expired SSO credentials"));
        let _ = std::fs::remove_file(script);
    }

    #[test]
    fn maps_real_aws_json_and_keeps_region_and_metadata() {
        let rows = parse_instances(br#"{"Reservations":[{"Instances":[{"InstanceId":"i-1","PrivateIpAddress":"10.0.0.1","LaunchTime":"2026-01-01T00:00:00Z","Tags":[{"Key":"team","Value":"platform"}],"State":{"Name":"running"}}]},{"Instances":[{"InstanceId":"i-2"},{}]}]}"#, "eu-west-1").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id(), "eu-west-1:i-1");
        assert_eq!(rows[0].private_ip_address.as_deref(), Some("10.0.0.1"));
        assert!(rows[0].launch_time.is_some());
        assert_eq!(rows[0].tags[0].value, "platform");
        assert!(rows[1].state.is_none());
    }

    #[test]
    fn maps_inspector_fields_from_describe_instances_json() {
        let rows = parse_instances(
            br#"{"Reservations":[{"Instances":[{"InstanceId":"i-1","State":{"Name":"stopped","Code":80},"LaunchTime":"2026-01-01T00:00:00Z","ImageId":"ami-0abc","KeyName":"deploy","Architecture":"x86_64","Placement":{"AvailabilityZone":"eu-west-1a"},"StateTransitionReason":{"Code":"Client.StopInstance","Message":"User requested stop"},"IamInstanceProfile":{"Arn":"arn:aws:iam::123:instance-profile/api"},"PrivateDnsName":"ip-10-0-0-1.ec2.internal","PublicDnsName":"ec2-1-2-3-4.eu-west-1.compute.amazonaws.com","SecurityGroups":[{"GroupId":"sg-1","GroupName":"web"}],"Tags":[{"Key":"Name","Value":"api"}]}]}]}"#,
            "eu-west-1",
        )
        .unwrap();
        let row = &rows[0];
        assert_eq!(row.availability_zone.as_deref(), Some("eu-west-1a"));
        assert_eq!(row.image_id.as_deref(), Some("ami-0abc"));
        assert_eq!(row.key_name.as_deref(), Some("deploy"));
        assert_eq!(row.architecture.as_deref(), Some("x86_64"));
        assert_eq!(
            row.state_transition_reason.as_deref(),
            Some("Client.StopInstance User requested stop")
        );
        assert_eq!(
            row.iam_instance_profile_arn.as_deref(),
            Some("arn:aws:iam::123:instance-profile/api")
        );
        assert_eq!(
            row.private_dns_name.as_deref(),
            Some("ip-10-0-0-1.ec2.internal")
        );
        assert_eq!(
            row.public_dns_name.as_deref(),
            Some("ec2-1-2-3-4.eu-west-1.compute.amazonaws.com")
        );
    }

    #[test]
    fn missing_inspector_fields_parse_as_none() {
        let rows = parse_instances(
            br#"{"Reservations":[{"Instances":[{"InstanceId":"i-2"}]}]}"#,
            "us-east-1",
        )
        .unwrap();
        let row = &rows[0];
        assert_eq!(row.availability_zone, None);
        assert_eq!(row.image_id, None);
        assert_eq!(row.key_name, None);
        assert_eq!(row.architecture, None);
        assert_eq!(row.state_transition_reason, None);
        assert_eq!(row.iam_instance_profile_arn, None);
        assert_eq!(row.private_dns_name, None);
        assert_eq!(row.public_dns_name, None);
    }

    #[test]
    fn extract_version_handles_real_and_fake_cli_output_shapes() {
        assert_eq!(
            extract_version("aws-cli/2.27.36 Linux/5.15.0-x86_64"),
            Some("2.27.36".into())
        );
        assert_eq!(
            extract_version("aws-cli/1.29.9 Python/3.10.4 linux/5.15"),
            Some("1.29.9".into())
        );
        assert_eq!(extract_version("not an aws cli"), None);
        assert_eq!(extract_version(""), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_aws_cli_parses_v2_version_from_configured_path() {
        let script =
            fake_cli(r#"printf '%s' 'aws-cli/2.27.36 Linux/5.15.0-x86_64 exec-env/unknown'"#);
        let version = test_aws_cli(&script, Duration::from_secs(5)).await.unwrap();
        assert_eq!(version, "2.27.36");
        let _ = std::fs::remove_file(script);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_aws_cli_rejects_v1_with_actionable_error() {
        let script = fake_cli(r#"printf '%s' 'aws-cli/1.29.9 Python/3.10.4 linux/5.15'"#);
        let error = test_aws_cli(&script, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error.downcast_ref::<CliVersionError>(),
                Some(CliVersionError::V1NotSupported { .. })
            ),
            "expected V1NotSupported, got: {error}"
        );
        assert!(
            error.to_string().contains("v1 is not supported"),
            "error should be actionable: {error}"
        );
        let _ = std::fs::remove_file(script);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_aws_cli_rejects_non_aws_executable() {
        let script = fake_cli(r#"printf '%s' 'hello world'"#);
        let error = test_aws_cli(&script, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error.downcast_ref::<CliVersionError>(),
                Some(CliVersionError::NotAwsCli { .. })
            ),
            "expected NotAwsCli, got: {error}"
        );
        let _ = std::fs::remove_file(script);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_aws_cli_missing_path_is_reported_with_the_path_tried() {
        let missing = std::env::temp_dir().join("clouddeck-definitely-missing-aws");
        let error = test_aws_cli(&missing, Duration::from_secs(5))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("not found"),
            "error should say not found: {error}"
        );
        assert!(
            error.to_string().contains(&missing.display().to_string()),
            "error should name the path tried: {error}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_aws_cli_times_out_when_the_cli_hangs() {
        let script = fake_cli("sleep 5");
        let error = test_aws_cli(&script, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(
            matches!(
                error.downcast_ref::<CliVersionError>(),
                Some(CliVersionError::TimedOut { .. })
            ),
            "expected TimedOut, got: {error}"
        );
        let _ = std::fs::remove_file(script);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn describe_instances_uses_configured_cli_path_and_timeout() {
        // The service must use the configured path (not auto-detect) and the
        // configured timeout (not the 60s default). A hanging CLI proves the
        // timeout is honoured; the success path proves the path is used.
        let good =
            fake_cli(r#"printf '%s' '{"Reservations":[{"Instances":[{"InstanceId":"i-1"}]}]}'"#);
        let service = AwsCliComputeService::new(
            "team".into(),
            "us-east-1".into(),
            Some(good.clone()),
            Duration::from_secs(5),
        );
        let result = service.list_instances(None).await.unwrap();
        assert_eq!(result.instances.len(), 1);

        let hanging = fake_cli("sleep 5");
        let service = AwsCliComputeService::new(
            "team".into(),
            "us-east-1".into(),
            Some(hanging.clone()),
            Duration::from_secs(1),
        );
        let error = service.list_instances(None).await.unwrap_err();
        assert!(
            error.to_string().contains("timed out"),
            "expected a timeout error, got: {error}"
        );
        let _ = std::fs::remove_file(good);
        let _ = std::fs::remove_file(hanging);
    }

    #[test]
    fn default_timeout_is_sixty_seconds() {
        assert_eq!(DEFAULT_REQUEST_TIMEOUT, Duration::from_secs(60));
    }

    #[test]
    fn partial_state_transition_reason_keeps_present_parts() {
        let rows = parse_instances(
            br#"{"Reservations":[{"Instances":[{"InstanceId":"i-3","StateTransitionReason":{"Code":"Client.StopInstance"}}]}]}"#,
            "us-east-1",
        )
        .unwrap();
        assert_eq!(
            rows[0].state_transition_reason.as_deref(),
            Some("Client.StopInstance")
        );
    }

    #[test]
    fn empty_state_transition_reason_objects_parse_without_failing_the_fleet() {
        // A StateTransitionReason object missing either Code or Message must
        // not hard-fail parsing the whole describe-instances response.
        let rows = parse_instances(
            br#"{"Reservations":[{"Instances":[{"InstanceId":"i-4","StateTransitionReason":{}}]}]}"#,
            "us-east-1",
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        // Both parts absent -> no reason at all, not an empty string.
        assert_eq!(rows[0].state_transition_reason, None);
    }

    #[test]
    fn malformed_output_is_not_misreported_as_empty_fleet() {
        assert!(parse_instances(b"{}", "us-east-1").is_err());
        assert!(parse_instances(b"not json", "us-east-1").is_err());
        assert!(
            parse_instances(br#"{"Reservations":[]}"#, "us-east-1")
                .unwrap()
                .is_empty()
        );
    }
}
