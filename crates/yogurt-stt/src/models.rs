//! Static model registry + download path resolver for whisper.cpp models.
//!
//! # ⚠️ WARNING: SHA256 PLACEHOLDERS
//!
//! The `sha256` values pinned in [`REGISTRY`] are a 2026-06 SNAPSHOT and
//! MUST be re-verified before merge.  The download path (`download_to`,
//! Plan 08-02 Task 3) is hard-failure on hash mismatch - if these
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
//! Models live under the ProjectDirs data dir (macOS:
//! `~/Library/Application Support/com.yogurt.yogurt/models/ggml-<name>.bin`,
//! NOT `~/.yogurt` - a doc lie that once sent debugging astray), resolved via
//! `directories::ProjectDirs::data_local_dir()`.  Phase 5 set the
//! `data_local_dir` base to `~/.yogurt`; if that ever changes, follow
//! Phase 5's convention here.
//!
//! See PRD §5.6 (model storage path) and §5.8 (Intel x86_64 supports
//! tiny/small only; medium/large are arm64-only due to whisper.cpp
//! Metal kernel requirements).

use crate::sha256;
use futures_util::StreamExt;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

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
    /// On-disk filename - `ggml-<name>.bin`.  Lives directly under
    /// `model_path()`'s parent.
    pub filename: &'static str,
    /// Approximate downloaded size in MB (used by the UI to surface
    /// "download 487 MB?" before kicking off the transfer).
    pub size_mb: u32,
    /// Canonical HuggingFace URL - `download_to` GETs this and
    /// supports `Range:` resume.
    pub url: &'static str,
    /// Lowercase hex SHA256 of the downloaded file.  See
    /// module-level WARNING - re-verify before merge.
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
        // Verified against HuggingFace blob on 2026-06-28 via
        // `curl -L <url> | shasum -a 256` (74 MB download).
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        intel_supported: true,
    },
    ModelSpec {
        name: "small.en",
        filename: "ggml-small.en.bin",
        size_mb: 487,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        // Verified against HuggingFace blob on 2026-06-28 via a user-driven
        // download that surfaced the placeholder mismatch (Plan 08-02's
        // HashMismatch path deleted the file and the dialog showed the
        // expected/actual pair, which is now pinned here).
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
        intel_supported: true,
    },
    ModelSpec {
        name: "medium.en",
        filename: "ggml-medium.en.bin",
        size_mb: 1_530,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        // Verified against HuggingFace blob on 2026-06-28 via
        // `./scripts/refresh-model-hashes.sh medium.en` (1.5 GB download).
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
        intel_supported: false,
    },
    ModelSpec {
        name: "large-v3",
        filename: "ggml-large-v3.bin",
        size_mb: 3_094,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        // Verified against HuggingFace blob on 2026-06-28 via a user-driven
        // 3 GB download that surfaced the placeholder mismatch (HashMismatch
        // path deleted the file and the dialog showed expected/actual pair,
        // now pinned here). All four placeholders are now real values.
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        intel_supported: false,
    },
];

/// Linear scan for a model by name.  Returns `None` for unknown names.
///
/// O(n) but `n == 4` so it doesn't matter; no hashmap overhead.
pub fn lookup(name: &str) -> Option<&'static ModelSpec> {
    REGISTRY.iter().find(|m| m.name == name)
}

/// Resolve `<data_dir>/models/<spec.filename>`.  Creates the directory
/// if it does not exist.
///
/// Phase 5 set the `data_local_dir` base to `~/.yogurt` via the
/// `("com", "yogurt", "yogurt")` ProjectDirs triple - this function
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

/// `true` iff the model file exists AND verifies against `spec.sha256`
/// (case-insensitive).  A file that exists but does not verify counts
/// as not-downloaded - callers should re-download.
///
/// Verification is CHEAP on the happy path: a sidecar
/// `<filename>.sha256` marker (single line `<hash> <len>`, written by
/// every successful full-file verification) is checked against the
/// spec hash and the file's current byte length - no hashing.  Only a
/// missing/invalid/stale marker falls back to a one-time full SHA256
/// (legacy migration), which self-heals the marker on success.  This
/// matters: hashing a 3 GB model per call starved the tokio runtime.
///
/// This is the source of truth the UI uses to render "✓ Downloaded"
/// vs "Download" in the model picker.  Any IO error (e.g., permission
/// denied) is treated as not-downloaded.
pub fn is_downloaded(spec: &ModelSpec) -> bool {
    let path = match model_path(spec) {
        Ok(p) => p,
        Err(_) => return false,
    };
    is_downloaded_at(&path, spec.sha256)
}

/// Path-injectable core of `is_downloaded` - see that fn's doc comment.
fn is_downloaded_at(path: &Path, expected_sha256: &str) -> bool {
    // Existence short-circuit; also gives us the current length for the
    // marker check without a second stat.
    let len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return false,
    };
    if let Some((hash, marker_len)) = read_marker(path) {
        if hash.eq_ignore_ascii_case(expected_sha256) && marker_len == len {
            return true;
        }
    }
    // Legacy migration / stale marker: hash once, self-heal on match.
    match sha256::hash_file(path) {
        Ok(actual) if actual.eq_ignore_ascii_case(expected_sha256) => {
            write_marker(path, &actual);
            true
        }
        _ => false,
    }
}

/// `<model path>.sha256` - appends to the FULL file name
/// (`ggml-x.bin` -> `ggml-x.bin.sha256`).  Not `with_extension`, which
/// would replace `.bin`.
///
/// `pub` so `yogurt-server`'s DELETE handler (`api::stt_models::delete_model`)
/// can remove the sidecar alongside the model file — otherwise a deleted
/// model leaves a stale marker behind that would falsely fast-path
/// `is_downloaded_at` against bytes that no longer exist.
pub fn marker_path(model: &Path) -> PathBuf {
    let mut s = model.as_os_str().to_os_string();
    s.push(".sha256");
    PathBuf::from(s)
}

/// Best-effort: record `<lowercase-hash> <len>` next to the model so
/// future `is_downloaded` calls skip hashing.  Failures are non-fatal -
/// the worst case is re-hashing on the next check.
fn write_marker(model: &Path, hash: &str) {
    let len = match std::fs::metadata(model) {
        Ok(m) => m.len(),
        Err(e) => {
            tracing::warn!(?model, %e, "sha256 marker: stat failed, skipping write");
            return;
        }
    };
    let line = format!("{} {}\n", hash.to_ascii_lowercase(), len);
    if let Err(e) = std::fs::write(marker_path(model), line) {
        tracing::warn!(?model, %e, "sha256 marker: write failed (non-fatal)");
    }
}

/// Parse the sidecar marker into `(hash, len)`.  Any IO or parse
/// failure -> `None` (caller falls back to hashing).
fn read_marker(model: &Path) -> Option<(String, u64)> {
    let s = std::fs::read_to_string(marker_path(model)).ok()?;
    let mut parts = s.split_whitespace();
    let hash = parts.next()?.to_string();
    let len = parts.next()?.parse().ok()?;
    Some((hash, len))
}

/// Progress tick reported by `download_to` (via the caller-supplied
/// `FnMut(DownloadProgress)`).  The UI uses these to render the
/// "Downloading 487 MB · 23%" surface in Settings.
///
/// `bytes_per_sec` and `eta_seconds` are EWMA'd over the last ~500 ms
/// window - they jitter less than instant-rate measurements.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub bytes_per_sec: u64,
    pub eta_seconds: Option<u64>,
}

/// Errors surfaced by `download_to` / `download`.
///
/// `HashMismatch` carries both digests so the UI can show the user
/// what we got vs what we expected - useful when re-verifying
/// REGISTRY placeholders.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sha256 mismatch: expected {expected}, actual {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("download cancelled")]
    Cancelled,
}

/// Download a file from `url` to `dest`, resuming a partial file via
/// `Range: bytes=N-` if present.  Verifies SHA256 after completion -
/// on mismatch, the file is DELETED so a subsequent call won't take
/// the fast-path against corrupted state.
///
/// `on_progress` is called periodically (≥500 ms cadence) plus once
/// at the very end with `bytes_downloaded == total_bytes`.
///
/// ## Fast path
///
/// If `dest` already exists AND its SHA256 matches `expected_sha256`,
/// we return `Ok(())` without touching the network - important for
/// the "model already installed" case on app boot.
///
/// ## Resume path
///
/// If `dest` exists but doesn't verify, we read its length and send
/// `Range: bytes={len}-`.  Server is expected to reply 206
/// PARTIAL_CONTENT; if it instead returns 200 (Range not supported)
/// we still proceed but truncate-overwrite from byte 0 - the
/// `append(true)` open + seek-to-end below handles both cases.
pub async fn download_to<F>(
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    mut on_progress: F,
) -> Result<(), DownloadError>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    // 1. Fast-path: file exists and hashes correctly.
    //
    // Hashing runs via `spawn_blocking` — `large-v3` is ~3 GB, and a
    // synchronous `sha256::hash_file` call here would block the tokio
    // reactor thread for as long as the hash takes (this is the same
    // class of bug the `list_models` handler comment in `stt_models.rs`
    // warns about, just reachable through the download path instead).
    if dest.exists() {
        let dest_owned = dest.to_path_buf();
        if let Ok(Ok(existing)) =
            tokio::task::spawn_blocking(move || sha256::hash_file(&dest_owned)).await
        {
            if existing.eq_ignore_ascii_case(expected_sha256) {
                write_marker(dest, &existing);
                return Ok(());
            }
        }
    }

    // 2. Detect partial-file size for resume.
    let existing_len: u64 = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);

    let client = reqwest::Client::builder()
        .user_agent("yogurt-stt/0.1")
        .build()?;

    let mut req = client.get(url);
    if existing_len > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={}-", existing_len));
    }
    let resp = req.send().await?.error_for_status()?;
    let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;

    // total_bytes = whatever the server is about to stream + what we
    // already have on disk (when resuming).  For a non-resumed
    // download, content_length() is the full payload.
    let stream_len = resp.content_length().unwrap_or(0);
    let total_bytes = if resumed {
        existing_len + stream_len
    } else {
        stream_len
    };

    // 3. Open file: append + seek for resume, create for full.
    let mut file: std::fs::File = if resumed && existing_len > 0 {
        let mut f = std::fs::OpenOptions::new().append(true).open(dest)?;
        f.seek(SeekFrom::End(0))?;
        f
    } else {
        // Either no existing file, or server ignored Range and is
        // sending the full body from byte 0 - truncate.
        std::fs::File::create(dest)?
    };

    // 4. Stream bytes; fire progress every ~500 ms.
    let mut downloaded: u64 = if resumed { existing_len } else { 0 };
    let mut bytes_since_tick: u64 = 0;
    let mut last_tick = Instant::now();

    let mut stream = resp.bytes_stream();
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        bytes_since_tick += chunk.len() as u64;

        let elapsed = last_tick.elapsed();
        if elapsed.as_millis() >= 500 {
            let bps = if elapsed.as_secs_f64() > 0.0 {
                (bytes_since_tick as f64 / elapsed.as_secs_f64()) as u64
            } else {
                0
            };
            let eta = if total_bytes > 0 && bps > 0 {
                Some(total_bytes.saturating_sub(downloaded) / bps)
            } else {
                None
            };
            on_progress(DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes,
                bytes_per_sec: bps,
                eta_seconds: eta,
            });
            bytes_since_tick = 0;
            last_tick = Instant::now();
        }
    }

    // Final tick at 100%.
    let final_total = if total_bytes == 0 {
        downloaded
    } else {
        total_bytes
    };
    on_progress(DownloadProgress {
        bytes_downloaded: downloaded,
        total_bytes: final_total,
        bytes_per_sec: 0,
        eta_seconds: Some(0),
    });

    file.sync_all()?;
    drop(file);

    // 5. Verify SHA256; delete on mismatch.
    //
    // Same reactor-blocking concern as the fast-path check above — push
    // the full-file hash onto the blocking pool rather than running it
    // inline on the async task.
    let dest_owned = dest.to_path_buf();
    let actual = tokio::task::spawn_blocking(move || sha256::hash_file(&dest_owned))
        .await
        .map_err(|e| std::io::Error::other(format!("hash join error: {e}")))??;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = std::fs::remove_file(dest);
        return Err(DownloadError::HashMismatch {
            expected: expected_sha256.to_string(),
            actual,
        });
    }
    write_marker(dest, &actual);

    Ok(())
}

/// Convenience wrapper: resolve `model_path(spec)` and call
/// `download_to(spec.url, path, spec.sha256, ...)`.
pub async fn download<F>(spec: &ModelSpec, on_progress: F) -> Result<(), DownloadError>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    let path = model_path(spec)?;
    download_to(spec.url, &path, spec.sha256, on_progress).await
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
        // Case-sensitive - we want the UI to be exact-match.
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

    // ── Sidecar `.sha256` marker behavior ───────────────────────────────

    /// Marker path convention shared by the tests below.
    fn marker_of(model: &Path) -> PathBuf {
        let mut s = model.as_os_str().to_os_string();
        s.push(".sha256");
        PathBuf::from(s)
    }

    #[test]
    fn valid_marker_skips_hashing() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("ggml-test.bin");
        let content = b"content that does NOT hash to the expected value";
        std::fs::write(&model, content).unwrap();
        // Expected hash is of DIFFERENT bytes - if is_downloaded_at hashed
        // the file it would return false. A matching marker must win.
        let expected = sha256::hash_bytes(b"the payload the registry pins");
        std::fs::write(
            marker_of(&model),
            format!("{} {}\n", expected, content.len()),
        )
        .unwrap();
        assert!(
            is_downloaded_at(&model, &expected),
            "valid marker (hash + length match) must short-circuit hashing"
        );
    }

    #[test]
    fn legacy_file_without_marker_self_heals() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("ggml-test.bin");
        let payload = b"legacy on-disk model bytes";
        std::fs::write(&model, payload).unwrap();
        let expected = sha256::hash_bytes(payload);
        assert!(
            is_downloaded_at(&model, &expected),
            "matching content must verify"
        );
        let marker =
            std::fs::read_to_string(marker_of(&model)).expect("self-heal must write the marker");
        assert_eq!(marker.trim(), format!("{} {}", expected, payload.len()));
    }

    #[test]
    fn stale_marker_length_falls_back_to_hash_and_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("ggml-test.bin");
        let payload = b"complete model bytes";
        std::fs::write(&model, payload).unwrap();
        let expected = sha256::hash_bytes(payload);
        // Right hash, wrong length - simulates a marker written for a
        // different (e.g. truncated-then-completed) file state.
        std::fs::write(
            marker_of(&model),
            format!("{} {}\n", expected, payload.len() + 1),
        )
        .unwrap();
        assert!(
            is_downloaded_at(&model, &expected),
            "re-hash fallback must verify"
        );
        let marker = std::fs::read_to_string(marker_of(&model)).unwrap();
        assert_eq!(
            marker.trim(),
            format!("{} {}", expected, payload.len()),
            "stale marker must be rewritten with the correct length"
        );
    }

    #[test]
    fn corrupt_file_returns_false_and_writes_no_marker() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("ggml-test.bin");
        std::fs::write(&model, b"corrupted bytes").unwrap();
        let expected = sha256::hash_bytes(b"what the registry expected");
        assert!(!is_downloaded_at(&model, &expected));
        assert!(
            !marker_of(&model).exists(),
            "a failed verification must never write a marker"
        );
    }

    #[test]
    fn missing_file_returns_false() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("does-not-exist.bin");
        assert!(!is_downloaded_at(&model, &sha256::hash_bytes(b"x")));
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
