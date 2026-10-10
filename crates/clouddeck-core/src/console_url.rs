use std::fmt;

use crate::models::EC2Instance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Partition {
    Aws,
    AwsCn,
    AwsUsGov,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleUrlError {
    EmptyRegion,
    InvalidRegion(String),
    EmptyInstanceId,
    InvalidInstanceId(String),
}

impl fmt::Display for ConsoleUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRegion => write!(f, "Cannot open the console: the region is empty."),
            Self::InvalidRegion(region) => write!(
                f,
                "Cannot open the console: {region:?} is not a valid AWS region."
            ),
            Self::EmptyInstanceId => {
                write!(f, "Cannot open the console: the instance ID is empty.")
            }
            Self::InvalidInstanceId(instance_id) => write!(
                f,
                "Cannot open the console: {instance_id:?} is not a valid EC2 instance ID."
            ),
        }
    }
}

impl std::error::Error for ConsoleUrlError {}

pub fn partition_for_region(region: &str) -> Partition {
    let region = region.trim();
    if region.starts_with("cn-") {
        Partition::AwsCn
    } else if region.starts_with("us-gov-") {
        Partition::AwsUsGov
    } else {
        Partition::Aws
    }
}

/// Builds the EC2 console URL for one instance; read-only, no network access.
///
/// The console host formats follow AWS documentation conventions and have not
/// been checked against the live console.
pub fn console_url_for_instance(
    region: &str,
    instance_id: &str,
) -> Result<String, ConsoleUrlError> {
    let region_input = region.trim();
    if region_input.is_empty() {
        return Err(ConsoleUrlError::EmptyRegion);
    }
    if !is_valid_region(region_input) {
        return Err(ConsoleUrlError::InvalidRegion(region.to_string()));
    }

    let instance_id_input = instance_id.trim();
    if instance_id_input.is_empty() {
        return Err(ConsoleUrlError::EmptyInstanceId);
    }
    if !is_valid_instance_id(instance_id_input) {
        return Err(ConsoleUrlError::InvalidInstanceId(instance_id.to_string()));
    }

    let host = match partition_for_region(region_input) {
        Partition::Aws => format!("{region_input}.console.aws.amazon.com"),
        Partition::AwsCn => format!("{region_input}.console.amazonaws.cn"),
        Partition::AwsUsGov => format!("{region_input}.console.amazonaws-us-gov.com"),
    };
    Ok(format!(
        "https://{host}/ec2/home?region={region_input}#InstanceDetails:instanceId={instance_id_input}"
    ))
}

pub fn console_url_for(instance: &EC2Instance) -> Result<String, ConsoleUrlError> {
    let region = instance.region.as_deref().unwrap_or_default();
    console_url_for_instance(region, &instance.instance_id)
}

fn is_valid_region(region: &str) -> bool {
    region
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn is_valid_instance_id(instance_id: &str) -> bool {
    let Some(suffix) = instance_id.strip_prefix("i-") else {
        return false;
    };
    matches!(suffix.len(), 8 | 17)
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance_in(region: Option<&str>) -> EC2Instance {
        EC2Instance {
            instance_id: "i-0123456789abcdef0".to_string(),
            region: region.map(str::to_string),
            instance_type: None,
            state: None,
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: Vec::new(),
            tags: Vec::new(),
            availability_zone: None,
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        }
    }

    #[test]
    fn builds_commercial_console_url() {
        assert_eq!(
            console_url_for_instance("us-east-1", "i-0123456789abcdef0").unwrap(),
            "https://us-east-1.console.aws.amazon.com/ec2/home?region=us-east-1#InstanceDetails:instanceId=i-0123456789abcdef0"
        );
    }

    #[test]
    fn builds_china_console_url() {
        assert_eq!(
            console_url_for_instance("cn-north-1", "i-0abc1234").unwrap(),
            "https://cn-north-1.console.amazonaws.cn/ec2/home?region=cn-north-1#InstanceDetails:instanceId=i-0abc1234"
        );
    }

    #[test]
    fn builds_govcloud_console_url() {
        assert_eq!(
            console_url_for_instance("us-gov-west-1", "i-0abc1234").unwrap(),
            "https://us-gov-west-1.console.amazonaws-us-gov.com/ec2/home?region=us-gov-west-1#InstanceDetails:instanceId=i-0abc1234"
        );
    }

    #[test]
    fn trims_region_and_instance_id_before_building() {
        assert_eq!(
            console_url_for_instance("  us-east-1 ", " i-0abc1234\n").unwrap(),
            "https://us-east-1.console.aws.amazon.com/ec2/home?region=us-east-1#InstanceDetails:instanceId=i-0abc1234"
        );
    }

    #[test]
    fn partition_table() {
        assert_eq!(partition_for_region("us-east-1"), Partition::Aws);
        assert_eq!(partition_for_region("eu-west-1"), Partition::Aws);
        assert_eq!(partition_for_region("cn-north-1"), Partition::AwsCn);
        assert_eq!(partition_for_region("cn-northwest-1"), Partition::AwsCn);
        assert_eq!(partition_for_region("us-gov-west-1"), Partition::AwsUsGov);
        assert_eq!(partition_for_region("us-gov-east-1"), Partition::AwsUsGov);
        assert_eq!(partition_for_region("  cn-north-1  "), Partition::AwsCn);
        assert_eq!(partition_for_region("CN-north-1"), Partition::Aws);
        assert_eq!(partition_for_region("US-GOV-WEST-1"), Partition::Aws);
        assert_eq!(partition_for_region("us-gov"), Partition::Aws);
        assert_eq!(partition_for_region(""), Partition::Aws);
        assert_eq!(partition_for_region("   "), Partition::Aws);
    }

    #[test]
    fn empty_region_is_rejected() {
        assert_eq!(
            console_url_for_instance("", "i-0abc1234"),
            Err(ConsoleUrlError::EmptyRegion)
        );
        assert_eq!(
            console_url_for_instance("   ", "i-0abc1234"),
            Err(ConsoleUrlError::EmptyRegion)
        );
    }

    #[test]
    fn empty_instance_id_is_rejected() {
        assert_eq!(
            console_url_for_instance("us-east-1", ""),
            Err(ConsoleUrlError::EmptyInstanceId)
        );
        assert_eq!(
            console_url_for_instance("us-east-1", "  "),
            Err(ConsoleUrlError::EmptyInstanceId)
        );
    }

    #[test]
    fn region_with_url_metacharacters_is_rejected() {
        for region in [
            "us-east-1/evil",
            "us east 1",
            "us-east-1#x",
            "us-east-1?x=y",
            "US-EAST-1",
            "us_east_1",
        ] {
            assert_eq!(
                console_url_for_instance(region, "i-0abc1234"),
                Err(ConsoleUrlError::InvalidRegion(region.to_string())),
                "region {region:?} should be rejected"
            );
        }
    }

    #[test]
    fn instance_id_without_prefix_is_rejected() {
        assert_eq!(
            console_url_for_instance("us-east-1", "0123456789abcdef0"),
            Err(ConsoleUrlError::InvalidInstanceId(
                "0123456789abcdef0".to_string()
            ))
        );
    }

    #[test]
    fn instance_id_with_bad_characters_is_rejected() {
        for instance_id in [
            "i-0123ABCDEF",
            "i-0123456g",
            "i-0123456789abcdeg0",
            "i-01234567/",
            "i-0123456#",
            "I-0123456789abcdef0",
        ] {
            assert_eq!(
                console_url_for_instance("us-east-1", instance_id),
                Err(ConsoleUrlError::InvalidInstanceId(instance_id.to_string())),
                "instance id {instance_id:?} should be rejected"
            );
        }
    }

    #[test]
    fn instance_id_with_wrong_length_is_rejected() {
        for instance_id in [
            "i-0123456",
            "i-0123456789abcdef",
            "i-0123456789abcdef012",
            "i-",
        ] {
            assert_eq!(
                console_url_for_instance("us-east-1", instance_id),
                Err(ConsoleUrlError::InvalidInstanceId(instance_id.to_string())),
                "instance id {instance_id:?} should be rejected"
            );
        }
    }

    #[test]
    fn instance_id_accepts_eight_and_seventeen_hex_digits() {
        assert!(console_url_for_instance("us-east-1", "i-01234567").is_ok());
        assert!(console_url_for_instance("us-east-1", "i-0123456789abcdef0").is_ok());
    }

    #[test]
    fn console_url_for_instance_uses_instance_region() {
        assert_eq!(
            console_url_for(&instance_in(Some("us-east-1"))).unwrap(),
            "https://us-east-1.console.aws.amazon.com/ec2/home?region=us-east-1#InstanceDetails:instanceId=i-0123456789abcdef0"
        );
    }

    #[test]
    fn console_url_for_instance_without_region_is_rejected() {
        assert_eq!(
            console_url_for(&instance_in(None)),
            Err(ConsoleUrlError::EmptyRegion)
        );
        assert_eq!(
            console_url_for(&instance_in(Some(""))),
            Err(ConsoleUrlError::EmptyRegion)
        );
    }
}
