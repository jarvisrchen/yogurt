---
phase: 02-audio-capture-highest-risk
plan: 03
subsystem: audio
tags: [audio, server, rest, wav, ear-test, human-verify]
dependency_graph:
  requires:
    - phase-02-01 (yogurt-audio scaffold + SCK spike PASS)
    - phase-02-02 (mic+system capture pipelines, broadcast channels, meeting-relative clock)
  provides:
    - GET /api/audio/devices — input device enumeration (AUDIO-07)
    - GET /api/audio/permission — TCC Screen Recording status
    - POST /api/meeting/{start,stop} — start_meeting_recording shim with RAII supervisor (AUDIO-06)
    - yogurt-audio::examples::wav_eartest — dual-channel WAV writer (LEFT=mic, RIGHT=system)
  affects:
    - Phase 3 STT (subscribes to broadcast receivers exposed via start_meeting)
    - Phase 7 onboarding (consumes /api/audio/permission for the Screen Recording step card)
tech_stack:
  added:
    - "hound 3.5 — WAV writer for ear-test artifacts (example-scoped, not in core lib)"
  patterns:
    - "RAII supervisor on Drop — /api/meeting/stop terminates broadcast senders + SCK stream cleanly"
    - "Arrival-order frame concatenation in WAV writer (NOT monotonic_ms bucketing — that path created 1ms zero-wedges per the 02-03-DIAGNOSIS.md fingerprint)"
key_files:
  - crates/yogurt-server/src/audio.rs (NEW — REST handlers + start_meeting_recording shim)
  - crates/yogurt-server/src/routes.rs (extended — /api/audio/* routes)
  - crates/yogurt-server/src/lib.rs (extended — AppState wiring for audio module)
  - crates/yogurt-server/tests/audio_api.rs (NEW — endpoint smoke tests)
  - crates/yogurt-audio/examples/wav_eartest.rs (NEW + fixed in c92af4a)
  - crates/yogurt-audio/Cargo.toml (added hound dev-dep, example registration)
commits:
  - "06c5008 feat(server): add /api/audio/{devices,permission} endpoints + meeting-recording shim"
  - "8d0e738 feat(audio): dual-channel WAV writer for ear-test acceptance gate"
  - "c92af4a fix(audio,quality): replace monotonic_ms bucketing in wav_eartest with arrival-order concat"
  - "674222f docs(02-03): add diagnosis for audio ear-test static/choppy defect"
requirements:
  satisfied: [AUDIO-06, AUDIO-07]
  notes:
    - "AUDIO-06 verified via RAII Drop tests in tests/audio_api.rs and live /api/meeting/stop smoke"
    - "AUDIO-07 verified via /api/audio/devices smoke test returning a non-empty list with at least the default mic"
verification:
  ear_test_gate:
    artifact: target/yogurt-audio-eartest.wav
    duration_s: 30
    format: 16 kHz / 16-bit signed LE / stereo (LEFT=mic, RIGHT=system)
    channel_attribution: verified by user listening (Glass.aiff RIGHT, voice LEFT, no swap)
    quality:
      first_pass: failed — "staticy and choppy" per user listener
      diagnosis: .planning/phases/02-audio-capture-highest-risk/02-03-DIAGNOSIS.md
      root_cause: WAV writer placed each frame at monotonic_ms*16 sample-offset, but Frame = 320 samples = 20ms exactly. Wall-clock jitter ±1-2ms produced 16-sample zero-wedges = audible 1ms crackle. Fingerprint: 477 exact-zero runs of length-16 on RIGHT channel.
      fix: replaced frames_to_aligned_pcm with frames_to_concat_pcm (arrival-order concat). 3 regression tests added.
      production_impact: NONE. The bug was example-scoped. The core pipeline (mic.rs, system.rs, start_capture.rs) pushes frames to broadcast in arrival order — never goes through the broken bucketing path. Phase 3 STT consumes the broadcast directly and does not inherit this bug.
      second_pass: approved by user 2026-06-25 ("ok looks good. continue")
---

# Plan 02-03 Summary — Audio API + Ear-Test Acceptance Gate

## Outcome

Plan 02-03 delivered the REST surface for the audio subsystem and validated the end-to-end Phase 2 pipeline through the dual-channel WAV ear-test acceptance gate. The user listened to the regenerated WAV with headphones after a quality-defect fix and confirmed clean audio. **Phase 2 acceptance gate cleared.**

## What was built

- **`/api/audio/devices`** — enumerates input devices via `yogurt-audio::list_input_devices()`
- **`/api/audio/permission`** — surfaces TCC Screen Recording status via `has_screen_recording_permission()`
- **`POST /api/meeting/start` + `POST /api/meeting/stop`** — RAII supervisor pattern; Drop on the supervisor handle cleanly terminates broadcast senders and the SCK stream
- **`yogurt-audio` example `wav_eartest`** — captures 30s of dual-channel audio (LEFT=mic, RIGHT=system) and writes a 16kHz 16-bit stereo WAV

## The ear-test gate

The acceptance flow:
1. Executor produced WAV → ffmpeg `astats` confirmed RIGHT channel had healthy levels (Glass.aiff captured)
2. LEFT channel was ambient-only (executor can't speak into mic) — user re-ran with their voice
3. User reported "staticy and choppy" on the first listen
4. Debugger investigated, isolated the bug to `wav_eartest.rs` (NOT the core pipeline), fixed with arrival-order concat, added 3 regression tests
5. User confirmed clean on re-listen → gate cleared

The diagnosis is preserved in `02-03-DIAGNOSIS.md` for future reference. The crucial finding: **the production audio path was never broken** — the bug was in the example/demo WAV writer that used `monotonic_ms`-based sample bucketing. Phase 3 STT consumers will not inherit this defect.

## Test count

- `cargo test --workspace --features yogurt-audio/synthetic` — 16 yogurt-audio + others (full count to be re-confirmed by Phase 2 verifier)
- 3 new regression tests in `wav_eartest` guarding against monotonic_ms re-introduction

## Deferred / followup notes

- The core capture pipeline has solid hardware smoke tests (mic_smoke, system_smoke, dual_smoke from Plan 02-02) but no automated regression coverage for "production frame producer never leaves gaps." Acceptable for Phase 2 — Phase 3 STT integration will exercise this surface continuously in practice.
- The `target/yogurt-audio-eartest.wav` artifact is gitignored (under `target/`); the diagnosis note + this summary are the durable record.
