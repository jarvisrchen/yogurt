//! Streaming SHA256 helper. Used by the model download flow (Plan 08-02) to
//! verify integrity without holding the full ~3 GB `large-v3` file in memory.
//!
//! Streams the file in 64 KB chunks through the hasher — peak memory usage is
//! the chunk size, not the file size. The hex digest is lowercase to match
//! the canonical `shasum -a 256 <file>` output style we ship in the
//! `models.rs` registry.
//!
//! ```ignore
//! let digest = yogurt_stt::sha256::hash_file(Path::new("ggml-tiny.en.bin"))?;
//! assert_eq!(digest.len(), 64); // 256 bits / 4 bits per hex char
//! ```

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Streaming SHA256 of a file at `path`. Reads in 64 KB chunks so a multi-GB
/// model never lives in memory all at once. Returns the lowercase hex digest.
///
/// I/O errors (missing file, permission denied) surface as `io::Error`. The
/// caller decides whether to treat that as "model not downloaded" (Plan 08-02
/// `models::is_downloaded`) or as a hard failure (the download finisher).
pub fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// In-memory SHA256 of a byte slice. Convenience wrapper used by tests
/// (asserting `hash_file == hash_bytes` for a temp-file roundtrip) and by
/// callers that already have the payload buffered (e.g., tiny config blobs).
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn it_hashes_empty_input() {
        // Canonical SHA256 of "" — the one digest every SHA256 implementation
        // gets right, and the one most likely to break if we accidentally
        // hash some metadata into the stream.
        assert_eq!(
            hash_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn it_hashes_short_input() {
        // The "hello" digest. Sanity check that the bytes we feed in are
        // exactly the bytes hashed (no UTF-8/UTF-16 mangling, no trailing
        // newline).
        assert_eq!(
            hash_bytes(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn it_hashes_a_file_matching_in_memory_hash() {
        // 200 KB > the 64 KB chunk buffer so the streaming loop runs at
        // least three times. If the chunked read accidentally swallows a
        // tail byte or double-hashes a boundary, this catches it.
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(&payload).unwrap();
        tmp.flush().unwrap();
        let from_file = hash_file(tmp.path()).unwrap();
        assert_eq!(from_file, hash_bytes(&payload));
    }
}
