use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use clouddeck_core::Provider;
use clouddeck_core::extension_host::{
    ProviderRegistry, Refusal, app_registry, bundled_extensions_root, extension_state_root,
    extensions_root,
};
use gpui::{App, Global};

pub struct AppProviders {
    registry: ProviderRegistry,
    #[cfg(test)]
    shutdowns: AtomicUsize,
}

impl AppProviders {
    pub(crate) fn new(registry: ProviderRegistry) -> Self {
        Self {
            registry,
            #[cfg(test)]
            shutdowns: AtomicUsize::new(0),
        }
    }

    fn shutdown(&self) {
        self.registry.shutdown();
        #[cfg(test)]
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) fn shutdowns(&self) -> usize {
        self.shutdowns.load(Ordering::SeqCst)
    }
}

impl Default for AppProviders {
    fn default() -> Self {
        Self::new(app_registry(
            bundled_extensions_root().as_deref(),
            &extensions_root(),
            &extension_state_root(),
            Vec::new(),
        ))
    }
}

impl Global for AppProviders {}

/// Built once at launch; quit covers every `cx.quit()` path, and process signals are not handled.
pub fn install(cx: &mut App) {
    cx.set_global(AppProviders::default());
    observe_quit(cx);
}

pub(crate) fn observe_quit(cx: &mut App) {
    cx.on_app_quit(|cx: &mut App| {
        shutdown(cx);
        std::future::ready(())
    })
    .detach();
}

pub(crate) fn refusals(cx: &App) -> Vec<Refusal> {
    cx.try_global::<AppProviders>()
        .map(|providers| providers.registry.refusals())
        .unwrap_or_default()
}

pub fn providers(cx: &mut App) -> Vec<Arc<dyn Provider>> {
    cx.default_global::<AppProviders>()
        .registry
        .providers()
        .to_vec()
}

/// Safe to call more than once; the window-close path on Linux calls it because no quit runs.
pub(crate) fn shutdown(cx: &App) {
    if let Some(providers) = cx.try_global::<AppProviders>() {
        providers.shutdown();
    }
}
