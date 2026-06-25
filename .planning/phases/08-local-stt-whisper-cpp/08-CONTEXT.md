# Phase 8: Local STT (whisper.cpp) - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Ship a fully-local STT path as a second adapter on the `SttEngine` trait introduced in Phase 3. `WhisperLocal` is powered by `whisper-rs` (bindings to `whisper.cpp`) running on Metal, gated behind the `local-stt` Cargo feature flag. Replaces the "Coming soon" Local card stub from Phase 5/7 settings with a working model picker + download dialog; `start_meeting()` branches between `DeepgramAdapter` (cloud) and `WhisperLocal` (local) based on the user's stored preference.

Verification gate: on an M1 Air, `small.en` produces transcript with < 3s lag and does not exhibit growing latency drift over a 30-min meeting. Not just M3 Max — M1 Air is the floor we must hold.

</domain>

<decisions>
## Implementation Decisions

### Speech engine
- **D-01:** `whisper-rs` 0.16 with the `metal` feature flag. CoreML acceleration is deferred — it requires a Swift/CoreML model conversion step that is out of scope for v1. Metal-only baseline.
- **D-02:** `small.en` is the baseline model. `tiny.en`, `medium.en`, `large-v3` ship in the registry but `small.en` is the recommended/default. English-only `.en` variants only; multilingual deferred.
- **D-03:** VAD-driven chunking with `webrtc-vad` ("Aggressive" mode) at 16 kHz / 30 ms frames (480 samples). Speech runs followed by ~600 ms of silence produce a `Segment` event handed to whisper for a `Final`. A separate 1 s ticker decodes the last 5 s of mic audio at faster (`Greedy { best_of: 1 }`) settings for `Partial` events.
- **D-04:** Dual `whisper_state` pattern — preview (greedy, fast) for partials + settled (beam search width 5) for finals. Reduces user-visible latency without losing accuracy on committed text.

### Cargo feature flag
- **D-05:** Entire adapter (and all whisper.cpp toolchain pulls) is gated behind a `local-stt` Cargo feature in `yogurt-stt`. Release builds default to enabled; contributors without the C++ toolchain or who want a fast `cargo build` can opt out. Acceptance criterion (per ROADMAP §Phase 8): without `--features local-stt`, `cargo build` completes in under 30 seconds (no whisper.cpp CMake step).

### Concurrency / runtime safety
- **D-06:** **ALL `whisper.cpp` calls run on `tokio::task::spawn_blocking`.** Never block the tokio scheduler. This is LOCAL-05 and a hard correctness invariant — model load, decode (preview), decode (settled), and any future per-segment work must go through `spawn_blocking`. axum routes and WS sends must remain responsive during inference.

### Model storage
- **D-07:** Models download on first use to `~/.yogurt/models/` (resolved via `directories::ProjectDirs` from Phase 5).
- **D-08:** SHA256 verification mandatory. Hardcoded hashes pinned in a static registry; download flow is hard-failure on mismatch and deletes the corrupt file. Re-download on next attempt.
- **D-09:** Download supports `Range:`-header resume from partial files. Progress fans out over the global app WebSocket (`stt_model_download_progress` event).
- **D-10:** Auto-download on app launch is explicitly NOT done. Always user-triggered from Settings.

### UI integration
- **D-11:** The first-time model-download modal (matcha progress bar, bytes / MB-per-sec / ETA, Cancel / Run-in-background buttons) was specified in Phase 7 (STATE-04). This phase WIRES it to real download progress.
- **D-12:** Local STT card replaces the Phase-5 "Coming soon" stub in `Settings → Transcription`. Pill row with model glyphs (`✓` selected, `↓` undownloaded, `slow` warning chip on Intel for medium/large).
- **D-13:** Cancel button in v1 closes the dialog only — download continues in background. True cancellation is a v1.1 follow-up.

### Performance target
- **D-14:** **Benchmark on M1 Air, not just M3 Max.** `small.en` first-transcript-bar latency must be < 3s on M1 Air. Larger models on Intel are marketed best-effort.

### Claude's Discretion
- Exact VAD tuning constants (MIN_SPEECH_MS, SILENCE_HANG_MS, MAX_SEGMENT_MS) within the documented baseline; can tune during M1 Air bench.
- Whether to refactor `start_meeting`'s adapter-selection block into a sync `select_stt(settings)` helper for testability (recommended in source plan).
- Frontend ws path (`/ws/app` vs shared `/ws`) — match whatever Phase 5 used.

</decisions>

<specifics>
## Specific Ideas

- "Most users stay on cloud STT and never see this" — body copy intentionally low-stakes; Local is the privacy escape hatch, not the daily driver.
- M1 Air bench is the kill criterion. If `small.en` is slower than 3s lag on M1 Air, Phase 8 isn't done — either tune the dual-state preview/settled split or escalate to WhisperKit/ANE (v2).
- Real-model smoke test gated behind `RUN_WHISPER_SMOKE=1` env var so CI does not need a 487 MB model file.
- Offline test is the user-facing acceptance: kill wifi, start a meeting, talk for 30s — must work without network.

</specifics>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Source-of-truth plan
- `docs/superpowers/plans/2026-06-25-yogurt-phase-8-local-stt.md` — Full 12-task superpowers plan with verbatim Rust + TS code for every file. Authoritative source for VAD constants, registry layout, dual-state decode params, and dialog visual spec.

### Project specs
- `docs/PRD.md` — §4 Q3 (pluggable STT — cloud default, local opt-in), §5.6 (Settings transcription card pair), §5.8 (Intel best-effort: `small.en` real-time only), §5.11 (whisper.cpp model download UI), §13 (whisper.cpp streaming partials worse than Deepgram is acceptable), §16.2 (matcha palette for Local card chrome).
- `.planning/REQUIREMENTS.md` "Local STT" section — LOCAL-01 through LOCAL-05.
- `.planning/ROADMAP.md` "Phase 8: Local STT (whisper.cpp)" — Goal, depends-on, 5 success criteria including the M1 Air < 3s lag and the `--features local-stt` cargo build < 30 s constraints.

### Forward reference (from prior phases)
- `.planning/phases/03-cloud-stt-live-transcript/03-SUMMARY.md` — Defines the `SttEngine` trait, `TranscriptEvent` enum (`Partial` / `Final`), `AudioFrame` / `Channel`, and `DeepgramAdapter` that `WhisperLocal` mirrors. **MUST** mirror exact signatures; if Phase 3 shipped with slight differences from the plan stub, this phase aligns.
- `.planning/phases/05-llm-client-settings-keychain/05-SUMMARY.md` — Settings DB row schema with `stt_provider` column. This phase adds `stt_model: Option<String>` via migration and consumes `directories::ProjectDirs` helper at `~/.yogurt/`.
- `.planning/phases/07-library-onboarding-states/07-SUMMARY.md` — STATE-04 first-time model download modal visual spec. This phase wires it to live download progress.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`SttEngine` / `Stt` trait (Phase 3)**: `WhisperLocal` is the second concrete impl. `DeepgramAdapter` is the first. Downstream pipeline (audio broadcast → STT adapter → mpsc → WS broadcaster → transcript persistence) is identical — only the constructor differs.
- **`AudioFrame` broadcast channel (Phase 2)**: 16 kHz mono i16 PCM frames per channel (`Mic` / `System`). VAD segmenter consumes these directly.
- **`Broadcaster` (Phase 3 / Phase 5)**: WS fan-out. This phase adds three new variants (`SttModelDownloadProgress`, `SttModelDownloadComplete`, `SttModelDownloadError`) and corresponding `send_*` methods.
- **`directories::ProjectDirs` (Phase 5)**: Resolves `~/.yogurt/`. We append `models/`.
- **TanStack Query client + Settings page (Phase 5)**: Existing query infra; this phase adds `sttKeys.models` and three hooks.
- **First-time download modal UI (Phase 7 STATE-04)**: Shipped visually — wire to live data here.

### Established Patterns
- **TDD-first integration tests**: `crates/yogurt-stt/tests/<area>.rs`. Mock HTTP via in-test axum server (catches real `Range:`-header bugs).
- **`#[ignore]` for real-model smoke**: `whisper_smoke.rs` documented to require `RUN_WHISPER_SMOKE=1` env var and a downloaded model.
- **Synthetic PCM in tests**: sine waves + `vec![0i16; n]` silence — deterministic, no audio files in git.
- **Migration files**: `crates/yogurt-db/migrations/<NNNN>_*.sql` per Phase 5.

### Integration Points
- `crates/yogurt-stt/Cargo.toml` — new feature `local-stt` gating whisper-rs / webrtc-vad / sha2 deps.
- `crates/yogurt-server/src/api/stt_models.rs` (NEW) — REST list / download / delete; depends on Phase 5 `AppState` and Phase 3 broadcaster.
- `crates/yogurt-server/src/meetings/start.rs` (MODIFY) — branch on `settings.stt_provider`; refactor adapter pick into testable `select_stt(&Settings)`.
- `crates/yogurt-server/src/ws.rs` (MODIFY) — three new event variants matching Phase 3 `#[serde(tag = "type", rename_all = "snake_case")]` convention.
- `web/src/components/settings/Settings.tsx` (MODIFY) — drop "Coming soon" badge; mount `LocalSTTCard`.

</code_context>

<specifics>
## Specifics — Acceptance-Gate Bench

- **M1 Air is required acceptance, not M3 Max.** The < 3s lag target was added to the roadmap explicitly because the source plan only assumed M3 Max would meet it. M1 Air is the floor.
- **SHA256 verification of downloaded models** is non-negotiable. Wrong hashes make the feature 100% broken. The Phase 8 implementer MUST re-fetch each model and replace placeholder hashes in `models.rs` with actual digests via `shasum -a 256 <file>` before merging.
- **The `local-stt` feature flag must measurably gate compile time.** Acceptance: `cargo build` without `--features local-stt` completes in < 30 s (no CMake / whisper.cpp source pull).

</specifics>

<deferred>
## Deferred Ideas

- **ANE acceleration via WhisperKit Swift sidecar** → v2 if Metal isn't fast enough on M1 Air. Adds a Swift toolchain dependency and IPC layer, so explicitly deferred unless the M1 Air bench fails.
- **Non-English / multilingual models** (`tiny`, `small`, `medium`, `large-v3-turbo` without `.en`) → follow-up phase.
- **Word-level transcript timestamps** from whisper.cpp → v2.
- **True download cancellation** (cancel-token plumbed through `download_to`) → v1.1; v1 "Cancel" just closes the dialog and lets download continue in background.
- **GPU offload on Intel** (CUDA/OpenCL) → out of scope; Intel falls back to CPU.
- **Auto-download on app launch** → explicitly NOT done; always user-triggered.
- **Per-model performance benchmarking surfaced in Settings** → v2.
- **CoreML feature flag for whisper-rs** → v2 follow-up if M1/M3 latency on `small.en` is tight.

</deferred>

---

*Phase: 08-local-stt-whisper-cpp*
*Context gathered: 2026-06-25*
