use std::ffi::c_void;
use std::sync::Once;

use clouddeck_extension_api::abi::{Buffer, HostTable, INIT_OK, Slice};
use clouddeck_extension_api::wire::Method;
use clouddeck_extension_api::{
    API_VERSION, Connection, ConnectionId, ErrorKind, KindId, ListResult, ProviderError, Selection,
};
use clouddeck_extension_toy::{
    clouddeck_api_version, clouddeck_build_info, clouddeck_call, clouddeck_free, clouddeck_init,
};

extern "C" fn log(_: *mut c_void, _: u32, _: Slice) {}

extern "C" fn data_dir(_: *mut c_void) -> Buffer {
    Buffer::from_vec(b"/tmp/toy".to_vec())
}

extern "C" fn lookup(_: *mut c_void, _: Slice, _: *mut Buffer) -> bool {
    false
}

extern "C" fn set_secret(_: *mut c_void, _: Slice, _: Slice, _: bool, _: *mut Buffer) -> bool {
    true
}

extern "C" fn open_url(_: *mut c_void, _: Slice, _: *mut Buffer) -> bool {
    true
}

extern "C" fn cancelled(_: *mut c_void) -> bool {
    false
}

extern "C" fn free(ptr: *mut u8, len: usize) {
    unsafe { Buffer { ptr, len }.release() }
}

fn ensure_init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let table = HostTable {
            ctx: std::ptr::null_mut(),
            log,
            data_dir,
            setting: lookup,
            secret: lookup,
            set_secret,
            open_url,
            cancelled,
            free,
        };
        assert_eq!(unsafe { clouddeck_init(&table) }, INIT_OK);
    });
}

fn call(method: Method, request: &[u8]) -> Vec<u8> {
    raw_call(method as u32, request)
}

fn raw_call(code: u32, request: &[u8]) -> Vec<u8> {
    ensure_init();
    let response = unsafe { clouddeck_call(code, Slice::of(request)) };
    let bytes = unsafe { response.bytes() }.to_vec();
    unsafe { clouddeck_free(response.ptr, response.len) };
    bytes
}

fn request(args: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "provider": "toy",
        "deadline_ms": null,
        "args": args,
    }))
    .unwrap()
}

#[test]
fn boundary_round_trips_through_exported_symbols() {
    assert_eq!(clouddeck_api_version(), API_VERSION);

    let info = unsafe { clouddeck_build_info().bytes() };
    assert!(
        info == b"panic=unwind" || info == b"panic=abort",
        "{}",
        String::from_utf8_lossy(info)
    );

    let list = request(serde_json::json!([
        serde_json::to_value(ConnectionId::from("default")).unwrap(),
        serde_json::to_value(KindId::instances()).unwrap(),
        serde_json::to_value(Selection::new()).unwrap(),
    ]));
    let listed: Result<ListResult, ProviderError> =
        serde_json::from_slice(&call(Method::List, &list)).unwrap();
    assert_eq!(listed.unwrap().resources.len(), 2);

    let connections = request(serde_json::json!(null));
    let connections: Result<Vec<Connection>, ProviderError> =
        serde_json::from_slice(&call(Method::Connections, &connections)).unwrap();
    assert_eq!(connections.unwrap()[0].id.as_str(), "default");
}

#[test]
fn unknown_method_and_malformed_request_return_errors() {
    let unknown: Result<serde_json::Value, ProviderError> =
        serde_json::from_slice(&raw_call(9999, b"{}")).unwrap();
    assert_eq!(unknown.unwrap_err().kind, ErrorKind::Unsupported);

    let malformed: Result<serde_json::Value, ProviderError> =
        serde_json::from_slice(&call(Method::List, b"not json")).unwrap();
    assert_eq!(malformed.unwrap_err().kind, ErrorKind::Other);
}

#[test]
fn unknown_provider_id_is_not_found() {
    let body = serde_json::to_vec(&serde_json::json!({
        "provider": "missing",
        "deadline_ms": null,
        "args": null,
    }))
    .unwrap();
    let outcome: Result<serde_json::Value, ProviderError> =
        serde_json::from_slice(&call(Method::Info, &body)).unwrap();
    assert_eq!(outcome.unwrap_err().kind, ErrorKind::NotFound);
}
