# Phase 2: Audio Capture (HIGHEST RISK) - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Stand up the `yogurt-audio` crate that captures two synchronized 16 kHz mono PCM streams — microphone (via `cpal` / CoreAudio) and macOS system audio (via the `screencapturekit` crate / ScreenCaptureKit framework) — and pushes both into Tokio broadcast channels for downstream STT consumers (Phase 3). Expose capture lifecycle and a typed `PermissionStatus` to `yogurt-server` via two new REST endpoints (`GET /api/audio/devices`, `GET /api/audio/permission`) so the Phase 7 onboarding UI can render the §5.10 / §5.11 permission flows.

**Why this phase is the highest risk in the v1 roadmap:** the `screencapturekit` crate is "mainly designed for screen capture" per PRD §13 risk #1. If its audio-loopback surface proves insufficient on real hardware, the project must pivot to a Swift sidecar pattern — and that pivot needs to happen *inside* this phase, not after Phase 3 has already wired STT consumers. A spike-first task (Task 2.0) retires this risk before any production code is written.

**The 30-second dual-channel ear-test is a phase gate, not a checkbox.** This phase only completes when a human listens to a WAV file produced from real concurrent mic + system audio capture and confirms both channels are audible, non-silent, non-clipped, and not swapped.

**No STT, no WebSocket plumbing, no UI rendering of permission state in this phase** — capture and emit only.

</domain>

<decisions>
## Implementation Decisions

### Crate boundary and platform isolation
- **D-01:** New `crates/yogurt-audio` crate is the only crate that touches native macOS audio APIs — keeps future Windows/Linux ports additive, keeps the rest of the workspace unit-testable.
- **D-02:** Public surface is platform-portable: `Channel::Mic` works everywhere (cpal is cross-platform); `Channel::System` returns `AudioError::UnsupportedPlatform` on non-macOS targets via `#[cfg(target_os = "macos")]` gating — keeps `cargo build` green on Linux CI.

### Audio format contract (load-bearing for Phase 3)
- **D-03:** Sample rate **16,000 Hz** (matches Deepgram `linear16` and whisper.cpp `pcm_s16le`).
- **D-04:** Channels **1 (mono)**; system loopback is L+R averaged to mono inside the capture callback.
- **D-05:** Sample format **`i16` signed 16-bit LE** — no f32-to-i16 quantization at the STT boundary.
- **D-06:** Frame size **320 samples (= 20 ms @ 16 kHz)** — canonical streaming-STT chunk for low-latency partials.
- **D-07:** Every `Frame` carries a `Channel` tag (`Mic` or `System`) plus `monotonic_ms: u64` — Phase 3 routes Mic → "Me" (ink) and System → "Them" (grey) per PRD §5.2 and uses `monotonic_ms` for `↳ HH:MM` deep-links per §5.3.
- **D-08:** Format constants live in `crates/yogurt-audio/src/frame.rs` as `pub const SAMPLE_RATE_HZ: u32 = 16_000;` and `pub const FRAME_SAMPLES: usize = 320;` — Phase 3 imports them, never hardcodes.

### ScreenCaptureKit vs Swift sidecar — the load-bearing risk decision
- **D-09:** Production path **defaults to in-process `screencapturekit` 0.3 crate** (PRD §13 mitigation #1). A mandatory 30-minute spike (Task 2.0) verifies a one-stream audio-only `SCStream` config with `set_captures_audio(true)` actually delivers non-zero PCM bytes on this Mac before any production code is written.
- **D-10:** Spike decision criteria are codified:
  - **PASS** (non-zero audio bytes captured) → proceed with in-process SCK path.
  - **PARTIAL** (buffer callbacks fire but bytes are all zero) → spend at most 30 more minutes debugging `excludes_current_process_audio` misconfig; otherwise switch to sidecar.
  - **FAIL** (compile fails, no callbacks, or panic) → switch to **Swift sidecar fallback path**: a ~150-line Swift binary `tools/yogurt-audio-helper/` using Apple's first-party `SCStream` API, writing 16 kHz mono i16 LE PCM to stdout, consumed via `tokio::process::Command` from `crates/yogurt-audio/src/system.rs`.
- **D-11:** The spike commits **only the decision note** (`docs/superpowers/notes/2026-06-25-sck-spike-result.md`), not the throwaway code — the decision is what the rest of the phase hinges on.
- **D-12:** The public `yogurt-audio` surface (`start_capture() → AudioStream`, `spawn_system_capture()`, etc.) is **identical under either path** — the trait keeps subprocess-vs-in-process swap-out a single-file change.

### Mic capture path
- **D-13:** Microphone capture uses **`cpal` 0.15** against the default CoreAudio input device (not SCK, not the cross-PR #894 SCK loopback path). cpal is the boring battle-tested CoreAudio binding; SCK mic capture exists but is the youngest API surface.
- **D-14:** Default input device is typically 48 kHz f32 (sometimes mono, sometimes stereo). Resample to 16 kHz i16 mono inside the cpal callback using **`rubato` 0.16 `SincFixedIn`** with `sinc_len: 64, oversampling_factor: 128, BlackmanHarris2` window.
- **D-15:** Resampler operates on **480-sample fixed-input chunks (10 ms @ 48 kHz)** → 160 samples @ 16 kHz, then a separate `FrameChunker` collects into 320-sample `Frame`s.

### System audio capture path (in-process SCK assuming spike PASS)
- **D-16:** SCStream configuration **MUST** set:
  - `set_captures_audio(true)` — enable audio output.
  - `set_excludes_current_process_audio(true)` — critical privacy/correctness guarantee per AUDIO-03; Yogurt's own UI sounds must NOT appear in the transcript.
  - `set_width(2)`, `set_height(2)`, `set_minimum_frame_interval(1000ms)` — minimum video dims; we ignore video output entirely.
- **D-17:** SCK delivers 48 kHz stereo f32 by default — same `Downmix` helper as mic path; L+R averaged to mono before resample.

### Broadcast channel topology
- **D-18:** Two separate `tokio::sync::broadcast::Sender<Frame>` channels, one per `Channel` — never multiplex into a single stream (Phase 3 STT runs one session per channel).
- **D-19:** Channel capacity **256 frames per channel** (~5 seconds of buffered audio). If consumers fall behind, `broadcast::Receiver::recv()` returns `Lagged(skipped)` and they drop late frames — correct behavior since STT is wall-clock-bound.
- **D-20:** Phase 3 will fan-in both receivers via `tokio::select!`; this phase exposes `AudioStream::subscribe_mic()` and `subscribe_system()` separately.

### Meeting-relative clock
- **D-21:** `monotonic_ms` is **milliseconds since `start_capture()` returned**, derived from `std::time::Instant::now()` captured at producer-spawn time. Each producer owns its own `Instant`; both are seeded synchronously inside `start_capture()` so drift between mic and system is bounded by spawn-order skew (microseconds).
- **D-22:** Drift tolerance: **< 50 ms** between mic and system streams at any wall-clock moment within a meeting (AUDIO-05). The 60-min synthetic test in success criterion #3 of ROADMAP §Phase 2 is a Phase 3 follow-up — this phase only ships the clock plumbing; verifying < 50 ms drift over 60 minutes is observable in Phase 3 once STT timestamps land.

### Permission flow
- **D-23:** `has_screen_recording_permission()` uses CoreGraphics `CGPreflightScreenCaptureAccess` (stable macOS 10.15+ public API, lives in CoreGraphics.framework — no extra `#[link]` needed). Returns `PermissionStatus::{Granted, Denied, NotRequired}`.
- **D-24:** `request_screen_recording_permission()` calls `CGRequestScreenCaptureAccess` — triggers the OS dialog. **TCC limitation:** the binary must be restarted after the user grants permission before the grant takes effect. UI surfaces PRD §5.10's "Restart once after granting" copy.
- **D-25:** `start_capture()` is a **permission gate** — fast-fails with `AudioError::PermissionDenied` *before* opening the mic, so the user never hears a recording indicator if system capture is going to fail.

### Supervisor termination
- **D-26:** `AudioStream` holds both `MicCapture` and `SystemCapture` via owned fields. Dropping `AudioStream` drops both, which drops the cpal `Stream` (stops capture) and the SCK `SCStream` (stops capture). No `Arc<Mutex<>>` wrapper, no explicit `.stop()` method — RAII via Drop guarantees no leaked SCK handles or orphan tasks (AUDIO-06).
- **D-27:** Sidecar path uses `tokio::process::Child` — drop sends SIGKILL to the helper; explicit `.kill()` not needed.

### Testing strategy
- **D-28:** **No real-device capture tests** in CI. GitHub Actions macOS runners have no audio devices and no TCC permission; failing CI on missing mic would block every PR.
- **D-29:** **Synthetic sine-wave generator** (`crates/yogurt-audio/src/synthetic.rs`, gated behind `synthetic` Cargo feature + `cfg(test)`) feeds the same broadcast plumbing real producers use — verifies frame size, monotonic cadence, multi-subscriber semantics on every platform.
- **D-30:** **Manual smoke binaries** (`examples/mic_smoke.rs`, `examples/system_smoke.rs`, `examples/dual_smoke.rs`) exercise real cpal/SCK and print frame count + peak amplitude. Not run by `cargo test`.
- **D-31:** Live-capture tests gated by `YOGURT_AUDIO_LIVE=1` env var — default off so `cargo test --workspace` stays green on dev laptops without prompting for Screen Recording.

### The 30-second WAV ear-test acceptance gate
- **D-32:** Phase completion requires a human to listen to a WAV file produced from a real concurrent mic + system audio capture (e.g., talking while Spotify plays) and confirm: **(a)** both channels audible, **(b)** no silence on either channel, **(c)** no clipping, **(d)** no channel swap (mic on left, system on right or vice versa — consistent with `Channel::Mic` / `Channel::System` tags). This is modeled as a `checkpoint:human-verify` task — Claude can produce the WAV but cannot verify it.

### Claude's Discretion
- Exact rubato `SincInterpolationParameters` tuning (D-14 lists sensible defaults; can be re-tuned if mic smoke reports audible artifacts).
- WAV writing helper for the ear-test gate (any standard `hound` or hand-rolled RIFF header is fine — output file lives in `target/` and is gitignored).
- Whether `pollster::block_on` is needed for SCK setup (depends on whether the crate exposes a sync `start_capture_sync()` in 0.3 — Task 2.0 spike will surface the answer).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase plan (source of truth)
- `docs/superpowers/plans/2026-06-25-yogurt-phase-2-audio-capture.md` — full implementation plan with code sketches for every task (2.0 spike → 2.9 smoke). 2,174 lines. **READ FIRST.** Defines task numbering, file structure, audio format contract, spike decision tree, both Path A (in-process SCK) and Path B (Swift sidecar) implementations, and the eight commits this phase produces.

### Product Requirements Document
- `docs/PRD.md` §5.1 — Record meeting (defines the 16 kHz / 16-bit / mono / two-channel contract).
- `docs/PRD.md` §5.10 — Onboarding Screen Recording permission step (defines "restart once" UX copy).
- `docs/PRD.md` §5.11 — Permission-denied recovery screen (defines `AudioError::PermissionDenied` user-facing surface).
- `docs/PRD.md` §7 — Architecture (in-process audio, no subprocesses preferred — informs Path A bias).
- `docs/PRD.md` §10 — API surface (`GET /api/audio/devices` listed in REST table).
- `docs/PRD.md` §13 risk #1 — SCK audio-loopback gaps → Swift sidecar fallback documented at product level.

### Yogurt planning artifacts
- `.planning/REQUIREMENTS.md` "Audio" section — AUDIO-01 through AUDIO-07 with exact wording each plan must satisfy.
- `.planning/ROADMAP.md` §Phase 2 — phase boundary statement, the five success criteria, the 30-second ear-test acceptance gate.

### Pitfall research (audio capture is the highest-risk phase — pitfall research is canonical)
- `docs/PITFALLS.md` — read in full. The SCK loopback risk, TCC permission timing quirk, mic↔system drift bounds, and broadcast channel capacity rationale all live here as pre-spike research.

### Prior phase summary (dependency)
- `.planning/phases/00-skeleton-foundations/00-CONTEXT.md` — Phase 0 produced `yogurt-server` and the workspace; this phase adds `yogurt-audio` as a new member.
- Plan summaries from Phase 0 will be referenced by Task 2.8 when wiring REST endpoints into `yogurt-server`.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets (from Phase 0)
- `crates/yogurt-server` axum router scaffolding — Task 2.8 mounts two new GET routes (`/api/audio/devices`, `/api/audio/permission`) and adds a `pub mod audio` module.
- Workspace-level `[workspace.dependencies]` block — Task 2.1 extends it with `cpal = "0.15"`, `screencapturekit = "0.3"`, `rubato = "0.16"`, `thiserror = "2"`.
- The `yogurt start` CLI entry point — Task 2.9 smokes the two new endpoints via `curl localhost:7878/api/audio/...`.

### Established Patterns (carry forward from Phase 0)
- Single static binary discipline — no subprocesses **unless** the spike forces the Swift sidecar (D-10/D-11). If that happens, Phase 9's distribution story has to bundle the helper next to `yogurt` in the Homebrew bottle.
- Workspace deps via `workspace = true` references — every new dep gets pinned in root `Cargo.toml` first.
- `tracing` for structured logs (already in Phase 0); audio producers emit `tracing::info!(%device_name, sample_rate, channels, ?sample_format, ...)` on start.

### Integration Points
- `crates/yogurt-audio/src/lib.rs` public surface → consumed by `crates/yogurt-server/src/audio.rs` (Task 2.8) and (later) `crates/yogurt-stt` in Phase 3.
- `crates/yogurt-server/src/routes.rs` → adds two route entries; no changes to existing health endpoint.
- `crates/yogurt-server/src/lib.rs` → adds `pub mod audio;` declaration.

</code_context>

<specifics>
## Specific Ideas

- **The 30-second dual-channel ear-test is the phase gate.** ROADMAP success criterion #1 reads: "30 seconds of mic + system audio captured during a real YouTube/Zoom playback writes a 2-channel WAV file that passes an ear-test (both channels audible, no silence, no clipping, no channel swap) — this gates the rest of the phase." This is a `checkpoint:human-verify` task — Claude produces the WAV from `examples/dual_smoke.rs` output, the human listens, the human signals approve/reject. No CI can do this.
- **The Task 2.0 spike must be run with audio actually playing.** Open Spotify (or any system audio source) in a separate window before running the spike. Without audio playing, even a passing SCK config will report zero bytes.
- **TCC "restart once after granting" is a load-bearing UX quirk, not a bug.** `CGPreflightScreenCaptureAccess` returns the cached state, not the live one. PRD §5.10 already commits to the "Restart once after granting — a macOS quirk, not us" footer copy; this phase surfaces the typed `AudioError::PermissionDenied` that Phase 7 onboarding consumes.
- **`excludes_current_process_audio = true` is verified by playing audio from Yogurt's own UI** and confirming it does NOT appear in the transcript. This is a Phase 3 verification once STT lands, but the SCK config flag is set from the first commit in Task 2.6.
- The spike result note (`docs/superpowers/notes/2026-06-25-sck-spike-result.md`) is the single source of truth for which path (A or B) Task 2.6 takes. Subsequent agents read this file before implementing system audio capture.

</specifics>

<deferred>
## Deferred Ideas

Explicitly out of scope for Phase 2 (later plans):

- **STT consumption of the broadcast channels** — Phase 3 (Deepgram cloud) and Phase 8 (whisper.cpp local).
- **`POST /api/meetings/:id/start` and `/ws/meetings/:id` wiring** — Phase 3 (the WebSocket layer is where mic/system frames become live transcript events).
- **The §5.11 permission-denied recovery UI** — Phase 7 (onboarding plan). This phase only ships the API the UI will consume.
- **Persistent storage of captured PCM to disk** — out of v1 entirely (PRD §2 non-goals). Per-meeting "keep audio" toggle is `LIB-V2-02`, deferred to v1.1.
- **Voice activity detection, noise suppression, AGC** — v2+.
- **Per-speaker diarization** — v2+ (PLAT-01, pyannote sidecar).
- **60-minute drift verification (mic↔system < 50 ms over a real hour)** — observable in Phase 3 once STT timestamps land; this phase ships the clock plumbing but does not assert long-run drift.
- **Settings UI "input device" dropdown wiring** — Phase 5 (settings plan; this phase exposes only the underlying REST endpoint).
- **Homebrew packaging of the Swift sidecar binary** (if Path B was taken) — Phase 9 distribution polish.

</deferred>

---

*Phase: 02-audio-capture-highest-risk*
*Context gathered: 2026-06-25*
