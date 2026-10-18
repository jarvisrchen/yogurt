//! Static model registry and download path resolver for whisper.cpp models.
//!
//! A `sha256` mismatch is a hard failure in `download_to`, so a [`REGISTRY`] hash that drifts from what HuggingFace serves breaks that model's download.
//! Re-verify with `scripts/refresh-model-hashes.sh <name>` before each release.
//!
//! Models live at `~/.yogurt/models/ggml-<name>.bin`, resolved via `directories::BaseDirs::home_dir()` like `yogurt-db::paths` and `yogurt-server::storage`.
//! `migrate_legacy_model` moves a model from the old `~/Library/Application Support/com.yogurt.yogurt/models/` path on first resolution.
//! Medium and large models are arm64-only because of whisper.cpp's Metal kernels.

use crate::sha256;
use futures_util::StreamExt;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Static description of one downloadable whisper.cpp model.
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    /// Handle used by `lookup()` and the UI selector, e.g. `"small.en"`.
    pub name: &'static str,
    /// On-disk filename, `ggml-<name>.bin`.
    pub filename: &'static str,
    /// Approximate download size in MB, shown before the transfer starts.
    pub size_mb: u32,
    /// Canonical HuggingFace URL, which `download_to` GETs with `Range:` resume.
    pub url: &'static str,
    /// GitHub release asset URL under the `models-v1` tag, or `None` when the model exceeds GitHub's 2 GiB asset cap (only `large-v3`).
    /// `download` tries it first so a network that blocks huggingface.co but reaches github.com still works.
    pub mirror_url: Option<&'static str>,
    /// Lowercase hex SHA256 of the downloaded file.
    pub sha256: &'static str,
    /// `true` if the model runs on Intel x86_64. Medium and large-v3 need arm64 Metal kernels.
    pub intel_supported: bool,
}

/// All available models, ascending by `size_mb`. The UI relies on this order and a test enforces it.
pub const REGISTRY: &[ModelSpec] = &[
    ModelSpec {
        name: "tiny.en",
        filename: "ggml-tiny.en.bin",
        size_mb: 75,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        mirror_url: Some(
            "https://github.com/jarvisrchen/yogurt/releases/download/models-v1/ggml-tiny.en.bin",
        ),
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        intel_supported: true,
    },
    ModelSpec {
        name: "small.en",
        filename: "ggml-small.en.bin",
        size_mb: 487,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        mirror_url: Some(
            "https://github.com/jarvisrchen/yogurt/releases/download/models-v1/ggml-small.en.bin",
        ),
        sha256: "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
        intel_supported: true,
    },
    ModelSpec {
        name: "medium.en",
        filename: "ggml-medium.en.bin",
        size_mb: 1_530,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        mirror_url: Some(
            "https://github.com/jarvisrchen/yogurt/releases/download/models-v1/ggml-medium.en.bin",
        ),
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
        intel_supported: false,
    },
    ModelSpec {
        name: "large-v3-turbo",
        filename: "ggml-large-v3-turbo.bin",
        size_mb: 1_620,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        mirror_url: Some(
            "https://github.com/jarvisrchen/yogurt/releases/download/models-v1/ggml-large-v3-turbo.bin",
        ),
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        intel_supported: false,
    },
    ModelSpec {
        name: "large-v3",
        filename: "ggml-large-v3.bin",
        size_mb: 3_094,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        // Not mirrored: 2.88 GiB is over GitHub's 2 GiB release asset cap.
        mirror_url: None,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        intel_supported: false,
    },
];

/// Linear scan for a model by name.
pub fn lookup(name: &str) -> Option<&'static ModelSpec> {
    REGISTRY.iter().find(|m| m.name == name)
}

/// Resolve `~/.yogurt/models/<spec.filename>`, creating the directory if needed.
///
/// Keep the base directory in sync with `yogurt-db/src/paths.rs` and `yogurt-server`'s storage modules, which are intentionally independent.
/// Side effect: runs [`migrate_legacy_model`] so previously downloaded models are not re-downloaded.
pub fn model_path(spec: &ModelSpec) -> std::io::Result<PathBuf> {
    let base = directories::BaseDirs::new().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve user home directory",
        )
    })?;
    let dir = base.home_dir().join(".yogurt").join("models");
    std::fs::create_dir_all(&dir)?;
    // The legacy `ProjectDirs` triple may appear only here.
    if let Some(project_dirs) = directories::ProjectDirs::from("com", "yogurt", "yogurt") {
        let old_models_dir = project_dirs.data_local_dir().join("models");
        migrate_legacy_model(&old_models_dir, &dir, spec.filename);
    }
    Ok(dir.join(spec.filename))
}

/// Read-only locations a model may also live in, checked after `~/.yogurt/models`.
///
/// This is the Homebrew companion-formula path: Homebrew's install sandbox cannot write to `$HOME`, so yogurt reads from the prefix instead.
/// `HOMEBREW_PREFIX` covers a custom prefix and the two literals are the Apple Silicon and Intel defaults.
/// `brew --prefix` would be authoritative but is a subprocess, which the one-process constraint forbids.
fn homebrew_model_dirs() -> Vec<PathBuf> {
    let mut prefixes: Vec<PathBuf> = Vec::with_capacity(3);
    if let Some(p) = std::env::var_os("HOMEBREW_PREFIX") {
        prefixes.push(PathBuf::from(p));
    }
    prefixes.push(PathBuf::from("/opt/homebrew"));
    prefixes.push(PathBuf::from("/usr/local"));
    prefixes
        .into_iter()
        .map(|p| p.join("share").join("yogurt").join("models"))
        .collect()
}

/// Where a verified copy of `spec` is, or `None`.
///
/// The user's download dir is searched first, then each [`homebrew_model_dirs`] entry.
/// Verification uses [`is_downloaded_at`], so a truncated file in the first location does not shadow a good copy in the second.
/// This is the single source for both `is_downloaded` and the path handed to whisper.
pub fn resolve_model(spec: &ModelSpec) -> Option<PathBuf> {
    let owned = model_path(spec).ok()?;
    let mut dirs = vec![owned.parent()?.to_path_buf()];
    dirs.extend(homebrew_model_dirs());
    resolve_in(&dirs, spec.filename, spec.sha256)
}

/// Path-injectable core of [`resolve_model`].
fn resolve_in(dirs: &[PathBuf], filename: &str, expected_sha256: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|d| d.join(filename))
        .find(|p| is_downloaded_at(p, expected_sha256))
}

/// `true` if `path` is yogurt's own download destination. A model resolved elsewhere is managed by whatever installed it (Homebrew), so the DELETE handler must refuse.
pub fn is_user_owned(path: &Path) -> bool {
    match directories::BaseDirs::new() {
        Some(base) => path.starts_with(base.home_dir().join(".yogurt").join("models")),
        None => false,
    }
}

/// Move `old_dir/filename` and its `.sha256` sidecar to `new_dir` exactly once, never clobbering.
///
/// `fs::rename` only: both dirs are under `$HOME` on one volume, so the move is atomic even for a 3 GB model.
/// Any failure is logged and ignored, and the SHA256-verified re-download path takes over.
fn migrate_legacy_model(old_dir: &Path, new_dir: &Path, filename: &str) {
    let new_path = new_dir.join(filename);
    if new_path.exists() {
        return; // never clobber an existing file at the new location
    }
    let old_path = old_dir.join(filename);
    if !old_path.exists() {
        return; // nothing to migrate
    }
    if let Err(e) = std::fs::rename(&old_path, &new_path) {
        tracing::warn!(
            ?old_path,
            ?new_path,
            %e,
            "legacy model migration: rename failed (re-download will recover)"
        );
        return;
    }
    let old_marker = marker_path(&old_path);
    if old_marker.exists() {
        if let Err(e) = std::fs::rename(&old_marker, marker_path(&new_path)) {
            tracing::warn!(
                ?old_marker,
                %e,
                "legacy model migration: sidecar rename failed (is_downloaded re-hashes)"
            );
        }
    }
}

/// `true` iff a copy of the model exists and verifies against `spec.sha256` (case-insensitive), in the user's dir or a Homebrew prefix.
/// A file that exists but fails verification counts as not downloaded, and so does any IO error.
///
/// The happy path does not hash: a sidecar `<filename>.sha256` marker (`<hash> <len>`) is compared against the spec hash and the file's byte length.
/// A missing, invalid or stale marker falls back to one full SHA256 that rewrites the marker on success.
/// Hashing a 3 GB model per call starved the tokio runtime.
pub fn is_downloaded(spec: &ModelSpec) -> bool {
    resolve_model(spec).is_some()
}

/// Path-injectable core of `is_downloaded`.
fn is_downloaded_at(path: &Path, expected_sha256: &str) -> bool {
    // Also yields the current length for the marker check.
    let len = match std::fs::metadata(path) {
        Ok(m) => m.len(),
        Err(_) => return false,
    };
    if let Some((hash, marker_len)) = read_marker(path) {
        if hash.eq_ignore_ascii_case(expected_sha256) && marker_len == len {
            return true;
        }
    }
    // Stale or missing marker: hash once and rewrite it on a match.
    match sha256::hash_file(path) {
        Ok(actual) if actual.eq_ignore_ascii_case(expected_sha256) => {
            write_marker(path, &actual);
            true
        }
        _ => false,
    }
}

/// `<model path>.sha256`, appended to the full file name (`with_extension` would replace `.bin`).
///
/// `pub` so the DELETE handler can remove the sidecar, which would otherwise fast-path `is_downloaded_at` against bytes that no longer exist.
pub fn marker_path(model: &Path) -> PathBuf {
    let mut s = model.as_os_str().to_os_string();
    s.push(".sha256");
    PathBuf::from(s)
}

/// Best-effort: record `<lowercase-hash> <len>` next to the model so later `is_downloaded` calls skip hashing.
///
/// In a non-writable Homebrew prefix every check would re-hash a multi-GB file, so the formula installs the `.sha256` sidecar itself.
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

/// Parse the sidecar marker into `(hash, len)`, or `None` on any failure.
fn read_marker(model: &Path) -> Option<(String, u64)> {
    let s = std::fs::read_to_string(marker_path(model)).ok()?;
    let mut parts = s.split_whitespace();
    let hash = parts.next()?.to_string();
    let len = parts.next()?.parse().ok()?;
    Some((hash, len))
}

/// Progress tick reported by `download_to`. `bytes_per_sec` and `eta_seconds` are EWMA'd over ~500 ms to reduce jitter.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    pub bytes_per_sec: u64,
    pub eta_seconds: Option<u64>,
}

/// Errors surfaced by `download_to` and `download`. `HashMismatch` carries both digests so the UI can show them.
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

/// Download `url` to `dest`, resuming a partial file via `Range: bytes=N-`, and verify SHA256 afterward.
/// On mismatch the file is deleted so a later call does not fast-path against corrupt state.
///
/// `on_progress` fires at a cadence of at least 500 ms and once at the end with `bytes_downloaded == total_bytes`.
///
/// If `dest` already verifies, it returns without touching the network.
/// Otherwise it sends `Range: bytes={len}-` from the existing length.
/// A server that replies 200 instead of 206 gets a truncate-overwrite from byte 0.
pub async fn download_to<F>(
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    mut on_progress: F,
) -> Result<(), DownloadError>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    // Hash on the blocking pool: `large-v3` is ~3 GB and an inline hash would block the tokio reactor.
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

    // Total is what the server will stream plus what is already on disk when resuming.
    let stream_len = resp.content_length().unwrap_or(0);
    let total_bytes = if resumed {
        existing_len + stream_len
    } else {
        stream_len
    };

    let mut file: std::fs::File = if resumed && existing_len > 0 {
        let mut f = std::fs::OpenOptions::new().append(true).open(dest)?;
        f.seek(SeekFrom::End(0))?;
        f
    } else {
        // No existing file, or the server ignored Range and sends the full body, so truncate.
        std::fs::File::create(dest)?
    };

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

    // Same reactor-blocking concern as the fast path.
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

/// Try `mirror_url` (if any) against `dest`, then fall back to `url`.
///
/// Path-injectable core of [`download`], so mirror-then-fallback is testable against a mock server.
/// The mirror is byte-identical, so the SHA256 check is the only correctness requirement.
/// A mirror failure falls through to `url` against the same `dest`, and the Range-resume picks up the bytes the mirror wrote.
async fn download_trying_mirror_first<F>(
    mirror_url: Option<&str>,
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    on_progress: F,
) -> Result<(), DownloadError>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    // `download_to` needs a `Send + 'static` callback, so the one `on_progress` is shared via `Arc<Mutex<_>>` across both attempts.
    let progress = std::sync::Arc::new(std::sync::Mutex::new(on_progress));
    if let Some(mirror_url) = mirror_url {
        let p = progress.clone();
        let cb = move |tick: DownloadProgress| {
            if let Ok(mut g) = p.lock() {
                g(tick);
            }
        };
        if download_to(mirror_url, dest, expected_sha256, cb)
            .await
            .is_ok()
        {
            return Ok(());
        }
    }
    let cb = move |tick: DownloadProgress| {
        if let Ok(mut g) = progress.lock() {
            g(tick);
        }
    };
    download_to(url, dest, expected_sha256, cb).await
}

/// Resolve `model_path(spec)` and download it, trying `spec.mirror_url` before `spec.url`.
pub async fn download<F>(spec: &ModelSpec, on_progress: F) -> Result<(), DownloadError>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    let path = model_path(spec)?;
    download_trying_mirror_first(spec.mirror_url, spec.url, &path, spec.sha256, on_progress).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_five_models_in_size_order() {
        assert_eq!(REGISTRY.len(), 5, "registry must contain exactly 5 models");
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
        assert_eq!(lookup("large-v3-turbo").unwrap().name, "large-v3-turbo");
        assert!(lookup("nonexistent").is_none());
        assert!(lookup("").is_none());
        assert!(lookup("Tiny.en").is_none());
    }

    #[test]
    fn intel_support_flags_match_prd() {
        assert!(lookup("tiny.en").unwrap().intel_supported);
        assert!(lookup("small.en").unwrap().intel_supported);
        assert!(!lookup("medium.en").unwrap().intel_supported);
        assert!(!lookup("large-v3").unwrap().intel_supported);
    }

    #[test]
    fn resolve_in_prefers_the_users_own_dir() {
        let own = tempfile::tempdir().unwrap();
        let brew = tempfile::tempdir().unwrap();
        let payload = b"the model bytes the registry pins";
        let expected = sha256::hash_bytes(payload);
        std::fs::write(own.path().join("ggml-test.bin"), payload).unwrap();
        std::fs::write(brew.path().join("ggml-test.bin"), payload).unwrap();
        let dirs = vec![own.path().to_path_buf(), brew.path().to_path_buf()];
        assert_eq!(
            resolve_in(&dirs, "ggml-test.bin", &expected),
            Some(own.path().join("ggml-test.bin")),
            "a copy in the user's own dir must win over a Homebrew copy"
        );
    }

    #[test]
    fn resolve_in_falls_through_to_homebrew() {
        let own = tempfile::tempdir().unwrap();
        let brew = tempfile::tempdir().unwrap();
        let payload = b"the model bytes the registry pins";
        let expected = sha256::hash_bytes(payload);
        std::fs::write(brew.path().join("ggml-test.bin"), payload).unwrap();
        let dirs = vec![own.path().to_path_buf(), brew.path().to_path_buf()];
        assert_eq!(
            resolve_in(&dirs, "ggml-test.bin", &expected),
            Some(brew.path().join("ggml-test.bin")),
            "with nothing downloaded, a Homebrew copy must resolve"
        );
    }

    #[test]
    fn resolve_in_skips_a_corrupt_copy_for_a_good_one() {
        let own = tempfile::tempdir().unwrap();
        let brew = tempfile::tempdir().unwrap();
        let payload = b"the model bytes the registry pins";
        let expected = sha256::hash_bytes(payload);
        // A truncated file in the user's dir must not shadow the Homebrew copy.
        std::fs::write(own.path().join("ggml-test.bin"), b"half a fi").unwrap();
        std::fs::write(brew.path().join("ggml-test.bin"), payload).unwrap();
        let dirs = vec![own.path().to_path_buf(), brew.path().to_path_buf()];
        assert_eq!(
            resolve_in(&dirs, "ggml-test.bin", &expected),
            Some(brew.path().join("ggml-test.bin")),
            "a corrupt file must not shadow a verified copy further down"
        );
    }

    #[test]
    fn resolve_in_returns_none_when_nothing_verifies() {
        let own = tempfile::tempdir().unwrap();
        let dirs = vec![own.path().to_path_buf()];
        assert_eq!(
            resolve_in(&dirs, "ggml-test.bin", &sha256::hash_bytes(b"anything")),
            None
        );
    }

    #[test]
    fn homebrew_dirs_cover_both_default_prefixes() {
        let dirs = homebrew_model_dirs();
        assert!(dirs.contains(&PathBuf::from("/opt/homebrew/share/yogurt/models")));
        assert!(dirs.contains(&PathBuf::from("/usr/local/share/yogurt/models")));
    }

    #[test]
    fn homebrew_paths_are_not_user_owned() {
        assert!(!is_user_owned(Path::new(
            "/opt/homebrew/share/yogurt/models/ggml-tiny.en.bin"
        )));
        let own = directories::BaseDirs::new()
            .unwrap()
            .home_dir()
            .join(".yogurt")
            .join("models")
            .join("ggml-tiny.en.bin");
        assert!(is_user_owned(&own));
    }

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
        // The expected hash is of different bytes, so a matching marker must win over hashing the file.
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
        // Right hash, wrong length.
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
            assert_eq!(
                spec.sha256,
                spec.sha256.to_ascii_lowercase(),
                "{} sha256 should be lowercase",
                spec.name
            );
        }
    }

    #[test]
    fn migration_moves_model_and_sidecar() {
        let tmp = tempfile::tempdir().unwrap();
        let old_dir = tmp.path().join("old");
        let new_dir = tmp.path().join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(old_dir.join("ggml-test.bin"), b"model bytes").unwrap();
        std::fs::write(old_dir.join("ggml-test.bin.sha256"), b"abc 11\n").unwrap();

        migrate_legacy_model(&old_dir, &new_dir, "ggml-test.bin");

        assert_eq!(
            std::fs::read(new_dir.join("ggml-test.bin")).unwrap(),
            b"model bytes"
        );
        assert_eq!(
            std::fs::read(new_dir.join("ggml-test.bin.sha256")).unwrap(),
            b"abc 11\n"
        );
        assert!(!old_dir.join("ggml-test.bin").exists());
        assert!(!old_dir.join("ggml-test.bin.sha256").exists());
    }

    #[test]
    fn migration_never_clobbers_existing_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let old_dir = tmp.path().join("old");
        let new_dir = tmp.path().join("new");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(old_dir.join("ggml-test.bin"), b"old bytes").unwrap();
        std::fs::write(new_dir.join("ggml-test.bin"), b"new bytes").unwrap();

        migrate_legacy_model(&old_dir, &new_dir, "ggml-test.bin");

        assert_eq!(
            std::fs::read(new_dir.join("ggml-test.bin")).unwrap(),
            b"new bytes",
            "existing file at the new path must never be overwritten"
        );
        assert_eq!(
            std::fs::read(old_dir.join("ggml-test.bin")).unwrap(),
            b"old bytes",
            "old file must be left untouched when the new path is occupied"
        );
    }

    #[test]
    fn migration_is_a_noop_when_old_dir_missing_or_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("new");
        std::fs::create_dir_all(&new_dir).unwrap();

        migrate_legacy_model(&tmp.path().join("nonexistent"), &new_dir, "ggml-test.bin");
        assert!(!new_dir.join("ggml-test.bin").exists());

        let empty_old = tmp.path().join("empty-old");
        std::fs::create_dir_all(&empty_old).unwrap();
        migrate_legacy_model(&empty_old, &new_dir, "ggml-test.bin");
        assert!(!new_dir.join("ggml-test.bin").exists());
    }

    /// Serve `payload` at `/ok` (200) and a 404 at `/missing`.
    async fn spawn_mirror_test_server(payload: Vec<u8>) -> String {
        use axum::{http::StatusCode, routing::get, Router};
        let app = Router::new()
            .route(
                "/ok",
                get(move || {
                    let payload = payload.clone();
                    async move { payload }
                }),
            )
            .route("/missing", get(|| async { StatusCode::NOT_FOUND }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn mirror_success_never_touches_the_fallback_url() {
        let payload = b"mirror bytes".to_vec();
        let expected = sha256::hash_bytes(&payload);
        let base = spawn_mirror_test_server(payload).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("ggml-test.bin");

        download_trying_mirror_first(
            Some(&format!("{base}/ok")),
            // Nothing listens here, so hitting the fallback would fail the test.
            "http://127.0.0.1:1/unreachable",
            &dest,
            &expected,
            |_| {},
        )
        .await
        .expect("mirror-only download should succeed");
        assert_eq!(sha256::hash_file(&dest).unwrap(), expected);
    }

    #[tokio::test]
    async fn mirror_404_falls_back_to_the_second_url() {
        let payload = b"fallback bytes".to_vec();
        let expected = sha256::hash_bytes(&payload);
        let base = spawn_mirror_test_server(payload).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("ggml-test.bin");

        download_trying_mirror_first(
            Some(&format!("{base}/missing")),
            &format!("{base}/ok"),
            &dest,
            &expected,
            |_| {},
        )
        .await
        .expect("fallback download should succeed after mirror 404s");
        assert_eq!(sha256::hash_file(&dest).unwrap(), expected);
    }

    #[tokio::test]
    async fn no_mirror_goes_straight_to_the_url() {
        let payload = b"only one source".to_vec();
        let expected = sha256::hash_bytes(&payload);
        let base = spawn_mirror_test_server(payload).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("ggml-test.bin");

        download_trying_mirror_first(None, &format!("{base}/ok"), &dest, &expected, |_| {})
            .await
            .expect("download should succeed with no mirror configured");
        assert_eq!(sha256::hash_file(&dest).unwrap(), expected);
    }

    #[test]
    fn large_v3_has_no_mirror_url() {
        assert_eq!(lookup("large-v3").unwrap().mirror_url, None);
    }

    #[test]
    fn mirrorable_models_have_a_mirror_url() {
        for name in ["tiny.en", "small.en", "medium.en", "large-v3-turbo"] {
            let spec = lookup(name).unwrap();
            assert!(
                spec.mirror_url.is_some(),
                "{name} should have a mirror_url pinned"
            );
        }
    }
}
