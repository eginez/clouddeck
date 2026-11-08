use std::ffi::c_void;
use std::ptr;
use std::slice;

pub const INIT_OK: i32 = 0;
pub const INIT_NULL_HOST: i32 = 1;
pub const INIT_ALREADY: i32 = 2;

/// A borrowed byte range; the receiver copies anything it keeps.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Slice {
    pub ptr: *const u8,
    pub len: usize,
}

impl Slice {
    pub fn of(bytes: &[u8]) -> Self {
        Self {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        }
    }

    /// # Safety
    /// `ptr` must be readable for `len` bytes for the chosen lifetime.
    pub unsafe fn bytes<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            return &[];
        }
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

/// An owned byte buffer; whichever library allocated it must release it.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Buffer {
    pub ptr: *mut u8,
    pub len: usize,
}

impl Buffer {
    pub const EMPTY: Self = Self {
        ptr: ptr::null_mut(),
        len: 0,
    };

    pub fn from_vec(bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        let ptr = Box::into_raw(bytes.into_boxed_slice()).cast::<u8>();
        Self { ptr, len }
    }

    /// # Safety
    /// The buffer must have been produced by `from_vec` in this library and not released yet.
    pub unsafe fn release(self) {
        if self.ptr.is_null() {
            return;
        }
        drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(self.ptr, self.len)) });
    }

    /// # Safety
    /// As for [`Slice::bytes`].
    pub unsafe fn bytes<'a>(self) -> &'a [u8] {
        unsafe {
            Slice {
                ptr: self.ptr,
                len: self.len,
            }
            .bytes()
        }
    }
}

/// The host's callbacks, passed to `clouddeck_init`. Buffers the host returns are host-allocated
/// and released through `free`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HostTable {
    pub ctx: *mut c_void,
    /// `level` is 0 debug, 1 info, 2 warn, 3 error.
    pub log: extern "C" fn(ctx: *mut c_void, level: u32, message: Slice),
    pub data_dir: extern "C" fn(ctx: *mut c_void) -> Buffer,
    /// On success writes a host-allocated value into `out` and returns true.
    pub setting: extern "C" fn(ctx: *mut c_void, key: Slice, out: *mut Buffer) -> bool,
    pub secret: extern "C" fn(ctx: *mut c_void, key: Slice, out: *mut Buffer) -> bool,
    /// `has_value` false deletes the secret. On failure writes a message into `error`.
    pub set_secret: extern "C" fn(
        ctx: *mut c_void,
        key: Slice,
        value: Slice,
        has_value: bool,
        error: *mut Buffer,
    ) -> bool,
    /// On failure writes a message into `error`.
    pub open_url: extern "C" fn(ctx: *mut c_void, url: Slice, error: *mut Buffer) -> bool,
    pub cancelled: extern "C" fn(ctx: *mut c_void) -> bool,
    pub free: extern "C" fn(ptr: *mut u8, len: usize),
}
