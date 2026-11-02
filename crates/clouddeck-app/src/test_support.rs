//! Shared test doubles for the `#[cfg(test)]` modules in this crate.
//!
//! A double defined once here is imported by the test modules instead of
//! being copy-pasted into each of them, so a change to a double is a
//! single edit. Nothing outside tests imports this module (it is declared
//! under `#[cfg(test)]` in `main.rs`).

use clouddeck_core::{
    ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeService, EC2Instance, EC2InstanceTag,
    EC2SecurityGroup,
};

/// A provider record in us-east-1 with a `Name` tag, for fleets fed to a fake compute service.
pub fn ec2_record(instance_id: &str, name: &str, state: &str, instance_type: &str) -> EC2Instance {
    EC2Instance {
        instance_id: instance_id.to_string(),
        region: Some("us-east-1".to_string()),
        instance_type: Some(instance_type.to_string()),
        state: Some(state.to_string()),
        launch_time: None,
        public_ip_address: None,
        private_ip_address: None,
        vpc_id: None,
        subnet_id: None,
        security_groups: vec![],
        tags: vec![ec2_tag("Name", name)],
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

/// Provider records as a fake compute service returns them.
pub type ProviderRecords = Vec<EC2Instance>;

/// A record like [`ec2_record`] with an optional `Owner` tag.
pub fn ec2_owned(instance_id: &str, name: &str, state: &str, owner: Option<&str>) -> EC2Instance {
    let mut record = ec2_record(instance_id, name, state, "t3.medium");
    if let Some(owner) = owner {
        record.tags.push(ec2_tag("Owner", owner));
    }
    record
}

/// Provider records for `(instance id, Name tag, state, instance type)` rows.
pub fn ec2_fleet(rows: &[(&str, &str, &str, &str)]) -> Vec<EC2Instance> {
    rows.iter()
        .map(|(id, name, state, instance_type)| ec2_record(id, name, state, instance_type))
        .collect()
}

/// The inspector fixture: one richly tagged running record and one untagged stopped record.
pub fn ec2_inspector_fixture() -> Vec<EC2Instance> {
    vec![ec2_running_with_ten_tags(), ec2_untagged_stopped()]
}

fn ec2_blank(id: &str) -> EC2Instance {
    EC2Instance {
        instance_id: id.to_string(),
        region: Some("us-east-1".to_string()),
        instance_type: None,
        state: None,
        launch_time: None,
        public_ip_address: None,
        private_ip_address: None,
        vpc_id: None,
        subnet_id: None,
        security_groups: vec![],
        tags: vec![],
        iam_instance_profile_arn: None,
        private_dns_name: None,
        public_dns_name: None,
        availability_zone: None,
        image_id: None,
        key_name: None,
        architecture: None,
        state_transition_reason: None,
    }
}

fn ec2_running_with_ten_tags() -> EC2Instance {
    let mut tags = vec![ec2_tag("Name", "api-gateway-01")];
    tags.extend((1..=9).map(|n| ec2_tag(&format!("k{n}"), &n.to_string())));
    EC2Instance {
        state: Some("running".to_string()),
        instance_type: Some("t3.medium".to_string()),
        availability_zone: Some("us-east-1a".to_string()),
        private_ip_address: Some("10.0.4.3".to_string()),
        private_dns_name: Some("ip-10-0-4-3.ec2.internal".to_string()),
        vpc_id: Some("vpc-098f0fdb8".to_string()),
        subnet_id: Some("subnet-0d99b9061".to_string()),
        image_id: Some("ami-027d083c0".to_string()),
        key_name: Some("ops-ed25519".to_string()),
        security_groups: vec![
            ec2_security_group("web-sg", "sg-0d2fc5662"),
            ec2_security_group("monitoring-sg", "sg-074bb5deb"),
        ],
        tags,
        ..ec2_blank("i-0123456789abcdef0")
    }
}

fn ec2_untagged_stopped() -> EC2Instance {
    EC2Instance {
        state: Some("stopped".to_string()),
        instance_type: Some("m6i.large".to_string()),
        ..ec2_blank("i-0d2fc5662eb309e50")
    }
}

pub fn ec2_tag(key: &str, value: &str) -> EC2InstanceTag {
    EC2InstanceTag {
        key: key.to_string(),
        value: value.to_string(),
    }
}

pub fn ec2_security_group(group_name: &str, group_id: &str) -> EC2SecurityGroup {
    EC2SecurityGroup {
        group_name: Some(group_name.to_string()),
        group_id: Some(group_id.to_string()),
    }
}

/// A [`ComputeService`] whose `list_instances` call blocks on an
/// externally-held [`tokio::sync::Notify`] until the test releases it.
///
/// A fixture-backed service like `MockComputeService` resolves too fast to
/// reliably observe the `is_loading` window between a load being kicked off
/// and its result landing (and `AppShell::build_with_scenario`'s startup
/// load can't be caught mid-flight against it at all). This double lets a
/// test deterministically observe both halves of that transition on the
/// `cx.spawn`-bridged tokio path the fleet refresh uses, without racing real
/// wall-clock scheduling.
pub struct GatedComputeService {
    pub gate: std::sync::Arc<tokio::sync::Notify>,
    pub instances: Vec<EC2Instance>,
    /// If `Some`, the service resolves to this error once the gate is
    /// released (instead of returning the instances). Used to deterministically
    /// test the error state after a picker-driven context switch.
    ///
    /// Uses `Arc<dyn Error + Send + Sync>` (which is `Clone`) rather than
    /// `Box<dyn Error>` because `list_instances` takes `&self` and needs to
    /// hand a clone of the error to the spawned future.
    pub error: Option<std::sync::Arc<dyn std::error::Error + Send + Sync>>,
}

impl ComputeService for GatedComputeService {
    fn list_instances<'life0, 'async_trait>(
        &'life0 self,
        _filters: Option<ComputeInstanceFilters>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<ComputeInstanceLoadResult, clouddeck_core::BoxError>,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let gate = self.gate.clone();
        let instances = self.instances.clone();
        let error = self.error.clone();
        Box::pin(async move {
            gate.notified().await;
            if let Some(err) = error {
                // Convert the `Arc<dyn Error>` into a `Box<dyn Error>` by
                // wrapping a new `io::Error` with the original's display
                // text (the view model only uses the error's `Display`
                // output via `CloudDeckError::from_service_error`).
                Err(Box::new(std::io::Error::other(err.to_string())) as clouddeck_core::BoxError)
            } else {
                Ok(ComputeInstanceLoadResult {
                    instances,
                    partial_failures: vec![],
                })
            }
        })
    }
}

/// Wraps an `Arc<dyn ComputeService>` behind a `Box<dyn ComputeService>` so
/// that a test can inject the same underlying service into multiple view
/// models (one per tab) created by `AppShell::apply_context`.
///
/// `AppShell`'s `apply_context` builds a new `InstanceListViewModel` each
/// time it is called, which needs a fresh `Box<dyn ComputeService>`. The
/// `Arc<dyn>` lets the test keep holding the gate after the box is handed
/// to the view model.
pub struct ArcComputeService {
    inner: std::sync::Arc<dyn ComputeService + Send + Sync>,
}

impl ArcComputeService {
    pub fn new(inner: std::sync::Arc<dyn ComputeService + Send + Sync>) -> Self {
        Self { inner }
    }
}

impl ComputeService for ArcComputeService {
    fn list_instances<'life0, 'async_trait>(
        &'life0 self,
        filters: Option<ComputeInstanceFilters>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<ComputeInstanceLoadResult, clouddeck_core::BoxError>,
                > + Send
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        // Delegate to the inner service. `list_instances` takes `&self`,
        // so the `Arc` is already dereferenced.
        self.inner.list_instances(filters)
    }
}
