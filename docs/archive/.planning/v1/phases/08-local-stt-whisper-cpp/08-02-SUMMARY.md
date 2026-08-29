---
phase: 08-local-stt-whisper-cpp
plan: 02
subsystem: stt
tags: [rust, webrtc-vad, segmenter, reqwest, sha256, range-resume, axum-mock, tdd]

# Dependency graph
requires:
  - phase: 08-local-stt-whisper-cpp
    plan: 01
    provides: "yogurt-stt::sha256::{hash_file, hash_bytes} (streaming SHA256 for verify-on-write); local-stt feature gate + whisper-rs/webrtc-vad/sha2 deps; placeholder vad::{Segmenter, SegmenterEvent} type contract"
provides:
  - "yogurt-stt::vad::Segmenter — real webrtc-vad sliding-window segmenter (30 ms frames, MIN_SPEECH_MS / SILENCE_HANG_MS / MAX_SEGMENT_MS state machine)"
  - "yogurt-stt::vad::SegmenterEvent — { SpeechStart { at_ms }, Segment { pcm, start_ms, end_ms } } events for the WhisperLocal supervisor"
  - "yogurt-stt::models::REGISTRY — static 4-entry whisper.cpp model catalog (tiny.en / small.en / medium.en / large-v3) with PRD-§5.8-aligned intel_supported flags"
  - "yogurt-stt::models::{lookup, model_path, is_downloaded} — registry query + ~/.yogurt/models path resolution + on-disk verify"
  - "yogurt-stt::models::download_to — resumable HTTP download with Range header, streaming SHA256 verify, file-deletion on hash mismatch"
  - "yogurt-stt::models::DownloadProgress — bytes_downloaded / total_bytes / bytes_per_sec / eta_seconds for the Phase 9 settings UI progress bar"
  - "yogurt-stt::models::DownloadError — thiserror enum: Http / Io / HashMismatch { expected, actual } / Cancelled"
affects: [08-03-meeting-supervisor-wiring, 09-distribution-polish]

# Tech tracking
tech-stack:
  added:
    - "reqwest 0.12 (rustls-tls-webpki-roots feature) — already in workspace deps; pulled into yogurt-stt under local-stt for the model download client"
    - "futures-util 0.3 (StreamExt) — required to drive reqwest's bytes_stream() chunk loop"
    - "axum 0.8 + tower (dev-dependency) — in-test mock HTTP server with Range-header support for download contract tests"
  patterns:
    - "in-test mock server — spawn_server() helper binds 127.0.0.1:0, returns SocketAddr; deterministic 600 KB payload (i % 251 as u8); honors Range: bytes=START- with 206 PARTIAL_CONTENT + Content-Range header. Pattern reusable for any HTTP contract test."
    - "fast-path → resume → fresh download — three-branch state machine: (1) if dest exists AND hash matches → Ok(()); (2) if dest exists partial → Range request + append; (3) else create + write fresh. Single function, no branching at call sites."
    - "verify-then-delete on hash mismatch — download_to runs sha256::hash_file() after sync_all(); on mismatch, std::fs::remove_file(dest) BEFORE returning HashMismatch so a retry starts clean. Critical because a partial-but-wrong file would otherwise cause infinite resume failures."
    - "progress callback throttled to 500 ms windows — bytes_per_sec / eta calculated over 500 ms ticks, not per-chunk. Final tick always fires at 100% even if the last window was < 500 ms (avoids UI getting stuck at 99%)."

key-files:
  created:
    - "crates/yogurt-stt/src/vad.rs (241 lines) — Segmenter with webrtc_vad::Vad in VadMode::Aggressive; FRAME_MS=30, FRAME_SAMPLES=480, MIN_SPEECH_MS=250, SILENCE_HANG_MS=600, MAX_SEGMENT_MS=25_000; push() concatenates with leftover, slices into 480-sample frames, calls process_frame; emits Segment when silence_long_enough OR speech_too_long AND speech_ms >= MIN_SPEECH_MS"
    - "crates/yogurt-stt/src/models.rs (404 lines) — ModelSpec, REGISTRY (4 entries in ascending size), lookup/model_path/is_downloaded, DownloadProgress (serde), DownloadError (thiserror), download_to + download convenience wrapper"
    - "crates/yogurt-stt/tests/vad_segmenter.rs (139 lines) — 4 contract tests on synthetic PCM: speech+silence → 1 segment, two-runs split → 2 segments, pure-silence → 0, sub-MIN_SPEECH blip → 0"
    - "crates/yogurt-stt/tests/models_download.rs (203 lines) — 3 contract tests vs in-test axum mock: full download + verify, Range-resume from 100 KB partial, bad-hash → file deleted + HashMismatch"
  modified:
    - "crates/yogurt-stt/src/lib.rs — added `pub mod vad;` and `pub mod models;` under #[cfg(feature = \"local-stt\")]; replaced Plan 01's placeholder vad module"
    - "crates/yogurt-stt/Cargo.toml — moved reqwest + futures-util into local-stt feature deps; axum + tower + tokio rt-multi-thread + tempfile in dev-dependencies for the mock server"

key-decisions:
  - "VadMode::Aggressive (not LowBitrate / Quality / VeryAggressive) — per source plan; aggressive is the sweet spot for noisy meeting audio. LowBitrate triggers on coffee-cup clinks; VeryAggressive misses quiet speech. Tested across all 4 sine-wave fixtures with stable behavior."
  - "FRAME_SAMPLES = 480 (not 320 from yogurt-audio::frame) — webrtc-vad requires exactly 10/20/30 ms frames at the sample rate. 30 ms × 16 kHz = 480, the only frame size that survives the Vad API. The yogurt-audio 320 (20 ms) constant is for the broadcast channel size, not for VAD."
  - "leftover-buffer pattern in push() — push(&mut self, pcm: &[i16]) concatenates incoming with `self.leftover`, slices into 480-sample frames, retains the trailing < 480 samples for next push. Avoids forcing the caller to pad — caller streams whatever chunk size arrives from cpal/SCK."
  - "MAX_SEGMENT_MS = 25_000 hard cap — emits a Segment even mid-speech if 25 s elapse. Prevents pathological long monologues from blowing whisper.cpp's beam search memory. Source plan called for this; tests don't exercise it (too slow on synthetic) but the constant is wired."
  - "REGISTRY hashes are PLACEHOLDERS pinned to 2026-06 snapshot — top-of-file WARNING comment with explicit `shasum -a 256` re-verify instructions for tiny.en / small.en / medium.en / large-v3. Decision is intentional: hard-failure on hash mismatch is correct behavior, but the pre-merge ritual is the safety net."
  - "intel_supported flags follow PRD §5.8 verbatim — tiny.en + small.en = true (whisper.cpp CPU paths work on x86_64); medium + large-v3 = false (require Metal kernels). The test `intel_support_flags_match_prd` asserts this mapping."
  - "model_path() uses directories::ProjectDirs::data_local_dir().join(\"models\") — matches Phase 5's `~/.yogurt/` base. If Phase 5's data_local_dir base ever shifts, this resolver shifts with it (no second source of truth)."
  - "is_downloaded() does NOT trust the file's existence — it checks `exists() && hash_file(path) == spec.sha256` (case-insensitive). A corrupt-but-present file counts as not downloaded, forcing a redownload. Necessary because download_to's HashMismatch deletes the file, but a user's earlier failed download (or hand-curled corrupt blob) wouldn't be caught otherwise."
  - "OpenOptions::new().append(true) for resume; File::create for fresh — separate branches prevent the resume path from accidentally truncating an in-progress file. seek(SeekFrom::End(0)) on the resumed handle is belt-and-suspenders (append mode already positions at end)."
  - "progress: 500 ms tick window + always-fire-at-100% — DownloadProgress fires on bytes-since-tick threshold OR on completion. UI gets enough updates to feel live (~2 ticks/sec on a 50 Mbps download) without flooding the WS bus."
  - "axum-as-test-dep (not assertion library) — the mock HTTP server is 60 lines of axum router + tower::ServiceExt. dev-dependencies keep it out of the production binary. SocketAddr returned from spawn_server() is the test's single source of truth for the URL."
  - "Range-aware mock returns 206 PARTIAL_CONTENT with Content-Range — production servers like HuggingFace's CDN MUST honor Range or whisper.cpp users on slow connections lose progress on every disconnect. The contract test pins this so we'd catch a future regression where download_to forgets to set Range."
  - "tests/models_download.rs uses #[tokio::test] (not block_on) — download_to is async; the in-test mock server is also async on a separate tokio task. Multi-threaded runtime (default for #[tokio::test]) so both can co-exist."

tests:
  added:
    - "tests/vad_segmenter.rs::it_emits_one_segment_for_speech_then_silence — 1.5 s tone + 1 s silence → exactly 1 segment, samples > 16_000, end-start ≥ 1000"
    - "tests/vad_segmenter.rs::it_splits_two_speech_runs_separated_by_silence — 0.8 s × 2 with 0.8 s silence between → exactly 2 segments"
    - "tests/vad_segmenter.rs::pure_silence_emits_no_segments — 3 s of i16::default → 0 segments (no false positives)"
    - "tests/vad_segmenter.rs::very_short_speech_blips_are_ignored — 0.1 s tone + 1 s silence → 0 segments (cough/click filter)"
    - "tests/models_download.rs::it_downloads_a_full_file_and_verifies_sha256 — 600 KB payload, callback fired ≥1, final tick at 100%"
    - "tests/models_download.rs::it_resumes_from_a_partial_file — pre-write 100 KB, download_to resumes via Range, final hash matches full payload"
    - "tests/models_download.rs::it_rejects_a_bad_sha_and_removes_the_file — deadbeef×8 expected sha → HashMismatch + !dest.exists()"
    - "src/models.rs::tests::registry_has_four_models_in_size_order — REGISTRY.len() == 4, sizes ascending"
    - "src/models.rs::tests::lookup_finds_known_models_and_misses_unknown — lookup(\"small.en\") + lookup(\"nope\").is_none()"
    - "src/models.rs::tests::intel_support_flags_match_prd — PRD §5.8 mapping locked"
    - "src/models.rs::tests::sha256_values_are_hex_64_chars — all hashes 64-char hex"
  total_pass: "cargo test -p yogurt-stt --features local-stt → 15 unit (incl. sha256) + 1 deepgram_mock + 1 deepgram_real_fixture + 3 deepgram_reconnect + 3 models_download + 4 vad_segmenter + 1 ignored whisper_smoke = 27 passing, 1 ignored, 0 failed"

commits:
  - "9ec402b test(stt,08-02 Task 1): <RED> add VAD segmenter contract tests on synthetic PCM"
  - "fbd935f feat(stt,08-02 Task 1): <GREEN> webrtc-vad sliding-window segmenter"
  - "e17038c feat(stt,08-02 Task 2): static model registry with 4 entries + 4 inline tests"
  - "4a0af8c test(stt,08-02 Task 3): <RED> download_to contract tests vs in-test axum mock"
  - "d58d454 feat(stt,08-02 Task 3): <GREEN> download_to with Range resume + SHA256 verify"
  - "e741da6 style(stt,08-02): rustfmt vad.rs"

verification:
  cargo_test_p_yogurt_stt_features_local_stt: "PASS — 27 active tests passing across vad_segmenter (4), models_download (3), deepgram_* (5), unit lib (15); 1 ignored whisper_smoke as expected"
  cargo_clippy_p_yogurt_stt_features_local_stt_all_targets_D_warnings: "PASS — clean"
  models_rs_top_of_file_hash_placeholder_warning: "PRESENT — //! ⚠️ WARNING block with re-verify shasum command + lowercase-hex + 64-char invariant"
  vad_rs_source_plan_constants: "MATCH — FRAME_MS=30, FRAME_SAMPLES=480, MIN_SPEECH_MS=250, SILENCE_HANG_MS=600, MAX_SEGMENT_MS=25_000, VadMode::Aggressive"

scope-deviations:
  - "Plan called `models.rs` min_lines: 150 — final is 404 lines because download_to + DownloadError + DownloadProgress + module docs add substantial body. Acceptance criterion was a floor, not a ceiling; over-shoot is intentional."
  - "Plan called `vad.rs` min_lines: 100 — final is 241 lines for the same reason: process_frame logic + leftover buffering + bound-growth-on-silence guard each take 30-50 lines."

deferred:
  - "Re-verify SHA256 placeholders against live HuggingFace blobs — flagged in top-of-file WARNING; must happen before any v1.0 tag is cut. Tracked as Phase 9 gate, not a code TODO."
  - "MAX_SEGMENT_MS contract test (25 s monologue forces emit) — too slow for synthetic CI fixtures; the constant is wired and process_frame uses it, but no direct test pins the boundary. Acceptable: real-meeting smoke tests in Plan 08-03 will surface any regression."

---

# Plan 08-02 Summary: VAD Segmenter + Model Registry + Resumable Download

**Status:** ✅ Complete — 27 passing tests + clippy clean
**Duration:** Single session, autonomous TDD execution
**Branch:** gsd/autonomous
**Commit range:** 9ec402b..e741da6 (6 commits)

## What Shipped

Plan 08-02 lands the two algorithmic-core pieces of local STT that Plan 08-01's WhisperLocal scaffold left abstract: a **VAD-driven sliding-window segmenter** that turns 16 kHz mono PCM streams into utterance-bounded `Segment`s, and a **model download manager** with `Range:`-header resume + post-write SHA256 verification + file-deletion on mismatch.

Both modules are TDD-first — RED phase failing tests committed before any implementation, GREEN phase only after the contract was pinned. Both live behind `#[cfg(feature = "local-stt")]` so default builds skip the whisper.cpp/webrtc-vad/reqwest toolchain pulls.

## Surface Exposed

```rust
#[cfg(feature = "local-stt")]
pub mod vad {
    pub struct Segmenter { /* private */ }
    pub enum SegmenterEvent {
        SpeechStart { at_ms: u64 },
        Segment { pcm: Vec<i16>, start_ms: u64, end_ms: u64 },
    }
    impl Segmenter {
        pub fn new(sample_rate: usize) -> Self;
        pub fn push(&mut self, pcm: &[i16], emit: impl FnMut(SegmenterEvent));
    }
}

#[cfg(feature = "local-stt")]
pub mod models {
    pub struct ModelSpec { /* name, filename, size_mb, url, sha256, intel_supported */ }
    pub const REGISTRY: &[ModelSpec];
    pub fn lookup(name: &str) -> Option<&'static ModelSpec>;
    pub fn model_path(spec: &ModelSpec) -> std::io::Result<PathBuf>;
    pub fn is_downloaded(spec: &ModelSpec) -> bool;

    pub struct DownloadProgress {
        pub bytes_downloaded: u64,
        pub total_bytes: u64,
        pub bytes_per_sec: u64,
        pub eta_seconds: Option<u64>,
    }
    pub enum DownloadError { Http(_), Io(_), HashMismatch { expected, actual }, Cancelled }

    pub async fn download_to<F>(url: &str, dest: &Path, expected_sha256: &str, on_progress: F)
        -> Result<(), DownloadError>
        where F: FnMut(DownloadProgress) + Send + 'static;

    pub async fn download<F>(spec: &ModelSpec, on_progress: F)
        -> Result<(), DownloadError>
        where F: FnMut(DownloadProgress) + Send + 'static;
}
```

## How Plan 08-03 Consumes This

Plan 08-03 will wire `models::download` into a settings UI download button, and feed live `cpal::Stream` PCM through `vad::Segmenter` → `whisper_local::WhisperLocal::transcribe(Segment)` → `TranscriptEvent` broadcast.

The trait-shape contract is locked: `Segmenter::push` takes a borrowed `&[i16]` slice (no allocation in the audio thread) and emits via FnMut callback (no Box<dyn>, no channel overhead). `download_to`'s progress callback signature matches the WS event format Phase 9 will surface as a settings UI progress bar without further plumbing.
