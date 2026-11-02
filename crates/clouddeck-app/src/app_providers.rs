use std::sync::Arc;

use clouddeck_core::extension_host::{
    ProviderRegistry, app_registry, extension_state_root, extensions_root,
};
use clouddeck_core::Provider;
use gpui::{App, Global};

pub struct AppProviders(ProviderRegistry);

impl Default for AppProviders {
    fn default() -> Self {
        Self(app_registry(&extensions_root(), &extension_state_root()))
    }
}

impl Global for AppProviders {}

/// Built once at launch; quit covers every `cx.quit()` path, and process signals are not handled.
pub fn install(cx: &mut App) {
    cx.set_global(AppProviders::default());
    cx.on_app_quit(|cx: &mut App| {
        shutdown(cx);
        std::future::ready(())
    })
    .detach();
}

pub fn providers(cx: &mut App) -> Vec<Arc<dyn Provider>> {
    cx.default_global::<AppProviders>().0.providers().to_vec()
}

fn shutdown(cx: &App) {
    if let Some(providers) = cx.try_global::<AppProviders>() {
        providers.0.shutdown();
    }
}
