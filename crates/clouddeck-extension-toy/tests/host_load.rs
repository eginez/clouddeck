mod common;

use clouddeck_core::extension_host::{ProviderRegistry, StateStore, state_path};
use clouddeck_core::{Call, ConnectionId};
use common::{NoHost, install};

// One test: the extension's host table is process-wide, so the library can be initialised once.
#[test]
fn installed_extension_loads_on_first_use_and_serves_calls() {
    let root = tempfile::tempdir().unwrap();
    let state_root = tempfile::tempdir().unwrap();
    let state_file = state_path(state_root.path(), "toy");
    install(root.path());
    StateStore::open(&state_file)
        .unwrap()
        .update(|state| state.enabled = true)
        .unwrap();

    let mut registry = ProviderRegistry::default().with_external(true);
    registry.discover(root.path(), state_root.path()).unwrap();
    assert!(registry.refusals().is_empty(), "{:?}", registry.refusals());
    let provider = &registry.providers()[0];

    let store = StateStore::open(&state_file).unwrap();
    assert!(
        !store.read(|state| state.launch_pending),
        "discovery must not load"
    );

    let host = NoHost;
    let cx = Call {
        deadline: None,
        host: &host,
    };
    let connections = provider.connections(&cx).unwrap();
    assert_eq!(connections[0].id, ConnectionId::from("default"));
    assert_eq!(provider.info().id, "toy");

    let store = StateStore::open(&state_file).unwrap();
    assert!(store.read(|state| state.launch_pending));
    assert_eq!(store.read(|state| state.crashes), 0);

    registry.shutdown();
    let store = StateStore::open(&state_file).unwrap();
    assert!(!store.read(|state| state.launch_pending));
    assert_eq!(store.read(|state| state.crashes), 0);
}
