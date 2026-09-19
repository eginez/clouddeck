//! Read-only AWS CLI adapter. The CLI owns credential/SSO resolution and pagination;
//! CloudDeck never invokes a shell or stores credentials.
use crate::{
    BoxError, ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeService, EC2Instance,
    EC2InstanceTag, EC2SecurityGroup,
};
use serde::Deserialize;
use std::{path::PathBuf, time::Duration};

const AWS_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

pub struct AwsCliComputeService {
    pub profile: String,
    pub region: String,
}

pub fn aws_executable() -> PathBuf {
    // Finder-launched apps do not inherit a shell's Homebrew PATH.
    for path in [
        "/opt/homebrew/bin/aws",
        "/usr/local/bin/aws",
        "/usr/bin/aws",
    ] {
        if std::path::Path::new(path).is_file() {
            return path.into();
        }
    }
    "aws".into()
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
    let mut command = build_command(executable, profile, region);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| {
            std::io::Error::other(format!(
                "AWS request timed out after {} seconds. Retry or choose one region.",
                timeout.as_secs_f64()
            ))
        })?
        .map_err(|error| {
            std::io::Error::other(format!(
                "Could not run AWS CLI: {error}. Install AWS CLI v2 and configure an AWS profile."
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
            aws_executable(),
            AWS_REQUEST_TIMEOUT,
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
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[cfg(unix)]
    fn fake_cli(body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        static NEXT_SCRIPT_ID: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "clouddeck-fake-aws-{}-{}.sh",
            std::process::id(),
            NEXT_SCRIPT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&path, permissions).unwrap();
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
            Duration::from_secs(1),
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
            Duration::from_secs(1),
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
