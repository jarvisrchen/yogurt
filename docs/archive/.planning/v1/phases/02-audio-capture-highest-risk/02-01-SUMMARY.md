---
phase: 02-audio-capture-highest-risk
plan: 01
subsystem: audio
tags: [audio, screencapturekit, permission, scaffolding, spike]
dependency_graph:
  requires:
    - phase-00 (workspace + Cargo.toml conventions)
    - phase-01 (no direct dep; cleared the wave 0 baseline)
  provides:
    - yogurt-audio crate (Frame, Channel, AudioError, format consts)
    - PermissionStatus + has_screen_recording_permission() + request_screen_recording_permission()
    - synthetic sine-wave Frame generator (broadcast-plumbing fixture)
    - Phase 2 path decision (Path A — in-process SCK)
  affects:
    - Cargo.toml workspace (+ 4 deps: cpal, screencapturekit, rubato, thiserror)
    - Cargo.lock (transitive lockfile churn from new deps)
tech_stack:
  added:
    - "cpal 0.15 (workspace dep — not yet used; lands in Plan 02-XX mic capture)"
    - "screencapturekit 8.0 with macos_13_0 feature (gated on cfg(target_os=\"macos\"))"
    - "rubato 0.16 (workspace dep — not yet used; lands in Plan 02-XX resampler)"
    - "thiserror 2 (workspace dep — used in yogurt-audio AudioError today)"
  patterns:
    - "Platform-gated crate-level: target.'cfg(target_os = \"macos\")'.dependencies for screencapturekit (keeps non-mac CI green per D-02)"
    - "Feature-gated public module: pub mod synthetic gated behind cfg(any(test, feature = \"synthetic\")) so the debug source is available in tests + when explicitly enabled"
    - "TDD micro-loop: integration test in tests/frame_contract.rs written first, then frame.rs + error.rs to make it pass"
    - "Spike-first risk retirement: throwaway crate at crates/yogurt-audio/spike/ that was built, run, observed, deleted (only the decision note committed) — exactly the pattern PRD §13 risk #1 prescribes"
key_files:
  created:
    - "docs/superpowers/notes/2026-06-25-sck-spike-result.md (223 lines — the decision Plan 02-XX hinges on)"
    - "crates/yogurt-audio/Cargo.toml"
    - "crates/yogurt-audio/README.md"
    - "crates/yogurt-audio/src/lib.rs (37 lines)"
    - "crates/yogurt-audio/src/frame.rs (60 lines — Frame, Channel, format consts)"
    - "crates/yogurt-audio/src/error.rs (36 lines — AudioError + Result alias)"
    - "crates/yogurt-audio/src/permission.rs (122 lines — PermissionStatus + CGPreflightScreenCaptureAccess FFI)"
    - "crates/yogurt-audio/src/synthetic.rs (109 lines — SineWaveConfig + spawn_sine_wave)"
    - "crates/yogurt-audio/tests/frame_contract.rs (26 lines)"
    - "crates/yogurt-audio/tests/synthetic.rs (71 lines)"
    - "crates/yogurt-audio/tests/permission.rs (44 lines — macOS-gated manual smoke)"
  modified:
    - "Cargo.toml (workspace members + 4 workspace.dependencies entries)"
    - "Cargo.lock (transitive lockfile updates from new crates)"
decisions:
  - "SCK 8.x crate is good enough for v1 audio capture — proceed with Path A (in-process SCK), skip Swift sidecar fallback (Path B)"
  - "Pin screencapturekit at version 8 with macos_13_0 feature (NOT 0.3 as the plan called for — crate has bumped multiple majors since plan was written)"
  - "Skip objc2 / objc2-foundation deps the plan called for — permission.rs uses bare extern \"C\" with #[link(name=\"CoreGraphics\", kind=\"framework\")] which is 3 lines and zero external dependencies"
  - "Audio-only SCStream config still needs valid video dims (width >= 2, height >= 2) — documented in spike note as 'API quirk #1' for Plan 02-XX implementer"
  - "Use only /usr/lib/swift on Swift runtime rpath; do NOT add Xcode's swift-5.5/macosx as a second fallback (causes duplicate-class warnings + spurious TCC denial)"
metrics:
  duration: "~1.2 hours (single executor session, autonomous)"
  completed: "2026-06-25T19:06:58Z"
  tasks_completed: 3
  commits: 3
  files_created: 11
  files_modified: 2
  tests_added: 8
  tests_ignored: 1
  total_workspace_tests: 36
---

# Phase 2 Plan 1: yogurt-audio scaffold + SCK spike Summary

`yogurt-audio` crate is stood up with a stable public surface (Frame, Channel,
format consts, AudioError, PermissionStatus), TDD-validated broadcast plumbing
via a synthetic sine-wave generator, and — most importantly — the project's
highest-risk technical question is now answered: **the modern `screencapturekit`
8.x crate delivers non-zero PCM bytes from a macOS system-audio loopback stream
on Apple Silicon macOS 15.6, so Plan 02-XX implements in-process SCK (Path A)
and the Swift sidecar fallback (Path B) is not needed.**

## What this plan accomplished

### Task 1 — SCK audio-loopback spike (the decision Plan 02 hinged on)

Built a throwaway spike crate (`crates/yogurt-audio/spike/`) against
`screencapturekit = "8"` with the `macos_13_0` feature, configured an
audio-only `SCStream` (`with_captures_audio(true)` +
`with_excludes_current_process_audio(true)` + minimal video dims `2x2`),
attached an `SCStreamOutputTrait` handler counting bytes per audio callback,
and ran it for 5 seconds with `afplay /System/Library/Sounds/Glass.aiff` +
`Funk.aiff` looping in the background.

**Empirical result:**

| Metric | Observed |
|---|---|
| Audio callbacks fired | 250 over 5 s (≈ 50 Hz, 20 ms cadence) |
| Total audio bytes | 1,920,000 |
| Non-zero bytes | 1,426,700 (74.3%) |
| Buffers per callback | 2 (= stereo, matches `.with_channel_count(2)`) |
| Bytes per buffer | 3,840 (= 960 samples × 4 bytes/f32 → confirms 48 kHz f32 stereo) |
| `stop_capture()` shutdown | clean, no orphan threads |

The silent stretches inside the run aligned with the gaps between the two
`.aiff` files — SCK is faithfully reporting silence vs. content, not
dropping audio.

**Outcome: PASS.** Spike crate deleted; 223-line decision note committed at
`docs/superpowers/notes/2026-06-25-sck-spike-result.md` documenting:
- The PASS outcome with empirical evidence.
- Three SCK 8.x API quirks Plan 02-XX must work around (audio-only still
  needs ≥2x2 video config; closure handlers must be `Fn + Send + Sync +
  'static`; audio buffers are **parallel L/R**, not interleaved).
- The Swift Concurrency rpath gotcha — the SCK crate's `build.rs` emits
  rpath args but they don't propagate to the binary's `LC_RPATH` table; a
  binary built normally dies at load with `dyld: Library not loaded:
  @rpath/libswift_Concurrency.dylib`. Worked around in the spike via
  `DYLD_FALLBACK_LIBRARY_PATH=/usr/lib/swift`. Plan 02-XX must add a
  `build.rs` to `yogurt-audio` (or to whichever bin crate links SCK)
  emitting `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift`.
- A subtle landmine: combining /usr/lib/swift **and** the Xcode toolchain
  swift dir on DYLD_FALLBACK loads two copies of libswift_Concurrency,
  triggers `objc[]: Class … is implemented in both …` warnings, **and
  then** causes `SCShareableContent::get()` to return `No shareable content
  available: Content unavailable: The user declined TCCs for application,
  window, display capture` — even though `CGPreflightScreenCaptureAccess`
  is simultaneously returning `true`. The TCC error is a red herring;
  the root cause is the duplicate Swift runtime.
- Implementer notes for Plan 02-XX Task 2.6.

### Task 2 — `yogurt-audio` crate bootstrap + public types

- Added `crates/yogurt-audio` to the workspace `members`.
- Pinned 4 new shared deps in `[workspace.dependencies]`: `cpal=0.15`,
  `screencapturekit = { version = "8", features = ["macos_13_0"] }` (NOT
  `0.3`), `rubato=0.16`, `thiserror=2`.
- New `crates/yogurt-audio/Cargo.toml` references all four via
  `workspace = true`; `screencapturekit` is gated to
  `target.'cfg(target_os = "macos")'.dependencies` per D-02 so Linux CI
  stays green.
- `src/frame.rs`: `pub const SAMPLE_RATE_HZ: u32 = 16_000`,
  `pub const FRAME_SAMPLES: usize = 320`, `Channel { Mic, System }`
  (serde lowercase), `Frame { channel, monotonic_ms, samples }` with a
  `Frame::new` constructor that asserts `samples.len() == FRAME_SAMPLES`.
- `src/error.rs`: `AudioError { PermissionDenied, MicUnavailable(String),
  SystemCaptureFailed(String), UnsupportedPlatform, Cpal(String), Io }`
  via `thiserror`; `pub type Result<T>` alias.
- `tests/frame_contract.rs` — three TDD-first integration tests
  (`it_exposes_format_constants`, `it_constructs_a_frame_with_correct_length`,
  `it_panics_on_wrong_length`). All passed.
- `README.md` documents the 16 kHz / mono / i16 / 320-sample contract,
  the macOS-permission surface, non-macOS behavior, and the Plan 02-01
  vs Plan 02-XX scope split.

### Task 3 — Permission detection + synthetic sine-wave generator

`src/permission.rs`:
- `PermissionStatus { Granted, Denied, NotRequired }` (serde snake_case).
- `has_screen_recording_permission()` — non-prompting probe, delegates
  to `macos::check()` on macOS (returns `NotRequired` elsewhere).
- `request_screen_recording_permission()` — triggers TCC dialog,
  documents the "must restart once after granting" UX quirk.
- `#[cfg(target_os = "macos")] mod macos` wraps two bare `extern "C"`
  declarations of CoreGraphics `CGPreflightScreenCaptureAccess` /
  `CGRequestScreenCaptureAccess` (SAFETY documented inline).
- **AUDIO-01 API surface satisfied** — Plan 02-XX `start_capture()`
  will use `has_screen_recording_permission()` as the permission gate
  per D-25.

`src/synthetic.rs`:
- `SineWaveConfig { channel, frequency_hz, amplitude }` with
  `default_for(channel)` (440 Hz, amplitude 16_000).
- `spawn_sine_wave(cfg, tx) -> JoinHandle<()>` emits Frame at 20 ms
  cadence via `tokio::time::interval` with `MissedTickBehavior::Delay`
  (drops late ticks per D-19 broadcast semantics).
- Internal `generate_chunk(cfg, frame_idx)` uses **global sample
  index** (`frame_idx * FRAME_SAMPLES + i`) — adjacent frames join
  without a phase click. Two in-module unit tests pin this.

Tests:
- `tests/synthetic.rs` — two integration tests
  (`it_emits_correct_length_frames_at_the_expected_cadence`,
  `multiple_subscribers_each_receive_the_same_frames`) running over
  the real `broadcast::Sender<Frame>` plumbing Plan 02-XX will reuse.
- `tests/permission.rs` — macOS-gated, single `#[test] #[ignore]`
  manual-smoke test that prints the two-pass (denied → granted)
  verification checklist per D-30 / D-31.

## Test counts

| Suite | Passed | Ignored |
|---|---|---|
| `yogurt-audio` unit tests (in `src`) | 3 | 0 |
| `frame_contract` integration | 3 | 0 |
| `synthetic` integration | 2 | 0 |
| `permission` integration | 0 | 1 (manual smoke) |
| **`yogurt-audio` total** | **8** | **1** |
| Workspace total (yogurt-cli + server + audio) | 36 | 1 |

Plan target was "at least 7 passed" — exceeded.

Pre-plan baseline: 28 Rust tests (Phase 0/1). Post-plan: 36. Delta: +8.

## Verification gates

| Gate | Status |
|---|---|
| `docs/superpowers/notes/2026-06-25-sck-spike-result.md` exists, names Path A | PASS |
| `crates/yogurt-audio/spike/` deleted | PASS |
| `cargo build -p yogurt-audio --all-targets --features synthetic` clean | PASS |
| `cargo test -p yogurt-audio --features synthetic` >= 7 passed | PASS (8 passed + 1 ignored) |
| `cargo test -p yogurt-audio permission` reports manual smoke ignored, not failed | PASS |
| `cargo clippy -p yogurt-audio --all-targets --features synthetic -- -D warnings` clean | PASS |
| Workspace-wide `cargo test --workspace` green | PASS (36 tests) |
| Three atomic commits in order: spike → bootstrap+types → permission+synthetic | PASS |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Planning bug] `screencapturekit` crate bumped from 0.3 → 8.0**

- **Found during:** Task 1 (`cargo search screencapturekit --limit 5`).
- **Issue:** The phase plan and 02-01 GSD plan both called for
  `screencapturekit = "0.3"`. The crate has had several major version
  bumps since the plan was authored; current is **8.0.0** with a
  substantially different builder-pattern API (`SCContentFilter::create()
  .with_*().build()`, `SCStreamConfiguration::new().with_*()`,
  closure-based output handlers via `add_output_handler`, etc.).
- **Fix:** Pinned `screencapturekit = { version = "8", features =
  ["macos_13_0"] }` in `[workspace.dependencies]`. Rewrote the spike
  source against the 8.x API. Documented all the API quirks (audio-only
  still needs valid video dims, `Fn + Send + Sync + 'static` handler
  trait bounds, parallel-L/R buffer layout) in the spike result note
  for Plan 02-XX Task 2.6.
- **Files modified:** `Cargo.toml`, the spike source (deleted).
- **Commit:** 5fa29d9.

**2. [Rule 1 - Plan documentation bug] `CGPreflightScreenCaptureAccess` link failure**

- **Found during:** Task 3, first `cargo test` run after writing
  `permission.rs` against the plan's recipe.
- **Issue:** Plan asserted (D-23) that CoreGraphics.framework is
  "auto-linked — no extra `#[link]` needed." It isn't, in a leaf
  library crate's unit tests. The bare `extern "C"` block produced
  `Undefined symbols for architecture arm64: "_CGPreflightScreenCaptureAccess"`
  at link time, blocking `cargo test`.
- **Fix:** Added `#[link(name = "CoreGraphics", kind = "framework")]`
  on the `extern "C"` block in `permission::macos`. Documented the
  reason inline in the source comment so a future maintainer doesn't
  strip it.
- **Files modified:** `crates/yogurt-audio/src/permission.rs`.
- **Commit:** 237b26f.

**3. [Rule 1 - Plan dependency over-spec] Dropped unused `objc2` + `objc2-foundation` deps**

- **Found during:** Task 2 implementation.
- **Issue:** Plan called for `objc2 = "0.5"` and `objc2-foundation = "0.2"`
  in the macOS-gated `[target.'cfg(target_os = "macos")'.dependencies]`
  block. The plan's stated reason was that `permission.rs` needed them
  to call `CGPreflightScreenCaptureAccess`. In practice, the bare
  `extern "C"` + `#[link]` approach (deviation #2 above) uses zero
  external crates.
- **Fix:** Omitted both `objc2` and `objc2-foundation` from the
  manifest. If a future task needs the Objective-C runtime (e.g., for
  the `NSScreenCaptureUsageDescription` Info.plist value at runtime,
  or for hooking into NSWorkspace) they can be re-added then.
- **Files modified:** `crates/yogurt-audio/Cargo.toml`.
- **Commit:** e12bb87.

**4. [Rule 3 - Build-environment fix] Swift Concurrency rpath gotcha**

- **Found during:** Task 1 spike (first run attempt).
- **Issue:** A binary that transitively depends on `screencapturekit`
  links against Swift Concurrency (the SCK Swift bridge uses
  `Swift.Concurrency`), but the SCK crate's `build.rs`'s
  `cargo:rustc-link-arg=-Wl,-rpath,...` flags **don't propagate**
  to the final binary's `LC_RPATH` table. The binary dies at load
  with `dyld: Library not loaded: @rpath/libswift_Concurrency.dylib`.
- **Fix for the spike:** worked around by setting
  `DYLD_FALLBACK_LIBRARY_PATH=/usr/lib/swift` at invocation time.
- **Deferred for Plan 02-XX:** add a `build.rs` to whichever crate
  ends up producing the final binary that links SCK (likely
  `yogurt-cli`) emitting `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift`.
  Critically, do **NOT** also add Xcode's swift-5.5 toolchain path —
  combining the two loads two copies of the dylib and produces
  duplicate-objc-class warnings plus spurious "TCC declined" errors
  from `SCShareableContent::get()`.
- **Status:** documented for Plan 02-XX in the spike result note;
  not a blocker for this plan (no binary in this plan links SCK).

### Authentication Gates

None. The terminal Claude is running in had Screen Recording permission
pre-granted at the TCC level (`CGPreflightScreenCaptureAccess()`
returns `true`), so the spike ran end-to-end without a manual user
intervention. Future runs on a fresh machine will hit a TCC prompt on
first invocation per the plan's documentation.

## Threat Flags

None new. The threat surface added by this plan is exactly what
`02-CONTEXT.md`'s threat model anticipates:
- `permission.rs` is a typed read-side probe of macOS TCC state; no
  new attack surface.
- `synthetic.rs` is test-only (or feature-gated debug) and feeds
  pre-determined sine-wave PCM; cannot leak real audio.
- `frame.rs` / `error.rs` are pure data definitions.

## Known Stubs

None. Every public function in this plan has a real implementation
behind it:
- `Frame::new` actually constructs and length-asserts.
- `has_screen_recording_permission()` actually calls the OS.
- `spawn_sine_wave` actually emits frames over the broadcast channel.

The follow-up Plan 02-XX is documented up-front as the place where
real mic + system capture lands (see `crates/yogurt-audio/README.md`'s
"Plan 02 scope" table). This is **deferred functionality**, not
**stubbed functionality** — the absent capability is honestly
absent at the API surface, not stubbed to return empty values.

## Directive for Plan 02-XX (Phase 2's next plan)

**Implement system audio capture via in-process `screencapturekit` 8.x
(Path A).** Do NOT implement the Swift sidecar fallback (Path B).

Specific implementation guidance for Plan 02-XX:

1. Implement `crates/yogurt-audio/src/system.rs` using the 8.x
   builder pattern documented in the spike result note. Use
   `screencapturekit::prelude::*` exclusively.
2. SCStream config: `with_width(2).with_height(2).with_captures_audio(true)
   .with_excludes_current_process_audio(true).with_sample_rate(48000)
   .with_channel_count(2)`. (Stick with SCK's native 48 kHz f32 stereo
   output and resample to 16 kHz mono i16 with `rubato` 0.16 per
   D-14 / D-17.)
3. Treat each callback's `audio_buffer_list()` as two parallel mono
   L/R buffers — not one interleaved buffer. Downmix via `(L+R)*0.5`.
4. Add a `build.rs` emitting
   `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift` to the binary
   crate (likely `yogurt-cli`) so the final binary loads
   `libswift_Concurrency.dylib` without `DYLD_FALLBACK_LIBRARY_PATH`.
   Do not add Xcode's swift-5.5 toolchain path as a fallback —
   duplicate Swift runtime causes spurious TCC errors (see spike note).
5. RAII via `Drop` on the producer struct stops the SCK stream
   without an explicit `.stop()` method per D-26.
6. The synthetic broadcast-plumbing tests in `tests/synthetic.rs`
   will continue to pass under Plan 02-XX's real producers because
   they exercise only the `broadcast::Sender<Frame>` contract.

## Commits

| # | Hash | Message |
|---|---|---|
| 1 | 5fa29d9 | spike(02-01): SCK system-audio loopback PASS — 8.0 crate, 74% nonzero bytes |
| 2 | e12bb87 | feat(audio): bootstrap yogurt-audio crate + Frame/Channel/AudioError types |
| 3 | 237b26f | feat(audio): add permission detection + synthetic sine-wave generator |

Plan duration: ~1.2 hours (single autonomous executor session).

## Self-Check: PASSED

Verified all claims against disk + git:

- `docs/superpowers/notes/2026-06-25-sck-spike-result.md` — FOUND (223 lines)
- `crates/yogurt-audio/Cargo.toml` — FOUND
- `crates/yogurt-audio/README.md` — FOUND
- `crates/yogurt-audio/src/lib.rs` — FOUND (37 lines)
- `crates/yogurt-audio/src/frame.rs` — FOUND (60 lines)
- `crates/yogurt-audio/src/error.rs` — FOUND (36 lines)
- `crates/yogurt-audio/src/permission.rs` — FOUND (122 lines)
- `crates/yogurt-audio/src/synthetic.rs` — FOUND (109 lines)
- `crates/yogurt-audio/tests/frame_contract.rs` — FOUND
- `crates/yogurt-audio/tests/synthetic.rs` — FOUND
- `crates/yogurt-audio/tests/permission.rs` — FOUND
- `crates/yogurt-audio/spike/` — CORRECTLY ABSENT (deleted per plan)
- Commit `5fa29d9` (spike) — FOUND in git log
- Commit `e12bb87` (bootstrap+types) — FOUND in git log
- Commit `237b26f` (permission+synthetic) — FOUND in git log
- `cargo test -p yogurt-audio --features synthetic` reports 8 passed + 1 ignored — VERIFIED
- `cargo clippy -p yogurt-audio --all-targets --features synthetic -- -D warnings` clean — VERIFIED
