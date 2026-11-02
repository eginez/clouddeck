//! Fleet fixtures and fixed providers for tests in the dependent crates.

use std::sync::{Arc, Condvar, Mutex};

use clouddeck_provider_api::{
    Call, Connection, ConnectionId, Dimension, FieldDef, InspectorSectionDef, KindId, KindInfo,
    ListResult, Provider, ProviderError, ProviderInfo, Selection,
};

use crate::models::{EC2InstanceTag, EC2SecurityGroup};
use crate::scenario::DirectoryScenarioCatalog;
use crate::service_provider::{
    connections, console_url, fields, info, inspector, kinds, service_provider, unsupported_kind,
    whole_load_failure,
};
use crate::{CloudDeckError, EC2Instance, Instance, ScenarioService, TestScenarioKind};

/// Blocks a fixture provider's load until [`Gate::open`] is called.
#[derive(Default)]
pub struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn open(&self) {
        *self.open.lock().expect("gate lock") = true;
        self.changed.notify_all();
    }

    fn wait(&self) {
        let mut open = self.open.lock().expect("gate lock");
        while !*open {
            open = self.changed.wait(open).expect("gate lock");
        }
    }
}

/// An instance in us-east-1 with a `Name` tag.
pub fn instance(instance_id: &str, name: &str, state: &str, instance_type: &str) -> Instance {
    Instance::from(&EC2Instance {
        state: Some(state.to_string()),
        instance_type: Some(instance_type.to_string()),
        tags: vec![tag("Name", name)],
        ..blank(instance_id)
    })
}

/// An instance like [`instance`] with an optional `Owner` tag.
pub fn owned_instance(instance_id: &str, name: &str, state: &str, owner: Option<&str>) -> Instance {
    let mut tags = vec![tag("Name", name)];
    tags.extend(owner.map(|owner| tag("Owner", owner)));
    Instance::from(&EC2Instance {
        state: Some(state.to_string()),
        instance_type: Some("t3.medium".to_string()),
        tags,
        ..blank(instance_id)
    })
}

/// Instances for `(instance id, Name tag, state, instance type)` rows.
pub fn instances(rows: &[(&str, &str, &str, &str)]) -> Vec<Instance> {
    rows.iter()
        .map(|(id, name, state, instance_type)| instance(id, name, state, instance_type))
        .collect()
}

/// The inspector fixture: one richly tagged running instance and one untagged stopped instance.
pub fn inspector_instances() -> Vec<Instance> {
    vec![
        Instance::from(&running_with_ten_tags()),
        Instance::from(&EC2Instance {
            state: Some("stopped".to_string()),
            instance_type: Some("m6i.large".to_string()),
            ..blank("i-0d2fc5662eb309e50")
        }),
    ]
}

/// 120 instances cloned from the first small-happy-path instance.
pub fn large_fleet() -> Vec<Instance> {
    let scenario = scenario_service(TestScenarioKind::SmallHappyPath);
    let scenario = scenario.scenario().expect("fixture-backed scenario");
    let (profile, region) = scenario
        .default_selection()
        .expect("fixture has a default selection");
    let template = scenario
        .instances(Some(&profile), &region)
        .expect("fixture instances should load")
        .into_iter()
        .next()
        .expect("fixture has at least one instance");
    (0..120)
        .map(|n| {
            let mut record = template.clone();
            record.instance_id = format!("i-large-{n:03}");
            record.state = Some(if n % 2 == 0 { "running" } else { "stopped" }.to_string());
            record.tags.retain(|tag| tag.key != "Name");
            record
                .tags
                .push(tag("Name", &format!("large-instance-{n:03}")));
            Instance::from(&record)
        })
        .collect()
}

/// A provider backed by the bundled scenario fixture for `kind`.
pub fn scenario_provider(kind: TestScenarioKind) -> Arc<dyn Provider> {
    service_provider(Box::new(scenario_service(kind).into_service()))
}

fn scenario_service(kind: TestScenarioKind) -> ScenarioService {
    let catalog = DirectoryScenarioCatalog::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/ec2-scenarios"
    ));
    ScenarioService::for_kind(kind, &catalog).expect("fixture scenario should load")
}

/// A provider that lists `instances` for every selection.
pub fn static_provider(instances: Vec<Instance>) -> Arc<dyn Provider> {
    Arc::new(StaticProvider {
        instances,
        failure: None,
        gate: None,
    })
}

/// A provider whose every load fails with `message`.
pub fn failing_provider(message: &str) -> Arc<dyn Provider> {
    Arc::new(StaticProvider {
        instances: Vec::new(),
        failure: Some(message.to_string()),
        gate: None,
    })
}

/// A provider that waits on `gate` before answering with `instances` or `failure`.
pub fn gated_provider(
    instances: Vec<Instance>,
    failure: Option<String>,
    gate: Arc<Gate>,
) -> Arc<dyn Provider> {
    Arc::new(StaticProvider {
        instances,
        failure,
        gate: Some(gate),
    })
}

struct StaticProvider {
    instances: Vec<Instance>,
    failure: Option<String>,
    gate: Option<Arc<Gate>>,
}

impl Provider for StaticProvider {
    fn info(&self) -> ProviderInfo {
        info()
    }

    fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        Ok(connections())
    }

    fn dimensions(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        Ok(Vec::new())
    }

    fn kinds(&self) -> Vec<KindInfo> {
        kinds()
    }

    fn list(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        if let Some(unsupported) = unsupported_kind(kind, selection) {
            return unsupported;
        }
        if let Some(gate) = &self.gate {
            gate.wait();
        }
        match &self.failure {
            Some(message) => whole_load_failure(
                &CloudDeckError::from_service_error(&std::io::Error::other(message.clone())),
                selection,
            ),
            None => ListResult {
                resources: self.instances.clone(),
                failures: Vec::new(),
            },
        }
    }

    fn console_url(
        &self,
        _connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String> {
        console_url(kind, resource)
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        fields(kind)
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        inspector(kind)
    }
}

fn blank(instance_id: &str) -> EC2Instance {
    EC2Instance {
        instance_id: instance_id.to_string(),
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

fn running_with_ten_tags() -> EC2Instance {
    let mut tags = vec![tag("Name", "api-gateway-01")];
    tags.extend((1..=9).map(|n| tag(&format!("k{n}"), &n.to_string())));
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
            security_group("web-sg", "sg-0d2fc5662"),
            security_group("monitoring-sg", "sg-074bb5deb"),
        ],
        tags,
        ..blank("i-0123456789abcdef0")
    }
}

fn tag(key: &str, value: &str) -> EC2InstanceTag {
    EC2InstanceTag {
        key: key.to_string(),
        value: value.to_string(),
    }
}

fn security_group(group_name: &str, group_id: &str) -> EC2SecurityGroup {
    EC2SecurityGroup {
        group_name: Some(group_name.to_string()),
        group_id: Some(group_id.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service_provider::LoadHost;
    use clouddeck_provider_api::{ErrorKind, KindId};

    fn list(provider: &dyn Provider, selection: &Selection) -> ListResult {
        let host = LoadHost;
        let cx = Call {
            deadline: None,
            host: &host,
        };
        provider.list(&cx, &ConnectionId::from("default"), &KindId::instances(), selection)
    }

    #[test]
    fn static_provider_lists_all_instances() {
        let provider = static_provider(instances(&[
            ("i-1", "a", "running", "t3.micro"),
            ("i-2", "b", "stopped", "t3.micro"),
        ]));

        let result = list(provider.as_ref(), &Selection::new());

        assert_eq!(result.resources.len(), 2);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn failing_provider_returns_whole_load_failure() {
        let provider = failing_provider("boom");
        let selection = Selection::new();

        let result = list(provider.as_ref(), &selection);

        assert!(result.resources.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].cell, selection);
        assert!(result.failures[0].error.message.contains("boom"));
        assert_eq!(result.failures[0].error.kind, ErrorKind::Other);
    }
}
