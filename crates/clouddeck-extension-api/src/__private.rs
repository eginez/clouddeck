//! Entry points behind `register_extension!`; not part of the author API.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use clouddeck_provider_api::ErrorKind;

use crate::Extension;
use crate::abi::{Buffer, HostTable, INIT_ALREADY, INIT_NULL_HOST, INIT_OK, Slice};
use crate::table_host::TableHost;
use crate::wire::{dispatch, error_response};

static HOST: OnceLock<TableHost> = OnceLock::new();

/// # Safety
/// `host` must be null or point to a table that the host keeps valid for the process lifetime.
pub unsafe fn init(host: *const HostTable) -> i32 {
    if host.is_null() {
        return INIT_NULL_HOST;
    }
    let table = unsafe { *host };
    match HOST.set(TableHost(table)) {
        Ok(()) => INIT_OK,
        Err(_) => INIT_ALREADY,
    }
}

/// # Safety
/// `request` must be readable for its length.
pub unsafe fn call<E: Extension>(slot: &OnceLock<E>, method: u32, request: Slice) -> Buffer {
    let response = catch_unwind(AssertUnwindSafe(|| match HOST.get() {
        Some(host) => {
            let bytes = unsafe { request.bytes() };
            dispatch(slot.get_or_init(E::new), host, method, bytes)
        }
        None => error_response(ErrorKind::Other, "extension used before init"),
    }))
    .unwrap_or_else(|_| error_response(ErrorKind::Other, "extension panicked"));
    Buffer::from_vec(response)
}

/// # Safety
/// `ptr` and `len` must describe a buffer returned by this library's `call`.
pub unsafe fn free(ptr: *mut u8, len: usize) {
    unsafe { Buffer { ptr, len }.release() }
}
