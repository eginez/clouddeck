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
                Err(Box::new(std::io::Error::other(err.to_string()))
                    as clouddeck_core::BoxError)
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
