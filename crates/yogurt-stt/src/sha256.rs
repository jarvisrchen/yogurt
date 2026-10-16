//! Streaming SHA256 so a multi-GB model never has to sit in memory.
//! The hex digest is lowercase, matching `shasum -a 256` and the `models.rs` registry.

use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// SHA256 of the file at `path`, read in 64 KB chunks, as lowercase hex.
///
/// I/O errors surface as `io::Error` so the caller decides whether that means "not downloaded" or a hard failure.
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
    Ok(format!("{:x}", hasher.finalize()))
}

/// In-memory SHA256 of a byte slice.
pub fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn it_hashes_empty_input() {
        assert_eq!(
            hash_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn it_hashes_short_input() {
        assert_eq!(
            hash_bytes(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn it_hashes_a_file_matching_in_memory_hash() {
        // 200 KB exceeds the 64 KB chunk so the streaming loop runs several times.
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(&payload).unwrap();
        tmp.flush().unwrap();
        let from_file = hash_file(tmp.path()).unwrap();
        assert_eq!(from_file, hash_bytes(&payload));
    }
}
