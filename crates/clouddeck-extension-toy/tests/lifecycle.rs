use std::ffi::c_void;

use clouddeck_extension_api::abi::{
    Buffer, HostTable, INIT_ALREADY, INIT_NULL_HOST, INIT_OK, Slice,
};
use clouddeck_extension_api::wire::Method;
use clouddeck_extension_api::{ErrorKind, ProviderError};
use clouddeck_extension_toy::{clouddeck_call, clouddeck_free, clouddeck_init};

extern "C" fn log(_: *mut c_void, _: u32, _: Slice) {}

extern "C" fn data_dir(_: *mut c_void) -> Buffer {
    Buffer::EMPTY
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

fn call_info() -> Result<serde_json::Value, ProviderError> {
    let body = serde_json::to_vec(&serde_json::json!({
        "provider": "toy",
        "deadline_ms": null,
        "args": null,
    }))
    .unwrap();
    let response = unsafe { clouddeck_call(Method::Info as u32, Slice::of(&body)) };
    let bytes = unsafe { response.bytes() }.to_vec();
    unsafe { clouddeck_free(response.ptr, response.len) };
    serde_json::from_slice(&bytes).unwrap()
}

// Separate test binary: the host is process-global, so these checks need a process no other test initialises.
#[test]
fn calls_before_init_fail_and_init_runs_once() {
    assert_eq!(call_info().unwrap_err().kind, ErrorKind::Other);

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
    assert_eq!(unsafe { clouddeck_init(std::ptr::null()) }, INIT_NULL_HOST);
    assert_eq!(unsafe { clouddeck_init(&table) }, INIT_OK);
    assert_eq!(unsafe { clouddeck_init(&table) }, INIT_ALREADY);
    assert!(call_info().is_ok());
}
