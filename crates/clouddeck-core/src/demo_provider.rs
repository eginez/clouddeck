use std::sync::Arc;

use clouddeck_provider_api::{
    Auth, AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Browse, Call, CellFailure,
    Connection, ConnectionId, DimValue, Dimension, DimensionId, ErrorKind, ExtrasSection, FieldDef,
    FlowId, INSTANCES, InspectorSectionDef, Instance, KindId, KindInfo, ListResult, Provider,
    ProviderError, ProviderInfo, ResourceId, Selection, ValueId,
};

use crate::parse_scenario;
use crate::workspace_provider::DemoFleet;
use clouddeck_provider_kit::{BoxError, ScopeSettings, list_scenario, requested_regions};

const REGION: &str = "region";
const DEMO_METHOD: &str = "demo";
const DEMO_SCENARIO: &str =
    include_str!("../../clouddeck-provider-kit/testdata/ec2-scenarios/small-happy-path.json");

pub struct DemoProvider {
    profiles: Vec<DemoProfile>,
}

struct DemoProfile {
    name: String,
    regions: Vec<(String, Vec<Instance>)>,
}

impl DemoProvider {
    pub fn from_scenario_json(json: &str) -> Result<Self, BoxError> {
        let scenario = parse_scenario(json)?;
        let profiles = scenario
            .profiles
            .iter()
            .map(|profile| DemoProfile {
                name: profile.name.clone(),
                regions: profile
                    .regions
                    .iter()
                    .map(|region| {
                        (
                            region.name.clone(),
                            region
                                .instances
                                .iter()
                                .map(|instance| {
                                    let mut instance = Instance::from(instance);
                                    instance.region = Some(region.name.clone());
                                    instance
                                })
                                .collect(),
                        )
                    })
                    .collect(),
            })
            .collect();
        Ok(Self { profiles })
    }

    fn profile(&self, connection: &ConnectionId) -> Option<&DemoProfile> {
        self.profiles
            .iter()
            .find(|profile| profile.name == connection.as_str())
    }

    fn list_fixture(
        &self,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        if kind.as_str() != INSTANCES {
            return failed(selection, ProviderError::unsupported());
        }
        let Some(profile) = self.profile(connection) else {
            return failed(selection, not_found());
        };

        let mut result = ListResult::default();
        for region in requested_regions(selection) {
            match profile.instances(&region) {
                Some(instances) => result.resources.extend(instances.iter().cloned()),
                None => result.failures.push(CellFailure {
                    cell: region_cell(&region),
                    error: not_found(),
                }),
            }
        }
        result.resources.sort_by(|a, b| a.id.cmp(&b.id));
        result
    }
}

impl DemoProfile {
    fn instances(&self, region: &str) -> Option<&[Instance]> {
        self.regions
            .iter()
            .find(|(name, _)| name == region)
            .map(|(_, instances)| instances.as_slice())
    }
}

pub fn demo_provider() -> DemoProvider {
    DemoProvider::from_scenario_json(DEMO_SCENARIO).expect("bundled demo scenario")
}

pub fn builtin_providers(demo: bool) -> Vec<Arc<dyn Provider>> {
    let mut providers: Vec<Arc<dyn Provider>> = Vec::new();
    if demo {
        providers.push(Arc::new(demo_provider()));
    }
    providers
}

fn not_found() -> ProviderError {
    ProviderError::of(ErrorKind::NotFound)
}

fn region_cell(region: &str) -> Selection {
    Selection::new().with(REGION, vec![ValueId::from(region)])
}

fn failed(selection: &Selection, error: ProviderError) -> ListResult {
    ListResult {
        resources: Vec::new(),
        failures: vec![CellFailure {
            cell: selection.clone(),
            error,
        }],
    }
}

impl Provider for DemoProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "demo".to_string(),
            name: "Demo".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            connection_label: "Profile".to_string(),
            onboarding: Default::default(),
        }
    }

    fn auth(&self) -> Option<&dyn Auth> {
        Some(self)
    }
}

impl Browse for DemoProvider {
    fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        Ok(self
            .profiles
            .iter()
            .map(|profile| Connection {
                id: ConnectionId::from(profile.name.as_str()),
                display_name: profile.name.clone(),
                detail: None,
                tags: Vec::new(),
            })
            .collect())
    }

    fn dimensions(
        &self,
        _cx: &Call<'_>,
        connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        let profile = self.profile(connection).ok_or_else(not_found)?;
        Ok(vec![Dimension {
            id: DimensionId::from(REGION),
            label: "Region".to_string(),
            plural_label: None,
            multi: true,
            optional: false,
            values: profile
                .regions
                .iter()
                .enumerate()
                .map(|(index, (name, _))| DimValue {
                    id: ValueId::from(name.as_str()),
                    label: name.clone(),
                    parent: None,
                    default: index == 0,
                })
                .collect(),
        }])
    }

    fn kinds(&self) -> Vec<KindInfo> {
        vec![KindInfo {
            id: KindId::instances(),
            label: "EC2 Instances".to_string(),
        }]
    }

    fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        let Some(settings) = ScopeSettings::from_host(cx.host) else {
            return self.list_fixture(connection, kind, selection);
        };
        let demo = settings.demo.unwrap_or(DemoFleet::Happy);
        if demo == DemoFleet::Happy && !settings.fleet && !connection.as_str().is_empty() {
            return self.list_fixture(connection, kind, selection);
        }
        list_scenario(demo, connection, kind, selection, settings.fleet)
    }

    fn describe(
        &self,
        _cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<ExtrasSection>, ProviderError> {
        if kind.as_str() != INSTANCES {
            return Err(ProviderError::unsupported());
        }
        let profile = self.profile(connection).ok_or_else(not_found)?;
        profile
            .regions
            .iter()
            .flat_map(|(_, instances)| instances)
            .find(|instance| instance.id == id.as_str())
            .map(|instance| instance.extras.clone())
            .ok_or_else(not_found)
    }

    fn console_url(
        &self,
        _connection: &ConnectionId,
        _kind: &KindId,
        _resource: &Instance,
    ) -> Option<String> {
        None
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        if kind.as_str() == INSTANCES {
            crate::aws_fields()
        } else {
            Vec::new()
        }
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        if kind.as_str() == INSTANCES {
            crate::aws_inspector::aws_inspector()
        } else {
            Vec::new()
        }
    }
}

impl Auth for DemoProvider {
    fn auth_methods(&self) -> Vec<AuthMethod> {
        vec![AuthMethod::new(
            DEMO_METHOD.to_string(),
            "Demo sign-in".to_string(),
        )]
    }

    fn begin_auth(
        &self,
        _cx: &Call<'_>,
        method: &AuthMethod,
        _input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        if method.id == DEMO_METHOD {
            Ok(AuthStep::Done)
        } else {
            Err(ProviderError::unsupported())
        }
    }

    fn continue_auth(
        &self,
        _cx: &Call<'_>,
        _flow: &FlowId,
        _answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        Err(ProviderError::new(
            ErrorKind::Auth,
            "Unknown sign-in flow. Start sign-in again.",
        ))
    }

    fn auth_status(&self, _cx: &Call<'_>, _connection: Option<&ConnectionId>) -> AuthStatus {
        AuthStatus::SignedIn { expires: None }
    }

    fn sign_out(
        &self,
        _cx: &Call<'_>,
        _connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        Ok(())
    }
}
