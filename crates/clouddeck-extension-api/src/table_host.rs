use std::ffi::c_void;
use std::path::PathBuf;

use clouddeck_provider_api::{ErrorKind, Host, LogLevel, ProviderError};

use crate::abi::{Buffer, HostTable, Slice};

pub(crate) struct TableHost(pub(crate) HostTable);

// SAFETY: the host promises that its table's context and functions may be used from any thread.
unsafe impl Send for TableHost {}
unsafe impl Sync for TableHost {}

type Lookup = extern "C" fn(*mut c_void, Slice, *mut Buffer) -> bool;

impl TableHost {
    fn take(&self, buffer: Buffer) -> String {
        if buffer.ptr.is_null() {
            return String::new();
        }
        // SAFETY: the host returned this buffer and keeps it valid until `free`.
        let text = String::from_utf8_lossy(unsafe { buffer.bytes() }).into_owned();
        (self.0.free)(buffer.ptr, buffer.len);
        text
    }

    fn lookup(&self, lookup: Lookup, key: &str) -> Option<String> {
        let mut out = Buffer::EMPTY;
        lookup(self.0.ctx, Slice::of(key.as_bytes()), &mut out).then(|| self.take(out))
    }

    fn failure(&self, error: Buffer) -> ProviderError {
        ProviderError::new(ErrorKind::Other, self.take(error))
    }
}

impl Host for TableHost {
    fn log(&self, level: LogLevel, message: &str) {
        (self.0.log)(self.0.ctx, level as u32, Slice::of(message.as_bytes()));
    }

    fn data_dir(&self) -> PathBuf {
        PathBuf::from(self.take((self.0.data_dir)(self.0.ctx)))
    }

    fn setting(&self, key: &str) -> Option<String> {
        self.lookup(self.0.setting, key)
    }

    fn secret(&self, key: &str) -> Option<String> {
        self.lookup(self.0.secret, key)
    }

    fn set_secret(&self, key: &str, value: Option<&str>) -> Result<(), ProviderError> {
        let mut error = Buffer::EMPTY;
        let (value, has_value) = match value {
            Some(value) => (Slice::of(value.as_bytes()), true),
            None => (Slice::of(&[]), false),
        };
        let stored = (self.0.set_secret)(
            self.0.ctx,
            Slice::of(key.as_bytes()),
            value,
            has_value,
            &mut error,
        );
        if stored { Ok(()) } else { Err(self.failure(error)) }
    }

    fn open_url(&self, url: &str) -> Result<(), ProviderError> {
        let mut error = Buffer::EMPTY;
        let opened = (self.0.open_url)(self.0.ctx, Slice::of(url.as_bytes()), &mut error);
        if opened { Ok(()) } else { Err(self.failure(error)) }
    }

    fn cancelled(&self) -> bool {
        (self.0.cancelled)(self.0.ctx)
    }
}
