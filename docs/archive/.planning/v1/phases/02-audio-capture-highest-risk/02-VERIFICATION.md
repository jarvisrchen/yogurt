---
phase: 02-audio-capture-highest-risk
verified: 2026-06-25T16:55:00Z
status: gaps_found
score: 5/7 must-haves verified
overrides_applied: 0
gaps:
  - truth: "cargo fmt --all -- --check passes cleanly"
    status: failed
    reason: "Module ordering violation in crates/yogurt-server/src/lib.rs — `pub mod audio` is interleaved between private modules instead of grouped with the other `pub mod` declarations."
    artifacts:
      - path: "crates/yogurt-server/src/lib.rs"
        issue: "Lines 1–7: `mod assets;`, `mod dev_proxy;`, `mod routes;`, `pub mod audio;`, `pub mod session;` etc. cargo fmt expects `pub mod audio;` reordered to sit with other `pub mod` declarations after the private mods."
    missing:
      - "Run `cargo fmt --all` in crates/yogurt-server/src/lib.rs to reorder modules to fmt's canonical order. One-line fix."
  - truth: "AUDIO-05: mic↔system drift bounded by an automated test (roadmap success criterion #3 expects a 60-min synthetic test with drift < 250 ms; AUDIO-05 itself requires < 50 ms)."
    status: partial
    reason: "Drift is reasoned about in docstrings (`spawn-order skew is microseconds, trivially < 50 ms`) and confirmed structurally (both `FrameChunker::new()` calls happen synchronously inside `start_capture()`), but no test asserts a measured drift bound. REQUIREMENTS.md self-documents that the 60-min assertion is deferred to Phase 3 (\"once STT timestamps land\"). The roadmap success criterion #3 explicitly calls for the 60-min synthetic test as part of Phase 2."
    artifacts:
      - path: "crates/yogurt-audio/src/lib.rs"
        issue: "Lines 113–118: docstring-only reasoning; no test."
      - path: "crates/yogurt-audio/examples/dual_smoke.rs"
        issue: "10-second smoke prints frame counts and peaks but does not measure or assert drift between the two channels' `monotonic_ms` baselines."
    missing:
      - "Either (a) add an automated synthetic-clock drift test against `start_capture()` that asserts cross-channel `monotonic_ms` drift < 50 ms over N seconds, OR (b) record an explicit deferral override accepting that the structural argument + Phase 3 60-min assertion together satisfy AUDIO-05."
deferred:
  - truth: "AUDIO-01: end-to-end 'first record triggers macOS Screen Recording prompt' verification"
    addressed_in: "Phase 3 (or further-out plan in Phase 2)"
    evidence: "REQUIREMENTS.md line for AUDIO-01: 'API ready (2026-06-25) … end-to-end \"prompt fires on first record\" verification deferred to Plan 02-XX once start_capture() exists' — the API (`request_screen_recording_permission`) is shipped, and `start_capture()` does fast-fail on Denied, but the deliberate one-shot prompt trigger flow is documented as deferred."
  - truth: "60-min mic↔system drift synthetic test (roadmap SC #3)"
    addressed_in: "Phase 3"
    evidence: "REQUIREMENTS.md AUDIO-05 row: 'Long-run 60-min drift assertion deferred to Phase 3 once STT timestamps land.' Roadmap SC #3 also calls for this 60-min test, but the requirement table explicitly defers it."
human_verification:
  - test: "Ear-test re-listen of target/yogurt-audio-eartest.wav with headphones."
    expected: "30-second stereo WAV, LEFT channel = mic, RIGHT channel = system audio, both audible during the middle 28 seconds, no static/crackle, no channel swap, no clipping. (Per the 02-03-SUMMARY user already approved this 2026-06-25 — re-listen only required if the user wants a fresh confirmation.)"
    why_human: "Acceptance is auditory perception; no automation can replace ear judgment of static/crackle on a real capture."
  - test: "AUDIO-01 manual permission-prompt smoke: revoke Screen Recording for the cargo test binary in System Settings, then run `cargo test -p yogurt-audio --test permission --ignored -- --nocapture`."
    expected: "Test prints 'CURRENT STATUS: Denied'. Then grant permission, quit the test runner, re-run; expect 'CURRENT STATUS: Granted'. Confirms the TCC API surface fires correctly."
    why_human: "TCC interaction cannot be automated; must be performed on a real Mac with the System Settings UI."
  - test: "AUDIO-06 manual supervisor termination smoke: run `cargo run -p yogurt-audio --example dual_smoke`, wait for the 10-second window to complete, observe process exits with no orphaned threads."
    expected: "Process returns exit code 0; no `objc[]` warnings, no leaked SCK handles in console, no audio recording indicator persists after exit."
    why_human: "RAII cleanup is structurally correct (`_stream` keep-alive on Drop), but verifying no orphan-thread / no audio-indicator-persistence requires watching the macOS UI and console during teardown."
---

# Phase 2: Audio Capture (HIGHEST RISK) Verification Report

**Phase Goal:** `yogurt-audio` crate captures mic + system audio via ScreenCaptureKit into a Tokio broadcast channel, with the meeting-relative clock model designed in from day one and a documented Swift sidecar fallback path if the SCK crate's audio loopback surface proves insufficient. The 30-second dual-channel PCM → WAV → ear-test acceptance is a phase gate, not a checkbox.

**Verified:** 2026-06-25T16:55:00Z
**Status:** gaps_found (one nuisance fmt fix, one drift-assertion gap with a deferred-by-design fallback)
**Re-verification:** No — initial verification.

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `yogurt-audio` crate exists and captures mic via `cpal` into 16 kHz mono i16 frames | ✓ VERIFIED | `crates/yogurt-audio/src/mic.rs:139` `spawn_mic_capture()` opens default input, routes through `Downmix` (resample.rs) → `FrameChunker` → broadcast. Tests in `mic.rs:235–278` cover chunker contract. |
| 2 | System audio captured via SCK 8.x with `excludesCurrentProcessAudio = true` and stereo→mono downmix | ✓ VERIFIED | `crates/yogurt-audio/src/system.rs:130–136` SCStreamConfiguration sets `with_excludes_current_process_audio(true)` AND `with_sample_rate(48_000)`/`with_channel_count(2)`. Downmix handled by the same shared `Downmix` helper. |
| 3 | Both streams pushed to Tokio broadcast channel with capacity ≥ 256 | ✓ VERIFIED | `crates/yogurt-audio/src/lib.rs:45` `pub const BROADCAST_CAPACITY: usize = 256`. lib.rs:127–128 both senders use this constant. |
| 4 | Meeting-relative clock from `Instant::now()` at start, mic↔system drift < 50 ms | ⚠️ PARTIAL | Structural correctness confirmed: `FrameChunker::new()` captures `Instant::now()` (mic.rs:79) and both chunkers are constructed synchronously inside `start_capture()` (lib.rs:132–134). NO automated drift measurement asserts the < 50 ms bound. REQUIREMENTS.md explicitly defers the 60-min long-run assertion to Phase 3. |
| 5 | Recording stops cleanly on Drop; per-meeting supervisor terminates cleanly (RAII, AUDIO-06) | ✓ VERIFIED | `MicCapture._stream: cpal::Stream` (mic.rs:33) and `SystemCapture._inner: macos::Inner` holding `_stream: SCStream` (system.rs:93) are keep-alive fields; their `Drop` impls terminate the underlying OS-level streams. `AudioStream` owns both, so dropping the handle from `start_meeting_recording()` tears down both atomically. |
| 6 | User can list mic input devices via REST (AUDIO-07) | ✓ VERIFIED | `GET /api/audio/devices` mounted in `crates/yogurt-server/src/routes.rs:17`. Handler `audio::get_devices` returns `Json<Vec<DeviceInfo>>` from `list_input_devices()`. Integration test `tests/audio_api.rs:11–50` asserts JSON-array shape with `name`, `is_default`, `sample_rate` keys. |
| 7 | 30-second dual-channel WAV produced + ear-tested by human, both channels audible, no static/swap | ✓ VERIFIED | `target/yogurt-audio-eartest.wav` exists, 1.8 MB ≈ 30 s stereo 16 kHz 16-bit (LEFT = mic, RIGHT = system). User approved second-pass listen 2026-06-25 ("ok looks good. continue") per 02-03-SUMMARY. |

**Score:** 5/7 truths fully VERIFIED; 1 PARTIAL (drift assertion deferred); 1 NEW FAILURE found by verifier (fmt — not in plan truths, but mandatory gate).

### Deferred Items

| # | Item | Addressed In | Evidence |
|---|------|-------------|----------|
| 1 | AUDIO-01 end-to-end "first record triggers Screen Recording prompt" | Future plan in Phase 2 or Phase 3 onboarding wiring | REQUIREMENTS.md AUDIO-01 row: "API ready (2026-06-25) — `has_screen_recording_permission()` + `request_screen_recording_permission()` exposed; end-to-end verification deferred to Plan 02-XX once start_capture() exists." |
| 2 | 60-min mic↔system drift synthetic test (roadmap Phase 2 SC #3) | Phase 3 (STT timestamps) | REQUIREMENTS.md AUDIO-05 row: "Long-run 60-min drift assertion deferred to Phase 3 once STT timestamps land." |

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/yogurt-audio/Cargo.toml` | screencapturekit 8 + cpal 0.15 + rubato 0.16 + hound (dev-dep) | ✓ EXISTS + SUBSTANTIVE | Cargo.toml declares all four. `screencapturekit` is correctly target-gated to `cfg(target_os = "macos")`. |
| `crates/yogurt-audio/build.rs` | rpath fix for `/usr/lib/swift` per SCK 8.x | ✓ EXISTS + SUBSTANTIVE | `println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift")` gated to macOS. |
| `crates/yogurt-audio/src/frame.rs` | Frame { channel, monotonic_ms, samples }, SAMPLE_RATE_HZ=16k, FRAME_SAMPLES=320 | ✓ EXISTS + SUBSTANTIVE | Exactly the contract. `Frame::new` panics on wrong sample count (frame.rs:45–51). |
| `crates/yogurt-audio/src/mic.rs` | cpal capture + FrameChunker + list_input_devices | ✓ EXISTS + SUBSTANTIVE | 280 lines, all three responsibilities, 3 unit tests. |
| `crates/yogurt-audio/src/system.rs` | SCK 8.x stream with excludes_current_process_audio=true | ✓ EXISTS + SUBSTANTIVE | 232 lines, AUDIO-03 satisfied (line 134). |
| `crates/yogurt-audio/src/lib.rs` | start_capture() returning AudioStream with both broadcasts | ✓ EXISTS + SUBSTANTIVE | 143 lines, BROADCAST_CAPACITY=256, permission gate fast-fails. |
| `crates/yogurt-audio/src/resample.rs` | 48 kHz f32 stereo → 16 kHz mono i16 downmix via rubato SincFixedIn | ✓ EXISTS + SUBSTANTIVE | 206 lines, 4 unit tests. |
| `crates/yogurt-audio/src/permission.rs` | CGPreflightScreenCaptureAccess / CGRequestScreenCaptureAccess bindings | ✓ EXISTS + SUBSTANTIVE | 138 lines, both check + request, NotRequired on non-macOS. |
| `crates/yogurt-audio/examples/wav_eartest.rs` | 30-second dual-channel WAV writer | ✓ EXISTS + SUBSTANTIVE | 367 lines including the post-fix `frames_to_concat_pcm` + 3 regression tests. |
| `crates/yogurt-audio/examples/dual_smoke.rs` | Manual hardware smoke for mic+system simultaneously | ✓ EXISTS | 86 lines. No drift assertion (see gap). |
| `crates/yogurt-server/src/audio.rs` | get_devices, get_permission, start_meeting_recording, AudioErrorWrap | ✓ EXISTS + SUBSTANTIVE | 101 lines, all four. |
| `crates/yogurt-server/src/routes.rs` | /api/audio/{devices,permission} GET routes | ✓ EXISTS + WIRED | routes.rs:17–18. **Note:** routes.rs does NOT mount POST /api/meeting/start or /stop — see "Wiring Discrepancy" below. |
| `crates/yogurt-server/tests/audio_api.rs` | Integration tests for both REST endpoints | ✓ EXISTS + SUBSTANTIVE | 79 lines, both tests assert shape contracts; observed PASS in `cargo test`. |
| `target/yogurt-audio-eartest.wav` | 30-s stereo 16 kHz 16-bit WAV | ✓ EXISTS | 1.8 MB, user-approved on 2026-06-25. |
| `docs/superpowers/notes/2026-06-25-sck-spike-result.md` | Documented Swift sidecar fallback path (Path B) | ✓ EXISTS | 11.3 KB; explicitly states "The crate's audio surface is good enough for v1. The Swift sidecar fallback (Path B) is not needed." Fallback path is documented for reach-for-later use, satisfying the goal requirement. |

**Artifacts:** 15/15 exist and are substantive.

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| `start_capture()` (lib.rs) | `spawn_mic_capture` + `spawn_system_capture` | direct call | ✓ WIRED | lib.rs:133–134 calls both synchronously and stores returned handles. |
| `MicCapture` Drop | cpal Stream Drop | RAII via `_stream` field | ✓ WIRED | mic.rs:33, `cpal::Stream` keep-alive; Drop stops capture. |
| `SystemCapture` Drop | SCStream Drop | RAII via `Inner { _stream: SCStream }` | ✓ WIRED | system.rs:93. |
| `cpal callback` | `Downmix.push() → FrameChunker.feed() → broadcast::Sender.send()` | Arc<Mutex<(Downmix, FrameChunker)>> | ✓ WIRED | mic.rs:163–222 (both F32 and I16 branches). |
| `SCK audio handler` | `Downmix.push() → FrameChunker.feed() → broadcast::Sender.send()` | Arc<Mutex<(Downmix, FrameChunker)>> | ✓ WIRED | system.rs:146–197 unpacks parallel L/R buffers, interleaves, feeds Downmix. |
| `yogurt-server::audio` | `yogurt_audio::{start_capture, has_screen_recording_permission, list_input_devices}` | `use yogurt_audio::{...}` | ✓ WIRED | audio.rs:24–27. |
| `routes.rs` | `audio::get_devices`, `audio::get_permission` | `.route("/api/audio/devices", get(audio::get_devices))` | ✓ WIRED | routes.rs:17–18. |
| `start_meeting_recording()` | `start_capture()` | direct call | ✓ WIRED | audio.rs:60–62. Returns `AudioStream` for Phase 3 STT consumption. |

**Wiring:** 8/8 production links verified.

#### Wiring Discrepancy (informational, not a gap against the PLAN truths)

- **SUMMARY/PLAN-frontmatter `provides`** list `POST /api/meeting/{start,stop} — start_meeting_recording shim with RAII supervisor (AUDIO-06)`.
- **Codebase reality:** `routes.rs` does NOT mount any `POST /api/meeting/*` route. Only the in-process `start_meeting_recording()` function exists (audio.rs:60–62), and its docstring correctly notes "Phase 3 will wire this into POST /api/meetings/:id/start once that lands."
- **PLAN must_haves.truths** (the authoritative contract) only require the in-process function, not the REST routes. Therefore this is not a truth failure — but the SUMMARY narrative is misleading on this point. Phase 3 will need to actually mount these POST routes.

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `start_capture()` returned `AudioStream` | mic_tx / system_tx broadcast::Sender<Frame> | cpal callback + SCK handler | Yes — hardware-verified: 249 frames in 5s mic, 248 frames in 5s SCK with Glass.aiff (per REQUIREMENTS.md AUDIO-02/03 rows) | ✓ FLOWING |
| `get_devices` response | `Vec<DeviceInfo>` | `cpal::default_host().input_devices()` | Yes — observed non-empty array on local Mac via test pass | ✓ FLOWING |
| `get_permission` response | `PermissionStatus` | `CGPreflightScreenCaptureAccess()` | Yes — TCC system call returns granted/denied | ✓ FLOWING |
| `target/yogurt-audio-eartest.wav` | LEFT/RIGHT i16 samples | mic + system broadcast receivers (arrival-order concat) | Yes — 1.8 MB artifact, user-approved | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Workspace builds with synthetic feature | `cargo build --workspace --features yogurt-audio/synthetic` | Finished `dev` profile in 4.06s, clean | ✓ PASS |
| Full test suite passes | `cargo test --workspace --features yogurt-audio/synthetic` | **46 passed, 1 ignored, 0 failed** across 15 suites in 1.87s | ✓ PASS |
| Clippy clean with -D warnings | `cargo clippy --all-targets --features yogurt-audio/synthetic -- -D warnings` | No issues | ✓ PASS |
| Format check | `cargo fmt --all -- --check` | **FAILED** — module ordering diff in `crates/yogurt-server/src/lib.rs` | ✗ FAIL |
| Ear-test artifact present | `ls -la target/yogurt-audio-eartest.wav` | 1.8 MB, exists | ✓ PASS |
| Production pipeline has no `monotonic_ms * 16` bug | `grep -rn "monotonic_ms\s*\*\s*[0-9]" crates/yogurt-audio/src/` | Zero matches; only the regression-test docstring in `examples/wav_eartest.rs` mentions the pattern | ✓ PASS |
| No STT/LLM scope leak in yogurt-audio | `grep -rn "deepgram\|whisper\|openai" crates/yogurt-audio/src/` excluding comments | Zero non-comment matches | ✓ PASS |

### Probe Execution

Phase 2 PLANs do not declare `scripts/*/tests/probe-*.sh` style probes. The acceptance gate is the human ear-test (already cleared) plus the manual `cargo run --example {mic,system,dual,wav_eartest}_smoke` paths (executable, but require hardware + human listening — surfaced under Human Verification).

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| AUDIO-01 | 02-01 | Screen Recording prompt triggers on first record | ⚠️ API READY / deferred end-to-end | `request_screen_recording_permission` exists in `permission.rs:53–62`; end-to-end "fires on first record" test deferred per REQUIREMENTS.md. Surfaced as human verification. |
| AUDIO-02 | 02-02 | Mic mono 16 kHz 16-bit PCM | ✓ SATISFIED | `spawn_mic_capture` + `Downmix` + `FrameChunker`; 249 frames in 5s hardware-verified. |
| AUDIO-03 | 02-02 | System loopback with `excludes_current_process_audio = true` | ✓ SATISFIED | system.rs:134 sets flag; 248 frames in 5s hardware-verified. |
| AUDIO-04 | 02-02 | Broadcast capacity ≥ 256 | ✓ SATISFIED | `BROADCAST_CAPACITY = 256` const, both channels. |
| AUDIO-05 | 02-02 | Meeting-relative clock, drift < 50 ms | ⚠️ STRUCTURAL ONLY | Both chunkers seeded synchronously; no measured drift assertion; 60-min test deferred to Phase 3 per REQUIREMENTS.md. See gap. |
| AUDIO-06 | 02-03 | Clean Drop termination | ✓ SATISFIED (structural) | RAII chain `AudioStream` → `MicCapture._stream` + `SystemCapture._inner._stream`. Surfaced as human verification for orphan-thread / indicator-persistence check. |
| AUDIO-07 | 02-03 | `/api/audio/devices` lists mic devices | ✓ SATISFIED | Route mounted, integration test asserts shape. |

**Coverage:** 5/7 SATISFIED, 1 STRUCTURAL-ONLY (AUDIO-05), 1 API-READY-DEFERRED (AUDIO-01).

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| crates/yogurt-server/src/lib.rs | 1–7 | `cargo fmt` module-ordering violation | ⚠️ Warning | Blocks `cargo fmt --check` gate; trivial fix. |
| (none) | — | TODO / FIXME / XXX / unimplemented! / todo! / placeholder | — | Grep finds zero occurrences in `crates/yogurt-audio/` or `crates/yogurt-server/src/audio.rs`. |
| (none) | — | `monotonic_ms * 16` bucketing in production code | — | Confirmed scoped to `examples/wav_eartest.rs` and its regression-test docstring only. |
| (none) | — | STT / LLM scope leak | — | Confirmed zero non-comment references to deepgram/whisper/openai in `crates/yogurt-audio/`. |

**Anti-patterns:** 1 found (1 warning, 0 blockers).

### Human Verification Required

See `human_verification:` frontmatter section above. Three items:

1. Optional ear-test re-listen of `target/yogurt-audio-eartest.wav` (already user-approved).
2. AUDIO-01 manual TCC prompt smoke (revoke → run `permission --ignored` → grant → re-run).
3. AUDIO-06 manual supervisor termination smoke (run `dual_smoke`, observe clean exit, no orphan-thread / persistent audio indicator).

## Gaps Summary

### Critical Gaps (Block Progress)

None. The two gaps are nuisance-level and a partially-met requirement with an explicit deferred-to-Phase-3 plan recorded in REQUIREMENTS.md.

### Non-Critical Gaps

1. **`cargo fmt --check` fails on module ordering in `crates/yogurt-server/src/lib.rs`.**
   - Fix: `cargo fmt --all` (one-line reorder). This was added during 02-03 wiring; the executor missed the auto-fmt pass.
   - Impact: trivial — does not affect runtime behavior, but blocks the CI fmt gate when one lands.

2. **AUDIO-05 drift assertion is structural-only, not measured.**
   - Fix options:
     - (a) Add a synthetic-clock drift test in `tests/` that drives two `spawn_sine_wave` producers and asserts cross-channel `monotonic_ms` baseline drift < 50 ms over N seconds; OR
     - (b) Accept the deferral via an override entry referencing REQUIREMENTS.md's explicit "deferred to Phase 3" line and roadmap SC #3's "60-min synthetic test" being a Phase-3-supported assertion.
   - Impact: low — the structural argument (synchronous seed inside `start_capture()`, microsecond spawn-order skew) is sound; what's missing is the literal "we measured it" evidence the roadmap SC #3 implies.

3. **SUMMARY narrative overstates REST surface.**
   - The 02-03-SUMMARY claims `POST /api/meeting/{start,stop}` are mounted; only the in-process `start_meeting_recording()` shim exists. This does not fail the PLAN's `must_haves.truths` (which only require the in-process function), but Phase 3 will need to actually mount the POST routes. Worth fixing the SUMMARY wording to avoid future misreads.

**This looks intentional for items 2 and 3.** To accept the AUDIO-05 measured-drift deferral, add to VERIFICATION.md frontmatter:

```yaml
overrides:
  - must_have: "AUDIO-05: mic↔system drift bounded by an automated test"
    reason: "REQUIREMENTS.md explicitly defers the 60-min drift assertion to Phase 3 (once STT timestamps land). The structural argument — both FrameChunker baselines seeded synchronously inside start_capture() with microsecond spawn-order skew — is sound; literal measurement requires Phase 3's STT timestamp track to exist as a reference."
    accepted_by: "<your name>"
    accepted_at: "<ISO timestamp>"
```

## Recommended Fix Plans

### 02-04-PLAN.md: Format + AUDIO-05 Drift Smoke (Small)

**Objective:** Clear the `cargo fmt --check` gate and convert AUDIO-05's structural argument into a measurable smoke test.

**Tasks:**
1. Run `cargo fmt --all` and commit the resulting module-reorder in `crates/yogurt-server/src/lib.rs`.
2. Add `crates/yogurt-audio/tests/drift.rs` that runs two `spawn_sine_wave` producers through `start_capture`-equivalent plumbing for 5 seconds and asserts cross-channel `monotonic_ms` baseline drift < 50 ms.
3. Update `02-03-SUMMARY.md` to reflect that POST /api/meeting/{start,stop} routes are Phase-3 work (the in-process shim is what Phase 2 delivers).
4. Re-run `cargo build && cargo test && cargo clippy && cargo fmt --check` — all green.

**Estimated scope:** Small (≤ 30 min).

---

## Verification Metadata

**Verification approach:** Goal-backward (derived from Phase 2 success criteria in ROADMAP.md + must_haves frontmatter in each plan).
**Must-haves source:** ROADMAP.md success criteria 1–5 + 02-{01,02,03}-PLAN.md frontmatter.
**Automated checks:** 6 passed (build, test, clippy, ear-test-exists, no-bucketing-leak, no-stt-scope-leak), 1 failed (fmt).
**Human checks required:** 3 (one optional re-listen, two manual hardware smokes).
**Total verification time:** ~15 min.

---

*Verified: 2026-06-25T16:55:00Z*
*Verifier: Claude (gsd-verifier)*
