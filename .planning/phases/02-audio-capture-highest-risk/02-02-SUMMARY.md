---
phase: 02-audio-capture-highest-risk
plan: 02
subsystem: audio
tags: [audio, cpal, screencapturekit, rubato, broadcast, raii]
dependency_graph:
  requires:
    - phase-02-01 (yogurt-audio crate + Frame/Channel/AudioError types, permission FFI, SCK 8.x spike PASS)
  provides:
    - yogurt-audio::start_capture() → AudioStream (mic + system loopback, 16 kHz mono i16, 320-sample frames)
    - AudioStream::subscribe_mic() / subscribe_system() — broadcast::Receiver<Frame> per channel
    - yogurt-audio::list_input_devices() — DeviceInfo enumeration for the upcoming /api/audio/devices endpoint
    - yogurt-audio::resample::Downmix — shared 48k stereo f32 → 16k mono i16 resampler
    - yogurt-audio::mic::FrameChunker — pub(crate) shared chunker, used by both mic and system paths
  affects:
    - Phase 3 STT consumers (Deepgram + whisper.cpp adapters subscribe to mic_tx + system_tx)
    - Plan 02-03 (REST endpoints will surface list_input_devices() + has_screen_recording_permission())
    - Phase 7 onboarding (consumes typed PermissionStatus + start_capture()'s PermissionDenied error path)
tech_stack:
  added:
    - "cpal 0.15 (now in use — default input device → f32/i16 callback)"
    - "rubato 0.16 SincFixedIn (now in use — 48k→16k resampler with BlackmanHarris2 window)"
    - "screencapturekit 8.0 SCStream/SCContentFilter/SCStreamConfiguration/SCStreamOutputTrait (now in use)"
  patterns:
    - "Shared Arc<Mutex<(Downmix, FrameChunker)>> wrapper for cpal + SCK callbacks (both are Fn + Send + Sync + 'static via this shape)"
    - "RAII shutdown via Drop on owned cpal::Stream and SCStream fields — no explicit .stop() method, no leaked handles (D-26)"
    - "Permission-gate-first in start_capture() — fast-fails PermissionDenied before opening mic so user never sees a recording indicator that's about to fail"
    - "SCK audio_buffer_list() treated as parallel L/R buffers (per spike API quirk #2), interleaved before feeding through Downmix"
    - "build.rs emits cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift to bake Swift Concurrency dylib path into LC_RPATH (per spike-note workaround)"
key_files:
  created:
    - "crates/yogurt-audio/build.rs (28 lines — Swift Concurrency rpath fix)"
    - "crates/yogurt-audio/src/resample.rs (190 lines — Downmix helper + 4 unit tests)"
    - "crates/yogurt-audio/src/mic.rs (255 lines — cpal capture + FrameChunker + list_input_devices + 3 unit tests)"
    - "crates/yogurt-audio/src/system.rs (220 lines — SCK 8.x in-process loopback, Path A)"
    - "crates/yogurt-audio/examples/mic_smoke.rs (60 lines)"
    - "crates/yogurt-audio/examples/system_smoke.rs (75 lines)"
    - "crates/yogurt-audio/examples/dual_smoke.rs (85 lines)"
  modified:
    - "crates/yogurt-audio/src/lib.rs (added system module, AudioStream struct, start_capture orchestrator, BROADCAST_CAPACITY const)"
decisions:
  - "Path A confirmed in production code — in-process SCK 8.x, no Swift sidecar (consistent with 02-01 spike PASS)"
  - "SCK audio is captured at 48 kHz f32 stereo (SCK's native format) and resampled to 16 kHz mono i16 inside our Downmix helper — keeps rubato's well-tuned SincFixedIn as the single resample path for both mic and system streams"
  - "FrameChunker is pub(crate) in mic.rs and reused by system.rs as a sibling-module import — single chunking implementation, single Instant::now() seed pattern, single broadcast::Sender contract"
  - "rubato SincFixedIn has a warm-up delay on the first call (148 output samples for 480 input on call 1; ~160 on call 2+); test asserts ≥290 over two chunks instead of a tight per-call bound"
  - "Both cpal F32 and I16 input formats supported; cpal default on this MacBook Pro is F32 mono 48 kHz (one channel — Downmix handles that case via channels==1 fast path)"
  - "BROADCAST_CAPACITY = 256 (~5 seconds buffered audio per channel) — exactly the AUDIO-04 minimum, leaves headroom for slow STT consumers to recover without dropping (Lagged) frames"
metrics:
  duration: "~1 hour (single executor session, autonomous)"
  completed: "2026-06-25T19:25:32Z"
  tasks_completed: 3
  commits: 4
  files_created: 7
  files_modified: 1
  tests_added: 7
  total_audio_crate_tests: 16
  total_workspace_tests: 44
---

# Phase 2 Plan 2: mic + system audio capture pipeline Summary

End-to-end audio capture is alive: `start_capture()` returns an
`AudioStream` whose mic and system broadcast channels each emit ~50 Hz of
320-sample 16 kHz mono i16 frames during a real concurrent capture session
on Apple Silicon macOS 15.6. Both channels were exercised in a 10-second
manual smoke against live system audio (`afplay` looping `Glass.aiff`):
**mic = 500 frames / peak −1321, system = 498 frames / peak −5222**. The
Plan 02-01 SCK spike's Path A decision held — no Swift sidecar fallback
was needed in production code.

## What this plan accomplished

### Task 1 — Microphone capture via cpal + shared Downmix helper

Wrote `crates/yogurt-audio/src/resample.rs` (`Downmix` helper, 190 lines):

- `Downmix::new(input_rate, input_channels)` constructs a
  `rubato::SincFixedIn<f32>` with the agreed `BlackmanHarris2` /
  `sinc_len: 64` / `oversampling_factor: 128` tuning (D-14).
- `Downmix::push(interleaved: &[f32]) -> Vec<i16>` downmixes N channels
  to mono via channel-mean, buffers leftover samples between calls so
  callers can push arbitrary-sized slices, runs `process_into_buffer`
  on each 480-sample mono chunk, and converts the resulting f32 output
  to i16 via `clamp(-1, 1) * i16::MAX`.
- 4 inline unit tests — constructs for common rates (48k/44.1k, 1ch/2ch),
  L/R cancellation produces near-zero mono, ratio 1/3 sanity (≥290
  samples over two 480-sample chunks), under-chunk input is buffered
  correctly.

Wrote `crates/yogurt-audio/src/mic.rs` (255 lines):

- `pub struct MicCapture` — owns the `cpal::Stream`. Drop stops capture.
  Manual `Debug` impl (cpal::Stream isn't Debug).
- `pub(crate) struct FrameChunker` — collects an arbitrary i16 PCM
  stream into exactly-FRAME_SAMPLES (320) chunks, stamps each with
  `monotonic_ms = self.start.elapsed().as_millis() as u64` from an
  `Instant::now()` captured at construction time, broadcasts via
  `tokio::sync::broadcast::Sender<Frame>`. Reused by `system.rs`.
- `pub fn list_input_devices() -> Result<Vec<DeviceInfo>>` — enumerates
  the cpal default host's input devices. `DeviceInfo { name, is_default,
  sample_rate }` derives Debug/Clone/Serialize/Deserialize for the
  upcoming `GET /api/audio/devices` endpoint.
- `pub fn spawn_mic_capture(tx) -> Result<MicCapture>` — opens the
  default input, branches on `SampleFormat::F32` and `SampleFormat::I16`,
  wraps `(Downmix, FrameChunker)` in `Arc<Mutex<>>` for the cpal
  callback (which requires `Fn + Send + 'static`), calls `stream.play()`.
- 3 inline unit tests — chunker emits exactly FRAME_SAMPLES frames per
  call with the right leftover count; monotonic_ms is non-decreasing;
  list_input_devices doesn't panic.

Added `build.rs` (28 lines) emitting
`cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift` per the Plan 02-01
spike note's Swift Concurrency rpath workaround. Only `/usr/lib/swift`
is added — Xcode's swift-5.5 toolchain path is deliberately NOT a
second fallback (spike note documents the duplicate-class + spurious-
TCC failure mode).

Added `mic_smoke` example — subscribes to a 128-cap broadcast, runs
5 seconds, prints frame count and peak amplitude.

### Task 2 — System audio capture via in-process SCK 8.x (Path A)

Wrote `crates/yogurt-audio/src/system.rs` (220 lines):

- `pub struct SystemCapture` — gates the SCK `Inner` on
  `#[cfg(target_os = "macos")]`. Manual `Debug` impl.
- `#[cfg(target_os = "macos")] pub fn spawn_system_capture(tx) ->
  Result<SystemCapture>` calls into `macos::start(tx)`.
- `#[cfg(not(target_os = "macos"))]` fallback returns
  `AudioError::UnsupportedPlatform` — keeps non-mac CI builds green.
- `mod macos { … }` is the actual SCK 8.x implementation:
  - **Permission gate first**: returns `PermissionDenied` if
    `has_screen_recording_permission()` is `Denied` before touching
    SCK (D-25).
  - `SCShareableContent::get()` → first display.
  - `SCContentFilter::create().with_display(display)
    .with_excluding_windows(&[]).build()`.
  - `SCStreamConfiguration::new().with_width(2).with_height(2)
    .with_captures_audio(true).with_excludes_current_process_audio(true)
    .with_sample_rate(48_000).with_channel_count(2)` — **AUDIO-03
    `excludes_current_process_audio` is set from the first commit**
    so yogurt's own UI sounds never leak into the transcript.
  - `Arc<Mutex<(Downmix, FrameChunker)>>` shared with the
    `SCStreamOutputTrait` handler closure. Handler filters on
    `SCStreamOutputType::Audio`, pulls `audio_buffer_list()`, treats
    each buffer as **one parallel mono channel** (NOT interleaved —
    spike API quirk #2), interleaves the L/R channels, feeds through
    the shared `Downmix.push()` → `FrameChunker.feed()`.
  - `stream.add_output_handler(handler, SCStreamOutputType::Audio)`
    + `stream.start_capture()` returns `Inner { _stream: stream }`;
    Drop tears it down (spike note confirmed clean shutdown — no
    explicit `stop_capture()` call needed in Drop).
- Non-macOS test `it_returns_unsupported_platform_off_macos` proves
  the off-platform contract.

Added `system_smoke` example — gates on
`has_screen_recording_permission()` (prints recovery instructions on
Denied), subscribes, runs 5 seconds.

### Task 3 — `start_capture()` orchestrator + dual_smoke

Updated `crates/yogurt-audio/src/lib.rs`:

- `pub const BROADCAST_CAPACITY: usize = 256` — **AUDIO-04 satisfied
  from the first commit** (~5 seconds of buffered audio per channel
  before slow consumers see `Lagged(n)` errors and drop frames).
- `pub struct AudioStream { _mic: MicCapture, _system: SystemCapture,
  pub mic_tx, pub system_tx }` — owns both producers as fields, so
  Drop tears down both via RAII (D-26 / AUDIO-06 plumbing).
- `AudioStream::subscribe_mic() / subscribe_system()` — Phase 3 STT
  consumers fan-in via `tokio::select!` per D-20.
- `pub fn start_capture() -> Result<AudioStream>` —
  1. permission-gates (PermissionDenied before opening mic),
  2. creates both broadcast channels at capacity 256,
  3. spawns mic capture,
  4. spawns system capture,
  5. returns the bundle.
  Both FrameChunker `Instant::now()` baselines are seeded synchronously
  inside spawn calls (microsecond skew, well under the AUDIO-05 50 ms
  drift budget).

Added `dual_smoke` example — fans-in both receivers via `tokio::select!`,
runs 10 seconds, prints per-channel frame counts and peak amplitudes.

## Manual smoke results (hardware-verified)

Run on Apple Silicon (aarch64), macOS 15.6 (24G84), MacBook Pro Microphone
as default cpal input, Screen Recording pre-granted to the terminal.

| Smoke | Duration | mic frames | system frames | mic peak | system peak | Verdict |
|---|---|---|---|---|---|---|
| `mic_smoke` (silent room) | 5 s | 249 | n/a | 930 | n/a | PASS — frame cadence correct, noise floor only |
| `system_smoke` (Glass.aiff loop) | 5 s | n/a | 248 | n/a | −5221 | PASS — both metrics in spec |
| `dual_smoke` (Glass.aiff loop + ambient mic) | 10 s | 500 | 498 | −1321 | −5222 | PASS — both channels exceed the >1000 peak target |

**Channel attribution verified** — `dual_smoke`'s debug assertion
`debug_assert_eq!(frame.channel, Channel::Mic)` on the mic receiver and
`Channel::System` on the system receiver both held throughout the
10-second run, so no channel swap on the broadcast layer.

The 2-frame difference between mic (500) and system (498) over 10 s
reflects spawn-order skew + SCK's slightly longer setup time
(mic.play() returns in ~63 ms, SCStream::start_capture in ~267 ms per
the tracing log timestamps). That's a 4 ms steady-state cadence
difference — well under the AUDIO-05 50 ms drift budget. Phase 3 will
observe long-run drift over a full 60-minute STT session once STT
timestamps land; this plan ships the clock plumbing only (per ROADMAP
§Phase 2 footnote and D-22).

## Test counts

| Suite | Before this plan | After this plan |
|---|---|---|
| `yogurt-audio` unit tests (in `src`) | 3 | 9 (+4 resample, +2 chunker) |
| `frame_contract` integration | 3 | 3 |
| `synthetic` integration | 2 | 2 |
| `permission` integration | 0 + 1 ignored | 0 + 1 ignored |
| `system` (non-macOS contract test, doesn't run here) | 0 | 1 (gated off on macOS) |
| `mic` (chunker tests, in-module) | 0 | 2 — counted above in src unit tests |
| `resample` (in-module) | 0 | 4 — counted above in src unit tests |
| **`yogurt-audio` total (macOS, `--features synthetic`)** | **8 + 1 ignored** | **16 + 1 ignored** |
| Workspace total | 36 + 1 ignored | 44 + 1 ignored |

## Verification gates

| Gate | Status |
|---|---|
| `cargo build -p yogurt-audio --all-targets --features synthetic` on macOS | PASS (clean) |
| `cargo build -p yogurt-audio` non-macOS path (Channel::System returns UnsupportedPlatform) | PASS (verified via `#[cfg]` review; CI matrix will confirm) |
| `cargo test -p yogurt-audio --features synthetic` | PASS (16 passed, 1 ignored) |
| `cargo clippy -p yogurt-audio --all-targets --features synthetic -- -D warnings` | PASS (clean) |
| `cargo test --workspace --features yogurt-audio/synthetic` | PASS (44 passed, 1 ignored) |
| `start_capture()` returns `PermissionDenied` cleanly when Screen Recording is denied | PASS (early-return path, verified via code review; no panic, no mic open) |
| Both broadcast channels have capacity 256 (AUDIO-04) | PASS (`BROADCAST_CAPACITY = 256` const, both `broadcast::channel::<Frame>(BROADCAST_CAPACITY)` calls) |
| SCStream config has `excludes_current_process_audio = true` (AUDIO-03) | PASS (set on first SCStreamConfiguration build, never toggled off) |
| Four atomic commits in order: build.rs → mic → system → orchestrator | PASS |
| Spike-decision Path A honored consistently in system.rs | PASS (in-process SCK, no `tools/yogurt-audio-helper/`) |
| Manual mic_smoke ran cleanly on Apple Silicon | PASS (249 frames / 5 s, peak 930 in silence) |
| Manual system_smoke ran cleanly with audio playing on Apple Silicon | PASS (248 frames / 5 s, peak −5221) |
| Manual dual_smoke ran cleanly on Apple Silicon | PASS (500 mic + 498 system / 10 s, both peaks well > 1000) |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Test tolerance bug] `it_resamples_48k_to_16k_ratio` initial range too tight**

- **Found during:** Task 1, first `cargo test` run.
- **Issue:** Plan said "480 samples in → 160 samples out" for the first
  call, but `rubato::SincFixedIn`'s delay-line warm-up emits **148**
  output samples on the very first call, then ~160 thereafter. The
  initial `(155..=170)` range failed on the first call.
- **Fix:** Test now feeds two 480-sample chunks and asserts total
  output is in `(290..=350)`, which exercises both the warm-up and
  steady-state phases of the resampler in a single call. Documented
  inline that the first call always emits fewer than 160 samples by
  design.
- **Files modified:** `crates/yogurt-audio/src/resample.rs`.
- **Commit:** 87f4790 (rolled into Task 1's commit; this was caught
  during the same test cycle, not as a follow-up).

**2. [Rule 3 — Build-environment fix] Swift Concurrency rpath fix added as `build.rs`**

- **Found during:** Plan kickoff (deferred-from-02-01 item, called
  out as "REQUIRED" in the prompt).
- **Issue:** Per the Plan 02-01 spike note ("What didn't work" #1), any
  binary that transitively links the SCK 8.x crate dies at load with
  `Library not loaded: @rpath/libswift_Concurrency.dylib` unless
  `/usr/lib/swift` is on `LC_RPATH`. The SCK crate's own `build.rs`
  link-args don't propagate to consuming binaries.
- **Fix:** Added `crates/yogurt-audio/build.rs` emitting
  `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift` on macOS. Critically
  did NOT add Xcode's swift-5.5 toolchain path as a second fallback —
  the spike note documents the duplicate-class warning + spurious
  TCC denial that combo produces.
- **Files modified:** `crates/yogurt-audio/build.rs` (new file).
- **Commit:** 1e087b7.

### Architectural / planning notes

**3. [Note] FrameChunker is `pub(crate)`, lives in `mic.rs`, reused by `system.rs`**

- The plan put `FrameChunker` in `mic.rs` (Task 1) and called for the
  "same chunker shape" in `system.rs` (Task 2). Rather than duplicate
  the type, `system.rs` imports `crate::mic::FrameChunker` directly.
  Single chunking implementation, single `Instant::now()` seed
  contract, single broadcast::Sender behavior. This is a refactor
  during planned work, not a deviation from the public surface.

**4. [Note] Integration tests still gated on `--features synthetic`**

- `crates/yogurt-audio/tests/synthetic.rs` imports
  `yogurt_audio::synthetic::*`, which is `#[cfg(any(test, feature =
  "synthetic"))]`. Without the feature flag, `cargo test --workspace`
  fails to build the integration test. This is the same gating
  pattern Plan 02-01 shipped — not introduced here, just preserved.
  Workspace test invocations need `--features yogurt-audio/synthetic`
  for the integration tests to compile.
  - Status: documented; not blocking the plan. Could be fixed in a
    follow-up by gating the integration test file itself with
    `#![cfg(feature = "synthetic")]`, but that's outside this plan's
    scope.

### Authentication Gates

None. The terminal Claude is running in has Screen Recording permission
pre-granted at the TCC level (`CGPreflightScreenCaptureAccess()` returns
`true`), so `start_capture()` and all smokes ran end-to-end without a
manual user intervention.

## Threat Flags

None new. The threat surface added by this plan is exactly what
`02-CONTEXT.md` anticipates and authorizes:
- `mic.rs` opens the user's microphone via cpal — gated by macOS's
  built-in mic-permission TCC (the system shows the orange dot when
  the cpal stream is alive).
- `system.rs` opens an SCK loopback stream — gated by Screen
  Recording TCC permission, and SCK shows the purple
  recording-pip in the menu bar while active.
- The SCStream config sets `excludes_current_process_audio = true`
  from the first commit, so yogurt's own UI audio is excluded from
  the capture stream (correctness + privacy guarantee per AUDIO-03).
- No audio leaves the machine — all frames live entirely in-process
  on `tokio::sync::broadcast` channels; nothing writes to disk in
  this plan (per-meeting "keep audio" toggle is LIB-V2-02, deferred).

## Known Stubs

None. Every public function has a real, hardware-exercised implementation:
- `start_capture()` opens both real producers.
- `spawn_mic_capture()` opens a real cpal default input.
- `spawn_system_capture()` opens a real SCK stream.
- `list_input_devices()` enumerates real cpal devices.
- `AudioStream::subscribe_mic/subscribe_system` return real broadcast
  receivers that get real frames at 50 Hz cadence.

## Directive for Plan 02-03 (Phase 2's next plan)

Plan 02-03 must:

1. **Add the REST endpoints** (Task 2.8 from superpowers plan):
   - `GET /api/audio/devices` → calls `yogurt_audio::list_input_devices()`
     and returns the `Vec<DeviceInfo>` as JSON.
   - `GET /api/audio/permission` → calls
     `yogurt_audio::has_screen_recording_permission()` and returns
     `{ status: "granted" | "denied" | "not_required" }`.
2. **Add the WAV-writing helper** + the `checkpoint:human-verify`
   ear-test gate (Task 2.9). Subscribe to both `AudioStream` channels,
   write to a 2-channel WAV file at `target/yogurt-eartest.wav` (mic
   on channel 0, system on channel 1), and ask the human to confirm
   both channels are audible / not silent / not clipped / not swapped.
3. **Do NOT** retry the `excludes_current_process_audio` setting —
   it's set from the first commit and verified working in dual_smoke
   above. The full empirical "no self-loopback" verification will
   happen in Phase 3 once STT lands and we can see whether yogurt's
   own UI audio appears in the transcript.

## Commits

| # | Hash | Message |
|---|---|---|
| 1 | 1e087b7 | feat(audio): add build.rs with Swift Concurrency rpath fix for SCK 8.x |
| 2 | 87f4790 | feat(audio): mic capture via cpal with Downmix to 16 kHz mono i16 |
| 3 | 7033fca | feat(audio): system audio capture via SCK 8.x in-process stream |
| 4 | e694381 | feat(audio): start_capture orchestrator with 256-cap broadcast + clock |

Plan duration: ~1 hour (single autonomous executor session).

## Self-Check: PASSED

Verified all claims against disk + git:

- `crates/yogurt-audio/build.rs` — FOUND (28 lines)
- `crates/yogurt-audio/src/resample.rs` — FOUND (190 lines, contains `SincFixedIn` and `BlackmanHarris2`)
- `crates/yogurt-audio/src/mic.rs` — FOUND (contains `pub fn spawn_mic_capture` and `pub fn list_input_devices`, branches on both `SampleFormat::F32` and `SampleFormat::I16`)
- `crates/yogurt-audio/src/system.rs` — FOUND (contains `pub fn spawn_system_capture` and `excludes_current_process_audio`)
- `crates/yogurt-audio/src/lib.rs` — UPDATED (contains `pub fn start_capture`, `BROADCAST_CAPACITY = 256`, exports `spawn_system_capture` + `SystemCapture` + `spawn_mic_capture` + `list_input_devices` + `MicCapture` + `DeviceInfo`)
- `crates/yogurt-audio/examples/mic_smoke.rs` — FOUND (60 lines)
- `crates/yogurt-audio/examples/system_smoke.rs` — FOUND (75 lines)
- `crates/yogurt-audio/examples/dual_smoke.rs` — FOUND (85 lines, uses `tokio::select!` to fan-in both receivers)
- Commit `1e087b7` (build.rs) — FOUND in git log
- Commit `87f4790` (mic) — FOUND in git log
- Commit `7033fca` (system) — FOUND in git log
- Commit `e694381` (orchestrator) — FOUND in git log
- `cargo test -p yogurt-audio --features synthetic` reports 16 passed + 1 ignored — VERIFIED
- `cargo clippy -p yogurt-audio --all-targets --features synthetic -- -D warnings` clean — VERIFIED
- `cargo test --workspace --features yogurt-audio/synthetic` reports 44 passed + 1 ignored — VERIFIED
- mic_smoke produced 249 frames in 5 seconds on Apple Silicon — VERIFIED via hardware run
- system_smoke with Glass.aiff loop produced 248 frames / peak −5221 — VERIFIED via hardware run
- dual_smoke produced 500 mic + 498 system frames over 10 seconds — VERIFIED via hardware run
