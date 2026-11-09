//! Author API for CloudDeck provider extensions. Implement [`Extension`] and export it with
//! [`register_extension!`] from a crate built as a `cdylib`.

pub use clouddeck_provider_api::*;

#[doc(hidden)]
pub mod __private;
pub mod abi;
pub mod conformance;
mod table_host;
pub mod wire;

/// Boundary version; a host refuses an extension whose `api_version()` differs.
pub const API_VERSION: u32 = 7;

pub trait Extension: Send + Sync + 'static {
    fn new() -> Self;
    fn provider(&self, id: &str) -> Option<Box<dyn Provider>>;
}

#[macro_export]
macro_rules! register_extension {
    ($extension:ty) => {
        static __CLOUDDECK_EXTENSION: ::std::sync::OnceLock<$extension> =
            ::std::sync::OnceLock::new();

        #[unsafe(no_mangle)]
        pub extern "C" fn clouddeck_api_version() -> u32 {
            $crate::API_VERSION
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn clouddeck_build_info() -> $crate::abi::Slice {
            $crate::abi::Slice::of(if cfg!(panic = "unwind") {
                b"panic=unwind"
            } else {
                b"panic=abort"
            })
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn clouddeck_init(host: *const $crate::abi::HostTable) -> i32 {
            unsafe { $crate::__private::init(host) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn clouddeck_call(
            method: u32,
            request: $crate::abi::Slice,
        ) -> $crate::abi::Buffer {
            unsafe { $crate::__private::call(&__CLOUDDECK_EXTENSION, method, request) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn clouddeck_free(ptr: *mut u8, len: usize) {
            unsafe { $crate::__private::free(ptr, len) }
        }
    };
}
