//! Shared test doubles for the `#[cfg(test)]` modules in this crate.
//!
//! A double defined once here is imported by the test modules instead of
//! being copy-pasted into each of them, so a change to a double is a
//! single edit. Nothing outside tests imports this module (it is declared
//! under `#[cfg(test)]` in `main.rs`).

use clouddeck_core::{
    ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeService, EC2Instance,
};

/// A [`ComputeService`] whose `list_instances` call blocks on an
/// externally-held [`tokio::sync::Notify`] until the test releases it.
///
/// A fixture-backed service like `MockComputeService` resolves too fast to
/// reliably observe the `is_loading` window between a load being kicked off
/// and its result landing (and `AppShell::build_with_scenario`'s startup
/// load can't be caught mid-flight against it at all). This double lets a
/// test deterministically observe both halves of that transition on the
/// `cx.spawn`-bridged tokio path `EC2Tab`/`InstanceListView::refresh` use,
/// without racing real wall-clock scheduling.
pub struct GatedComputeService {
    pub gate: std::sync::Arc<tokio::sync::Notify>,
    pub instances: Vec<EC2Instance>,
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
        Box::pin(async move {
            gate.notified().await;
            Ok(ComputeInstanceLoadResult {
                instances,
                partial_failures: vec![],
            })
        })
    }
}
