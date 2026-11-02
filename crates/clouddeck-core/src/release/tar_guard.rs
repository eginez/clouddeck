//! Byte-stream guard for tar archives.
//!
//! The tar crate reads GNU longname/linkname and PAX extension bodies into memory without
//! a size check. This reader follows the 512-byte block framing of the bytes it passes
//! through, refuses extension bodies over a small cap before tar buffers them, and fails
//! closed on anything it cannot frame exactly: sparse entries, base-256 sizes and
//! malformed PAX records.

use std::io::{self, Read};

pub(super) const EXTENSION_TOO_LARGE: &str =
    "archive extension header is larger than the size limit";
pub(super) const PAX_SIZE_OVERRIDE: &str = "archive pax header overrides entry size";
pub(super) const PAX_MALFORMED: &str = "archive pax header is malformed";
pub(super) const SPARSE_ENTRY: &str = "archive has a sparse entry";
pub(super) const BINARY_SIZE: &str = "archive entry size uses base-256 encoding";

const BLOCK: usize = 512;
const MAX_EXTENSION_HEADER_BYTES: u64 = 64 * 1024;

pub(super) struct ExtensionGuard<R> {
    pub(super) inner: R,
    pub(super) state: GuardState,
}

#[derive(Default)]
pub(super) struct GuardState {
    header: Vec<u8>,
    body_left: u64,
    body_kind: Option<u8>,
    body_declared: u64,
    body: Vec<u8>,
}

struct Entry {
    extension: Option<u8>,
    size: u64,
}

impl<R: Read> Read for ExtensionGuard<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.state.observe(&buf[..n])?;
        Ok(n)
    }
}

impl GuardState {
    fn observe(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            if self.body_left > 0 {
                let take = self.body_left.min(bytes.len() as u64) as usize;
                if self.body_kind.is_some() {
                    self.body.extend_from_slice(&bytes[..take]);
                }
                self.body_left -= take as u64;
                bytes = &bytes[take..];
                if self.body_left == 0 {
                    self.finish_body()?;
                }
                continue;
            }
            let take = (BLOCK - self.header.len()).min(bytes.len());
            self.header.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if self.header.len() == BLOCK {
                let entry = header_check(&self.header)?;
                self.header.clear();
                self.body_kind = entry.extension;
                self.body_declared = entry.size;
                self.body.clear();
                self.body_left = entry.size.div_ceil(BLOCK as u64) * BLOCK as u64;
                if self.body_left == 0 && self.body_kind.is_some() {
                    self.finish_body()?;
                }
            }
        }
        Ok(())
    }

    fn finish_body(&mut self) -> io::Result<()> {
        if matches!(self.body_kind, Some(b'x' | b'g')) {
            pax_check(&self.body[..self.body_declared as usize])?;
        }
        self.body_kind = None;
        self.body.clear();
        Ok(())
    }
}

fn header_check(header: &[u8]) -> io::Result<Entry> {
    let kind = header[156];
    if kind == b'S' {
        return Err(io::Error::other(SPARSE_ENTRY));
    }
    let field = &header[124..136];
    if field[0] & 0x80 != 0 {
        return Err(io::Error::other(BINARY_SIZE));
    }
    let size = parse_octal_size(field)?;
    let extension = matches!(kind, b'L' | b'K' | b'x' | b'g').then_some(kind);
    if extension.is_some() && size > MAX_EXTENSION_HEADER_BYTES {
        return Err(io::Error::other(EXTENSION_TOO_LARGE));
    }
    Ok(Entry { extension, size })
}

fn parse_octal_size(field: &[u8]) -> io::Result<u64> {
    let invalid = || io::Error::other("invalid archive entry size");
    let end = field
        .iter()
        .position(|&b| b == 0 || b == b' ')
        .unwrap_or(field.len());
    if field[end..].iter().any(|&b| b != 0 && b != b' ') {
        return Err(invalid());
    }
    field[..end].iter().try_fold(0u64, |value, &digit| {
        if !(b'0'..=b'7').contains(&digit) {
            return Err(invalid());
        }
        value
            .checked_mul(8)
            .and_then(|v| v.checked_add(u64::from(digit - b'0')))
            .ok_or_else(invalid)
    })
}

fn pax_check(mut body: &[u8]) -> io::Result<()> {
    let malformed = || io::Error::other(PAX_MALFORMED);
    while !body.is_empty() {
        let space = body.iter().position(|&b| b == b' ').ok_or_else(malformed)?;
        let digits = &body[..space];
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return Err(malformed());
        }
        let len = digits.iter().try_fold(0usize, |value, &digit| {
            value
                .checked_mul(10)
                .and_then(|v| v.checked_add(usize::from(digit - b'0')))
                .ok_or_else(malformed)
        })?;
        if len == 0 || len > body.len() {
            return Err(malformed());
        }
        let record = &body[..len];
        if record[len - 1] != b'\n' {
            return Err(malformed());
        }
        let text = record.get(space + 1..len - 1).ok_or_else(malformed)?;
        let eq = text.iter().position(|&b| b == b'=').ok_or_else(malformed)?;
        if &text[..eq] == b"size" {
            return Err(io::Error::other(PAX_SIZE_OVERRIDE));
        }
        body = &body[len..];
    }
    Ok(())
}
