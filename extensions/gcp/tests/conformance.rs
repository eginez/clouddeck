mod common;

use std::sync::Arc;

use clouddeck_extension_api::conformance::check_provider;
use common::{FakeHttp, provider};

#[test]
fn provider_passes_the_conformance_kit() {
    let violations = check_provider(Arc::new(provider(Arc::new(FakeHttp::default()))));
    assert!(violations.is_empty(), "{violations:#?}");
}
