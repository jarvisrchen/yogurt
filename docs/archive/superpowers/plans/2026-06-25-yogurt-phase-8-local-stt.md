# Yogurt v1 — Phase 8: Local STT via whisper.cpp Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a fully-local STT path. Wire a second adapter onto the `yogurt-stt` trait — `WhisperLocal` powered by `whisper-rs` (Rust bindings to `whisper.cpp`) with Metal acceleration on Apple Silicon. Replace the "Coming soon" Local STT card in Settings with a working model picker + download dialog, and let `start_meeting()` switch between `DeepgramAdapter` (cloud) and `WhisperLocal` (local) based on the user's stored preference.

**Architecture:** `whisper.cpp` does not natively stream like Deepgram. We implement chunked decoding with a sliding-window + VAD strategy: incoming 16 kHz mono PCM frames feed a `webrtc-vad` instance; when a speech run is followed by a sustained silence, we slice the run and feed it to `whisper.cpp` for a `Final` event. In parallel a 1-second sliding window emits `Partial` events for the "still listening" feel — quality is worse than cloud streaming, accepted per PRD §13. Model files (`tiny.en` 75MB → `large-v3` 3GB) are downloaded from Hugging Face on first use into `~/.yogurt/models/`, SHA256-verified, with resume support and live progress fanned out over the existing meeting WebSocket.

**Tech Stack:** `whisper-rs = "0.13"` with `["metal"]` feature for Apple Silicon Metal acceleration · `webrtc-vad = "0.4"` for voice activity detection · `reqwest::Client` (workspace dep) for chunked download with resume · `sha2 = "0.10"` for hash verification · `directories = "5"` (Phase 5) for `~/.yogurt/models/` resolution · React 19 + TanStack Query 5 for the Settings Local card · native `<dialog>` element for the download modal.

**Reference:** `docs/PRD.md` §4 Q3 (pluggable STT — cloud default, local opt-in), §5.6 (Settings transcription card pair), §5.8 (Intel best-effort: `small.en` real-time only), §5.11 (whisper.cpp model download UI), §13 (whisper.cpp streaming partials worse than Deepgram is acceptable), §16.2 (matcha palette for the Local card chrome).

**Dependencies on prior phases:**
- **Phase 0** — Cargo workspace, `yogurt-server` crate, axum router scaffolding.
- **Phase 2** — `yogurt-audio` crate emitting 16 kHz mono PCM frames over a Tokio broadcast channel.
- **Phase 3** — `yogurt-stt` crate defining the `Stt` trait, `TranscriptEvent` enum (`{Partial, Final}`), `DeepgramAdapter`, and the `/ws/meetings/:id` `transcript` event shape.
- **Phase 5** — Settings DB table, `stt_provider` column (currently `"cloud"` only), TanStack Query client, the Local-STT card stubbed with a "Coming soon" badge, `directories::ProjectDirs` helper at `~/.yogurt/`, Keychain wiring (unused here — local STT has no secrets).
- **Phase 7** — Library + onboarding (not strictly required, but the Settings entry point is reached from the sidebar built in Phase 7).

**Out of scope (deferred):**
- Non-English models (`tiny`, `small`, `medium`, `large-v3-turbo` multilingual). v1 ships English-only `.en` models + `large-v3` for accuracy. Multilingual gets a follow-up phase.
- Word-level timestamps from whisper.cpp. Current `TranscriptEvent` shape carries `ts_ms` per segment; per-word offsets defer to v2.
- Streaming chunked Greedy decoding — whisper.cpp's experimental streaming example is fragile; we use the well-trodden sliding-window approach.
- Custom VAD models (Silero, etc.). `webrtc-vad` is good enough and pure-Rust binding-free.
- Diarization. Per PRD §2 non-goals.
- Auto-download on app launch. Download is explicitly user-triggered from Settings.
- GPU offload on Intel (CUDA / OpenCL). Intel Macs fall back to CPU.

---

## File structure produced by this phase

```
yogurt/
├── crates/
│   ├── yogurt-stt/
│   │   ├── Cargo.toml                        # MODIFY · add whisper-rs, webrtc-vad, sha2, directories, hf-hub-shaped deps
│   │   ├── src/
│   │   │   ├── lib.rs                        # MODIFY · pub use whisper_local; pub use models
│   │   │   ├── whisper_local.rs              # NEW · WhisperLocal impl of Stt trait
│   │   │   ├── vad.rs                        # NEW · webrtc-vad wrapper + sliding-window segmenter
│   │   │   ├── models.rs                     # NEW · model registry + download + SHA256 verify + resume
│   │   │   └── sha256.rs                     # NEW · streaming SHA256 helper
│   │   └── tests/
│   │       ├── vad_segmenter.rs              # NEW · TDD on VAD speech/silence segmentation (synthetic PCM)
│   │       ├── models_download.rs            # NEW · TDD on resume + SHA256 verify against mock HTTP server
│   │       └── whisper_smoke.rs              # NEW · #[ignore] manual smoke (requires real small.en file)
│   └── yogurt-server/
│       ├── Cargo.toml                        # MODIFY · add yogurt-stt feature wiring, futures-util for streaming
│       └── src/
│           ├── api/
│           │   └── stt_models.rs             # NEW · REST: GET list, POST download, DELETE
│           ├── api/mod.rs                    # MODIFY · mount stt_models routes
│           ├── ws.rs                         # MODIFY · emit stt_model_download_progress events
│           └── meetings/
│               └── start.rs                  # MODIFY · branch on stt_provider, instantiate WhisperLocal vs DeepgramAdapter
├── web/
│   └── src/
│       ├── lib/
│       │   └── api/
│       │       └── stt.ts                    # NEW · typed wrappers for /api/stt/models endpoints
│       ├── components/
│       │   ├── settings/
│       │   │   ├── Settings.tsx              # MODIFY · drop the "Coming soon" badge on the Local card
│       │   │   ├── LocalSTTCard.tsx          # NEW · replaces Phase-5 stub; matcha card with model picker
│       │   │   └── ModelPicker.tsx           # NEW · pill row (tiny.en · small.en ✓ · medium.en · large-v3 ↓)
│       │   └── dialogs/
│       │       └── ModelDownloadDialog.tsx   # NEW · matcha dialog per PRD §5.11
│       └── hooks/
│           └── useModelDownloadProgress.ts   # NEW · WS subscription → TanStack Query cache update
└── docs/
    └── superpowers/plans/
        └── 2026-06-25-yogurt-phase-8-local-stt.md   # this file
```

**Why this split:** `whisper_local.rs`, `vad.rs`, `models.rs`, `sha256.rs` are isolated under `yogurt-stt` so the trait crate stays cohesive — every STT-adjacent concern lives in one place. The REST handlers live in `yogurt-server` because they need DB access (for "which models are downloaded") and need the WS broadcaster for progress fanout. The frontend mirrors the Phase 5 settings folder convention. The download modal is a new dialog primitive under `components/dialogs/` because Phase 5 had no dialogs (settings is all inline cards).

---

## Test conventions reused from prior phases

- **Rust unit tests:** `#[cfg(test)] mod tests` inside the source file under test (continued from Phase 0).
- **Rust integration tests:** `crates/yogurt-stt/tests/<area>.rs` — one file per logical area. TDD-first: write the failing test, then the impl.
- **Mock HTTP for download tests:** spin up a local axum server inside the test that returns canned bytes + supports `Range:` headers, rather than mocking `reqwest` internals. This catches real range-request bugs.
- **Synthetic PCM for VAD tests:** generate sine waves at known amplitudes; silence is `vec![0i16; n]`. Cheap, deterministic, no audio files in git.
- **Real-model smoke test:** `whisper_smoke.rs` is `#[ignore]`d. Documentation explains how to run it: `RUN_WHISPER_SMOKE=1 cargo test -p yogurt-stt --test whisper_smoke -- --ignored --nocapture`. Requires `~/.yogurt/models/ggml-small.en.bin` to exist.
- **Frontend unit tests:** Vitest (`web/src/**/*.test.ts(x)`) — mock the WS hook with `vi.mock`, assert dialog states render correctly.
- **No E2E in this phase.** Playwright lands in Phase 9.

---

## Phase 8 task list

12 tasks. Each task ends with a commit. Approximate sequence: ~14–16 hours of focused work, fits the 2-day budget.

---

### Task 8.1 · Add whisper-rs + webrtc-vad + sha2 to `yogurt-stt`

**Files:**
- Modify: `crates/yogurt-stt/Cargo.toml`
- Modify: `Cargo.toml` (workspace deps)

- [ ] **Step 1: Confirm Xcode Command Line Tools are installed (required for Metal).**

Run: `xcode-select -p`
Expected: prints `/Library/Developer/CommandLineTools` or `/Applications/Xcode.app/Contents/Developer`. If missing: `xcode-select --install`.

Document this in the PR description: building `whisper-rs` with `metal` requires Xcode CLT for the Metal framework headers. Linux contributors can build without Metal (CPU only) — Phase 9 packaging covers cross-target.

- [ ] **Step 2: Add new deps to workspace `Cargo.toml` `[workspace.dependencies]`.**

```toml
whisper-rs = { version = "0.13", default-features = false }
webrtc-vad = "0.4"
sha2 = "0.10"
futures-util = "0.3"
hex = "0.4"
```

`whisper-rs` is declared without features at the workspace level; the consumer (`yogurt-stt`) opts into `metal` so the crate stays portable for hypothetical future server-only builds.

- [ ] **Step 3: Modify `crates/yogurt-stt/Cargo.toml` to consume them.**

Add to `[dependencies]`:

```toml
whisper-rs = { workspace = true, features = ["metal"] }
webrtc-vad = { workspace = true }
sha2 = { workspace = true }
hex = { workspace = true }
futures-util = { workspace = true }
directories = { workspace = true }   # already added in Phase 5 workspace deps
tokio = { workspace = true }
reqwest = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
async-trait = { workspace = true }   # already used by the Stt trait in Phase 3
anyhow = { workspace = true }
```

Add to `[dev-dependencies]`:

```toml
axum = { workspace = true }
tower = { workspace = true }
tempfile = "3"
```

(`tempfile` is a one-consumer dep, declared inline.)

- [ ] **Step 4: Verify the workspace compiles.**

Run: `cargo check -p yogurt-stt`
Expected: download of `whisper-rs` triggers a long C++ build (~2–5 min first time — whisper.cpp source). Subsequent builds cached. If Metal headers are missing the build will fail with a clear `metal/metal.h not found` — that's the signal to install Xcode CLT.

- [ ] **Step 5: Commit.**

```bash
git add Cargo.toml crates/yogurt-stt/Cargo.toml
git commit -m "build(stt): add whisper-rs (metal), webrtc-vad, sha2 to yogurt-stt"
```

---

### Task 8.2 · `sha256.rs` streaming hasher with TDD

**Files:**
- Create: `crates/yogurt-stt/src/sha256.rs`
- Modify: `crates/yogurt-stt/src/lib.rs`

- [ ] **Step 1: Write the failing unit tests inline.**

Create `crates/yogurt-stt/src/sha256.rs`:

```rust
//! Streaming SHA256 helper. Used by the model download flow to verify
//! integrity without holding the full 3 GB file in memory.

use sha2::{Digest, Sha256};
use std::io::Read;

/// Hash a file at `path`, returning the lowercase hex digest.
pub fn hash_file(path: &std::path::Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Hash an in-memory byte slice. Used in tests.
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
        assert_eq!(hash_bytes(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn it_hashes_short_input() {
        assert_eq!(hash_bytes(b"hello"), "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn it_hashes_a_file_matching_in_memory_hash() {
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.as_file().write_all(&payload).unwrap();
        let from_file = hash_file(tmp.path()).unwrap();
        assert_eq!(from_file, hash_bytes(&payload));
    }
}
```

- [ ] **Step 2: Wire the module into `lib.rs`.**

Append to `crates/yogurt-stt/src/lib.rs`:

```rust
pub mod sha256;
```

- [ ] **Step 3: Run — expect 3 passing tests.**

Run: `cargo test -p yogurt-stt sha256`
Expected: `test result: ok. 3 passed`.

- [ ] **Step 4: Commit.**

```bash
git add crates/yogurt-stt/src/sha256.rs crates/yogurt-stt/src/lib.rs
git commit -m "feat(stt): add streaming sha256 helper for model verification"
```

---

### Task 8.3 · `models.rs` registry with hardcoded URLs + expected SHA256

**Files:**
- Create: `crates/yogurt-stt/src/models.rs`
- Modify: `crates/yogurt-stt/src/lib.rs`

- [ ] **Step 1: Write `models.rs` with the static registry.**

```rust
//! Static registry of the four whisper.cpp models Yogurt ships support for.
//!
//! URLs point at ggerganov's canonical Hugging Face release of the ggml
//! quantized models. SHA256 hashes pinned from the 2026-06 release of
//! whisper.cpp/models. If ggerganov re-uploads, hashes must be re-pinned.
//!
//! Per PRD §5.8: Intel Macs are flagged "best-effort" beyond small.en.

use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ModelSpec {
    pub name: &'static str,
    pub filename: &'static str,
    pub size_mb: u32,
    pub url: &'static str,
    pub sha256: &'static str,
    /// Marketed as fully supported on Intel CPUs. Above this, we render a warning chip.
    pub intel_supported: bool,
}

pub const REGISTRY: &[ModelSpec] = &[
    ModelSpec {
        name: "tiny.en",
        filename: "ggml-tiny.en.bin",
        size_mb: 75,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        // SHA256 from ggerganov/whisper.cpp release, pinned 2026-06-24.
        sha256: "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
        intel_supported: true,
    },
    ModelSpec {
        name: "small.en",
        filename: "ggml-small.en.bin",
        size_mb: 487,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        sha256: "f953ad0fd29cacd07d5a9eef5c1e3778e8759bd9e2d8c9d4eecfae04d2b34d1f",
        intel_supported: true,
    },
    ModelSpec {
        name: "medium.en",
        filename: "ggml-medium.en.bin",
        size_mb: 1530,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin",
        sha256: "cc37e93478338ec7700281a7ac30a10128929eb8f427dda2e865faa8f6da4356",
        intel_supported: false,
    },
    ModelSpec {
        name: "large-v3",
        filename: "ggml-large-v3.bin",
        size_mb: 3094,
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        intel_supported: false,
    },
];

/// **WARNING:** the SHA256 values above are placeholders pinned to a known
/// snapshot. Before merging Phase 8, re-fetch each model and replace the
/// `sha256` entries with the actual digests (use `shasum -a 256 <file>`
/// after a clean download). The download flow is hard-failure on mismatch,
/// so wrong hashes will make the feature 100% broken.
pub fn lookup(name: &str) -> Option<&'static ModelSpec> {
    REGISTRY.iter().find(|m| m.name == name)
}

/// Resolve the on-disk path for a model.
pub fn model_path(spec: &ModelSpec) -> std::io::Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "yogurt", "yogurt")
        .ok_or_else(|| std::io::Error::other("no project dirs"))?;
    // PRD §5.6 + §5.11 say `~/.yogurt/models/`. Phase 5 sets the
    // base dir to ~/.yogurt; this just appends `models/`.
    let base = dirs.data_local_dir();
    let dir = base.join("models");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(spec.filename))
}

/// True if the model file exists on disk and matches its pinned SHA256.
/// A file that exists but doesn't verify is treated as not downloaded —
/// the next download attempt will overwrite it.
pub fn is_downloaded(spec: &ModelSpec) -> bool {
    let Ok(path) = model_path(spec) else { return false; };
    if !path.exists() { return false; }
    match crate::sha256::hash_file(&path) {
        Ok(h) => h.eq_ignore_ascii_case(spec.sha256),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_four_models_in_size_order() {
        assert_eq!(REGISTRY.len(), 4);
        let sizes: Vec<u32> = REGISTRY.iter().map(|m| m.size_mb).collect();
        let mut sorted = sizes.clone();
        sorted.sort();
        assert_eq!(sizes, sorted, "registry should be ordered smallest → largest");
    }

    #[test]
    fn lookup_finds_known_models_and_misses_unknown() {
        assert_eq!(lookup("small.en").unwrap().name, "small.en");
        assert_eq!(lookup("large-v3").unwrap().name, "large-v3");
        assert!(lookup("nonexistent").is_none());
    }

    #[test]
    fn intel_support_flags_match_prd() {
        // PRD §5.8: tiny.en + small.en are fully supported on Intel; medium.en/large-v3 warn.
        assert!(lookup("tiny.en").unwrap().intel_supported);
        assert!(lookup("small.en").unwrap().intel_supported);
        assert!(!lookup("medium.en").unwrap().intel_supported);
        assert!(!lookup("large-v3").unwrap().intel_supported);
    }

    #[test]
    fn sha256_values_are_hex_64_chars() {
        for m in REGISTRY {
            assert_eq!(m.sha256.len(), 64, "{}: sha256 must be 64 hex chars", m.name);
            assert!(m.sha256.chars().all(|c| c.is_ascii_hexdigit()), "{}: non-hex char", m.name);
        }
    }
}
```

- [ ] **Step 2: Wire into `lib.rs`.**

Append:

```rust
pub mod models;
```

- [ ] **Step 3: Run.**

Run: `cargo test -p yogurt-stt models`
Expected: 4 passing tests.

- [ ] **Step 4: Commit.**

```bash
git add crates/yogurt-stt/src/models.rs crates/yogurt-stt/src/lib.rs
git commit -m "feat(stt): add whisper.cpp model registry with pinned SHA256"
```

---

### Task 8.4 · Download + resume + SHA256 verify with mock-HTTP TDD

**Files:**
- Modify: `crates/yogurt-stt/src/models.rs` (add `download` fn + progress callback)
- Create: `crates/yogurt-stt/tests/models_download.rs`

- [ ] **Step 1: Write the failing integration test against a real (local) HTTP server.**

```rust
// crates/yogurt-stt/tests/models_download.rs
//
// Spins up a tiny axum server that serves a fixed payload and respects the
// Range: header. Verifies: (1) full download succeeds + hash matches,
// (2) resume from partial file completes correctly, (3) hash mismatch is
// reported as an error and the file is deleted.

use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use yogurt_stt::models::{self, DownloadProgress};
use yogurt_stt::sha256::hash_bytes;

fn payload() -> Vec<u8> {
    (0..600_000u32).map(|i| (i % 251) as u8).collect()
}

async fn handler(req: Request) -> Response {
    let body = payload();
    let total = body.len();
    if let Some(range) = req.headers().get(header::RANGE) {
        // Parse `bytes=START-`
        let s = range.to_str().unwrap();
        let start: usize = s
            .strip_prefix("bytes=").unwrap()
            .split('-').next().unwrap()
            .parse().unwrap();
        Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_RANGE, format!("bytes {start}-{}/{total}", total - 1))
            .header(header::CONTENT_LENGTH, total - start)
            .body(Body::from(body[start..].to_vec()))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_LENGTH, total)
            .header(header::ACCEPT_RANGES, "bytes")
            .body(Body::from(body))
            .unwrap()
    }
}

async fn spawn_server() -> SocketAddr {
    let app = Router::new().route("/file.bin", get(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    addr
}

#[tokio::test]
async fn it_downloads_a_full_file_and_verifies_sha256() {
    let addr = spawn_server().await;
    let url = format!("http://{addr}/file.bin");
    let expected_sha = hash_bytes(&payload());

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("file.bin");

    let progress = Arc::new(Mutex::new(Vec::<DownloadProgress>::new()));
    let progress_clone = progress.clone();

    models::download_to(&url, &dest, &expected_sha, move |p| {
        let progress_clone = progress_clone.clone();
        tokio::spawn(async move { progress_clone.lock().await.push(p); });
    })
    .await
    .expect("download succeeds");

    let actual_sha = yogurt_stt::sha256::hash_file(&dest).unwrap();
    assert_eq!(actual_sha, expected_sha);
    let updates = progress.lock().await;
    assert!(!updates.is_empty(), "progress callback fired at least once");
    let last = updates.last().unwrap();
    assert_eq!(last.bytes_downloaded, last.total_bytes);
}

#[tokio::test]
async fn it_resumes_from_a_partial_file() {
    let addr = spawn_server().await;
    let url = format!("http://{addr}/file.bin");
    let expected_sha = hash_bytes(&payload());

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("file.bin");

    // Pre-populate with the first 100KB.
    std::fs::write(&dest, &payload()[..100_000]).unwrap();

    models::download_to(&url, &dest, &expected_sha, |_p| {})
        .await
        .expect("resume succeeds");

    let actual_sha = yogurt_stt::sha256::hash_file(&dest).unwrap();
    assert_eq!(actual_sha, expected_sha);
}

#[tokio::test]
async fn it_rejects_a_bad_sha_and_removes_the_file() {
    let addr = spawn_server().await;
    let url = format!("http://{addr}/file.bin");

    let tmp = tempfile::tempdir().unwrap();
    let dest = tmp.path().join("file.bin");

    let err = models::download_to(&url, &dest, "deadbeef".repeat(8).as_str(), |_p| {})
        .await
        .expect_err("bad sha should error");
    let msg = format!("{err}");
    assert!(msg.contains("sha256") || msg.contains("hash"), "error should mention hash: {msg}");
    assert!(!dest.exists(), "bad-hash file must be removed");
}
```

- [ ] **Step 2: Run — expect compile failure (`download_to`, `DownloadProgress` don't exist).**

Run: `cargo test -p yogurt-stt --test models_download`
Expected: `error[E0432]: unresolved import` for `download_to` and `DownloadProgress`.

- [ ] **Step 3: Implement the download fn in `models.rs`.**

Append to `crates/yogurt-stt/src/models.rs`:

```rust
use futures_util::StreamExt;
use serde::Deserialize;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub bytes_downloaded: u64,
    pub total_bytes: u64,
    /// Bytes per second over the last second.
    pub bytes_per_sec: u64,
    /// Estimated seconds remaining; `None` until at least 2 progress ticks.
    pub eta_seconds: Option<u64>,
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sha256 mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("download cancelled")]
    Cancelled,
}

/// Download `url` → `dest`, resuming from an existing partial file via `Range:`,
/// emitting progress through the callback, and verifying SHA256 on completion.
/// On hash mismatch the file is deleted. Idempotent: if the file already
/// exists and verifies, returns immediately.
pub async fn download_to<F>(
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    mut on_progress: F,
) -> Result<(), DownloadError>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    // Already-good fast path.
    if dest.exists() {
        if let Ok(h) = crate::sha256::hash_file(dest) {
            if h.eq_ignore_ascii_case(expected_sha256) {
                return Ok(());
            }
        }
    }

    let existing = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
    let client = reqwest::Client::builder()
        .user_agent("yogurt-stt/0.1")
        .build()?;

    let mut req = client.get(url);
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let resp = req.send().await?.error_for_status()?;
    let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let content_len = resp.content_length().unwrap_or(0);
    let total = if resumed { existing + content_len } else { content_len };

    let mut file = if resumed {
        use std::io::Seek;
        let mut f = std::fs::OpenOptions::new().append(true).open(dest)?;
        f.seek(std::io::SeekFrom::End(0))?;
        f
    } else {
        std::fs::File::create(dest)?
    };

    let mut downloaded: u64 = if resumed { existing } else { 0 };
    let mut last_tick = Instant::now();
    let mut bytes_since_tick: u64 = 0;
    let mut last_rate: u64 = 0;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        use std::io::Write;
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        bytes_since_tick += chunk.len() as u64;

        let elapsed = last_tick.elapsed();
        if elapsed.as_millis() >= 500 {
            let rate = (bytes_since_tick as f64 / elapsed.as_secs_f64()) as u64;
            last_rate = rate;
            let eta = if rate > 0 && total > downloaded {
                Some((total - downloaded) / rate.max(1))
            } else {
                None
            };
            on_progress(DownloadProgress {
                bytes_downloaded: downloaded,
                total_bytes: total,
                bytes_per_sec: rate,
                eta_seconds: eta,
            });
            last_tick = Instant::now();
            bytes_since_tick = 0;
        }
    }
    // Final tick so the UI sees 100%.
    on_progress(DownloadProgress {
        bytes_downloaded: downloaded,
        total_bytes: total,
        bytes_per_sec: last_rate,
        eta_seconds: Some(0),
    });
    file.sync_all()?;
    drop(file);

    // Verify.
    let actual = crate::sha256::hash_file(dest)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = std::fs::remove_file(dest);
        return Err(DownloadError::HashMismatch {
            expected: expected_sha256.to_string(),
            actual,
        });
    }
    Ok(())
}

/// Convenience that resolves the on-disk path from the registry.
pub async fn download<F>(spec: &ModelSpec, on_progress: F) -> Result<(), DownloadError>
where
    F: FnMut(DownloadProgress) + Send + 'static,
{
    let dest = model_path(spec)?;
    download_to(spec.url, &dest, spec.sha256, on_progress).await
}
```

Make sure `thiserror` is in the workspace deps already (Phase 3 added it for `Stt` trait errors). If not, add `thiserror = "1"` to `[workspace.dependencies]` and re-declare under `yogurt-stt`.

- [ ] **Step 4: Run tests — expect PASS.**

Run: `cargo test -p yogurt-stt --test models_download`
Expected: 3 passing tests. The resume test confirms `Range:` works end-to-end; the bad-hash test confirms cleanup.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-stt/src/models.rs crates/yogurt-stt/tests/models_download.rs
git commit -m "feat(stt): add model download with resume, progress, and sha256 verify"
```

---

### Task 8.5 · VAD segmenter with TDD

**Files:**
- Create: `crates/yogurt-stt/src/vad.rs`
- Create: `crates/yogurt-stt/tests/vad_segmenter.rs`
- Modify: `crates/yogurt-stt/src/lib.rs`

- [ ] **Step 1: Sketch the public API in the failing test.**

```rust
// crates/yogurt-stt/tests/vad_segmenter.rs
//
// Drives the VAD segmenter with synthetic PCM and asserts that
// speech/silence transitions produce the expected segment boundaries.

use yogurt_stt::vad::{Segmenter, SegmenterEvent};

/// 16 kHz mono 16-bit PCM: 1 second of a 440 Hz sine at moderate amplitude.
fn tone(seconds: f32) -> Vec<i16> {
    let n = (16_000.0 * seconds) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / 16_000.0;
            ((t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 16_384.0) as i16
        })
        .collect()
}

fn silence(seconds: f32) -> Vec<i16> {
    vec![0i16; (16_000.0 * seconds) as usize]
}

#[test]
fn it_emits_one_segment_for_speech_then_silence() {
    let mut seg = Segmenter::new(16_000);
    let mut events = vec![];

    seg.push(&tone(1.5), |e| events.push(e));
    seg.push(&silence(1.0), |e| events.push(e));

    // Expect SpeechStart at frame 0 and SpeechEnd somewhere after ~1.5s of audio,
    // followed by exactly one Segment carrying the PCM.
    let segments: Vec<_> = events.iter().filter_map(|e| match e {
        SegmenterEvent::Segment { pcm, start_ms, end_ms } => Some((pcm.len(), *start_ms, *end_ms)),
        _ => None,
    }).collect();

    assert_eq!(segments.len(), 1, "exactly one segment for one speech run");
    let (samples, start_ms, end_ms) = segments[0];
    assert!(samples > 16_000, "segment should be at least 1s of samples, got {samples}");
    assert!(end_ms > start_ms);
    assert!(end_ms - start_ms >= 1000);
}

#[test]
fn it_splits_two_speech_runs_separated_by_silence() {
    let mut seg = Segmenter::new(16_000);
    let mut events = vec![];

    seg.push(&tone(0.8), |e| events.push(e));
    seg.push(&silence(0.8), |e| events.push(e));
    seg.push(&tone(0.8), |e| events.push(e));
    seg.push(&silence(0.8), |e| events.push(e));

    let n_segments = events.iter().filter(|e| matches!(e, SegmenterEvent::Segment { .. })).count();
    assert_eq!(n_segments, 2);
}

#[test]
fn pure_silence_emits_no_segments() {
    let mut seg = Segmenter::new(16_000);
    let mut events = vec![];
    seg.push(&silence(3.0), |e| events.push(e));
    let n_segments = events.iter().filter(|e| matches!(e, SegmenterEvent::Segment { .. })).count();
    assert_eq!(n_segments, 0);
}

#[test]
fn very_short_speech_blips_are_ignored() {
    // <200ms blip should not produce a segment (avoids false-positive cough/keyboard click).
    let mut seg = Segmenter::new(16_000);
    let mut events = vec![];
    seg.push(&tone(0.1), |e| events.push(e));
    seg.push(&silence(1.0), |e| events.push(e));
    let n_segments = events.iter().filter(|e| matches!(e, SegmenterEvent::Segment { .. })).count();
    assert_eq!(n_segments, 0);
}
```

- [ ] **Step 2: Run — expect compile failure (no `vad` module).**

Run: `cargo test -p yogurt-stt --test vad_segmenter`
Expected: `error[E0432]: unresolved import 'yogurt_stt::vad'`.

- [ ] **Step 3: Implement `vad.rs`.**

```rust
//! VAD-based segmenter. Wraps webrtc-vad and buffers frames between
//! speech-start and a sustained silence threshold to emit complete
//! `Segment` events suitable for batch decoding by whisper.cpp.
//!
//! webrtc-vad operates on 10/20/30 ms frames at 8/16/32/48 kHz. We standardize
//! on **30 ms frames at 16 kHz** (480 samples per frame) — matches whisper.cpp's
//! preferred sample rate and gives the most stable VAD decisions.

use webrtc_vad::{Vad, VadMode, SampleRate};

pub const FRAME_MS: usize = 30;
pub const SAMPLE_RATE_HZ: usize = 16_000;
pub const FRAME_SAMPLES: usize = SAMPLE_RATE_HZ * FRAME_MS / 1000; // 480

/// Minimum speech run before we emit a segment. Filters out coughs/clicks.
const MIN_SPEECH_MS: usize = 250;
/// Trailing silence required to close a segment. Tuned to avoid clipping the
/// last word; matches Granola's perceived end-of-utterance feel.
const SILENCE_HANG_MS: usize = 600;
/// Hard cap on segment length so whisper.cpp decode time stays bounded.
const MAX_SEGMENT_MS: usize = 25_000;

pub enum SegmenterEvent {
    SpeechStart { at_ms: u64 },
    /// A complete utterance, ready to hand to whisper.cpp.
    Segment { pcm: Vec<i16>, start_ms: u64, end_ms: u64 },
}

pub struct Segmenter {
    vad: Vad,
    sample_rate: usize,
    leftover: Vec<i16>,
    in_speech: bool,
    speech_start_ms: u64,
    silence_ms: usize,
    speech_ms: usize,
    cursor_ms: u64,
    buffer: Vec<i16>,
}

impl Segmenter {
    pub fn new(sample_rate: usize) -> Self {
        let sr = match sample_rate {
            8_000 => SampleRate::Rate8kHz,
            16_000 => SampleRate::Rate16kHz,
            32_000 => SampleRate::Rate32kHz,
            48_000 => SampleRate::Rate48kHz,
            _ => SampleRate::Rate16kHz,
        };
        let mut vad = Vad::new_with_rate_and_mode(sr, VadMode::Aggressive);
        // Quality knob — Aggressive is most appropriate for noisy meeting audio.
        // The other modes (Quality, LowBitrate, VeryAggressive) over- or under-trigger.
        let _ = &mut vad;
        Self {
            vad,
            sample_rate,
            leftover: Vec::with_capacity(FRAME_SAMPLES),
            in_speech: false,
            speech_start_ms: 0,
            silence_ms: 0,
            speech_ms: 0,
            cursor_ms: 0,
            buffer: Vec::with_capacity(SAMPLE_RATE_HZ * 30), // 30s reserve
        }
    }

    /// Feed a chunk of mono i16 PCM at the configured sample rate.
    /// `emit` is called for each event in order.
    pub fn push(&mut self, pcm: &[i16], mut emit: impl FnMut(SegmenterEvent)) {
        // Concatenate with leftover from a prior partial frame.
        self.leftover.extend_from_slice(pcm);
        let frame_n = FRAME_SAMPLES;
        let mut i = 0;
        while i + frame_n <= self.leftover.len() {
            let frame = &self.leftover[i..i + frame_n];
            self.process_frame(frame, &mut emit);
            i += frame_n;
        }
        // Keep the trailing partial frame for next push.
        self.leftover.drain(..i);
    }

    fn process_frame(&mut self, frame: &[i16], emit: &mut dyn FnMut(SegmenterEvent)) {
        let is_voice = self.vad.is_voice_segment(frame).unwrap_or(false);
        self.buffer.extend_from_slice(frame);
        self.cursor_ms += FRAME_MS as u64;

        if is_voice {
            self.silence_ms = 0;
            if !self.in_speech {
                self.in_speech = true;
                self.speech_start_ms = self.cursor_ms - FRAME_MS as u64;
                emit(SegmenterEvent::SpeechStart { at_ms: self.speech_start_ms });
                // Trim the buffer to start at the speech onset.
                self.buffer = frame.to_vec();
            }
            self.speech_ms += FRAME_MS;
        } else if self.in_speech {
            self.silence_ms += FRAME_MS;
        }

        let speech_too_long = self.speech_ms >= MAX_SEGMENT_MS;
        let silence_long_enough = self.silence_ms >= SILENCE_HANG_MS;

        if self.in_speech && (silence_long_enough || speech_too_long) {
            if self.speech_ms >= MIN_SPEECH_MS {
                let pcm = std::mem::take(&mut self.buffer);
                emit(SegmenterEvent::Segment {
                    pcm,
                    start_ms: self.speech_start_ms,
                    end_ms: self.cursor_ms,
                });
            } else {
                self.buffer.clear();
            }
            self.in_speech = false;
            self.silence_ms = 0;
            self.speech_ms = 0;
        }

        // Bound buffer growth during long silences (we wouldn't have emitted yet
        // but we also shouldn't accumulate forever).
        if !self.in_speech && self.buffer.len() > self.sample_rate * 2 {
            self.buffer.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn segmenter_constants_are_consistent() {
        assert_eq!(FRAME_SAMPLES, 480);
    }
}
```

- [ ] **Step 4: Wire `vad` into `lib.rs`.**

Append:

```rust
pub mod vad;
```

- [ ] **Step 5: Run all tests.**

Run: `cargo test -p yogurt-stt`
Expected: VAD test file passes 4/4, sha256 passes 3/3, models passes 4/4, download passes 3/3. The `very_short_speech_blips_are_ignored` test may need slight tuning of `MIN_SPEECH_MS` depending on webrtc-vad's behavior on pure tones (which have very different envelopes than human speech). If it fails because VAD sees the tone as silence, swap synthetic tone for white noise or accept that the test asserts the boundary condition rather than the VAD's exact response to sine waves — the comment should make that clear.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-stt/src/vad.rs crates/yogurt-stt/src/lib.rs crates/yogurt-stt/tests/vad_segmenter.rs
git commit -m "feat(stt): add webrtc-vad-based sliding-window segmenter"
```

---

### Task 8.6 · `WhisperLocal` adapter implementing the `Stt` trait

**Files:**
- Create: `crates/yogurt-stt/src/whisper_local.rs`
- Modify: `crates/yogurt-stt/src/lib.rs`
- Create: `crates/yogurt-stt/tests/whisper_smoke.rs`

- [ ] **Step 1: Re-read the `Stt` trait from Phase 3 to confirm signatures.**

Run: `cat crates/yogurt-stt/src/lib.rs` (the part defining the trait — Phase 3 added it).
Expected:

```rust
#[async_trait::async_trait]
pub trait Stt: Send + Sync {
    async fn start(
        &self,
        audio_rx: tokio::sync::broadcast::Receiver<AudioFrame>,
        tx: tokio::sync::mpsc::Sender<TranscriptEvent>,
    ) -> anyhow::Result<()>;
}

pub struct AudioFrame { pub channel: Channel, pub pcm: Vec<i16>, pub ts_ms: u64 }
pub enum Channel { Mic, System }
pub enum TranscriptEvent { Partial { ... }, Final { ... } }
```

If the exact signatures differ in Phase 3, mirror them precisely — `WhisperLocal` MUST be a drop-in `dyn Stt`.

- [ ] **Step 2: Write `whisper_local.rs`.**

```rust
//! Local STT via whisper.cpp (whisper-rs bindings).
//!
//! Streaming strategy: each audio channel runs its own VAD segmenter.
//! On `SegmenterEvent::Segment`, the PCM is handed to a worker task that
//! calls whisper.cpp and emits a `TranscriptEvent::Final`. Independently,
//! every ~1s, the most recent 5s of buffered audio per channel is run
//! through whisper.cpp at a faster decoding setting and emitted as
//! `TranscriptEvent::Partial` for the "still listening" indicator.
//!
//! Per PRD §13 the partial quality is openly worse than Deepgram — it's
//! the privacy escape hatch, not the daily driver.

use crate::vad::{Segmenter, SegmenterEvent};
use crate::{AudioFrame, Channel, Stt, TranscriptEvent};
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct WhisperLocal {
    model_path: PathBuf,
    /// Heavyweight; share across segments.
    ctx: Arc<WhisperContext>,
}

impl WhisperLocal {
    pub fn load(model_path: PathBuf) -> Result<Self> {
        if !model_path.exists() {
            return Err(anyhow!("whisper model not found at {}", model_path.display()));
        }
        let ctx = WhisperContext::new_with_params(
            model_path.to_str().context("model path is not utf-8")?,
            WhisperContextParameters::default(),
        )
        .context("loading whisper model")?;
        Ok(Self {
            model_path,
            ctx: Arc::new(ctx),
        })
    }

    /// Run whisper.cpp on a PCM segment. Returns the joined transcribed text.
    /// `fast` cuts beam search width — used for the partial-window decoder.
    fn decode(ctx: &WhisperContext, pcm_i16: &[i16], fast: bool) -> Result<String> {
        // whisper-rs wants f32 in [-1.0, 1.0].
        let mut f32_buf = vec![0.0f32; pcm_i16.len()];
        whisper_rs::convert_integer_to_float_audio(pcm_i16, &mut f32_buf)
            .context("pcm conversion")?;

        let mut state = ctx.create_state().context("create whisper state")?;
        let mut params = if fast {
            FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
        } else {
            FullParams::new(SamplingStrategy::BeamSearch { beam_size: 5, patience: 1.0 })
        };
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_no_context(fast); // partials: no cross-segment carry; finals: yes
        // Run.
        state.full(params, &f32_buf).context("whisper decode")?;

        let n_segments = state.full_n_segments().context("n_segments")?;
        let mut out = String::new();
        for i in 0..n_segments {
            let s = state.full_get_segment_text(i).context("segment text")?;
            out.push_str(&s);
        }
        Ok(out.trim().to_string())
    }
}

#[async_trait::async_trait]
impl Stt for WhisperLocal {
    async fn start(
        &self,
        mut audio_rx: broadcast::Receiver<AudioFrame>,
        tx: mpsc::Sender<TranscriptEvent>,
    ) -> Result<()> {
        let ctx_mic = self.ctx.clone();
        let ctx_sys = self.ctx.clone();
        let ctx_partial = self.ctx.clone();

        // Per-channel segmenter + final-decode pipeline.
        let (mic_seg_tx, mut mic_seg_rx) = mpsc::channel::<(Vec<i16>, u64, u64)>(8);
        let (sys_seg_tx, mut sys_seg_rx) = mpsc::channel::<(Vec<i16>, u64, u64)>(8);

        let tx_mic = tx.clone();
        tokio::spawn(async move {
            while let Some((pcm, start_ms, end_ms)) = mic_seg_rx.recv().await {
                let ctx = ctx_mic.clone();
                let tx2 = tx_mic.clone();
                let text = tokio::task::spawn_blocking(move || Self::decode(&ctx, &pcm, false))
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or_default();
                if text.is_empty() { continue; }
                let _ = tx2
                    .send(TranscriptEvent::Final {
                        channel: Channel::Mic,
                        text,
                        ts_ms: start_ms,
                        end_ms,
                    })
                    .await;
            }
        });

        let tx_sys = tx.clone();
        tokio::spawn(async move {
            while let Some((pcm, start_ms, end_ms)) = sys_seg_rx.recv().await {
                let ctx = ctx_sys.clone();
                let tx2 = tx_sys.clone();
                let text = tokio::task::spawn_blocking(move || Self::decode(&ctx, &pcm, false))
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or_default();
                if text.is_empty() { continue; }
                let _ = tx2
                    .send(TranscriptEvent::Final {
                        channel: Channel::System,
                        text,
                        ts_ms: start_ms,
                        end_ms,
                    })
                    .await;
            }
        });

        // Partial-window decoder: every 1s, decode the last 5s of mic audio.
        // Uses a separate, capped rolling buffer; partials on system audio
        // skipped in v1 to halve whisper.cpp pressure (TODO: revisit per perf).
        let partial_buf: Arc<tokio::sync::Mutex<Vec<i16>>> =
            Arc::new(tokio::sync::Mutex::new(Vec::with_capacity(16_000 * 5)));
        let partial_buf_writer = partial_buf.clone();
        let tx_partial = tx.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(1000));
            ticker.tick().await; // skip first immediate tick
            loop {
                ticker.tick().await;
                let snapshot = {
                    let buf = partial_buf.lock().await;
                    if buf.len() < 16_000 { continue; } // need ≥1s
                    buf.clone()
                };
                let ctx = ctx_partial.clone();
                let text = tokio::task::spawn_blocking(move || Self::decode(&ctx, &snapshot, true))
                    .await
                    .ok()
                    .and_then(|r| r.ok())
                    .unwrap_or_default();
                if text.is_empty() { continue; }
                let _ = tx_partial
                    .send(TranscriptEvent::Partial {
                        channel: Channel::Mic,
                        text,
                    })
                    .await;
            }
        });

        // Main pump: split incoming frames by channel into the two segmenters,
        // and feed the partial rolling buffer.
        let mut mic_seg = Segmenter::new(16_000);
        let mut sys_seg = Segmenter::new(16_000);

        loop {
            match audio_rx.recv().await {
                Ok(frame) => {
                    match frame.channel {
                        Channel::Mic => {
                            // Update rolling partial buffer (last 5s).
                            {
                                let mut buf = partial_buf_writer.lock().await;
                                buf.extend_from_slice(&frame.pcm);
                                let max = 16_000 * 5;
                                if buf.len() > max {
                                    let excess = buf.len() - max;
                                    buf.drain(..excess);
                                }
                            }
                            let tx_seg = mic_seg_tx.clone();
                            mic_seg.push(&frame.pcm, |e| {
                                if let SegmenterEvent::Segment { pcm, start_ms, end_ms } = e {
                                    let _ = tx_seg.try_send((pcm, start_ms, end_ms));
                                }
                            });
                        }
                        Channel::System => {
                            let tx_seg = sys_seg_tx.clone();
                            sys_seg.push(&frame.pcm, |e| {
                                if let SegmenterEvent::Segment { pcm, start_ms, end_ms } = e {
                                    let _ = tx_seg.try_send((pcm, start_ms, end_ms));
                                }
                            });
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(?n, "whisper_local audio rx lagged; dropping");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("audio channel closed; whisper_local exiting");
                    break;
                }
            }
        }
        Ok(())
    }
}
```

If the Phase 3 `TranscriptEvent::Final` variant has different fields (e.g., no `end_ms`), strip the mismatch — the goal is a drop-in adapter, not a schema change.

- [ ] **Step 3: Wire into `lib.rs`.**

Append:

```rust
pub mod whisper_local;
pub use whisper_local::WhisperLocal;
```

- [ ] **Step 4: Write the `#[ignore]`d smoke test.**

```rust
// crates/yogurt-stt/tests/whisper_smoke.rs
//
// Manual smoke test — requires ~/.yogurt/models/ggml-small.en.bin to exist.
// Run with: RUN_WHISPER_SMOKE=1 cargo test -p yogurt-stt --test whisper_smoke -- --ignored --nocapture

use std::path::PathBuf;
use std::time::Duration;
use yogurt_stt::{AudioFrame, Channel, Stt, TranscriptEvent, WhisperLocal};
use tokio::sync::{broadcast, mpsc};

#[tokio::test]
#[ignore]
async fn it_transcribes_a_sine_wave_run_without_crashing() {
    if std::env::var("RUN_WHISPER_SMOKE").is_err() {
        eprintln!("set RUN_WHISPER_SMOKE=1 to run this test");
        return;
    }
    let model = directories::ProjectDirs::from("com", "yogurt", "yogurt")
        .unwrap()
        .data_local_dir()
        .join("models/ggml-small.en.bin");
    assert!(model.exists(), "model not at {}", model.display());

    let stt = WhisperLocal::load(model).expect("load model");
    let (audio_tx, audio_rx) = broadcast::channel::<AudioFrame>(64);
    let (event_tx, mut event_rx) = mpsc::channel::<TranscriptEvent>(32);

    let stt = std::sync::Arc::new(stt);
    let stt_clone = stt.clone();
    let runner = tokio::spawn(async move { stt_clone.start(audio_rx, event_tx).await });

    // Push 3s of silence followed by 2s of "speech" (sine wave, will likely
    // transcribe as empty or noise — purpose is to confirm we don't crash).
    let silence = vec![0i16; 16_000 * 3];
    let tone: Vec<i16> = (0..16_000 * 2)
        .map(|i| ((i as f32 / 16_000.0 * 440.0 * 2.0 * std::f32::consts::PI).sin() * 8000.0) as i16)
        .collect();

    audio_tx.send(AudioFrame { channel: Channel::Mic, pcm: silence, ts_ms: 0 }).unwrap();
    audio_tx.send(AudioFrame { channel: Channel::Mic, pcm: tone, ts_ms: 3000 }).unwrap();

    // Give whisper.cpp a few seconds to chew.
    tokio::time::sleep(Duration::from_secs(8)).await;
    drop(audio_tx);

    // Drain.
    let mut events = vec![];
    while let Ok(e) = tokio::time::timeout(Duration::from_secs(2), event_rx.recv()).await {
        if let Some(e) = e { events.push(e); } else { break; }
    }
    let _ = runner.await;

    eprintln!("captured {} events", events.len());
    // No hard assertion on text content — we just want "didn't crash" + "task exited".
}
```

- [ ] **Step 5: Run the (non-ignored) crate tests.**

Run: `cargo test -p yogurt-stt`
Expected: all prior tests still pass; whisper_smoke is `ignored`.

- [ ] **Step 6: (Optional, manual.) Run the smoke against a real model.**

```bash
RUN_WHISPER_SMOKE=1 cargo test -p yogurt-stt --test whisper_smoke -- --ignored --nocapture
```

Requires `~/.yogurt/models/ggml-small.en.bin` to exist (download via the UI built in Task 8.10, or `curl` from the registry URL). Expected: prints "captured N events" without panicking; on Apple Silicon decode should complete in a few seconds.

- [ ] **Step 7: Commit.**

```bash
git add crates/yogurt-stt/src/whisper_local.rs crates/yogurt-stt/src/lib.rs crates/yogurt-stt/tests/whisper_smoke.rs
git commit -m "feat(stt): add WhisperLocal adapter (whisper.cpp + Metal) via Stt trait"
```

---

### Task 8.7 · REST endpoints for model management

**Files:**
- Create: `crates/yogurt-server/src/api/stt_models.rs`
- Modify: `crates/yogurt-server/src/api/mod.rs` (mount routes)
- Modify: `crates/yogurt-server/Cargo.toml` (depend on `yogurt-stt`)

- [ ] **Step 1: Add the dep.**

In `crates/yogurt-server/Cargo.toml` `[dependencies]`:

```toml
yogurt-stt = { path = "../yogurt-stt" }
futures-util = { workspace = true }
```

- [ ] **Step 2: Write the handlers.**

`crates/yogurt-server/src/api/stt_models.rs`:

```rust
//! REST endpoints for whisper.cpp model management.
//!
//! GET    /api/stt/models             → list registry with `downloaded` flags
//! POST   /api/stt/models/:name/download → kick off download (progress over WS)
//! DELETE /api/stt/models/:name       → remove from disk
//!
//! The download endpoint returns 202 Accepted immediately; progress streams
//! over the global app WebSocket as `stt_model_download_progress` events
//! (see crates/yogurt-server/src/ws.rs and Task 8.8).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use yogurt_stt::models::{self, DownloadProgress, ModelSpec};

use crate::AppState; // existing in Phase 5

#[derive(Serialize)]
struct ModelView {
    name: &'static str,
    size_mb: u32,
    downloaded: bool,
    intel_supported: bool,
}

impl From<&ModelSpec> for ModelView {
    fn from(m: &ModelSpec) -> Self {
        Self {
            name: m.name,
            size_mb: m.size_mb,
            downloaded: models::is_downloaded(m),
            intel_supported: m.intel_supported,
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/stt/models", get(list))
        .route("/api/stt/models/:name/download", post(start_download))
        .route("/api/stt/models/:name", delete(remove))
}

async fn list() -> impl IntoResponse {
    let v: Vec<ModelView> = models::REGISTRY.iter().map(ModelView::from).collect();
    Json(v)
}

async fn start_download(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let spec = models::lookup(&name)
        .ok_or((StatusCode::NOT_FOUND, format!("unknown model: {name}")))?;

    // Detach: download runs in the background; UI subscribes to WS for progress.
    let spec = spec.clone();
    let broadcaster = state.ws_broadcaster.clone();
    tokio::spawn(async move {
        let model_name = spec.name.to_string();
        let bcast = broadcaster.clone();
        let res = models::download(&spec, move |p: DownloadProgress| {
            let bcast = bcast.clone();
            let model_name = model_name.clone();
            tokio::spawn(async move {
                bcast.send_stt_model_download_progress(&model_name, &p).await;
            });
        })
        .await;
        match res {
            Ok(()) => {
                broadcaster
                    .send_stt_model_download_complete(&spec.name)
                    .await;
            }
            Err(e) => {
                tracing::warn!(model = spec.name, error = ?e, "model download failed");
                broadcaster
                    .send_stt_model_download_error(&spec.name, &format!("{e}"))
                    .await;
            }
        }
    });

    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({ "status": "started" }))))
}

async fn remove(Path(name): Path<String>) -> Result<impl IntoResponse, (StatusCode, String)> {
    let spec = models::lookup(&name)
        .ok_or((StatusCode::NOT_FOUND, format!("unknown model: {name}")))?;
    let path = models::model_path(spec).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}")))?;
    }
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 3: Mount in `api/mod.rs`.**

Append:

```rust
pub mod stt_models;
```

And in the place where the API router is composed (Phase 5 introduced this), merge:

```rust
.merge(stt_models::router())
```

- [ ] **Step 4: Smoke check the routes compile.**

Run: `cargo check -p yogurt-server`
Expected: compiles. The `AppState::ws_broadcaster` field and the `send_stt_model_download_*` methods don't exist yet — they're added in Task 8.8. Until then, leave the file with `#[allow(dead_code)]` on the `start_download` body, OR sequence Task 8.8 before this step. **Recommendation:** do Task 8.8 (WS event emit) before the REST handlers; the plan orders REST first because conceptually it's the "outer surface," but if `cargo check` complains, swap.

- [ ] **Step 5: Commit (only the file, leave it unwired until 8.8 lands).**

```bash
git add crates/yogurt-server/src/api/stt_models.rs crates/yogurt-server/src/api/mod.rs crates/yogurt-server/Cargo.toml
git commit -m "feat(server): add /api/stt/models REST endpoints (list/download/delete)"
```

---

### Task 8.8 · WS event: `stt_model_download_progress`

**Files:**
- Modify: `crates/yogurt-server/src/ws.rs`

- [ ] **Step 1: Locate the Phase 3 WS event enum.**

Run: `rg -n "enum.*WsEvent|enum.*ServerEvent" crates/yogurt-server/src/ws.rs`
Expected: a `serde`-tagged enum with at minimum `Transcript`, `EnhanceProgress`, `ChatChunk` variants from prior phases.

- [ ] **Step 2: Add three new variants.**

In the same enum, add:

```rust
SttModelDownloadProgress {
    model: String,
    bytes_downloaded: u64,
    total_bytes: u64,
    bytes_per_sec: u64,
    eta_seconds: Option<u64>,
},
SttModelDownloadComplete { model: String },
SttModelDownloadError { model: String, error: String },
```

(Tagged via `#[serde(tag = "type")]` to match the convention established in Phase 3.)

- [ ] **Step 3: Add fan-out methods on the broadcaster.**

Locate the `Broadcaster` (or equivalent) struct from Phase 3. Add:

```rust
impl Broadcaster {
    pub async fn send_stt_model_download_progress(
        &self,
        model: &str,
        p: &yogurt_stt::models::DownloadProgress,
    ) {
        let _ = self
            .send(WsEvent::SttModelDownloadProgress {
                model: model.to_string(),
                bytes_downloaded: p.bytes_downloaded,
                total_bytes: p.total_bytes,
                bytes_per_sec: p.bytes_per_sec,
                eta_seconds: p.eta_seconds,
            })
            .await;
    }

    pub async fn send_stt_model_download_complete(&self, model: &str) {
        let _ = self
            .send(WsEvent::SttModelDownloadComplete { model: model.to_string() })
            .await;
    }

    pub async fn send_stt_model_download_error(&self, model: &str, error: &str) {
        let _ = self
            .send(WsEvent::SttModelDownloadError {
                model: model.to_string(),
                error: error.to_string(),
            })
            .await;
    }
}
```

The download progress / complete / error events broadcast to *all* connected WS clients — they are app-global, not meeting-scoped. Phase 5's `Settings` page is the consumer; meeting WS clients ignore them.

- [ ] **Step 4: Unit-test serialization.**

Add inside `ws.rs`:

```rust
#[cfg(test)]
mod tests_dl_events {
    use super::*;
    #[test]
    fn dl_progress_serializes_with_tag() {
        let ev = WsEvent::SttModelDownloadProgress {
            model: "small.en".into(),
            bytes_downloaded: 100,
            total_bytes: 500,
            bytes_per_sec: 50,
            eta_seconds: Some(8),
        };
        let j = serde_json::to_value(&ev).unwrap();
        assert_eq!(j["type"], "stt_model_download_progress");
        assert_eq!(j["model"], "small.en");
        assert_eq!(j["bytes_downloaded"], 100);
        assert_eq!(j["eta_seconds"], 8);
    }
}
```

(`#[serde(rename_all = "snake_case")]` should already be set on the enum from Phase 3 — verify before relying on the snake_case tag.)

- [ ] **Step 5: Run.**

Run: `cargo test -p yogurt-server ws::tests_dl_events`
Expected: pass.

Run: `cargo check -p yogurt-server`
Expected: compiles cleanly — `stt_models.rs` from 8.7 now finds its broadcaster methods.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-server/src/ws.rs
git commit -m "feat(server): add stt_model_download_{progress,complete,error} ws events"
```

---

### Task 8.9 · Active-STT switch: branch on `stt_provider` in `start_meeting`

**Files:**
- Modify: `crates/yogurt-server/src/meetings/start.rs`

- [ ] **Step 1: Read the existing Phase 3 implementation.**

Run: `rg -n "DeepgramAdapter|fn start_meeting" crates/yogurt-server/src/meetings/start.rs`
Expected: a function (likely `pub async fn start_meeting(...)`) that constructs a `DeepgramAdapter`, calls `Stt::start`, and pipes results into a `mpsc` consumed by the WS broadcaster.

- [ ] **Step 2: Branch on settings.**

Replace the hardcoded `DeepgramAdapter` construction with:

```rust
use yogurt_stt::{Stt, WhisperLocal};
use yogurt_stt::models;
use std::sync::Arc;

let stt: Arc<dyn Stt> = match settings.stt_provider.as_str() {
    "cloud" => Arc::new(yogurt_stt::DeepgramAdapter::new(settings.deepgram_api_key.clone())?),
    "local" => {
        let model_name = settings.stt_model.as_deref().unwrap_or("small.en");
        let spec = models::lookup(model_name)
            .ok_or_else(|| anyhow!("unknown local stt model: {model_name}"))?;
        if !models::is_downloaded(spec) {
            return Err(anyhow!(
                "local stt model {model_name} is not downloaded; \
                 download it from Settings → Transcription → Local"
            ));
        }
        let path = models::model_path(spec)?;
        // Loading the model is blocking; spawn_blocking keeps the runtime healthy.
        let loaded = tokio::task::spawn_blocking(move || WhisperLocal::load(path))
            .await
            .map_err(|e| anyhow!("join: {e}"))??;
        Arc::new(loaded)
    }
    other => return Err(anyhow!("unknown stt_provider: {other}")),
};
```

Everything downstream (channel wiring, broadcast pipe) stays identical because both adapters implement the same trait.

- [ ] **Step 3: Verify the `Settings` struct has `stt_model: Option<String>`.**

Phase 5 added `stt_provider: String`; it likely did NOT add `stt_model` because Local was stubbed. Add it now if missing:

- DB migration: `ALTER TABLE settings ADD COLUMN stt_model TEXT;`
- Rust struct: add `pub stt_model: Option<String>` to the settings row struct.
- Default to `"small.en"` when local is selected and the column is null.

Add a migration file under `crates/yogurt-db/migrations/<NNNN>_add_stt_model.sql` following the Phase 5 migration convention.

- [ ] **Step 4: Add a unit test exercising the branch.**

Inside `start.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_provider() {
        let s = Settings {
            stt_provider: "satellite".into(),
            stt_model: None,
            deepgram_api_key: None,
            ..Default::default()
        };
        // Direct call to a helper; if start_meeting is async-only, factor out the
        // adapter-selection block into `fn select_stt(settings) -> Result<...>`.
        let r = select_stt(&s);
        assert!(r.is_err());
    }

    #[test]
    fn rejects_local_when_model_missing() {
        let s = Settings {
            stt_provider: "local".into(),
            stt_model: Some("ghost.en".into()),
            ..Default::default()
        };
        let r = select_stt(&s);
        let msg = format!("{}", r.unwrap_err());
        assert!(msg.contains("unknown") || msg.contains("not downloaded"));
    }
}
```

Refactor the adapter-selection block into a synchronous `fn select_stt(settings: &Settings) -> Result<...>` helper to make it unit-testable (returns the spec to load, defers async work to the caller).

- [ ] **Step 5: Run.**

Run: `cargo test -p yogurt-server meetings::start`
Expected: pass.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-server/src/meetings/start.rs crates/yogurt-db/migrations/
git commit -m "feat(server): branch start_meeting on stt_provider (cloud vs local)"
```

---

### Task 8.10 · Frontend API client + TanStack Query hooks

**Files:**
- Create: `web/src/lib/api/stt.ts`
- Create: `web/src/hooks/useModelDownloadProgress.ts`

- [ ] **Step 1: Write the typed client.**

```ts
// web/src/lib/api/stt.ts
//
// Typed wrappers + TanStack Query hooks for /api/stt/models.

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";

export interface ModelView {
  name: "tiny.en" | "small.en" | "medium.en" | "large-v3";
  size_mb: number;
  downloaded: boolean;
  intel_supported: boolean;
}

export const sttKeys = {
  models: ["stt", "models"] as const,
};

export async function fetchModels(): Promise<ModelView[]> {
  const r = await fetch("/api/stt/models");
  if (!r.ok) throw new Error(`models list failed: ${r.status}`);
  return r.json();
}

export function useModels() {
  return useQuery({ queryKey: sttKeys.models, queryFn: fetchModels });
}

export function useDownloadModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => {
      const r = await fetch(`/api/stt/models/${encodeURIComponent(name)}/download`, {
        method: "POST",
      });
      if (!r.ok) throw new Error(`download start failed: ${r.status}`);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: sttKeys.models }),
  });
}

export function useDeleteModel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (name: string) => {
      const r = await fetch(`/api/stt/models/${encodeURIComponent(name)}`, { method: "DELETE" });
      if (!r.ok && r.status !== 204) throw new Error(`delete failed: ${r.status}`);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: sttKeys.models }),
  });
}
```

- [ ] **Step 2: Write the WS-progress hook.**

```ts
// web/src/hooks/useModelDownloadProgress.ts
//
// Subscribes to the global app WS (established in Phase 5) and surfaces
// per-model download progress as plain React state.

import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { sttKeys } from "../lib/api/stt";

export interface DownloadState {
  bytesDownloaded: number;
  totalBytes: number;
  bytesPerSec: number;
  etaSeconds: number | null;
  complete: boolean;
  error: string | null;
}

type ServerEvent =
  | { type: "stt_model_download_progress"; model: string; bytes_downloaded: number; total_bytes: number; bytes_per_sec: number; eta_seconds: number | null }
  | { type: "stt_model_download_complete"; model: string }
  | { type: "stt_model_download_error"; model: string; error: string }
  | { type: string; [k: string]: unknown };

export function useModelDownloadProgress(model: string | null): DownloadState | null {
  const qc = useQueryClient();
  const [state, setState] = useState<DownloadState | null>(null);

  useEffect(() => {
    if (!model) return;
    setState(null);
    const url = `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws/app`;
    const ws = new WebSocket(url);
    ws.onmessage = (e) => {
      let ev: ServerEvent;
      try { ev = JSON.parse(e.data); } catch { return; }
      if (!("model" in ev) || ev.model !== model) return;
      if (ev.type === "stt_model_download_progress") {
        setState({
          bytesDownloaded: ev.bytes_downloaded,
          totalBytes: ev.total_bytes,
          bytesPerSec: ev.bytes_per_sec,
          etaSeconds: ev.eta_seconds,
          complete: false,
          error: null,
        });
      } else if (ev.type === "stt_model_download_complete") {
        setState((s) => s ? { ...s, complete: true } : { bytesDownloaded: 0, totalBytes: 0, bytesPerSec: 0, etaSeconds: 0, complete: true, error: null });
        qc.invalidateQueries({ queryKey: sttKeys.models });
      } else if (ev.type === "stt_model_download_error") {
        setState((s) => s ? { ...s, error: ev.error } : { bytesDownloaded: 0, totalBytes: 0, bytesPerSec: 0, etaSeconds: null, complete: false, error: ev.error });
      }
    };
    return () => ws.close();
  }, [model, qc]);

  return state;
}
```

If Phase 5 used a different WS path (e.g. `/ws` shared with meetings instead of `/ws/app`), match the actual path — the hook just needs to receive `stt_model_download_*` events.

- [ ] **Step 3: Commit.**

```bash
git add web/src/lib/api/stt.ts web/src/hooks/useModelDownloadProgress.ts
git commit -m "feat(web): add /api/stt/models client + download progress hook"
```

---

### Task 8.11 · Local STT card, model picker, and download dialog

**Files:**
- Modify: `web/src/components/settings/Settings.tsx` (drop "Coming soon" badge)
- Create: `web/src/components/settings/LocalSTTCard.tsx`
- Create: `web/src/components/settings/ModelPicker.tsx`
- Create: `web/src/components/dialogs/ModelDownloadDialog.tsx`
- Optional: small Vitest smoke for the dialog rendering states

- [ ] **Step 1: `ModelPicker.tsx` — the pill row from PRD §5.6.**

```tsx
// web/src/components/settings/ModelPicker.tsx
//
// Renders the four whisper.cpp models as pills. Selected gets a check;
// undownloaded shows a ↓ glyph; medium.en/large-v3 on Intel show a warning chip.

import { type ModelView } from "../../lib/api/stt";
import clsx from "clsx";

const PLATFORM_IS_INTEL =
  typeof navigator !== "undefined" && /Intel/.test(navigator.userAgent);

interface Props {
  models: ModelView[];
  selected: string | null;
  onSelect: (name: string) => void;
  onRequestDownload: (name: string) => void;
}

export function ModelPicker({ models, selected, onSelect, onRequestDownload }: Props) {
  return (
    <div className="flex flex-wrap gap-2">
      {models.map((m) => {
        const isSelected = selected === m.name;
        const warn = PLATFORM_IS_INTEL && !m.intel_supported;
        return (
          <button
            key={m.name}
            type="button"
            onClick={() => (m.downloaded ? onSelect(m.name) : onRequestDownload(m.name))}
            className={clsx(
              "inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-sm",
              "border transition-colors",
              isSelected
                ? "border-[color:var(--matcha,_#5E9E73)] bg-[color:var(--mtsoft,_#E7F0E8)] text-[color:var(--ink,_#211D18)]"
                : "border-[color:var(--line,_#EBE3D5)] bg-white text-[color:var(--ink,_#211D18)] hover:border-[color:var(--matcha,_#5E9E73)]"
            )}
            aria-pressed={isSelected}
          >
            <span className="font-medium">{m.name}</span>
            {m.downloaded ? (
              isSelected ? <span aria-hidden>✓</span> : null
            ) : (
              <span aria-hidden title={`download (${m.size_mb} MB)`}>↓</span>
            )}
            {warn && (
              <span
                className="ml-1 rounded bg-[color:var(--straw,_#E07A66)]/10 px-1 text-[10px] text-[color:var(--straw,_#E07A66)]"
                title="Slow on Intel CPUs — small.en recommended"
              >
                slow
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 2: `ModelDownloadDialog.tsx` — the §5.11 modal.**

```tsx
// web/src/components/dialogs/ModelDownloadDialog.tsx
//
// Matcha-themed modal per PRD §5.11. Uses the native <dialog> element so
// no portal infrastructure is required. Body copy intentionally low-stakes:
// "Most users stay on cloud STT and never see this".

import { useEffect, useRef } from "react";
import { useModelDownloadProgress } from "../../hooks/useModelDownloadProgress";
import { useDownloadModel } from "../../lib/api/stt";

interface Props {
  model: string | null;
  sizeMb: number | null;
  onClose: () => void;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 ** 2).toFixed(1)} MB`;
}

function formatEta(s: number | null): string {
  if (s == null) return "—";
  if (s < 60) return `${s}s left`;
  return `${Math.round(s / 60)}m left`;
}

export function ModelDownloadDialog({ model, sizeMb, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const progress = useModelDownloadProgress(model);
  const { mutate: startDownload } = useDownloadModel();

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (model) {
      d.showModal();
      startDownload(model);
    } else {
      d.close();
    }
  }, [model, startDownload]);

  useEffect(() => {
    if (progress?.complete) {
      // Auto-close on completion after a brief beat so user sees 100%.
      const t = setTimeout(onClose, 600);
      return () => clearTimeout(t);
    }
  }, [progress?.complete, onClose]);

  const pct =
    progress && progress.totalBytes > 0
      ? (progress.bytesDownloaded / progress.totalBytes) * 100
      : 0;

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      className="rounded-[14px] p-0 backdrop:bg-black/30 backdrop:backdrop-blur-sm"
    >
      <div className="w-[420px] bg-white p-6">
        <div className="mb-4 flex items-start gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-full bg-[color:var(--mtsoft,_#E7F0E8)] text-[color:var(--matcha,_#5E9E73)]">
            <span aria-hidden>↓</span>
          </div>
          <div>
            <h2 className="font-[Hanken_Grotesk] text-[17px] font-semibold text-[color:var(--ink,_#211D18)]">
              {progress?.error
                ? `Couldn't download ${model}`
                : progress?.complete
                ? `Downloaded ${model}`
                : `Downloading ${model}`}
            </h2>
            <p className="font-mono text-[12px] text-[color:var(--mut,_#8A8174)]">
              whisper.cpp · {sizeMb} MB
            </p>
          </div>
        </div>

        <div className="mb-2 h-2 w-full overflow-hidden rounded-full bg-[color:var(--line,_#EBE3D5)]">
          <div
            className="h-full bg-[color:var(--matcha,_#5E9E73)] transition-[width] duration-300"
            style={{ width: `${pct}%` }}
            role="progressbar"
            aria-valuenow={Math.round(pct)}
          />
        </div>
        <p className="mb-4 font-mono text-[11px] text-[color:var(--mut,_#8A8174)]">
          {progress
            ? `${formatBytes(progress.bytesDownloaded)} / ${formatBytes(progress.totalBytes)} · ${formatBytes(progress.bytesPerSec)}/s · ${formatEta(progress.etaSeconds)}`
            : "Starting…"}
        </p>

        {progress?.error && (
          <p className="mb-3 rounded bg-[color:var(--straw,_#E07A66)]/10 p-2 text-[12px] text-[color:var(--straw,_#E07A66)]">
            {progress.error}
          </p>
        )}

        <p className="mb-5 text-[12px] leading-relaxed text-[color:var(--mut,_#8A8174)]">
          Most users stay on cloud STT and never see this. The file lives at
          <code className="mx-1 rounded bg-[color:var(--line,_#EBE3D5)]/40 px-1 font-mono">
            ~/.yogurt/models/
          </code>
          and you can delete it anytime.
        </p>

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-[9px] border border-[color:var(--line,_#EBE3D5)] bg-white px-3 py-1.5 text-[13.5px] font-medium text-[color:var(--ink,_#211D18)]"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-[9px] bg-[color:var(--matcha,_#5E9E73)] px-3 py-1.5 text-[13.5px] font-semibold text-white shadow"
          >
            Run in background
          </button>
        </div>
      </div>
    </dialog>
  );
}
```

Note on "Cancel" — true cancellation requires plumbing a cancel-token through `download_to`. v1 ships "Cancel" as "close the dialog; download keeps running in the background." This matches the "Run in background" button next to it and is consistent with PRD §5.11 button copy. A v1.1 enhancement can add real cancellation.

- [ ] **Step 3: `LocalSTTCard.tsx` — the matcha card.**

```tsx
// web/src/components/settings/LocalSTTCard.tsx
//
// Replaces the Phase-5 "Coming soon" stub. Shows the model picker and opens
// the download dialog when the user clicks an undownloaded model.

import { useState } from "react";
import { useModels } from "../../lib/api/stt";
import { ModelPicker } from "./ModelPicker";
import { ModelDownloadDialog } from "../dialogs/ModelDownloadDialog";

interface Props {
  active: boolean;
  selectedModel: string;
  onSelectModel: (name: string) => void;
  onActivate: () => void;
}

export function LocalSTTCard({ active, selectedModel, onSelectModel, onActivate }: Props) {
  const { data: models = [], isLoading } = useModels();
  const [downloading, setDownloading] = useState<{ name: string; sizeMb: number } | null>(null);

  return (
    <section
      className={`rounded-[14px] border bg-white p-5 shadow-sm transition-colors ${
        active
          ? "border-[color:var(--matcha,_#5E9E73)]"
          : "border-[color:var(--line,_#EBE3D5)]"
      }`}
    >
      <header className="mb-3 flex items-baseline justify-between">
        <h3 className="font-[Hanken_Grotesk] text-[17px] font-semibold text-[color:var(--ink,_#211D18)]">
          Local · whisper.cpp
        </h3>
        <label className="inline-flex items-center gap-1.5 text-[12px] text-[color:var(--mut,_#8A8174)]">
          <input
            type="radio"
            name="stt-provider"
            checked={active}
            onChange={onActivate}
            className="accent-[color:var(--matcha,_#5E9E73)]"
          />
          Use Local
        </label>
      </header>

      <p className="mb-4 text-[13px] text-[color:var(--mut,_#8A8174)]">
        Runs entirely on this Mac. No audio leaves your machine. Apple Silicon
        recommended — Intel works on small.en.
      </p>

      {isLoading ? (
        <p className="font-mono text-[12px] text-[color:var(--mut,_#8A8174)]">loading models…</p>
      ) : (
        <ModelPicker
          models={models}
          selected={selectedModel}
          onSelect={(n) => onSelectModel(n)}
          onRequestDownload={(n) => {
            const m = models.find((x) => x.name === n);
            if (m) setDownloading({ name: m.name, sizeMb: m.size_mb });
          }}
        />
      )}

      <p className="mt-3 font-mono text-[11px] text-[color:var(--mut,_#8A8174)]">
        Models download on first use · stored in <code>~/.yogurt/models</code>
      </p>

      <ModelDownloadDialog
        model={downloading?.name ?? null}
        sizeMb={downloading?.sizeMb ?? null}
        onClose={() => setDownloading(null)}
      />
    </section>
  );
}
```

- [ ] **Step 4: Update `Settings.tsx`.**

Find the Phase-5 transcription section. Replace the stub with:

```tsx
import { LocalSTTCard } from "./LocalSTTCard";
// ... inside the transcription section:
<div className="grid grid-cols-2 gap-4">
  <CloudSTTCard ... />
  <LocalSTTCard
    active={settings.stt_provider === "local"}
    selectedModel={settings.stt_model ?? "small.en"}
    onSelectModel={(name) => updateSettings({ stt_provider: "local", stt_model: name })}
    onActivate={() => updateSettings({ stt_provider: "local", stt_model: settings.stt_model ?? "small.en" })}
  />
</div>
```

Delete the `Coming soon` badge JSX, and remove its CSS class if it was unique to the stub.

- [ ] **Step 5: Add a Vitest smoke for the picker.**

`web/src/components/settings/ModelPicker.test.tsx`:

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ModelPicker } from "./ModelPicker";
import type { ModelView } from "../../lib/api/stt";

const models: ModelView[] = [
  { name: "tiny.en", size_mb: 75, downloaded: true, intel_supported: true },
  { name: "small.en", size_mb: 487, downloaded: false, intel_supported: true },
  { name: "medium.en", size_mb: 1530, downloaded: false, intel_supported: false },
  { name: "large-v3", size_mb: 3094, downloaded: false, intel_supported: false },
];

describe("ModelPicker", () => {
  it("calls onSelect when a downloaded model is clicked", () => {
    const onSelect = vi.fn();
    const onReq = vi.fn();
    render(<ModelPicker models={models} selected="tiny.en" onSelect={onSelect} onRequestDownload={onReq} />);
    fireEvent.click(screen.getByRole("button", { name: /tiny\.en/ }));
    expect(onSelect).toHaveBeenCalledWith("tiny.en");
    expect(onReq).not.toHaveBeenCalled();
  });

  it("calls onRequestDownload when an undownloaded model is clicked", () => {
    const onSelect = vi.fn();
    const onReq = vi.fn();
    render(<ModelPicker models={models} selected="tiny.en" onSelect={onSelect} onRequestDownload={onReq} />);
    fireEvent.click(screen.getByRole("button", { name: /small\.en/ }));
    expect(onReq).toHaveBeenCalledWith("small.en");
    expect(onSelect).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 6: Run frontend tests + build.**

Run: `pnpm --dir web test`
Expected: all tests pass.

Run: `pnpm --dir web build`
Expected: tsc + vite both succeed. If `clsx` isn't a Phase-5 dep, add it: `pnpm --dir web add clsx`.

- [ ] **Step 7: Commit.**

```bash
git add web/src/components/settings/ web/src/components/dialogs/ web/package.json web/pnpm-lock.yaml
git commit -m "feat(web): wire Local STT card with model picker + download dialog"
```

---

### Task 8.12 · End-to-end smoke + acceptance + push

**Files:** none — verification only.

- [ ] **Step 1: Run the full test suite.**

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test --workspace
pnpm --dir web test
pnpm --dir web build
```

Expected: all green.

- [ ] **Step 2: Real-model manual smoke (Apple Silicon Mac required).**

```bash
pnpm --dir web build
cargo run -p yogurt --release -- start --no-open
# In a browser: open http://localhost:7878/settings
# 1. Click "Use Local" radio on the Local · whisper.cpp card
# 2. Click `small.en ↓` pill — download dialog should appear
# 3. Watch the matcha progress bar fill; verify bytes / MB/s / ETA tick visibly
# 4. After ~30s–2min (network-dependent), the dialog closes
# 5. The pill should now render `small.en ✓`
# 6. Click "+ New meeting", talk for 30s, click End meeting
# 7. Transcript should appear with Finals from whisper.cpp
# 8. Disable wifi — close the browser, restart yogurt, talk again
# 9. Transcription should still work (no network)
```

Expected outcomes:
- Download dialog matches PRD §5.11 visually (matcha tinting, mono caption, byte readout).
- SHA256 verification passes (no error chip).
- After download, `~/.yogurt/models/ggml-small.en.bin` exists and `shasum -a 256` of it matches the registry value.
- A real 30-second sentence transcribes recognizably (not Deepgram-quality, but coherent English).
- The offline test is the acceptance kill criterion: if it works with wifi off, Phase 8 is done.

- [ ] **Step 3: Document the Intel-Mac fallback in the PR description.**

In the PR body, paste:

> **Intel Mac behavior:** whisper.cpp falls back to CPU decode (no Metal). Per PRD §5.8, only `tiny.en` and `small.en` are marketed as fully supported. `medium.en` and `large-v3` show a yellow "slow" chip in the picker. On an i7 2.6 GHz, `small.en` decodes a 5s segment in roughly 8-12s — slower than realtime for partials but acceptable for finals.

- [ ] **Step 4: Push.**

```bash
git push origin main
```

- [ ] **Step 5: Tag the phase milestone — only with explicit user confirmation.**

```bash
git tag -a v0.0.8-phase-8 -m "Phase 8 complete: local STT via whisper.cpp"
git push origin v0.0.8-phase-8
```

---

## Phase 8 acceptance criteria

All five must be true:

1. `cargo test --workspace` passes (including the new VAD, sha256, models_download tests; whisper_smoke remains `#[ignore]`d).
2. `pnpm --dir web test` and `pnpm --dir web build` pass.
3. **Settings flow:** on Apple Silicon, switching the Local card to active and clicking `small.en ↓` opens the download dialog, the matcha progress bar fills, SHA256 verifies, and the pill turns to `small.en ✓` without page reload.
4. **End-to-end local meeting:** after the download completes, starting a new meeting transcribes a 30-second sentence using only on-device compute — verified by killing network connectivity before recording.
5. **Drop-in adapter:** `start_meeting` selects between `DeepgramAdapter` and `WhisperLocal` based on `settings.stt_provider` with no other code paths touched — downstream WS broadcasting + transcript persistence work identically.

## What this phase does NOT do

Explicitly out of scope (next phase or deferred to v2):
- Non-English / multilingual whisper.cpp models (`small`, `medium`, `large-v3` without `.en`).
- True download cancellation (Cancel button currently just closes the dialog; download continues in background).
- Word-level transcript timestamps from whisper.cpp.
- GPU offload on non-Apple-Silicon (CUDA/OpenCL).
- Auto-download on app launch (download is always user-triggered).
- Per-model performance benchmarking surfaced in Settings.

## Build-time requirements

- **macOS:** Xcode Command Line Tools (`xcode-select --install`) — needed for the Metal framework headers `whisper-rs[metal]` links against.
- **Disk space:** at least 4 GB free for `large-v3`. Plan recommends only fetching `small.en` (487 MB) by default during development.
- **Network:** model files are downloaded from Hugging Face; first-time download for `small.en` is typically 30s–2min depending on connection.

## Next plan

After Phase 8 lands, write `docs/superpowers/plans/<date>-yogurt-phase-9-polish-distribution.md` covering:

- Markdown export polish (front-matter, transcript appendix, atomic writes).
- Homebrew tap formula (`homebrew-yogurt`) with auto-PR on release.
- GitHub Actions release workflow: universal binary (arm64 + x86_64 lipo), tarball, SHA256, GH Release attach.
- README.md with screencast + screenshots.
- Code-signing + notarization stretch goal (out of scope if it slips).
- Playwright E2E covering: open library → start meeting → end meeting → see enriched notes.
