use std::cell::RefCell;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clouddeck_extension_api::abi::{Buffer, HostTable, INIT_OK, Slice};

use super::manifest::Capability;
use super::state::StateStore;
use super::{check_api_version, check_build_info};

type ApiVersionFn = unsafe extern "C" fn() -> u32;
type BuildInfoFn = unsafe extern "C" fn() -> Slice;
type InitFn = unsafe extern "C" fn(*const HostTable) -> i32;
type CallFn = unsafe extern "C" fn(u32, Slice) -> Buffer;
type FreeFn = unsafe extern "C" fn(*mut u8, usize);

const LEVEL_NAMES: [&str; 4] = ["debug", "info", "warn", "error"];

/// Per-extension state the host hands to an extension through [`HostTable`].
pub struct HostContext {
    id: String,
    data_dir: PathBuf,
    declared: Vec<Capability>,
    state: Arc<StateStore>,
}

impl HostContext {
    pub fn new(
        id: String,
        data_dir: PathBuf,
        declared: Vec<Capability>,
        state: Arc<StateStore>,
    ) -> Self {
        Self {
            id,
            data_dir,
            declared,
            state,
        }
    }

    /// A capability is in effect only when the manifest declares it and the user granted it.
    fn allows(&self, capability: &Capability) -> bool {
        self.declared.contains(capability) && self.state.read(|s| s.is_granted(capability))
    }

    fn table(&self) -> HostTable {
        HostTable {
            ctx: ptr::from_ref(self).cast_mut().cast::<c_void>(),
            log,
            data_dir,
            setting,
            secret,
            set_secret,
            open_url,
            cancelled,
            free,
        }
    }
}

thread_local! {
    static CANCEL: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// Runs `body` on this thread with `flag` answering the extension's `cancelled()` queries.
pub fn with_cancel_flag<T>(flag: Arc<AtomicBool>, body: impl FnOnce() -> T) -> T {
    CANCEL.with(|slot| *slot.borrow_mut() = Some(flag));
    let value = body();
    CANCEL.with(|slot| *slot.borrow_mut() = None);
    value
}

/// A loaded extension library. Libraries are never unloaded: the extension keeps pointers into
/// the host table for the life of the process.
pub struct Library {
    _library: libloading::Library,
    _host: Arc<HostContext>,
    call: CallFn,
    free: FreeFn,
}

impl Library {
    pub fn open(path: &Path, host: Arc<HostContext>) -> Result<Self, String> {
        // SAFETY: loading runs the library's initializers; the crash guard in the caller covers that.
        let library = unsafe { libloading::Library::new(path) }
            .map_err(|error| format!("cannot load {}: {error}", path.display()))?;
        match bind(&library, &host) {
            Ok((call, free)) => Ok(Self {
                _library: library,
                _host: host,
                call,
                free,
            }),
            Err(error) => {
                // The library may already have started threads; its code must stay mapped.
                std::mem::forget(library);
                Err(error)
            }
        }
    }

    pub fn call(&self, method: u32, request: &[u8]) -> Vec<u8> {
        let buffer = unsafe { (self.call)(method, Slice::of(request)) };
        if buffer.ptr.is_null() {
            return Vec::new();
        }
        let response = unsafe { buffer.bytes() }.to_vec();
        unsafe { (self.free)(buffer.ptr, buffer.len) };
        response
    }
}

fn bind(library: &libloading::Library, host: &HostContext) -> Result<(CallFn, FreeFn), String> {
    let api_version = unsafe { symbol::<ApiVersionFn>(library, b"clouddeck_api_version")? };
    check_api_version(unsafe { api_version() }).map_err(|error| error.to_string())?;
    let build_info = unsafe { symbol::<BuildInfoFn>(library, b"clouddeck_build_info")? };
    check_build_info(unsafe { build_info().bytes() }).map_err(|error| error.to_string())?;
    let init = unsafe { symbol::<InitFn>(library, b"clouddeck_init")? };
    let table = host.table();
    if unsafe { init(&table) } != INIT_OK {
        return Err("the extension rejected host initialization".into());
    }
    let call = unsafe { symbol::<CallFn>(library, b"clouddeck_call")? };
    let free = unsafe { symbol::<FreeFn>(library, b"clouddeck_free")? };
    Ok((call, free))
}

/// # Safety
/// The caller must use `F` only with the signature the library exports under `name`.
unsafe fn symbol<F: Copy>(library: &libloading::Library, name: &[u8]) -> Result<F, String> {
    unsafe { library.get::<F>(name) }
        .map(|symbol| *symbol)
        .map_err(|error| format!("missing {}: {error}", String::from_utf8_lossy(name)))
}

unsafe fn context<'a>(ctx: *mut c_void) -> &'a HostContext {
    unsafe { &*ctx.cast::<HostContext>() }
}

/// # Safety
/// `slice` must be readable for its length.
unsafe fn text(slice: Slice) -> String {
    String::from_utf8_lossy(unsafe { slice.bytes() }).into_owned()
}

fn write_out(out: *mut Buffer, value: Option<String>) -> bool {
    match value {
        Some(value) if !out.is_null() => {
            unsafe { *out = Buffer::from_vec(value.into_bytes()) };
            true
        }
        _ => false,
    }
}

fn write_error(out: *mut Buffer, message: String) {
    if !out.is_null() {
        unsafe { *out = Buffer::from_vec(message.into_bytes()) };
    }
}

extern "C" fn log(ctx: *mut c_void, level: u32, message: Slice) {
    let host = unsafe { context(ctx) };
    let level = LEVEL_NAMES.get(level as usize).copied().unwrap_or("log");
    eprintln!("[extension {}] {level}: {}", host.id, unsafe {
        text(message)
    });
}

extern "C" fn data_dir(ctx: *mut c_void) -> Buffer {
    let host = unsafe { context(ctx) };
    Buffer::from_vec(host.data_dir.to_string_lossy().into_owned().into_bytes())
}

extern "C" fn setting(ctx: *mut c_void, key: Slice, out: *mut Buffer) -> bool {
    let host = unsafe { context(ctx) };
    let key = unsafe { text(key) };
    write_out(out, host.state.read(|s| s.settings.get(&key).cloned()))
}

/// No secret store is wired yet, so no secret is ever returned.
extern "C" fn secret(_ctx: *mut c_void, _key: Slice, _out: *mut Buffer) -> bool {
    false
}

extern "C" fn set_secret(
    ctx: *mut c_void,
    _key: Slice,
    _value: Slice,
    _has_value: bool,
    error: *mut Buffer,
) -> bool {
    let host = unsafe { context(ctx) };
    let reason = if host.allows(&Capability::Secrets) {
        "secret storage is not available in this build"
    } else {
        "the secrets capability is not granted"
    };
    write_error(error, format!("{}: {reason}", host.id));
    false
}

extern "C" fn open_url(_ctx: *mut c_void, url: Slice, error: *mut Buffer) -> bool {
    let url = unsafe { text(url) };
    match open_in_browser(&url) {
        Ok(()) => true,
        Err(message) => {
            write_error(error, message);
            false
        }
    }
}

fn open_in_browser(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!(
            "refusing to open {url}: only http and https links open"
        ));
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    Command::new(opener)
        .arg(url)
        .status()
        .map(drop)
        .map_err(|error| format!("cannot run {opener}: {error}"))
}

extern "C" fn cancelled(_ctx: *mut c_void) -> bool {
    CANCEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    })
}

extern "C" fn free(ptr: *mut u8, len: usize) {
    unsafe { Buffer { ptr, len }.release() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_needs_both_declaration_and_grant() {
        let dir = tempfile::tempdir().unwrap();
        let state = Arc::new(StateStore::open(&dir.path().join("state.json")).unwrap());
        let host = HostContext::new(
            "toy".into(),
            dir.path().join("data"),
            vec![Capability::Secrets],
            Arc::clone(&state),
        );
        assert!(!host.allows(&Capability::Secrets));
        state.update(|s| s.grant(Capability::Secrets)).unwrap();
        assert!(host.allows(&Capability::Secrets));
        assert!(!host.allows(&Capability::Credentials));

        let undeclared = HostContext::new("toy".into(), dir.path().join("data"), Vec::new(), state);
        assert!(!undeclared.allows(&Capability::Secrets));
    }

    #[test]
    fn only_http_links_open() {
        assert!(open_in_browser("file:///etc/passwd").is_err());
        assert!(open_in_browser("javascript:alert(1)").is_err());
    }
}
