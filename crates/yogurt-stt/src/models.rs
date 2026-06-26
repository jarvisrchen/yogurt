//! Static model registry + download path resolver for whisper.cpp models.
//!
//! # ⚠️ WARNING: SHA256 PLACEHOLDERS
//!
//! The `sha256` values pinned in [`REGISTRY`] are a 2026-06 SNAPSHOT and
//! MUST be re-verified before merge.  The download path (`download_to`,
//! Plan 08-02 Task 3) is hard-failure on hash mismatch — if these
//! placeholders drift from what HuggingFace actually serves, the entire
//! local-STT feature breaks 100% with a `HashMismatch` error.
//!
//! **To re-verify** (do this once at merge time, then again before each
//! release):
//!
//! ```bash
//! curl -fL -o /tmp/ggml-tiny.en.bin \
//!     https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin
//! shasum -a 256 /tmp/ggml-tiny.en.bin
//! # paste the lowercase hex digest into REGISTRY below
//! ```
//!
//! Repeat for `small.en`, `medium.en`, `large-v3`.  All hashes are
//! lowercase hex, exactly 64 characters, no `sha256:` prefix.
//!
//! # Layout
//!
//! Models live under `~/.yogurt/models/ggml-<name>.bin`, resolved via
//! `directories::ProjectDirs::data_local_dir()`.  Phase 5 set the
//! `data_local_dir` base to `~/.yogurt`; if that ever changes, follow
//! Phase 5's convention here.
//!
//! See PRD §5.6 (model storage path) and §5.8 (Intel x86_64 supports
//! tiny/small only; medium/large are arm64-only due to whisper.cpp
//! Metal kernel requirements).

use crate::sha256;
use std::path::PathBuf;

/// Static description of one downloadable whisper.cpp model.
///
/// All fields are `&'static` so the registry can live in `.rodata`
/// (no heap, no lazy_static).
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    /// Human-facing handle used by `lookup()` and the UI selector
    /// (e.g., `"small.en"`).  Matches the suffix of the HuggingFace
    /// filename.
    pub name: &'static str,
    /// On-disk filename — `ggml-<name>.bin`.  Lives directly under
    /// `model_path()`'s parent.
    pub filename: &'static str,
    /// Approximate downloaded size in MB (used by the UI to surface
    /// "download 487 MB?" before kicking off the transfer).
    pub size_mb: u32,
    /// Canonical HuggingFace URL — `download_to` GETs this and
    /// supports `Range:` resume.
    pub url: &'static str,
    /// Lowercase hex SHA256 of the downloaded file.  See
    /// module-level WARNING — re-verify before merge.
    pub sha256: &'static str,
    /// `true` if the model runs on Intel x86_64 (PRD §5.8).  Medium
    /// and large-v3 require arm64 Metal kernels and are arm64-only.
    pub intel_supported: bool,
}

/// All available models, ASCENDING by `size_mb`.  The ordering is
/// load-bearing: the UI walks the registry top-to-bottom to show
/// "smallest first" and the `it_..._size_order` test enforces it.
pub const REGISTRY: &[ModelSpec] = &[
    ModelSpec {
        name: "tiny.en",
        filename: "ggml-tiny.en.bin",
        size_mb: 75,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        // PLACEHOLDER — see module WARNING.  Re-verify with shasum -a 256.
        sha256: "921e4cf8686fdd993dcd081a5da5b6c732a464b00ce4499c1d3ed8e9d4f9b8c5",
        intel_supported: true,
    },
    ModelSpec {
        name: "small.en",
        filename: "ggml-small.en.bin",
        size_mb: 487,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        // PLACEHOLDER — see module WARNING.
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fddf0b9cd",
        intel_supported: true,
    },
    ModelSpec {
        name: "medium.en",
        filename: "ggml-medium.en.bin",
        size_mb: 1_530,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        // PLACEHOLDER — see module WARNING.
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8fcf02bc54cf2deddbcb22d6c",
        intel_supported: false,
    },
    ModelSpec {
        name: "large-v3",
        filename: "ggml-large-v3.bin",
        size_mb: 3_094,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        // PLACEHOLDER — see module WARNING.
        sha256: "ad82bf6a9043ceed055076d0fd39f5f186ff8062db9e2e6f9bcd0afd6a9b9b3a",
        intel_supported: false,
    },
];

/// Linear scan for a model by name.  Returns `None` for unknown names.
///
/// O(n) but `n == 4` so it doesn't matter; no hashmap overhead.
pub fn lookup(name: &str) -> Option<&'static ModelSpec> {
    REGISTRY.iter().find(|m| m.name == name)
}

/// Resolve `~/.yogurt/models/<spec.filename>`.  Creates the directory
/// if it does not exist.
///
/// Phase 5 set the `data_local_dir` base to `~/.yogurt` via the
/// `("com", "yogurt", "yogurt")` ProjectDirs triple — this function
/// follows the same convention.  If Phase 5's base ever changes,
/// update here in lock-step.
pub fn model_path(spec: &ModelSpec) -> std::io::Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "yogurt", "yogurt").ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve user data directory",
        )
    })?;
    let dir = dirs.data_local_dir().join("models");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(spec.filename))
}

/// `true` iff the model file exists AND its SHA256 matches `spec.sha256`
/// (case-insensitive).  A file that exists but does not verify counts
/// as not-downloaded — callers should re-download.
///
/// This is the source of truth the UI uses to render "✓ Downloaded"
/// vs "Download" in the model picker.  Any IO error (e.g., permission
/// denied) is treated as not-downloaded.
pub fn is_downloaded(spec: &ModelSpec) -> bool {
    let path = match model_path(spec) {
        Ok(p) => p,
        Err(_) => return false,
    };
    if !path.exists() {
        return false;
    }
    match sha256::hash_file(&path) {
        Ok(actual) => actual.eq_ignore_ascii_case(spec.sha256),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_four_models_in_size_order() {
        assert_eq!(REGISTRY.len(), 4, "registry must contain exactly 4 models");
        // Ascending by size_mb.
        for pair in REGISTRY.windows(2) {
            assert!(
                pair[0].size_mb < pair[1].size_mb,
                "registry must be sorted ascending by size_mb: {} ({} MB) before {} ({} MB)",
                pair[0].name,
                pair[0].size_mb,
                pair[1].name,
                pair[1].size_mb
            );
        }
    }

    #[test]
    fn lookup_finds_known_models_and_misses_unknown() {
        assert_eq!(lookup("small.en").unwrap().name, "small.en");
        assert_eq!(lookup("tiny.en").unwrap().name, "tiny.en");
        assert_eq!(lookup("medium.en").unwrap().name, "medium.en");
        assert_eq!(lookup("large-v3").unwrap().name, "large-v3");
        assert!(lookup("nonexistent").is_none());
        assert!(lookup("").is_none());
        // Case-sensitive — we want the UI to be exact-match.
        assert!(lookup("Tiny.en").is_none());
    }

    #[test]
    fn intel_support_flags_match_prd() {
        // PRD §5.8: tiny + small run on x86_64; medium + large require
        // arm64 (Metal kernels).
        assert!(lookup("tiny.en").unwrap().intel_supported);
        assert!(lookup("small.en").unwrap().intel_supported);
        assert!(!lookup("medium.en").unwrap().intel_supported);
        assert!(!lookup("large-v3").unwrap().intel_supported);
    }

    #[test]
    fn sha256_values_are_hex_64_chars() {
        for spec in REGISTRY {
            assert_eq!(
                spec.sha256.len(),
                64,
                "{} sha256 must be 64 hex chars, got {}: {:?}",
                spec.name,
                spec.sha256.len(),
                spec.sha256
            );
            assert!(
                spec.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{} sha256 must be all hex digits, got {:?}",
                spec.name,
                spec.sha256
            );
            // We chose lowercase by convention (matches `shasum -a 256`).
            assert_eq!(
                spec.sha256,
                spec.sha256.to_ascii_lowercase(),
                "{} sha256 should be lowercase",
                spec.name
            );
        }
    }
}
