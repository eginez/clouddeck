//! Google Compute Engine provider extension for CloudDeck.

mod auth;
mod config;
mod fields;
mod http;
mod map;
mod provider;

use std::sync::Arc;

pub use auth::{AdcCreds, TokenSource, load_adc, parse_adc};
pub use http::{Http, HttpError, Request, UreqHttp};
pub use map::{
    ProjectsPage, ZonesPage, console_url, map_connections, map_instance, map_state, region_of,
    zone_dimension,
};
pub use provider::GcpProvider;

use clouddeck_extension_api::{Extension, Provider, register_extension};

pub struct Gcp {
    http: Arc<dyn Http>,
    tokens: Arc<TokenSource>,
}

impl Extension for Gcp {
    fn new() -> Self {
        let http: Arc<dyn Http> = Arc::new(UreqHttp::new());
        let tokens = Arc::new(TokenSource::new(Arc::clone(&http), Box::new(load_adc)));
        Gcp { http, tokens }
    }

    fn provider(&self, id: &str) -> Option<Box<dyn Provider>> {
        (id == "gcp").then(|| {
            let provider = GcpProvider::new(Arc::clone(&self.http), Arc::clone(&self.tokens));
            Box::new(provider) as Box<dyn Provider>
        })
    }
}

register_extension!(Gcp);
