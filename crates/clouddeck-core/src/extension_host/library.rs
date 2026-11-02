use std::cell::RefCell;
use std::ffi::c_void;
#[cfg(unix)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use clouddeck_extension_api::abi::{Buffer, HostTable, INIT_OK, Slice};
use clouddeck_provider_api::{ErrorKind, ProviderError};

use super::manifest::Capability;
use super::state::StateStore;
use super::{check_api_version, check_build_info};

type ApiVersionFn = unsafe extern "C" fn() -> u32;
type BuildInfoFn = unsafe extern "C" fn() -> Slice;
type InitFn = unsafe extern "C" fn(*const HostTable) -> i32;
type CallFn = unsafe extern "C" fn(u32, Slice) -> Buffer;
type FreeFn = unsafe extern "C" fn(*mut u8, usize);

const LEVEL_NAMES: [&str; 4] = ["debug", "info", "warn", "error"];
const MAX_REQUEST_BYTES: usize = 1 << 20;
const MAX_RESPONSE_BYTES: usize = 16 << 20;

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
    /// dlopen runs the library constructors before our version and build checks, which is why
    /// ownership and permission checks come first.
    pub fn open(path: &Path, root: &Path, host: Arc<HostContext>) -> Result<Self, String> {
        check_trusted_location(path, root)?;
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

    pub fn call(&self, method: u32, request: &[u8]) -> Result<Vec<u8>, ProviderError> {
        check_request(request)?;
        let buffer = unsafe { (self.call)(method, Slice::of(request)) };
        take_response(buffer, self.free)
    }
}

fn check_request(request: &[u8]) -> Result<(), ProviderError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(ProviderError::new(
            ErrorKind::Other,
            "extension request too large",
        ));
    }
    Ok(())
}

/// The length comes from the extension, so it is checked before any copy. An oversized buffer is
/// handed back to the extension's `free` without being read.
fn take_response(buffer: Buffer, free: FreeFn) -> Result<Vec<u8>, ProviderError> {
    if buffer.ptr.is_null() {
        return Ok(Vec::new());
    }
    if buffer.len > MAX_RESPONSE_BYTES {
        unsafe { free(buffer.ptr, buffer.len) };
        return Err(ProviderError::new(
            ErrorKind::Other,
            "extension response too large",
        ));
    }
    let response = unsafe { buffer.bytes() }.to_vec();
    unsafe { free(buffer.ptr, buffer.len) };
    Ok(response)
}

/// `path` is canonical and must lie under `root`. Every folder from `root` down to the library and
/// the library itself must belong to the current user and must not be writable by group or others.
#[cfg(unix)]
fn check_trusted_location(path: &Path, root: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot read {}: {error}", root.display()))?;
    if !path.starts_with(&root) {
        return Err(format!(
            "{} is outside the extensions folder {}",
            path.display(),
            root.display()
        ));
    }
    if !fs::metadata(path).is_ok_and(|m| m.is_file()) {
        return Err(format!("{} is not a regular file", path.display()));
    }
    // SAFETY: geteuid has no preconditions and cannot fail.
    let euid = unsafe { libc::geteuid() };
    for folder in path
        .ancestors()
        .skip(1)
        .take_while(|folder| folder.starts_with(&root))
    {
        check_owner_and_mode(folder, euid)?;
    }
    check_owner_and_mode(path, euid)
}

#[cfg(unix)]
fn check_owner_and_mode(path: &Path, euid: u32) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata =
        fs::metadata(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if metadata.uid() != euid {
        return Err(format!(
            "{} is owned by uid {}, not by the user running CloudDeck",
            path.display(),
            metadata.uid()
        ));
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(format!(
            "{} is writable by other users; fix it with: chmod go-w {}",
            path.display(),
            path.display()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_trusted_location(_path: &Path, _root: &Path) -> Result<(), String> {
    Err("extensions can only be loaded on macOS and Linux".into())
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
    use std::sync::atomic::AtomicUsize;

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

    static OVERSIZED_FREES: AtomicUsize = AtomicUsize::new(0);
    static OVERSIZED_FREED_LEN: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn record_free(_ptr: *mut u8, len: usize) {
        OVERSIZED_FREES.fetch_add(1, Ordering::SeqCst);
        OVERSIZED_FREED_LEN.store(len, Ordering::SeqCst);
    }

    #[test]
    fn oversized_response_is_freed_without_copying() {
        let claimed = MAX_RESPONSE_BYTES + 1;
        let buffer = Buffer {
            ptr: std::ptr::NonNull::<u8>::dangling().as_ptr(),
            len: claimed,
        };
        let error = take_response(buffer, record_free).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Other);
        assert_eq!(error.message, "extension response too large");
        assert_eq!(OVERSIZED_FREES.load(Ordering::SeqCst), 1);
        assert_eq!(OVERSIZED_FREED_LEN.load(Ordering::SeqCst), claimed);
    }

    #[test]
    fn response_at_the_cap_is_returned() {
        let bytes = vec![7u8; MAX_RESPONSE_BYTES];
        let buffer = Buffer::from_vec(bytes);
        let response = take_response(buffer, free).unwrap();
        assert_eq!(response.len(), MAX_RESPONSE_BYTES);
        assert!(response.iter().all(|&b| b == 7));
    }

    #[test]
    fn request_over_the_cap_is_refused() {
        let request = vec![0u8; MAX_REQUEST_BYTES + 1];
        let error = check_request(&request).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Other);
        assert_eq!(error.message, "extension request too large");
        assert!(check_request(&request[..MAX_REQUEST_BYTES]).is_ok());
    }
}
