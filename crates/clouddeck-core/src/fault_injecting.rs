use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::models::{ComputeInstanceFilters, ComputeInstanceLoadResult};
use crate::service::{BoxError, ComputeService};

/// What a fault-injecting service does with a call (Swift: `FaultInjectionOutcome`).
///
/// `FaultError` (a clonable boxed error) rather than a concrete error type
/// because callers must be able to inject and downcast arbitrary errors,
/// mirroring the Swift oracle where the outcome carries any `Error`.
#[derive(Debug, Clone)]
pub enum FaultInjectionOutcome {
    /// Forward the call to the wrapped base service unmodified.
    Passthrough,
    /// Reject the call with the configured error without calling the base.
    Failure(FaultError),
}

/// Clonable boxed error (Swift: `Error` references are trivially copyable).
///
/// `BoxError` is not `Clone`, so the configured error lives behind an `Arc` and
/// is re-boxed on every call — matching `MockComputeService`'s approach.
#[derive(Debug, Clone)]
pub struct FaultError(Arc<dyn std::error::Error + Send + Sync>);

impl FaultError {
    pub fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Arc::from(error))
    }

    pub fn boxed(error: BoxError) -> Self {
        Self(Arc::from(error))
    }

    /// The configured error as a boxed error (re-boxed per call).
    pub fn into_box(self) -> BoxError {
        let inner: Box<dyn std::error::Error + Send + Sync> = Box::new(self.0.clone());
        inner
    }

    /// The error's `Display` output.
    pub fn display(&self) -> String {
        self.0.to_string()
    }
}

impl std::fmt::Display for FaultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for FaultError {}

/// Configured fault-injection behavior (Swift: `FaultInjectionBehavior`).
///
/// Defaults to passthrough with no delay, matching the Swift oracle's defaults.
#[derive(Debug, Clone)]
pub struct FaultInjectionBehavior {
    delay: Option<Duration>,
    outcome: FaultInjectionOutcome,
}

impl Default for FaultInjectionBehavior {
    fn default() -> Self {
        Self {
            delay: None,
            outcome: FaultInjectionOutcome::Passthrough,
        }
    }
}

impl FaultInjectionBehavior {
    /// Passthrough with no delay.
    pub fn passthrough() -> Self {
        Self::default()
    }

    /// Set the injected delay applied before the outcome (consumes self).
    pub fn with_delay(mut self, delay: Option<Duration>) -> Self {
        self.delay = delay;
        self
    }

    /// Set the outcome: passthrough or a configured failure (consumes self).
    pub fn with_outcome(mut self, outcome: FaultInjectionOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// The injected delay, if any.
    pub fn delay(&self) -> Option<Duration> {
        self.delay
    }

    /// The configured outcome.
    pub fn outcome(&self) -> &FaultInjectionOutcome {
        &self.outcome
    }
}

/// Wraps any `ComputeService` and injects a configurable delay and/or failure
/// outcome (passthrough or throw) on `list_instances`
/// (Swift: `FaultInjectingComputeService`).
///
/// The delay mechanism is injectable so tests run deterministically without
/// real waiting (Swift: same, via the `Sleep` closure).
pub struct FaultInjectingComputeService {
    base: Box<dyn ComputeService + Send + Sync>,
    behavior: FaultInjectionBehavior,
    sleep: Box<dyn Sleep + Send>,
}

/// Future an injected sleep yields (Swift: the awaited `Sleep` closure).
type SleepFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Injectable sleep mechanism (Swift: `@Sendable (Duration) async throws -> Void`).
///
/// Takes the configured delay and returns a future to await, so tests can
/// record the requested duration and complete immediately.
///
/// `Sync` is required so the default (blocking) sleep can be held across an
/// await while the future is polled on any thread.
pub trait Sleep: Send + Sync {
    fn sleep(&self, duration: Duration) -> SleepFuture<'_>;
}

/// Default sleep: block the current thread for the full duration
/// (Swift: `Task.sleep(for: duration)`).
///
/// Tests should inject a no-op or recording sleep instead of relying on this.
pub struct DefaultSleep;

impl Sleep for DefaultSleep {
    fn sleep(&self, duration: Duration) -> SleepFuture<'_> {
        Box::pin(async move {
            std::thread::sleep(duration);
        })
    }
}

impl FaultInjectingComputeService {
    /// Wrap `base` with passthrough behavior and the default (blocking) sleep
    /// (Swift: same defaults).
    pub fn new(base: impl ComputeService + Send + Sync + 'static) -> Self {
        Self::with_sleep(base, FaultInjectionBehavior::default(), DefaultSleep)
    }

    /// Wrap `base` with the given behavior and the given sleep mechanism.
    pub fn with_sleep(
        base: impl ComputeService + Send + Sync + 'static,
        behavior: FaultInjectionBehavior,
        sleep: impl Sleep + 'static,
    ) -> Self {
        Self {
            base: Box::new(base),
            behavior,
            sleep: Box::new(sleep),
        }
    }

    /// The configured behavior.
    pub fn behavior(&self) -> &FaultInjectionBehavior {
        &self.behavior
    }
}

#[async_trait::async_trait]
impl ComputeService for FaultInjectingComputeService {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        if let Some(delay) = self.behavior.delay() {
            self.sleep.sleep(delay).await;
        }

        match self.behavior.outcome().clone() {
            FaultInjectionOutcome::Passthrough => self.base.list_instances(filters).await,
            FaultInjectionOutcome::Failure(error) => Err(Box::new(error.clone())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CloudDeckError;
    use crate::mock::MockComputeService;
    use crate::models::EC2Instance;
    use std::sync::{Arc, Mutex};

    fn instance(instance_id: &str) -> EC2Instance {
        EC2Instance {
            instance_id: instance_id.to_string(),
            region: None,
            instance_type: None,
            state: Some("running".to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![],
        }
    }

    fn stopped_instance(instance_id: &str) -> EC2Instance {
        EC2Instance {
            state: Some("stopped".to_string()),
            ..instance(instance_id)
        }
    }

    struct NoopSleep;

    impl Sleep for NoopSleep {
        fn sleep(&self, _duration: Duration) -> SleepFuture<'_> {
            Box::pin(async {})
        }
    }

    struct RecordingSleep {
        observed: Arc<Mutex<Vec<Duration>>>,
    }

    impl Sleep for RecordingSleep {
        fn sleep(&self, duration: Duration) -> SleepFuture<'_> {
            let observed = self.observed.clone();
            Box::pin(async move {
                observed.lock().unwrap().push(duration);
            })
        }
    }

    struct SleepingSleep {
        called: Arc<Mutex<bool>>,
    }

    impl Sleep for SleepingSleep {
        fn sleep(&self, _duration: Duration) -> SleepFuture<'_> {
            let called = self.called.clone();
            Box::pin(async move {
                *called.lock().unwrap() = true;
            })
        }
    }

    #[tokio::test]
    async fn passthrough_forwards_call_and_result_unmodified() {
        let base = MockComputeService::new(vec![instance("i-1"), instance("i-2")], None);
        let service = FaultInjectingComputeService::with_sleep(
            base,
            FaultInjectionBehavior::default(),
            NoopSleep,
        );

        let result = service.list_instances(None).await.unwrap();
        let ids: Vec<&str> = result
            .instances
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect();
        assert_eq!(ids, vec!["i-1", "i-2"]);
        assert!(result.partial_failures.is_empty());
    }

    #[tokio::test]
    async fn passthrough_forwards_filters_to_base() {
        let base = MockComputeService::new(vec![instance("i-a"), stopped_instance("i-b")], None);
        let service = FaultInjectingComputeService::with_sleep(
            base,
            FaultInjectionBehavior::default(),
            NoopSleep,
        );

        let filters = ComputeInstanceFilters {
            name_contains: None,
            states: vec!["running".to_string()],
            instance_types: vec![],
        };
        let result = service.list_instances(Some(filters)).await.unwrap();
        let ids: Vec<&str> = result
            .instances
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect();
        assert_eq!(ids, vec!["i-a"]);
    }

    #[tokio::test]
    async fn failure_returns_configured_error_without_calling_base() {
        let error = CloudDeckError::Unknown("injected failure".to_string());
        // The base would succeed with an instance — any success here would
        // prove the wrapper forwarded the call to it.
        let base = MockComputeService::new(vec![instance("i-1")], None);
        let service = FaultInjectingComputeService::with_sleep(
            base,
            FaultInjectionBehavior::default()
                .with_outcome(FaultInjectionOutcome::Failure(FaultError::new(error))),
            NoopSleep,
        );

        let result = service.list_instances(None).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), "injected failure");
    }

    #[tokio::test]
    async fn delay_is_applied_before_failure_outcome_via_injected_sleep() {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let behavior = FaultInjectionBehavior::default()
            .with_delay(Some(Duration::from_millis(250)))
            .with_outcome(FaultInjectionOutcome::Failure(FaultError::new(
                CloudDeckError::Unknown("slow failure".to_string()),
            )));

        let service = FaultInjectingComputeService::with_sleep(
            MockComputeService::new(vec![instance("i-1")], None),
            behavior,
            RecordingSleep {
                observed: observed.clone(),
            },
        );

        let result = service.list_instances(None).await;
        assert!(result.is_err());
        let delays = observed.lock().unwrap().clone();
        assert_eq!(delays, vec![Duration::from_millis(250)]);
    }

    #[tokio::test]
    async fn delay_is_applied_before_base_call_on_passthrough() {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let behavior =
            FaultInjectionBehavior::default().with_delay(Some(Duration::from_millis(100)));

        let service = FaultInjectingComputeService::with_sleep(
            MockComputeService::new(vec![instance("i-1")], None),
            behavior,
            RecordingSleep {
                observed: observed.clone(),
            },
        );

        let result = service.list_instances(None).await.unwrap();
        assert_eq!(result.instances.len(), 1);
        let delays = observed.lock().unwrap().clone();
        assert_eq!(delays, vec![Duration::from_millis(100)]);
    }

    #[tokio::test]
    async fn no_delay_skips_sleep_entirely() {
        let called = Arc::new(Mutex::new(false));
        let service = FaultInjectingComputeService::with_sleep(
            MockComputeService::new(vec![instance("i-1")], None),
            FaultInjectionBehavior::default(),
            SleepingSleep {
                called: called.clone(),
            },
        );

        let result = service.list_instances(None).await.unwrap();
        assert_eq!(result.instances.len(), 1);
        assert!(!*called.lock().unwrap());
    }

    #[test]
    fn default_behavior_is_passthrough_with_no_delay() {
        let behavior = FaultInjectionBehavior::default();
        assert!(matches!(
            behavior.outcome(),
            FaultInjectionOutcome::Passthrough
        ));
        assert_eq!(behavior.delay(), None);
    }
}
