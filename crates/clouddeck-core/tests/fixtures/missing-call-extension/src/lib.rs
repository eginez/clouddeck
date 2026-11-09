#[repr(C)]
pub struct Slice {
    ptr: *const u8,
    len: usize,
}

#[unsafe(no_mangle)]
pub extern "C" fn clouddeck_api_version() -> u32 {
    7
}

#[unsafe(no_mangle)]
pub extern "C" fn clouddeck_build_info() -> Slice {
    let info: &'static [u8] = b"panic=unwind";
    Slice {
        ptr: info.as_ptr(),
        len: info.len(),
    }
}

// Deliberately not INIT_OK: if the host initialises before checking for `clouddeck_call`,
// the load fails with "rejected host initialization" instead of naming the missing symbol.
#[unsafe(no_mangle)]
pub extern "C" fn clouddeck_init(_host: *const u8) -> i32 {
    2
}
