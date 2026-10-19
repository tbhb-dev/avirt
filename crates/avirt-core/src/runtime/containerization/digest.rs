//! SHA-256 of files and streams, as the lowercase hexadecimal digits that the pins and the OCI
//! digests use.

use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

/// Whether `text` is 64 lowercase hexadecimal characters.
#[must_use]
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Copies `source` to `sink` and returns the SHA-256 of the bytes.
///
/// # Errors
///
/// Returns an error if reading or writing fails.
pub fn copy_hashing<R, W>(source: &mut R, sink: &mut W) -> io::Result<String>
where
    R: Read,
    W: Write,
{
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = source.read(&mut buffer)?;
        let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
            return Ok(hex(&hasher.finalize()));
        };
        hasher.update(chunk);
        sink.write_all(chunk)?;
    }
}

/// Lowercase hexadecimal digits for `bytes`.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        text.extend(char::from_digit(u32::from(byte >> 4u8), 16u32));
        text.extend(char::from_digit(u32::from(byte & 0xfu8), 16u32));
    }
    text
}

/// The SHA-256 of `bytes`.
#[must_use]
pub fn bytes(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// The SHA-256 of the file at `path`.
///
/// # Errors
///
/// Returns an error if reading the file fails.
pub fn file(path: &Path) -> io::Result<String> {
    copy_hashing(&mut File::open(path)?, &mut io::sink())
}
