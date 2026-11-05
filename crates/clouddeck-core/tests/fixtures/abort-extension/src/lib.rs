#[repr(C)]
pub struct Slice {
    ptr: *const u8,
    len: usize,
}

#[unsafe(no_mangle)]
pub extern "C" fn clouddeck_api_version() -> u32 {
    3
}

#[unsafe(no_mangle)]
pub extern "C" fn clouddeck_build_info() -> Slice {
    let info: &'static [u8] = if cfg!(panic = "unwind") {
        b"panic=unwind"
    } else {
        b"panic=abort"
    };
    Slice {
        ptr: info.as_ptr(),
        len: info.len(),
    }
}
