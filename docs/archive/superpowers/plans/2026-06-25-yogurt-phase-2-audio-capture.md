# Yogurt v1 — Phase 2: Audio Capture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a `yogurt-audio` crate that captures **two synchronized 16 kHz mono PCM streams** — microphone (via `cpal` / CoreAudio) and macOS system audio (via the `screencapturekit` crate / ScreenCaptureKit framework) — and pushes both into Tokio broadcast channels for downstream STT consumers (Phase 3). Expose the capture lifecycle and a typed `PermissionStatus` to `yogurt-server` via two new REST endpoints (`GET /api/audio/devices`, `GET /api/audio/permission`) so the Phase 7 onboarding UI can render the §5.10 / §5.11 permission flows. **No STT, no WebSocket plumbing, no UI rendering of permission state in this phase** — capture and emit only.

**Architecture:** New `crates/yogurt-audio` crate sits behind a small public surface (`AudioStream`, `Channel`, `Frame`, `start_capture()`, `has_screen_recording_permission()`). The `Channel::Mic` capture path uses `cpal` and runs on every platform; `Channel::System` uses `screencapturekit` and is `#[cfg(target_os = "macos")]`-gated, returning a typed `AudioError::UnsupportedPlatform` elsewhere. Both producers run on dedicated threads (cpal/SCK use native callbacks); each callback resamples / channel-mixes into 16 kHz mono `i16` and forwards to a `tokio::sync::broadcast::Sender<Frame>`. `yogurt-server` gets a thin `audio` module that re-exports `start_meeting_recording()` returning `(BroadcastReceiver<Frame>, BroadcastReceiver<Frame>)` plus the two REST handlers — but no WebSocket wiring yet (that's Phase 3).

**Tech Stack:** Rust 1.83+ · `cpal` 0.15 · `screencapturekit` 0.3 (verify on crates.io in Step 0) · `tokio` 1 (broadcast, sync, task) · `rubato` 0.16 for sample-rate conversion · `thiserror` 2 for typed errors · `objc2` / `objc2-foundation` (transitive via `screencapturekit`) · macOS 13+ for SCK · `serde` for the REST response DTOs.

**Reference:**
- `docs/PRD.md` §5.1 (Record meeting — defines the 16 kHz / 16-bit / mono / two-channel contract).
- `docs/PRD.md` §5.10 (Onboarding — Screen Recording permission step).
- `docs/PRD.md` §5.11 (Permission-denied recovery screen).
- `docs/PRD.md` §7 (Architecture — in-process audio, no subprocesses preferred).
- `docs/PRD.md` §10 (API surface — `GET /api/audio/devices` is listed in REST table).
- `docs/PRD.md` §13 risk #1 (SCK crate may have audio-loopback gaps → Swift sidecar fallback).
- `docs/superpowers/plans/2026-06-25-yogurt-phase-0-skeleton.md` (workspace + server scaffolding produced by Phase 0).

**Out of scope (deferred to later phase plans):**
- STT engine consumption of the broadcast channel (Phase 3 — Deepgram cloud adapter).
- Local whisper.cpp STT (Phase 8).
- WebSocket `/ws/meetings/:id` endpoint that fan-outs `transcript` events (Phase 3).
- UI rendering of the permission recovery screen (Phase 7 — onboarding plan).
- Settings UI "input device" dropdown (Phase 5 — settings plan; this phase ships only the REST endpoint it consumes).
- Persistent storage of captured PCM to disk (out of v1 entirely — PRD §2 non-goals).
- Voice activity detection, noise suppression, AGC (deferred to v2+).

---

## File structure produced by this phase

```
yogurt/
├── Cargo.toml                              # MODIFY · add yogurt-audio to members; new workspace deps
├── crates/
│   ├── yogurt-audio/                       # NEW crate
│   │   ├── Cargo.toml                      # NEW
│   │   ├── README.md                       # NEW · contract: 16kHz mono i16, two channels
│   │   ├── src/
│   │   │   ├── lib.rs                      # NEW · public AudioStream / Frame / Channel / start_capture
│   │   │   ├── error.rs                    # NEW · typed AudioError enum (thiserror)
│   │   │   ├── frame.rs                    # NEW · Frame struct + Channel enum + format consts
│   │   │   ├── permission.rs               # NEW · has_screen_recording_permission() + PermissionStatus
│   │   │   ├── resample.rs                 # NEW · 48k→16k mono i16 helper (used by both producers)
│   │   │   ├── mic.rs                      # NEW · cpal mic capture producer
│   │   │   ├── system.rs                   # NEW · screencapturekit system loopback producer
│   │   │   └── synthetic.rs                # NEW · sine-wave PCM generator (test fixture + future debug mode)
│   │   ├── tests/
│   │   │   ├── synthetic.rs                # NEW · broadcast plumbing unit test using sine-wave frames
│   │   │   └── permission.rs               # NEW · manual smoke checklist for permission detection
│   │   └── benches/                        # (intentionally empty — perf comes later)
│   └── yogurt-server/
│       ├── Cargo.toml                      # MODIFY · add yogurt-audio path dep
│       └── src/
│           ├── lib.rs                      # MODIFY · register new audio routes
│           ├── audio.rs                    # NEW · start_meeting_recording() + REST handlers
│           └── routes.rs                   # MODIFY · mount /api/audio/* endpoints
└── docs/
    └── superpowers/plans/
        └── 2026-06-25-yogurt-phase-2-audio-capture.md   # this file
```

**Why this split:** `yogurt-audio` is the only crate that touches native macOS audio APIs. Keeping it isolated means (a) future Windows/Linux ports are additive (PRD §5.8), (b) the rest of the workspace stays platform-portable for unit tests, and (c) if the Swift sidecar fallback (per §13 risk #1) becomes necessary, the swap stays inside this one crate.

---

## The audio format contract (load-bearing for Phase 3)

Every `Frame` emitted by **either** producer MUST conform to:

| Field | Value | Why |
|---|---|---|
| Sample rate | **16,000 Hz** | What `whisper.cpp` and Deepgram's `linear16` PCM stream both expect natively. Any other rate forces a resample at the STT boundary. |
| Channels | **1 (mono)** | Mic input is mono-only on most hardware; system loopback is downmixed L+R → mono before emit. Phase 3 STT engines are mono-only. |
| Sample format | **`i16` (signed 16-bit LE)** | Matches Deepgram's `linear16` and whisper.cpp's `pcm_s16le`. No `f32`-to-`i16` quantization in the STT layer. |
| Buffer length | **320 samples (= 20 ms @ 16 kHz)** | 20 ms is the canonical streaming-STT chunk for low-latency partials. |
| `Channel` tag | `Channel::Mic` or `Channel::System` | Phase 3 routes mic → "Me" (ink black) and system → "Them" (grey) per PRD §5.2. |
| `monotonic_ms` | `u64` since `start_capture()` returned | Phase 3 uses this to align partial transcripts with notes via `↳ HH:MM` deep-links (§5.3). Wall-clock is recorded separately at session start. |

These constants live in `crates/yogurt-audio/src/frame.rs` as `pub const SAMPLE_RATE_HZ: u32 = 16_000;` etc. Phase 3 should `use yogurt_audio::{SAMPLE_RATE_HZ, FRAME_SAMPLES, Frame, Channel};` — never hardcode the numbers.

---

## Test conventions specific to this phase

- **Pure-logic unit tests** (resampler, frame chunking, synthetic generator, error mapping) — `#[cfg(test)]` modules inline, no platform gates.
- **Broadcast-plumbing integration tests** — `crates/yogurt-audio/tests/synthetic.rs` feeds sine-wave PCM through the same broadcast path the real producers use, verifying receivers get exactly the bytes the producer sent. Runs on **all platforms** (no SCK / cpal device access).
- **Permission detection test** — `crates/yogurt-audio/tests/permission.rs` is gated `#[cfg(target_os = "macos")]` and `#[ignore]` by default. It is a **manual smoke test** with a printed checklist; CI does not run it. See Task 2.7 for the checklist contents.
- **Real-device capture tests** — none in this phase. Real SCK / cpal exercise happens in Task 2.8 manual smoke, *not* as `cargo test`. Reason: GitHub Actions runners have no audio devices and no TCC permission; failing CI on a missing mic would block every PR.
- **Runtime opt-in flag for live capture in tests.** A `YOGURT_AUDIO_LIVE=1` env var unlocks any test that would touch real devices. Default off. This keeps `cargo test --workspace` green on dev laptops without prompting for Screen Recording on every test run.

---

## Phase 2 task list

9 tasks. Each task ends with a commit. Approximate sequence: 2 days (~12–14 hours of focused work, with the SCK spike concentrated in Task 2.0 and Task 2.5).

---

### Task 2.0 · SPIKE — verify `screencapturekit` crate supports audio-only loopback

> **This task is mandatory and runs first.** Per PRD §13 risk #1, the `screencapturekit` crate is "mainly designed for screen capture." Before we build the production code paths in Task 2.5, we need a 30-minute spike that proves a one-stream audio-only ScreenCaptureKit configuration captures real system audio on this Mac. If it doesn't, we switch the Task 2.5 implementation to the Swift sidecar pattern documented at the bottom of this task.

**Files:**
- Create: `crates/yogurt-audio/spike/Cargo.toml` (throwaway — deleted at end of Task 2.0 Step 8)
- Create: `crates/yogurt-audio/spike/src/main.rs` (throwaway)
- Create: `docs/superpowers/notes/2026-06-25-sck-spike-result.md` (committed; documents the decision)

- [ ] **Step 1: Check the latest `screencapturekit` crate version.**

Run: `cargo search screencapturekit --limit 5`
Expected: a row like `screencapturekit = "0.3.x"`. Note the exact latest version — if it's newer than `0.3` use that and update `[workspace.dependencies]` in Task 2.1 accordingly. If only `0.2.x` exists, **stop and re-check** before proceeding (the public API differs substantially).

- [ ] **Step 2: Create a throwaway spike crate outside the workspace.**

The spike crate lives at `crates/yogurt-audio/spike/` but is **not** added to the workspace members — keep it isolated so a half-working spike never affects the main build. Add `crates/yogurt-audio/spike/` to `.gitignore` for the duration of the spike (we delete it in Step 8 anyway).

Write `crates/yogurt-audio/spike/Cargo.toml`:

```toml
[package]
name = "sck-spike"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
screencapturekit = "0.3"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

- [ ] **Step 3: Write `crates/yogurt-audio/spike/src/main.rs`.**

Goal: configure a `SCStream` with `captures_audio: true`, `excludes_current_process_audio: true`, no video output, and log every audio buffer arrival for 5 seconds. We don't decode or play it back — we only need to see (a) the permission dialog fire on first run, (b) buffer callbacks arrive, (c) the buffer carries non-zero PCM (which would mean SCK successfully tapped system audio).

```rust
use anyhow::Result;
use screencapturekit::{
    shareable_content::SCShareableContent,
    stream::{
        configuration::SCStreamConfiguration,
        content_filter::SCContentFilter,
        output_type::SCStreamOutputType,
        SCStream, SCStreamDelegate,
    },
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct AudioCounter(Arc<AtomicUsize>);

impl SCStreamDelegate for AudioCounter {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: screencapturekit::cm_sample_buffer::CMSampleBuffer,
        of_type: SCStreamOutputType,
    ) {
        if matches!(of_type, SCStreamOutputType::Audio) {
            // Pull the audio buffer list and count non-zero bytes.
            // (Exact API depends on screencapturekit 0.3.x — adjust if needed.)
            let len = sample_buffer
                .get_audio_buffer_list()
                .map(|abl| abl.total_bytes())
                .unwrap_or(0);
            self.0.fetch_add(len, Ordering::Relaxed);
            println!("audio buffer: {} bytes (total {})", len, self.0.load(Ordering::Relaxed));
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let shareable = SCShareableContent::get().await?;
    let display = shareable.displays().first().cloned().expect("no display");

    let filter = SCContentFilter::new_with_display(display);
    let mut config = SCStreamConfiguration::new();
    config.set_captures_audio(true);
    config.set_excludes_current_process_audio(true);
    config.set_width(2);   // minimum allowed; we ignore video
    config.set_height(2);
    config.set_minimum_frame_interval(std::time::Duration::from_millis(1000));

    let counter = Arc::new(AtomicUsize::new(0));
    let delegate = AudioCounter(counter.clone());

    let mut stream = SCStream::new(filter, config, delegate);
    stream.add_output(SCStreamOutputType::Audio);
    stream.start_capture().await?;

    println!("capturing 5s — play any system audio now (Spotify, YouTube, beep, …)");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    stream.stop_capture().await?;

    let total = counter.load(Ordering::Relaxed);
    println!("\n=== SPIKE RESULT ===");
    println!("total audio bytes captured: {}", total);
    if total == 0 {
        anyhow::bail!("FAIL: zero bytes — SCK is not delivering audio. Fall back to Swift sidecar.");
    }
    println!("PASS: SCK delivered audio. Continue with Task 2.1.");
    Ok(())
}
```

> **API caveat:** the exact method names on `screencapturekit` 0.3 may differ from this sketch (e.g. `get_audio_buffer_list` might be `audio_buffer_list()`, `set_captures_audio` might be `set_capture_audio`). When the spike fails to compile, read `cargo doc --open -p screencapturekit` and adjust. The point of the spike is to find these mismatches *before* we wire the production code.

- [ ] **Step 4: Build + run the spike with audio playing.**

Open Spotify (or any source of system audio) in a separate window. Then:

```bash
cd crates/yogurt-audio/spike
cargo run --release
```

On first run, macOS triggers the Screen Recording permission dialog. Grant it. The dialog will say "sck-spike would like to record this computer's screen and audio" — that wording is fine for the spike; the production binary will say "yogurt" once it's signed in Phase 9.

After granting, you must **quit and re-run** (macOS TCC requires a relaunch for the grant to take effect — this is the same "restart once" friction documented in PRD §5.10).

Play audio for the full 5 seconds.

- [ ] **Step 5: Interpret the result and decide the Task 2.5 path.**

Three possible outcomes:

| Outcome | Decision |
|---|---|
| **PASS** — non-zero bytes logged. | Proceed with the **in-process** SCK implementation in Task 2.5. Document in `2026-06-25-sck-spike-result.md` what version of `screencapturekit` worked and any API quirks discovered. |
| **PARTIAL** — buffer callbacks fire but bytes are all zero. | Likely an `excludes_current_process_audio` misconfig or microphone vs. system mix-up. Spend at most 30 more minutes debugging; if still zero, choose **Swift sidecar**. |
| **FAIL** — compile fails, no callbacks, or panic. | Switch Task 2.5 to the **Swift sidecar pattern** (see Step 7 below). Document the exact failure mode in `2026-06-25-sck-spike-result.md`. |

- [ ] **Step 6: Write the spike result note.**

Create `docs/superpowers/notes/2026-06-25-sck-spike-result.md`:

```markdown
# SCK audio-only loopback spike — result

**Date:** 2026-06-25
**Machine:** <e.g. MacBook Pro M3 Max, macOS 15.0>
**`screencapturekit` version tested:** <0.3.x>

## Outcome

<PASS / PARTIAL / FAIL>

## What worked / what didn't

<bullet list — at minimum: did permission dialog fire? did buffers arrive? were bytes non-zero? any API mismatches between the spike code and the actual 0.3 surface?>

## Decision

<"Proceed with in-process SCK (Task 2.5 path A)" OR "Switch to Swift sidecar (Task 2.5 path B)">

## Notes for Task 2.5 implementer

<any API quirks, e.g. "must call SCStreamConfiguration::set_width(2) or capture fails", "buffer list iter requires explicit lock", etc.>
```

- [ ] **Step 7: (Fallback) Swift sidecar design — read only if Step 5 said FAIL or PARTIAL.**

If we need the sidecar, the design is:
1. A 150-line Swift binary `tools/yogurt-audio-helper/main.swift` that uses Apple's first-party `SCStream` API directly (no Rust binding).
2. The helper writes 16 kHz mono `i16` PCM to **stdout** as a continuous byte stream. No framing — Rust reads it in 640-byte chunks (320 samples × 2 bytes = 20 ms).
3. `crates/yogurt-audio/src/system.rs` becomes `tokio::process::Command::new("yogurt-audio-helper").stdout(Stdio::piped()).spawn()` and an async reader loop that builds `Frame`s from the stdout bytes.
4. The helper is built via `swift build -c release` and shipped in the Homebrew bottle next to `yogurt` (Phase 9 distribution problem — for now, just `cargo run` resolves it from `target/release/`).
5. Permission detection (`permission.rs`) doesn't change — it can still use `CGPreflightScreenCaptureAccess` via a tiny `objc2` call (no SCK dependency).

The **public surface of `yogurt-audio` stays identical** under either path. That's the whole point of the spike-first approach: the contract `start_capture() -> (mic_rx, system_rx)` doesn't leak whether system audio comes from in-process SCK or a subprocess.

If you take the sidecar path, **add `tools/yogurt-audio-helper/`** to the file structure section above and add Task 2.5.B (mirror of 2.5 but with subprocess spawning) instead of Task 2.5.A. Both paths still satisfy the same `tests/synthetic.rs` (Task 2.3) because that test doesn't touch real audio.

- [ ] **Step 8: Delete the spike directory and commit only the result note.**

```bash
rm -rf crates/yogurt-audio/spike/
git add docs/superpowers/notes/2026-06-25-sck-spike-result.md
git commit -m "docs(audio): record SCK audio-loopback spike result (phase 2 task 2.0)"
```

We commit the *decision*, not the throwaway code — the decision is what the next 6 hours of work hinge on.

---

### Task 2.1 · Bootstrap the `yogurt-audio` crate

**Files:**
- Modify: `Cargo.toml` (workspace root — add `yogurt-audio` to members + new workspace deps)
- Create: `crates/yogurt-audio/Cargo.toml`
- Create: `crates/yogurt-audio/src/lib.rs` (stub — real public API lands in Task 2.2)
- Create: `crates/yogurt-audio/README.md`

- [ ] **Step 1: Add `yogurt-audio` to the workspace members and pin the new shared deps.**

In the root `Cargo.toml`:

```toml
[workspace]
members = [
    "crates/yogurt-cli",
    "crates/yogurt-server",
    "crates/yogurt-audio",          # NEW
]
```

In `[workspace.dependencies]`, append:

```toml
# Audio capture
cpal = "0.15"
screencapturekit = "0.3"
rubato = "0.16"
# Typed errors
thiserror = "2"
```

(Keep the existing tokio / serde / anyhow / tracing entries; `yogurt-audio` reuses them via `workspace = true`.)

- [ ] **Step 2: Write `crates/yogurt-audio/Cargo.toml`.**

```toml
[package]
name = "yogurt-audio"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "macOS audio capture (mic + system loopback) for yogurt — 16 kHz mono i16."

[dependencies]
tokio = { workspace = true }
tracing = { workspace = true }
anyhow = { workspace = true }
serde = { workspace = true }
thiserror = { workspace = true }
cpal = { workspace = true }
rubato = { workspace = true }

[target.'cfg(target_os = "macos")'.dependencies]
screencapturekit = { workspace = true }
# objc2 brings in the bindings we need for CGPreflightScreenCaptureAccess
# in permission.rs without dragging the rest of SCK in on non-macos targets.
objc2 = "0.5"
objc2-foundation = "0.2"

[dev-dependencies]
tokio = { workspace = true }
tracing-subscriber = { workspace = true }
```

> **Why `objc2` is direct, not transitive.** `screencapturekit` re-exports some `objc2` types, but `permission.rs` calls `CGPreflightScreenCaptureAccess` (a Core Graphics fn) — that's not part of SCK. We need our own objc2 import. Pinning `0.5` here matches what `screencapturekit` 0.3 uses; if a `cargo tree` shows a duplicate, bump to the version SCK uses.

- [ ] **Step 3: Write the placeholder `crates/yogurt-audio/src/lib.rs`.**

```rust
//! Yogurt audio capture — 16 kHz mono i16 PCM, two channels (mic + system).
//!
//! Phase 2 scope: capture only. STT is Phase 3.
//!
//! # Format contract
//! Every [`Frame`] emitted by [`start_capture`] is 16 kHz mono i16 PCM,
//! 320 samples (20 ms) per frame. See `crates/yogurt-audio/README.md`.

#![deny(rust_2018_idioms, missing_debug_implementations)]

// Real modules land in subsequent tasks; this stub just lets the workspace build.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

- [ ] **Step 4: Write `crates/yogurt-audio/README.md`.**

```markdown
# yogurt-audio

macOS audio capture for [yogurt](../../README.md). Two synchronized 16 kHz mono `i16` PCM streams:

- `Channel::Mic` — default input device via `cpal` / CoreAudio.
- `Channel::System` — system audio loopback via `screencapturekit` / ScreenCaptureKit. macOS 13+.

## Format contract (load-bearing for Phase 3)

| | |
|---|---|
| Sample rate | 16,000 Hz |
| Channels   | 1 (mono) |
| Sample fmt | `i16` (signed 16-bit LE) |
| Frame size | 320 samples (20 ms) |

Phase 3 STT engines (Deepgram, whisper.cpp) consume this format directly — no resampling at the STT boundary.

## Permissions

System loopback requires macOS Screen Recording permission. Call
[`has_screen_recording_permission`] before [`start_capture`] and surface
[`PermissionStatus::Denied`] to the user as the §5.11 recovery screen.
The OS prompts for permission on the first call to `start_capture` —
the user must restart yogurt once after granting (macOS TCC limitation).

## Non-macOS platforms

`Channel::System` returns `AudioError::UnsupportedPlatform` on non-macOS targets.
`Channel::Mic` works everywhere (`cpal` is cross-platform).
```

- [ ] **Step 5: Build to verify the workspace integrates.**

Run: `cargo build -p yogurt-audio`
Expected: clean build. `cargo metadata --no-deps | grep yogurt-audio` should list the package.

- [ ] **Step 6: Commit.**

```bash
git add Cargo.toml crates/yogurt-audio/
git commit -m "feat(audio): bootstrap yogurt-audio crate (stub, deps wired)"
```

---

### Task 2.2 · Public types — `Frame`, `Channel`, `AudioError`, format consts

**Files:**
- Create: `crates/yogurt-audio/src/frame.rs`
- Create: `crates/yogurt-audio/src/error.rs`
- Modify: `crates/yogurt-audio/src/lib.rs` (export the new types)

- [ ] **Step 1: Write the failing unit test for `Frame::from_samples` first.**

This is the smallest piece we can TDD before any platform code exists. Append to `crates/yogurt-audio/src/frame.rs` (file doesn't exist yet — Step 2 creates it; we write the test second since the module-level test sits at the bottom of the same file).

For now, write the test in a new file `crates/yogurt-audio/tests/frame_contract.rs`:

```rust
use yogurt_audio::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};

#[test]
fn it_exposes_format_constants() {
    assert_eq!(SAMPLE_RATE_HZ, 16_000);
    assert_eq!(FRAME_SAMPLES, 320, "20ms @ 16kHz = 320 samples");
}

#[test]
fn it_constructs_a_frame_with_correct_length() {
    let samples = vec![0i16; FRAME_SAMPLES];
    let f = Frame::new(Channel::Mic, 0, samples);
    assert_eq!(f.channel, Channel::Mic);
    assert_eq!(f.samples.len(), FRAME_SAMPLES);
    assert_eq!(f.monotonic_ms, 0);
}

#[test]
#[should_panic(expected = "FRAME_SAMPLES")]
fn it_panics_on_wrong_length() {
    let _ = Frame::new(Channel::Mic, 0, vec![0i16; 100]);
}
```

- [ ] **Step 2: Run — expect compile failure (types don't exist).**

Run: `cargo test -p yogurt-audio --test frame_contract`
Expected: `unresolved import yogurt_audio::Channel` and three other unresolved-import errors.

- [ ] **Step 3: Write `crates/yogurt-audio/src/frame.rs`.**

```rust
use serde::{Deserialize, Serialize};

/// Sample rate of every [`Frame`]. 16 kHz matches Deepgram `linear16` and
/// whisper.cpp `pcm_s16le` — Phase 3 STT engines consume this directly.
pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// Number of `i16` samples per frame. 20 ms @ 16 kHz = 320 samples.
/// 20 ms is the canonical streaming-STT chunk for low-latency partials.
pub const FRAME_SAMPLES: usize = 320;

/// Audio source. Phase 3 routes `Mic` → "Me" (ink black) and `System` →
/// "Them" (grey) per PRD §5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Mic,
    System,
}

/// One frame of 16 kHz mono i16 PCM. Length is always [`FRAME_SAMPLES`].
#[derive(Debug, Clone)]
pub struct Frame {
    pub channel: Channel,
    /// Milliseconds since `start_capture()` returned. Used by Phase 3 to align
    /// partial transcripts with notes via `↳ HH:MM` deep-links (PRD §5.3).
    pub monotonic_ms: u64,
    pub samples: Vec<i16>,
}

impl Frame {
    /// Construct a frame. Panics if `samples.len() != FRAME_SAMPLES` — this
    /// is a programmer error, not a runtime condition the user can recover from.
    pub fn new(channel: Channel, monotonic_ms: u64, samples: Vec<i16>) -> Self {
        assert_eq!(
            samples.len(),
            FRAME_SAMPLES,
            "Frame::new: samples.len()={} but FRAME_SAMPLES={}",
            samples.len(),
            FRAME_SAMPLES
        );
        Self { channel, monotonic_ms, samples }
    }
}
```

- [ ] **Step 4: Write `crates/yogurt-audio/src/error.rs`.**

```rust
use thiserror::Error;

/// All errors `yogurt-audio` can surface. Each variant maps to a distinct
/// user-facing recovery in the UI (the Phase 7 onboarding / §5.11 plan).
#[derive(Debug, Error)]
pub enum AudioError {
    /// macOS Screen Recording permission has not been granted. Phase 7 renders
    /// this as the §5.11 "Yogurt can't hear the call yet" recovery card.
    #[error("macOS Screen Recording permission is required for system audio capture")]
    PermissionDenied,

    /// The selected microphone device disappeared (unplugged, switched).
    #[error("microphone device unavailable: {0}")]
    MicUnavailable(String),

    /// SCK refused to start — usually a transient OS-level issue.
    #[error("system audio capture failed to start: {0}")]
    SystemCaptureFailed(String),

    /// We're not on macOS. Mic still works; system loopback does not.
    #[error("system audio capture is only supported on macOS 13+")]
    UnsupportedPlatform,

    /// Wrapped cpal error.
    #[error("cpal error: {0}")]
    Cpal(String),

    /// Wrapped IO error (sidecar stdout reads, etc.).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AudioError>;
```

- [ ] **Step 5: Update `crates/yogurt-audio/src/lib.rs` to expose the new types.**

```rust
//! Yogurt audio capture — 16 kHz mono i16 PCM, two channels (mic + system).
//!
//! Phase 2 scope: capture only. STT is Phase 3.

#![deny(rust_2018_idioms, missing_debug_implementations)]

mod error;
mod frame;

pub use error::{AudioError, Result};
pub use frame::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
```

- [ ] **Step 6: Run the tests — expect PASS.**

Run: `cargo test -p yogurt-audio --test frame_contract`
Expected: `3 passed`.

- [ ] **Step 7: Commit.**

```bash
git add crates/yogurt-audio/
git commit -m "feat(audio): add Frame/Channel/AudioError public types + format consts"
```

---

### Task 2.3 · Synthetic PCM generator + broadcast-plumbing integration test

**Files:**
- Create: `crates/yogurt-audio/src/synthetic.rs`
- Create: `crates/yogurt-audio/tests/synthetic.rs`
- Modify: `crates/yogurt-audio/src/lib.rs` (expose `synthetic` module behind `cfg(any(test, feature = "synthetic"))`)
- Modify: `crates/yogurt-audio/Cargo.toml` (add the `synthetic` feature)

This task lets us **build the entire broadcast-channel plumbing** before touching cpal or SCK. The synthetic generator emits a 440 Hz sine wave as a `tokio::task` that pushes `Frame`s into a `broadcast::Sender`. Tests subscribe and verify exactly what bytes arrived.

- [ ] **Step 1: Add the `synthetic` feature to `crates/yogurt-audio/Cargo.toml`.**

Append:

```toml
[features]
default = []
synthetic = []
```

Reason for a feature flag: Phase 5 settings might expose a "synthetic audio source" for users to debug their pipeline without a working mic, and the test path always enables it via `--features synthetic`.

- [ ] **Step 2: Write the failing integration test first.**

Create `crates/yogurt-audio/tests/synthetic.rs`:

```rust
use std::time::Duration;
use tokio::sync::broadcast;
use yogurt_audio::{
    synthetic::{spawn_sine_wave, SineWaveConfig},
    Channel, Frame, FRAME_SAMPLES,
};

#[tokio::test]
async fn it_emits_correct_length_frames_at_the_expected_cadence() {
    let (tx, mut rx) = broadcast::channel::<Frame>(64);
    let handle = spawn_sine_wave(
        SineWaveConfig {
            channel: Channel::Mic,
            frequency_hz: 440.0,
            amplitude: 16_000,
        },
        tx,
    );

    // Collect 5 frames (~100ms of audio).
    let mut frames = Vec::with_capacity(5);
    for _ in 0..5 {
        let f = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("frame arrived within 500ms")
            .expect("recv ok");
        frames.push(f);
    }
    handle.abort();

    for f in &frames {
        assert_eq!(f.channel, Channel::Mic);
        assert_eq!(f.samples.len(), FRAME_SAMPLES);
    }

    // Monotonic time should increase by roughly 20ms per frame.
    for w in frames.windows(2) {
        let dt = w[1].monotonic_ms.saturating_sub(w[0].monotonic_ms);
        assert!(
            (15..=40).contains(&dt),
            "expected ~20ms between frames, got {}ms",
            dt
        );
    }

    // Sine wave should produce non-zero, non-constant samples.
    let s = &frames[0].samples;
    assert!(s.iter().any(|&x| x != 0), "sine wave should not be all-zero");
    assert!(s.iter().any(|&x| x != s[0]), "sine wave should vary across samples");
}

#[tokio::test]
async fn multiple_subscribers_each_receive_the_same_frames() {
    let (tx, mut rx1) = broadcast::channel::<Frame>(64);
    let mut rx2 = tx.subscribe();
    let handle = spawn_sine_wave(SineWaveConfig::default_for(Channel::System), tx);

    let f1 = tokio::time::timeout(Duration::from_millis(500), rx1.recv())
        .await.unwrap().unwrap();
    let f2 = tokio::time::timeout(Duration::from_millis(500), rx2.recv())
        .await.unwrap().unwrap();
    handle.abort();

    assert_eq!(f1.monotonic_ms, f2.monotonic_ms);
    assert_eq!(f1.samples, f2.samples);
    assert_eq!(f1.channel, Channel::System);
}
```

- [ ] **Step 3: Run — expect compile failure (`synthetic` module doesn't exist).**

Run: `cargo test -p yogurt-audio --test synthetic --features synthetic`
Expected: `unresolved import yogurt_audio::synthetic`.

- [ ] **Step 4: Write `crates/yogurt-audio/src/synthetic.rs`.**

```rust
//! Synthetic sine-wave PCM generator. Used by tests to exercise broadcast
//! plumbing without touching real audio devices, and exposed via the
//! `synthetic` feature so Phase 5 can offer it as a debug input source.

use crate::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
use std::time::Duration;
use tokio::{sync::broadcast, task::JoinHandle, time::Instant};

#[derive(Debug, Clone)]
pub struct SineWaveConfig {
    pub channel: Channel,
    pub frequency_hz: f32,
    pub amplitude: i16,
}

impl SineWaveConfig {
    pub fn default_for(channel: Channel) -> Self {
        Self { channel, frequency_hz: 440.0, amplitude: 16_000 }
    }
}

/// Spawn a task that emits sine-wave frames at the real 20 ms cadence.
/// The returned handle can be `.abort()`-ed to stop generation.
pub fn spawn_sine_wave(
    cfg: SineWaveConfig,
    tx: broadcast::Sender<Frame>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let start = Instant::now();
        let mut frame_idx: u64 = 0;
        let mut interval = tokio::time::interval(Duration::from_millis(20));
        // Avoid catching up after a slow consumer.
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            interval.tick().await;
            let samples = generate_chunk(&cfg, frame_idx);
            let monotonic_ms = start.elapsed().as_millis() as u64;
            let frame = Frame::new(cfg.channel, monotonic_ms, samples);
            // If no receivers, broadcast::Sender::send returns Err — drop silently.
            let _ = tx.send(frame);
            frame_idx += 1;
        }
    })
}

fn generate_chunk(cfg: &SineWaveConfig, frame_idx: u64) -> Vec<i16> {
    let sr = SAMPLE_RATE_HZ as f32;
    let two_pi_f = 2.0 * std::f32::consts::PI * cfg.frequency_hz;
    let base_sample = frame_idx * FRAME_SAMPLES as u64;
    (0..FRAME_SAMPLES)
        .map(|i| {
            let t = (base_sample + i as u64) as f32 / sr;
            (cfg.amplitude as f32 * (two_pi_f * t).sin()) as i16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_generates_frame_sized_chunks() {
        let cfg = SineWaveConfig::default_for(Channel::Mic);
        let chunk = generate_chunk(&cfg, 0);
        assert_eq!(chunk.len(), FRAME_SAMPLES);
    }

    #[test]
    fn it_produces_continuous_phase_across_frames() {
        // The last sample of frame N and the first of frame N+1 should differ
        // by roughly one sample-period of the sine wave — proves we're using
        // global sample index, not per-frame index (which would click).
        let cfg = SineWaveConfig::default_for(Channel::Mic);
        let chunk_a = generate_chunk(&cfg, 0);
        let chunk_b = generate_chunk(&cfg, 1);
        let last_of_a = chunk_a[FRAME_SAMPLES - 1] as i32;
        let first_of_b = chunk_b[0] as i32;
        // 440 Hz at 16 kHz = ~36 samples per cycle, peak-to-peak amplitude
        // ~32000. Adjacent samples differ by at most ~2800.
        assert!(
            (last_of_a - first_of_b).abs() < 5_000,
            "phase discontinuity: last_of_a={last_of_a}, first_of_b={first_of_b}"
        );
    }
}
```

- [ ] **Step 5: Update `crates/yogurt-audio/src/lib.rs` to expose `synthetic`.**

```rust
#![deny(rust_2018_idioms, missing_debug_implementations)]

mod error;
mod frame;

#[cfg(any(test, feature = "synthetic"))]
pub mod synthetic;

pub use error::{AudioError, Result};
pub use frame::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
```

> **Why `cfg(any(test, feature = "synthetic"))`:** Without `test`, `cargo test --test synthetic --features synthetic` works but `cargo test` without features wouldn't expose the module to the integration test. Adding `test` lets in-crate unit tests (Step 4 above) see `synthetic` too without needing `--features synthetic` for plain `cargo test -p yogurt-audio`.

- [ ] **Step 6: Run.**

Run: `cargo test -p yogurt-audio --features synthetic`
Expected: 3 frame tests + 2 synthetic unit tests + 2 synthetic integration tests = **7 passed**.

- [ ] **Step 7: Commit.**

```bash
git add crates/yogurt-audio/
git commit -m "feat(audio): add synthetic sine-wave generator + broadcast-plumbing tests"
```

---

### Task 2.4 · Permission detection (`PermissionStatus` + `has_screen_recording_permission`)

**Files:**
- Create: `crates/yogurt-audio/src/permission.rs`
- Create: `crates/yogurt-audio/tests/permission.rs`
- Modify: `crates/yogurt-audio/src/lib.rs` (export permission module)

- [ ] **Step 1: Write the typed enum + the public function (no platform code yet — stub both branches).**

Create `crates/yogurt-audio/src/permission.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Screen Recording permission state — surfaced to the UI for the §5.11 recovery flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    /// Granted. System audio capture will work.
    Granted,
    /// Explicitly denied or never asked. UI should show the §5.11 recovery card.
    Denied,
    /// Not applicable on this platform (non-macOS).
    NotRequired,
}

/// Detect Screen Recording permission. Does **not** prompt — call [`request_screen_recording_permission`]
/// to trigger the OS dialog.
pub fn has_screen_recording_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::check()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

/// Trigger the macOS Screen Recording permission dialog if not yet granted.
/// On non-macOS, no-op.
///
/// **TCC limitation:** after the user grants permission, the binary must be
/// restarted before the grant takes effect. Surface PRD §5.10's "restart once"
/// message in the UI after calling this.
pub fn request_screen_recording_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::request()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::PermissionStatus;

    // CoreGraphics functions for TCC screen-recording status. These are stable
    // public API on macOS 10.15+. They live in CoreGraphics.framework, which
    // is linked by default — no extra `#[link]` attribute required.
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn check() -> PermissionStatus {
        // SAFETY: CGPreflightScreenCaptureAccess is a thread-safe C fn with no
        // arguments; it returns a bool and has no preconditions.
        if unsafe { CGPreflightScreenCaptureAccess() } {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }

    pub fn request() -> PermissionStatus {
        // SAFETY: same as `check`. This call may trigger the system dialog and
        // returns immediately with whatever the *current* (likely still pending)
        // state is — the actual grant arrives after user interaction + relaunch.
        let granted = unsafe { CGRequestScreenCaptureAccess() };
        if granted { PermissionStatus::Granted } else { PermissionStatus::Denied }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn it_returns_not_required_on_non_macos() {
        assert_eq!(has_screen_recording_permission(), PermissionStatus::NotRequired);
        assert_eq!(request_screen_recording_permission(), PermissionStatus::NotRequired);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn it_returns_granted_or_denied_on_macos() {
        // Can't assert which — depends on the user's TCC state. Just assert
        // it doesn't panic and returns one of the two valid macOS variants.
        let status = has_screen_recording_permission();
        assert!(
            matches!(status, PermissionStatus::Granted | PermissionStatus::Denied),
            "macOS should never return NotRequired"
        );
    }
}
```

- [ ] **Step 2: Wire `permission` into `lib.rs`.**

```rust
mod error;
mod frame;
pub mod permission;

#[cfg(any(test, feature = "synthetic"))]
pub mod synthetic;

pub use error::{AudioError, Result};
pub use frame::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
pub use permission::{
    has_screen_recording_permission, request_screen_recording_permission, PermissionStatus,
};
```

- [ ] **Step 3: Write the manual-smoke integration test with a printed checklist.**

Create `crates/yogurt-audio/tests/permission.rs`:

```rust
//! Manual smoke test for Screen Recording permission detection.
//!
//! This test is `#[ignore]` by default — CI cannot grant TCC permissions,
//! so we run it locally with `cargo test -p yogurt-audio --test permission --ignored`.

#![cfg(target_os = "macos")]

use yogurt_audio::{has_screen_recording_permission, PermissionStatus};

#[test]
#[ignore = "manual smoke — requires a real Mac with TCC interaction"]
fn manual_smoke_permission_detection() {
    println!();
    println!("=== Manual smoke: Screen Recording permission ===");
    println!();
    println!("Run this test in two passes:");
    println!();
    println!("  PASS 1 — without permission");
    println!("    1. Open System Settings → Privacy & Security → Screen Recording");
    println!("    2. If `yogurt` (or the cargo test binary) is listed, toggle it OFF and quit it.");
    println!("    3. Run: cargo test -p yogurt-audio --test permission --ignored -- --nocapture");
    println!("    4. Expect: 'CURRENT STATUS: Denied' printed below.");
    println!();
    println!("  PASS 2 — with permission");
    println!("    1. Toggle the cargo test binary ON in System Settings.");
    println!("    2. Quit the test runner (Cmd-Q if any window is open).");
    println!("    3. Re-run the same cargo command.");
    println!("    4. Expect: 'CURRENT STATUS: Granted'.");
    println!();
    println!("  ALSO VERIFY:");
    println!("    [ ] Apple Silicon (M-series) Mac — run both passes.");
    println!("    [ ] Intel Mac (if available) — run both passes.");
    println!("    [ ] macOS 13 (minimum supported) — at least one pass.");
    println!("    [ ] macOS 14 + 15 — at least one pass each.");
    println!();

    let status = has_screen_recording_permission();
    println!("CURRENT STATUS: {:?}", status);
    println!();

    // No assertion on the value — both Granted and Denied are valid outcomes
    // depending on what the human just configured. We assert only that we
    // returned *some* valid macOS-side variant.
    assert!(matches!(status, PermissionStatus::Granted | PermissionStatus::Denied));
}
```

- [ ] **Step 4: Run the unit test (non-ignored part).**

Run: `cargo test -p yogurt-audio permission`
Expected: the in-module unit test passes; the integration test is reported as `ignored`.

- [ ] **Step 5: Run the manual smoke locally and follow the printed checklist.**

Run: `cargo test -p yogurt-audio --test permission --ignored -- --nocapture`
Expected: the checklist prints, the current status is logged. **Walk through both passes** (denied → granted) per the printed instructions.

> **CI note:** GitHub Actions macOS runners cannot grant TCC permissions. The non-ignored unit test (which only verifies the call doesn't panic and returns a valid variant) is what CI runs. The full smoke is human-driven.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-audio/
git commit -m "feat(audio): add Screen Recording permission detection + manual smoke checklist"
```

---

### Task 2.5 · Microphone capture via `cpal`

**Files:**
- Create: `crates/yogurt-audio/src/mic.rs`
- Create: `crates/yogurt-audio/src/resample.rs`
- Modify: `crates/yogurt-audio/src/lib.rs` (declare modules; not yet re-exported publicly — `start_capture` in Task 2.6 is the public surface)

The cpal default input device is almost always 48 kHz `f32`. We resample to 16 kHz `i16` inside the capture callback using `rubato`'s `SincFixedIn` resampler, then chunk into 320-sample `Frame`s, then broadcast.

- [ ] **Step 1: Write `crates/yogurt-audio/src/resample.rs` (helper used by both producers).**

```rust
//! 48 kHz f32 stereo → 16 kHz i16 mono resampler/downmixer.
//!
//! Both cpal mic (typically 48k stereo or mono f32) and SCK system audio
//! (typically 48k stereo f32) need to land at the Frame contract: 16k mono i16.
//!
//! We use `rubato`'s sinc resampler (high-quality but cheap enough for real-time
//! at 20 ms chunks) and a simple L+R average for downmix.

use rubato::{Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

/// Resample/downmix state. Construct once per producer thread; feed it
/// arbitrary-size f32 input buffers; collect output in `out_buf`.
pub struct Downmix {
    input_channels: u16,
    input_rate: u32,
    resampler: SincFixedIn<f32>,
    pending_mono: Vec<f32>,
}

impl Downmix {
    pub fn new(input_rate: u32, input_channels: u16) -> Self {
        // Resampler with a fixed input chunk size of 480 samples
        // (10 ms at 48 kHz). 480 → 160 at 16 kHz, then we collect into 320-sample frames.
        let params = SincInterpolationParameters {
            sinc_len: 64,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 128,
            window: WindowFunction::BlackmanHarris2,
        };
        let resampler = SincFixedIn::<f32>::new(
            16_000.0 / input_rate as f64,
            1.0,
            params,
            480,
            1,
        ).expect("rubato init");
        Self { input_channels, input_rate, resampler, pending_mono: Vec::with_capacity(2_048) }
    }

    /// Push raw interleaved f32 samples. Returns any newly-available 16k mono i16
    /// samples (un-chunked — caller is responsible for splitting into FRAME_SAMPLES).
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<i16> {
        // 1. Downmix to mono.
        let mut mono: Vec<f32> = if self.input_channels == 1 {
            interleaved.to_vec()
        } else {
            interleaved
                .chunks_exact(self.input_channels as usize)
                .map(|frame| frame.iter().sum::<f32>() / self.input_channels as f32)
                .collect()
        };
        self.pending_mono.append(&mut mono);

        // 2. Resample in 480-sample chunks (rubato fixed-input contract).
        let mut out: Vec<i16> = Vec::new();
        while self.pending_mono.len() >= 480 {
            let chunk: Vec<f32> = self.pending_mono.drain(..480).collect();
            let resampled = self.resampler.process(&[chunk], None).expect("rubato process");
            for &s in &resampled[0] {
                // f32 in [-1, 1] → i16. Clamp to avoid wrap on tiny over-amplitude.
                let clamped = s.clamp(-1.0, 1.0);
                out.push((clamped * i16::MAX as f32) as i16);
            }
        }
        out
    }

    /// Input rate for diagnostics.
    pub fn input_rate(&self) -> u32 { self.input_rate }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_downmixes_stereo_to_mono() {
        let mut d = Downmix::new(16_000, 2); // no resample, only downmix
        // 480 stereo-interleaved samples = 960 f32 values.
        let input: Vec<f32> = (0..960).map(|i| if i % 2 == 0 { 0.5 } else { -0.5 }).collect();
        let out = d.push(&input);
        // 480 mono samples at 16k → 480 i16 out.
        assert_eq!(out.len(), 480);
        // L=0.5, R=-0.5 → mean 0 → ~0 i16.
        for s in &out { assert!(s.abs() < 100, "expected ~0, got {s}"); }
    }

    #[test]
    fn it_resamples_48k_to_16k() {
        let mut d = Downmix::new(48_000, 1);
        let input: Vec<f32> = vec![0.0; 480]; // 10 ms @ 48k
        let out = d.push(&input);
        // 10 ms output @ 16k = 160 samples.
        assert_eq!(out.len(), 160);
    }
}
```

- [ ] **Step 2: Write the failing unit test for mic capture cadence.**

We can't easily unit-test cpal because the test runner has no audio device. Instead, write a **structural** test that verifies `MicCapture::spawn` plumbs frames correctly when fed via the synthetic resampler path. Append to `crates/yogurt-audio/src/mic.rs` (file not yet created — written next step):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Channel, FRAME_SAMPLES};
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn chunker_emits_exactly_frame_samples_per_frame() {
        let (tx, mut rx) = broadcast::channel::<Frame>(8);
        let mut chunker = FrameChunker::new(Channel::Mic, tx);

        // Feed 1000 samples: should emit 3 full frames (3*320=960), 40 buffered.
        chunker.feed(&vec![1i16; 1000]);

        let f1 = rx.recv().await.unwrap();
        let f2 = rx.recv().await.unwrap();
        let f3 = rx.recv().await.unwrap();
        assert_eq!(f1.samples.len(), FRAME_SAMPLES);
        assert_eq!(f2.samples.len(), FRAME_SAMPLES);
        assert_eq!(f3.samples.len(), FRAME_SAMPLES);
        // 4th frame not yet available.
        assert!(rx.try_recv().is_err());
    }
}
```

- [ ] **Step 3: Write `crates/yogurt-audio/src/mic.rs`.**

```rust
//! Microphone capture via `cpal`. Default input device, resample/downmix to
//! 16 kHz mono i16, broadcast as [`Frame`]s.

use crate::{
    error::{AudioError, Result},
    resample::Downmix,
    Channel, Frame, FRAME_SAMPLES,
};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, SampleFormat, Stream, StreamConfig,
};
use std::{
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::broadcast;

/// A live microphone capture. Holding this keeps the cpal stream alive;
/// dropping it stops capture.
pub struct MicCapture {
    _stream: Stream,
    pub device_name: String,
}

impl std::fmt::Debug for MicCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicCapture").field("device_name", &self.device_name).finish()
    }
}

/// Splits a stream of arbitrary-sized i16 buffers into exact-`FRAME_SAMPLES` frames.
pub(crate) struct FrameChunker {
    channel: Channel,
    tx: broadcast::Sender<Frame>,
    buf: Vec<i16>,
    start: Instant,
}

impl FrameChunker {
    pub fn new(channel: Channel, tx: broadcast::Sender<Frame>) -> Self {
        Self { channel, tx, buf: Vec::with_capacity(FRAME_SAMPLES * 2), start: Instant::now() }
    }

    pub fn feed(&mut self, samples: &[i16]) {
        self.buf.extend_from_slice(samples);
        while self.buf.len() >= FRAME_SAMPLES {
            let chunk: Vec<i16> = self.buf.drain(..FRAME_SAMPLES).collect();
            let monotonic_ms = self.start.elapsed().as_millis() as u64;
            let frame = Frame::new(self.channel, monotonic_ms, chunk);
            let _ = self.tx.send(frame); // ignore if no subscribers
        }
    }
}

/// List available input devices (name + sample rate). Powers `GET /api/audio/devices`.
pub fn list_input_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok());
    let mut infos = Vec::new();
    for device in host.input_devices().map_err(|e| AudioError::Cpal(e.to_string()))? {
        let name = device.name().map_err(|e| AudioError::Cpal(e.to_string()))?;
        let is_default = default_name.as_deref() == Some(name.as_str());
        let sample_rate = device
            .default_input_config()
            .ok()
            .map(|c| c.sample_rate().0);
        infos.push(DeviceInfo { name, is_default, sample_rate });
    }
    Ok(infos)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rate: Option<u32>,
}

/// Start capturing the default mic device. Returns immediately once the
/// stream is built; holds the cpal stream alive in [`MicCapture`].
pub fn spawn_mic_capture(tx: broadcast::Sender<Frame>) -> Result<MicCapture> {
    let host = cpal::default_host();
    let device: Device = host
        .default_input_device()
        .ok_or_else(|| AudioError::MicUnavailable("no default input device".into()))?;
    let device_name = device.name().unwrap_or_else(|_| "<unnamed>".into());

    let config = device
        .default_input_config()
        .map_err(|e| AudioError::Cpal(e.to_string()))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    let chunker = Arc::new(Mutex::new(FrameChunker::new(Channel::Mic, tx)));
    let downmix = Arc::new(Mutex::new(Downmix::new(sample_rate, channels)));

    tracing::info!(%device_name, sample_rate, channels, ?sample_format, "starting mic capture");

    let err_fn = |err| tracing::error!(?err, "cpal mic error");

    let stream = match sample_format {
        SampleFormat::F32 => {
            let ch = chunker.clone();
            let dm = downmix.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let i16s = dm.lock().unwrap().push(data);
                    ch.lock().unwrap().feed(&i16s);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let ch = chunker.clone();
            let dm = downmix.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    // Convert i16→f32 for the resampler, then back.
                    let f: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    let i16s = dm.lock().unwrap().push(&f);
                    ch.lock().unwrap().feed(&i16s);
                },
                err_fn,
                None,
            )
        }
        other => return Err(AudioError::Cpal(format!("unsupported sample format: {other:?}"))),
    }
    .map_err(|e| AudioError::Cpal(e.to_string()))?;

    stream.play().map_err(|e| AudioError::Cpal(e.to_string()))?;

    Ok(MicCapture { _stream: stream, device_name })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn chunker_emits_exactly_frame_samples_per_frame() {
        let (tx, mut rx) = broadcast::channel::<Frame>(8);
        let mut chunker = FrameChunker::new(Channel::Mic, tx);
        chunker.feed(&vec![1i16; 1000]);
        let f1 = rx.recv().await.unwrap();
        let f2 = rx.recv().await.unwrap();
        let f3 = rx.recv().await.unwrap();
        assert_eq!(f1.samples.len(), FRAME_SAMPLES);
        assert_eq!(f2.samples.len(), FRAME_SAMPLES);
        assert_eq!(f3.samples.len(), FRAME_SAMPLES);
        assert!(rx.try_recv().is_err(), "4th frame should not yet be ready");
    }

    #[test]
    fn list_input_devices_does_not_panic() {
        // On CI runners there may be zero input devices — empty Vec is fine.
        let _ = list_input_devices();
    }
}
```

- [ ] **Step 4: Update `lib.rs` to declare the modules.**

```rust
#![deny(rust_2018_idioms, missing_debug_implementations)]

mod error;
mod frame;
mod mic;
mod resample;
pub mod permission;

#[cfg(any(test, feature = "synthetic"))]
pub mod synthetic;

pub use error::{AudioError, Result};
pub use frame::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
pub use mic::{list_input_devices, DeviceInfo};
pub use permission::{
    has_screen_recording_permission, request_screen_recording_permission, PermissionStatus,
};
```

- [ ] **Step 5: Build + test.**

Run: `cargo test -p yogurt-audio --features synthetic`
Expected: all prior tests still pass, plus 2 new resample tests + 1 chunker test + 1 list_devices test = ~11 tests passing.

- [ ] **Step 6: Manual mic smoke (Apple Silicon AND Intel if available).**

Write a one-off binary `crates/yogurt-audio/examples/mic_smoke.rs`:

```rust
use std::time::Duration;
use tokio::sync::broadcast;
use yogurt_audio::{Frame, FRAME_SAMPLES};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let (tx, mut rx) = broadcast::channel::<Frame>(128);
    let _mic = yogurt_audio::mic::spawn_mic_capture(tx)?;

    let mut frames = 0usize;
    let mut peak: i16 = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Ok(f) = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
            let f = f?;
            frames += 1;
            for &s in &f.samples { peak = peak.max(s.abs()); }
        }
    }
    println!("captured {frames} frames in 5s (~{} expected)", 5 * 1000 / 20);
    println!("peak amplitude: {peak} (talk/clap during the run to see this above 1000)");
    Ok(())
}
```

> **Note:** `spawn_mic_capture` is `pub(crate)` to the integration test but `pub` to the example. Either expose it as `pub fn spawn_mic_capture` in `mic.rs` (preferred — Phase 5's settings UI may want to restart capture on device change) or scope-promote via `pub use mic::spawn_mic_capture` in `lib.rs`. Doing the latter here.

Add to `lib.rs`: `pub use mic::spawn_mic_capture;`

Run: `cargo run -p yogurt-audio --example mic_smoke`
Expected: ~250 frames in 5 seconds, peak amplitude > 1000 when you talk. **Verify on Apple Silicon AND Intel** (if you have access to both — note in commit message if Intel was skipped due to hardware unavailability).

- [ ] **Step 7: Commit.**

```bash
git add crates/yogurt-audio/
git commit -m "feat(audio): cpal microphone capture → 16kHz mono i16 broadcast frames"
```

---

### Task 2.6 · System audio capture via `screencapturekit` (or Swift sidecar per Task 2.0 outcome)

**Files:**
- Create: `crates/yogurt-audio/src/system.rs`
- Modify: `crates/yogurt-audio/src/lib.rs`

**Pre-read Task 2.0 result.** If `docs/superpowers/notes/2026-06-25-sck-spike-result.md` says **PASS**, follow Path A below. If **FAIL/PARTIAL**, follow Path B (Swift sidecar). Either path produces the same `pub fn spawn_system_capture(tx: broadcast::Sender<Frame>) -> Result<SystemCapture>` public function — downstream code in Task 2.7 doesn't care which is in play.

#### Path A — in-process SCK (preferred)

- [ ] **Step A1: Write `crates/yogurt-audio/src/system.rs` (macOS-only impl + non-macos stub).**

```rust
//! System audio capture via ScreenCaptureKit. macOS 13+ only.

use crate::{
    error::{AudioError, Result},
    resample::Downmix,
    Channel, Frame, FRAME_SAMPLES,
};
use tokio::sync::broadcast;

/// Holds the underlying SCK stream alive. Drop to stop capture.
pub struct SystemCapture {
    #[cfg(target_os = "macos")]
    _inner: macos::Inner,
}

impl std::fmt::Debug for SystemCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemCapture").finish_non_exhaustive()
    }
}

#[cfg(target_os = "macos")]
pub fn spawn_system_capture(tx: broadcast::Sender<Frame>) -> Result<SystemCapture> {
    Ok(SystemCapture { _inner: macos::start(tx)? })
}

#[cfg(not(target_os = "macos"))]
pub fn spawn_system_capture(_tx: broadcast::Sender<Frame>) -> Result<SystemCapture> {
    Err(AudioError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::permission::{has_screen_recording_permission, PermissionStatus};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    pub struct Inner {
        // The actual screencapturekit::stream::SCStream is held here. Exact
        // field types depend on the crate version — the spike (Task 2.0)
        // pinned what works.
        _stream: screencapturekit::stream::SCStream,
    }

    pub fn start(tx: broadcast::Sender<Frame>) -> Result<Inner> {
        // 1. Permission gate — fast-fail with a typed error the UI can render.
        if has_screen_recording_permission() == PermissionStatus::Denied {
            return Err(AudioError::PermissionDenied);
        }

        // 2. Build SCK content filter + audio-only config. Exact API may vary
        //    slightly from this sketch — adjust per Task 2.0 spike findings.
        let shareable = pollster::block_on(
            screencapturekit::shareable_content::SCShareableContent::get()
        ).map_err(|e| AudioError::SystemCaptureFailed(format!("shareable: {e}")))?;
        let display = shareable
            .displays()
            .first()
            .cloned()
            .ok_or_else(|| AudioError::SystemCaptureFailed("no displays".into()))?;
        let filter = screencapturekit::stream::content_filter::SCContentFilter::new_with_display(display);

        let mut config = screencapturekit::stream::configuration::SCStreamConfiguration::new();
        config.set_captures_audio(true);
        config.set_excludes_current_process_audio(true);
        // Minimum video dims; we don't consume video.
        config.set_width(2);
        config.set_height(2);
        config.set_minimum_frame_interval(std::time::Duration::from_millis(1000));

        let start_instant = Instant::now();
        let chunker = Arc::new(Mutex::new(crate::mic::FrameChunker::new(Channel::System, tx)));
        // SCK delivers audio at 48 kHz stereo f32 by default. Confirm via the
        // first buffer's stream description; for now, assume the common case.
        let downmix = Arc::new(Mutex::new(Downmix::new(48_000, 2)));

        struct Delegate {
            chunker: Arc<Mutex<crate::mic::FrameChunker>>,
            downmix: Arc<Mutex<Downmix>>,
            start: Instant,
        }

        impl screencapturekit::stream::SCStreamDelegate for Delegate {
            fn did_output_sample_buffer(
                &self,
                sample_buffer: screencapturekit::cm_sample_buffer::CMSampleBuffer,
                of_type: screencapturekit::stream::output_type::SCStreamOutputType,
            ) {
                use screencapturekit::stream::output_type::SCStreamOutputType;
                if !matches!(of_type, SCStreamOutputType::Audio) { return; }

                // Pull the interleaved f32 from the CMSampleBuffer. Exact API
                // depends on screencapturekit 0.3 — Task 2.0 confirmed the
                // correct accessor.
                let abl = match sample_buffer.get_audio_buffer_list() {
                    Some(abl) => abl,
                    None => return,
                };
                let bytes = abl.as_bytes();
                // 4 bytes/sample (f32).
                let f32_samples: &[f32] = unsafe {
                    std::slice::from_raw_parts(
                        bytes.as_ptr() as *const f32,
                        bytes.len() / 4,
                    )
                };

                let i16s = self.downmix.lock().unwrap().push(f32_samples);
                self.chunker.lock().unwrap().feed(&i16s);
                let _ = self.start; // unused but kept for future timestamp realignment
            }
        }

        let delegate = Delegate { chunker, downmix, start: start_instant };
        let mut stream = screencapturekit::stream::SCStream::new(filter, config, delegate);
        stream.add_output(screencapturekit::stream::output_type::SCStreamOutputType::Audio);
        pollster::block_on(stream.start_capture())
            .map_err(|e| AudioError::SystemCaptureFailed(format!("start: {e}")))?;

        Ok(Inner { _stream: stream })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn it_returns_unsupported_platform_off_macos() {
        let (tx, _rx) = tokio::sync::broadcast::channel(1);
        let err = spawn_system_capture(tx).unwrap_err();
        assert!(matches!(err, AudioError::UnsupportedPlatform));
    }
}
```

> **API caveat (again).** The exact symbols on `screencapturekit` 0.3 may differ — `pollster` may not be needed if there's a sync API, `get_audio_buffer_list().as_bytes()` may be `audio_data().bytes()`, and `SCStreamDelegate::did_output_sample_buffer` may take a different signature. The spike (Task 2.0) committed the actual working symbols into `docs/superpowers/notes/2026-06-25-sck-spike-result.md` — translate accordingly. **If you need `pollster`, add it to the macos-only deps in `Cargo.toml`:**
>
> ```toml
> [target.'cfg(target_os = "macos")'.dependencies]
> pollster = "0.4"
> ```
>
> The reason for `pollster::block_on` instead of awaiting: cpal/SCK callbacks fire on non-tokio threads, and the SCK start_capture is a one-time setup call we want to make synchronously inside `spawn_system_capture` to return a typed error before the function returns. If SCK 0.3 exposes a sync `start_capture_sync()`, prefer it and drop pollster.

Skip to **Step 3** below.

#### Path B — Swift sidecar (only if Task 2.0 FAILED)

- [ ] **Step B1: Add the helper to file structure + Cargo.toml.**

```bash
mkdir -p tools/yogurt-audio-helper
```

Write `tools/yogurt-audio-helper/Package.swift`:

```swift
// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "yogurt-audio-helper",
    platforms: [.macOS(.v13)],
    targets: [.executableTarget(name: "yogurt-audio-helper", path: "Sources")]
)
```

Write `tools/yogurt-audio-helper/Sources/main.swift` (~120 lines): set up an `SCStream` with audio-only config, register a `SCStreamOutput` delegate, downmix L+R → mono, resample 48k → 16k via `AVAudioConverter`, write interleaved `Int16` little-endian to `FileHandle.standardOutput.write()` in 640-byte chunks. (Full Swift code is too long to inline; cribbing from any "system-audio-capture macOS Swift" example online is fine — the file is throwaway shape.)

- [ ] **Step B2: Write `crates/yogurt-audio/src/system.rs` Path B.**

Same public surface, but `start()` does:

```rust
use tokio::process::{Child, Command};
use tokio::io::{AsyncReadExt, BufReader};

let mut child = Command::new(helper_path())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::inherit())
    .spawn()
    .map_err(AudioError::Io)?;
let stdout = child.stdout.take().ok_or_else(|| AudioError::SystemCaptureFailed("no stdout".into()))?;

let chunker = Arc::new(Mutex::new(crate::mic::FrameChunker::new(Channel::System, tx)));
tokio::spawn(async move {
    let mut reader = BufReader::with_capacity(8192, stdout);
    let mut buf = vec![0u8; FRAME_SAMPLES * 2];
    loop {
        match reader.read_exact(&mut buf).await {
            Ok(_) => {
                let samples: Vec<i16> = buf.chunks_exact(2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]))
                    .collect();
                chunker.lock().unwrap().feed(&samples);
            }
            Err(e) => {
                tracing::warn!(?e, "helper stdout EOF / read error — sidecar exited");
                break;
            }
        }
    }
});

Ok(Inner { _child: child })
```

`helper_path()` returns the absolute path to `yogurt-audio-helper`. Resolution order: (1) `YOGURT_AUDIO_HELPER` env var if set; (2) `$exe_dir/yogurt-audio-helper` (release bottle); (3) `target/release/yogurt-audio-helper` (dev). Phase 9 handles the Homebrew packaging; for now, document the dev workflow:

```bash
cd tools/yogurt-audio-helper && swift build -c release
cp .build/release/yogurt-audio-helper ../../target/release/
```

- [ ] **Step 3 (both paths): Update `lib.rs` to export `spawn_system_capture` + `SystemCapture`.**

```rust
mod error;
mod frame;
mod mic;
mod resample;
mod system;
pub mod permission;

#[cfg(any(test, feature = "synthetic"))]
pub mod synthetic;

pub use error::{AudioError, Result};
pub use frame::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
pub use mic::{list_input_devices, spawn_mic_capture, DeviceInfo, MicCapture};
pub use system::{spawn_system_capture, SystemCapture};
pub use permission::{
    has_screen_recording_permission, request_screen_recording_permission, PermissionStatus,
};
```

- [ ] **Step 4: Build on both platforms (or document Intel skip).**

Run: `cargo build -p yogurt-audio --all-targets`
Expected: clean. The `#[cfg(not(target_os = "macos"))]` stub means Linux CI doesn't break.

If you have an Intel Mac available: `cargo build --target x86_64-apple-darwin -p yogurt-audio`. **Note in commit if skipped.**

- [ ] **Step 5: Manual system-audio smoke.**

Write `crates/yogurt-audio/examples/system_smoke.rs`:

```rust
use std::time::Duration;
use tokio::sync::broadcast;
use yogurt_audio::{Frame, has_screen_recording_permission, PermissionStatus};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    match has_screen_recording_permission() {
        PermissionStatus::Granted => println!("✓ permission granted"),
        PermissionStatus::Denied => {
            println!("✗ permission denied — open System Settings → Privacy → Screen Recording, then re-run.");
            return Ok(());
        }
        PermissionStatus::NotRequired => println!("not a macos target — bail"),
    }

    let (tx, mut rx) = broadcast::channel::<Frame>(128);
    let _sys = yogurt_audio::spawn_system_capture(tx)?;
    println!("capturing system audio for 5s — play music NOW");

    let mut frames = 0usize;
    let mut peak: i16 = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Ok(Ok(f)) = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await {
            frames += 1;
            for &s in &f.samples { peak = peak.max(s.abs()); }
        }
    }
    println!("captured {frames} frames; peak amplitude {peak} (expect >1000 when audio is playing)");
    Ok(())
}
```

Run: `cargo run -p yogurt-audio --example system_smoke`
Expected: ~250 frames; peak > 1000 when audio is playing. **Manual checklist:**

- [ ] Apple Silicon: works.
- [ ] Intel (if available): works.
- [ ] With permission revoked: returns `AudioError::PermissionDenied` cleanly (no panic).
- [ ] After permission grant + binary restart: works on subsequent runs.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-audio/
# If Path B was taken:
# git add tools/yogurt-audio-helper/
git commit -m "feat(audio): system audio capture via ScreenCaptureKit (or Swift sidecar fallback)"
```

---

### Task 2.7 · `start_capture` orchestrator + dual-channel smoke

**Files:**
- Modify: `crates/yogurt-audio/src/lib.rs` (add `AudioStream` + `start_capture`)
- Create: `crates/yogurt-audio/examples/dual_smoke.rs`

This task is the public surface that `yogurt-server` (Task 2.8) consumes: a single call that returns both producers' broadcast receivers + a handle that keeps both streams alive.

- [ ] **Step 1: Add `AudioStream` and `start_capture` to `lib.rs`.**

```rust
use tokio::sync::broadcast;

/// Live capture handle. Drop to stop both streams.
#[derive(Debug)]
pub struct AudioStream {
    _mic: MicCapture,
    _system: SystemCapture,
    pub mic_tx: broadcast::Sender<Frame>,
    pub system_tx: broadcast::Sender<Frame>,
}

impl AudioStream {
    /// New subscriber to the mic channel.
    pub fn subscribe_mic(&self) -> broadcast::Receiver<Frame> {
        self.mic_tx.subscribe()
    }
    /// New subscriber to the system-audio channel.
    pub fn subscribe_system(&self) -> broadcast::Receiver<Frame> {
        self.system_tx.subscribe()
    }
}

/// Start mic + system audio capture. Returns an [`AudioStream`] that owns
/// both producers; dropping it stops capture.
///
/// **Permission gate:** if Screen Recording is denied, returns
/// [`AudioError::PermissionDenied`] *before* mic capture starts. The caller
/// (the Phase 7 onboarding UI) should render the §5.11 recovery screen.
pub fn start_capture() -> Result<AudioStream> {
    // Fast-fail on permission BEFORE we open the mic — the user shouldn't
    // hear a recording indicator if system capture is going to fail.
    if has_screen_recording_permission() == PermissionStatus::Denied {
        return Err(AudioError::PermissionDenied);
    }

    let (mic_tx, _) = broadcast::channel::<Frame>(256);
    let (system_tx, _) = broadcast::channel::<Frame>(256);

    let _mic = spawn_mic_capture(mic_tx.clone())?;
    let _system = spawn_system_capture(system_tx.clone())?;

    Ok(AudioStream { _mic, _system, mic_tx, system_tx })
}
```

> **Channel capacity = 256 frames = ~5 seconds of buffered audio per channel.** Phase 3 STT consumers run at near-real-time so this is generous; if they fall behind, `broadcast::Receiver::recv()` returns `Lagged(skipped)` and they drop the late frames (correct behavior — STT is wall-clock-bound).

- [ ] **Step 2: Write `crates/yogurt-audio/examples/dual_smoke.rs`.**

```rust
use std::time::Duration;
use yogurt_audio::start_capture;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let stream = start_capture()?;
    let mut mic = stream.subscribe_mic();
    let mut sys = stream.subscribe_system();

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    let mut mic_n = 0;
    let mut sys_n = 0;
    let mut mic_peak: i16 = 0;
    let mut sys_peak: i16 = 0;

    while std::time::Instant::now() < deadline {
        tokio::select! {
            Ok(f) = mic.recv() => {
                mic_n += 1;
                for &s in &f.samples { mic_peak = mic_peak.max(s.abs()); }
            }
            Ok(f) = sys.recv() => {
                sys_n += 1;
                for &s in &f.samples { sys_peak = sys_peak.max(s.abs()); }
            }
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }
    }
    println!("mic    : {mic_n} frames, peak {mic_peak}");
    println!("system : {sys_n} frames, peak {sys_peak}");
    println!("expect ~500 frames each over 10s");
    Ok(())
}
```

- [ ] **Step 3: Manual dual-channel smoke.**

Run: `cargo run -p yogurt-audio --example dual_smoke`

For 10 seconds: talk into the mic AND play music. Expected:

- mic: ~500 frames, peak > 1000.
- system: ~500 frames, peak > 1000.

**Apple Silicon AND Intel** if available.

- [ ] **Step 4: Build the full crate clean.**

Run: `cargo build -p yogurt-audio --all-targets`
Run: `cargo test -p yogurt-audio --features synthetic`
Run: `cargo clippy -p yogurt-audio --all-targets -- -D warnings`
Expected: clean all three.

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-audio/
git commit -m "feat(audio): add start_capture() orchestrator + AudioStream public surface"
```

---

### Task 2.8 · `yogurt-server` integration — REST endpoints + `start_meeting_recording`

**Files:**
- Modify: `crates/yogurt-server/Cargo.toml` (add `yogurt-audio` path dep)
- Create: `crates/yogurt-server/src/audio.rs`
- Modify: `crates/yogurt-server/src/lib.rs` (declare audio module)
- Modify: `crates/yogurt-server/src/routes.rs` (mount `/api/audio/*` endpoints)

- [ ] **Step 1: Add `yogurt-audio` to `crates/yogurt-server/Cargo.toml`.**

Append to `[dependencies]`:

```toml
yogurt-audio = { path = "../yogurt-audio" }
```

- [ ] **Step 2: Write the failing REST integration test for both endpoints first.**

Create `crates/yogurt-server/tests/audio_api.rs`:

```rust
use std::time::Duration;

#[tokio::test]
async fn it_lists_audio_devices() {
    let addr = "127.0.0.1:17890".parse().unwrap();
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, yogurt_server::Mode::Release).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17890/api/audio/devices")
        .await.unwrap()
        .json::<serde_json::Value>()
        .await.unwrap();

    // On any machine there's at least one device (or an empty array — both valid);
    // we only assert the shape.
    assert!(body.is_array(), "expected JSON array, got {body}");
    if let Some(first) = body.as_array().and_then(|a| a.first()) {
        assert!(first.get("name").is_some());
        assert!(first.get("is_default").is_some());
    }

    handle.abort();
}

#[tokio::test]
async fn it_reports_permission_status() {
    let addr = "127.0.0.1:17891".parse().unwrap();
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, yogurt_server::Mode::Release).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17891/api/audio/permission")
        .await.unwrap()
        .json::<serde_json::Value>()
        .await.unwrap();

    let status = body.get("status").and_then(|v| v.as_str()).expect("status field");
    assert!(
        ["granted", "denied", "not_required"].contains(&status),
        "unexpected status: {status}"
    );

    handle.abort();
}
```

- [ ] **Step 3: Run — expect 404s (endpoints don't exist yet).**

Run: `cargo test -p yogurt-server --test audio_api`
Expected: tests fail with `405` or JSON-parse errors (axum returns 404 + plain text, which fails `.json()`).

- [ ] **Step 4: Write `crates/yogurt-server/src/audio.rs`.**

```rust
//! Audio-related HTTP handlers + the lifecycle hook Phase 3 will call to begin
//! recording. The actual capture lives in `yogurt-audio`; this module is the
//! axum-facing shim.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Serialize;
use yogurt_audio::{
    has_screen_recording_permission, list_input_devices, start_capture, AudioError, AudioStream,
    DeviceInfo, PermissionStatus,
};

#[derive(Serialize)]
struct PermissionResponse {
    status: PermissionStatus,
}

/// `GET /api/audio/permission` — current Screen Recording permission state.
/// The Phase 7 onboarding UI polls this to decide whether to show the §5.11
/// recovery screen or proceed to the model-setup step.
pub async fn get_permission() -> Json<PermissionResponse> {
    Json(PermissionResponse { status: has_screen_recording_permission() })
}

/// `GET /api/audio/devices` — list mic input devices for the §5.6 settings dropdown.
pub async fn get_devices() -> Result<Json<Vec<DeviceInfo>>, (StatusCode, String)> {
    list_input_devices()
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// Begin recording. Returns an [`AudioStream`] the caller must hold for the
/// duration of the meeting; dropping it stops capture. Phase 3 will wire this
/// into the `POST /api/meetings/:id/start` handler and fan-out to the
/// `/ws/meetings/:id` WebSocket — not in this phase.
///
/// Surfaces [`AudioError::PermissionDenied`] for the §5.11 recovery state.
pub fn start_meeting_recording() -> Result<AudioStream, AudioError> {
    start_capture()
}

impl IntoResponse for AudioErrorWrap {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = match self.0 {
            AudioError::PermissionDenied => (
                StatusCode::FORBIDDEN,
                serde_json::json!({
                    "error": "permission_denied",
                    "message": "macOS Screen Recording permission is required",
                    "recovery": "open System Settings → Privacy & Security → Screen Recording",
                }),
            ),
            AudioError::UnsupportedPlatform => (
                StatusCode::NOT_IMPLEMENTED,
                serde_json::json!({
                    "error": "unsupported_platform",
                    "message": "system audio capture requires macOS 13+",
                }),
            ),
            other => (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({ "error": "audio", "message": other.to_string() }),
            ),
        };
        (status, Json(body)).into_response()
    }
}

/// Newtype so we can `impl IntoResponse` without an orphan-rule violation.
/// Phase 3 will use this when wiring `POST /api/meetings/:id/start`.
pub struct AudioErrorWrap(pub AudioError);

impl From<AudioError> for AudioErrorWrap {
    fn from(e: AudioError) -> Self { Self(e) }
}
```

- [ ] **Step 5: Mount the routes in `crates/yogurt-server/src/routes.rs`.**

Replace the `router()` function body:

```rust
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use crate::assets::serve_embedded;
use crate::audio;
use crate::Mode;

pub fn router(mode: Mode) -> Router {
    let mut router = Router::new()
        .route("/api/health", get(health))
        .route("/api/audio/devices", get(audio::get_devices))
        .route("/api/audio/permission", get(audio::get_permission));

    router = match mode {
        Mode::Release => router.fallback(serve_embedded),
        Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
    };
    router
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}
```

- [ ] **Step 6: Declare the `audio` module in `crates/yogurt-server/src/lib.rs`.**

Append (next to the existing `mod assets; mod dev_proxy; mod routes;`):

```rust
pub mod audio;
```

(Made `pub` because Phase 3 will reach into `yogurt_server::audio::start_meeting_recording` from the meeting-start handler that lives in a future module.)

- [ ] **Step 7: Run.**

Run: `cargo test -p yogurt-server`
Expected: all existing tests still pass + 2 new audio API tests pass.

- [ ] **Step 8: Manual smoke — curl both endpoints.**

```bash
cargo run -p yogurt -- start --no-open &
sleep 1
curl -s localhost:7878/api/audio/devices | jq
curl -s localhost:7878/api/audio/permission | jq
kill %1
```

Expected:
- `/api/audio/devices` → JSON array of `{name, is_default, sample_rate}` objects (at least one entry on a Mac with a built-in mic).
- `/api/audio/permission` → `{"status": "granted"}` or `{"status": "denied"}`.

- [ ] **Step 9: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): expose /api/audio/devices + /api/audio/permission + start_meeting_recording()"
```

---

### Task 2.9 · End-to-end smoke + lint + push

**Files:** none — verification + housekeeping only.

- [ ] **Step 1: Full workspace test.**

Run: `cargo test --workspace --features yogurt-audio/synthetic`
Expected: all green. (Yes, you need to enable the synthetic feature at the workspace test level because the integration test uses it.)

- [ ] **Step 2: Format + clippy.**

Run: `cargo fmt --all && cargo clippy --all-targets --workspace -- -D warnings`
Expected: clean.

- [ ] **Step 3: Frontend tests still pass.**

Run: `pnpm --dir web test`
Expected: 2 passed (unchanged from Phase 0).

- [ ] **Step 4: Fresh-clone smoke (same shape as Phase 0 Task 0.10).**

```bash
cd /tmp && rm -rf yogurt-smoke-p2
git clone /Users/rchen/Documents/code/yogurt yogurt-smoke-p2
cd yogurt-smoke-p2
pnpm --dir web install && pnpm --dir web build
cargo build --release
./target/release/yogurt start --no-open &
sleep 1
curl -s localhost:7878/api/health
echo
curl -s localhost:7878/api/audio/devices | jq '. | length'
curl -s localhost:7878/api/audio/permission | jq
kill %1
cd - && rm -rf /tmp/yogurt-smoke-p2
```

Expected: health is `ok`, devices count is `>= 1` (on a Mac), permission status is one of the three known values.

- [ ] **Step 5: Dual-stream live capture smoke.**

```bash
cargo run -p yogurt-audio --example dual_smoke
```

Talk + play music for 10 seconds. Both channels should report ~500 frames with peak amplitudes > 1000.

**Mark these in the commit message:**

- [ ] Apple Silicon: passed.
- [ ] Intel: passed | skipped (no hardware).
- [ ] macOS 13: passed | skipped.
- [ ] macOS 14/15: passed.
- [ ] Permission revoke → `AudioError::PermissionDenied` returned cleanly.

- [ ] **Step 6: Push.**

```bash
git push origin main
```

- [ ] **Step 7: Tag — only with explicit user confirmation.**

```bash
git tag -a v0.0.2-phase-2 -m "Phase 2 complete: dual-stream audio capture + REST permission/devices endpoints"
git push origin v0.0.2-phase-2
```

---

## Phase 2 acceptance criteria

All six must be true:

1. `cargo test --workspace --features yogurt-audio/synthetic` passes on macOS.
2. `cargo clippy --all-targets --workspace -- -D warnings` is clean.
3. `cargo build -p yogurt-audio` succeeds on a non-macOS target (e.g. Linux CI) — `Channel::System` path returns `AudioError::UnsupportedPlatform` instead of failing to compile.
4. `cargo run -p yogurt-audio --example dual_smoke` produces ~500 frames per channel over 10 seconds, both with peak amplitudes > 1000 when audio is present.
5. `curl localhost:7878/api/audio/permission` returns valid JSON with one of `granted` / `denied` / `not_required`.
6. `curl localhost:7878/api/audio/devices` returns a JSON array of `{name, is_default, sample_rate}` entries.

## What this phase does NOT do

Explicitly out of scope (later plans):
- STT consumption of the broadcast channels — Phase 3 (Deepgram cloud) and Phase 8 (whisper.cpp).
- `POST /api/meetings/:id/start` and `/ws/meetings/:id` wiring — Phase 3 (the WebSocket layer is where mic/system frames become live transcript events).
- The §5.11 permission-denied recovery UI — Phase 7 (onboarding plan). This phase only ships the API the UI will consume.
- Persistent audio recording to disk — not a v1 feature (PRD §2 non-goals).
- Voice activity detection, noise suppression, AGC — v2+.
- Per-speaker diarization — v2+ (PRD §6 item 6).
- Settings UI device dropdown wiring — Phase 5 (this phase exposes only the underlying REST endpoint).

## Risks discovered during this phase (update §13 if needed)

- **(Carried from PRD §13 risk #1)** If Task 2.0 spike fails, the Swift sidecar fallback is the path forward. Document the decision in `docs/superpowers/notes/2026-06-25-sck-spike-result.md`. The Homebrew distribution story for the sidecar binary is a Phase 9 problem — note it there.
- **Intel-Mac performance unknown.** `rubato` SIMD paths assume NEON on Apple Silicon and SSE on Intel; both should be fine but real perf data is a manual-smoke output, not enforced by tests.
- **CoreGraphics permission API.** `CGPreflightScreenCaptureAccess` returns the cached TCC state, not the live one — meaning the very first call right after a grant may still return `false` until the binary restarts (this is the same macOS quirk PRD §5.10 already documents as "restart once after granting"). No code fix; UX surface this in the §5.11 recovery card.

## Next plan

After Phase 2 lands, write `docs/superpowers/plans/<date>-yogurt-phase-3-stt-deepgram.md` covering:
- `yogurt-stt` crate with a `SttEngine` trait + Deepgram WebSocket adapter as the first impl.
- `POST /api/meetings/:id/start` handler that calls `yogurt_server::audio::start_meeting_recording()` and pipes both broadcast receivers into the Deepgram client.
- `GET /ws/meetings/:id` WebSocket endpoint that fan-outs `transcript` events to the browser per the PRD §10 WS message shape.
- End-to-end smoke: speak into mic + play "Test 1 2 3" through Spotify → see two transcript lines (`Me`, `Them`) appear in a minimal scratch UI within 2 seconds.

Subsequent phases follow the PRD §12 roadmap.
