//! Minimal extension used to exercise the boundary and the conformance kit.

use clouddeck_extension_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStep, Call, Connection, ConnectionId, Dimension,
    ErrorKind, Extension, FieldDef, FlowId, INSTANCES, Instance, KindId, KindInfo, ListResult,
    Provider, ProviderError, ProviderInfo, ResourceId, Selection, register_extension,
};

pub struct Toy;

impl Extension for Toy {
    fn new() -> Self {
        Toy
    }

    fn provider(&self, id: &str) -> Option<Box<dyn Provider>> {
        (id == "toy").then(|| Box::new(ToyProvider) as Box<dyn Provider>)
    }
}

register_extension!(Toy);

pub struct ToyProvider;

fn toy_instances() -> Vec<Instance> {
    ["alpha", "beta"]
        .into_iter()
        .map(|name| Instance {
            id: name.to_string(),
            name: Some(name.to_string()),
            ..Default::default()
        })
        .collect()
}

impl Provider for ToyProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "toy".into(),
            name: "Toy".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            connection_label: "Profile".into(),
        }
    }

    fn connections(&self, _: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        Ok(vec![Connection {
            id: "default".into(),
            display_name: "Default".into(),
        }])
    }

    fn dimensions(&self, _: &Call<'_>, _: &ConnectionId) -> Result<Vec<Dimension>, ProviderError> {
        Ok(Vec::new())
    }

    fn kinds(&self) -> Vec<KindInfo> {
        vec![KindInfo {
            id: KindId::instances(),
            label: "Instances".into(),
        }]
    }

    fn list(&self, _: &Call<'_>, _: &ConnectionId, kind: &KindId, _: &Selection) -> ListResult {
        if kind.as_str() != INSTANCES {
            return ListResult::default();
        }
        ListResult {
            resources: toy_instances(),
            failures: Vec::new(),
        }
    }

    fn describe(
        &self,
        _: &Call<'_>,
        _: &ConnectionId,
        _: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<clouddeck_extension_api::ExtrasSection>, ProviderError> {
        toy_instances()
            .into_iter()
            .find(|i| i.id == id.as_str())
            .map(|i| i.extras)
            .ok_or_else(|| ProviderError::of(ErrorKind::NotFound))
    }

    fn console_url(&self, _: &ConnectionId, _: &KindId, _: &Instance) -> Option<String> {
        None
    }

    fn fields(&self, _: &KindId) -> Vec<FieldDef> {
        Vec::new()
    }

    fn auth_methods(&self) -> Vec<AuthMethod> {
        Vec::new()
    }

    fn begin_auth(
        &self,
        _: &Call<'_>,
        _: &AuthMethod,
        _: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        Err(ProviderError::of(ErrorKind::Unsupported))
    }

    fn continue_auth(
        &self,
        _: &Call<'_>,
        _: &FlowId,
        _: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        Err(ProviderError::of(ErrorKind::Unsupported))
    }
}
