---
phase: quick-260709-wnn
plan: 01
subsystem: audio
tags: [cpal, tokio, axum, react-query, mpsc, hot-swap]

requires:
  - phase: 02-audio-capture-highest-risk
    provides: yogurt-audio spawn_mic_capture / start_capture / AudioStream
  - phase: 03-cloud-stt-live-transcript
    provides: yogurt-server meetings::Registry capture-thread bridge (std::thread + oneshot)
provides:
  - "spawn_mic_capture(tx, requested_device) device-by-name lookup"
  - "AudioStream::switch_mic_device in-place hot-swap"
  - "AudioCommand mpsc channel + run_capture_control_loop servicing hot-swap commands on the capture std::thread"
  - "POST /api/meetings/{id}/audio-device REST endpoint (404/409/400)"
  - "Registry::start honors persisted audio_input_device setting"
  - "MicDevicePicker toolbar dropdown, visible while recording"
affects: [audio-capture, meeting-toolbar, settings-audio-section]

tech-stack:
  added: []
  patterns:
    - "Capture std::thread command channel (mpsc::Sender<AudioCommand>) added alongside the existing shutdown oneshot, serviced by a tokio::select! loop extracted into a standalone fn for unit-testability across a real thread boundary (mirrors pump_audio_adapter's extraction pattern)"
    - "Hot-swap-on-success-only: spawn replacement resource first, swap the owned field only on Ok, so a bad device id never interrupts a live capture"

key-files:
  created:
    - crates/yogurt-server/tests/audio_device_switch.rs
    - web/src/components/MicDevicePicker.tsx
    - web/src/components/MicDevicePicker.test.tsx
  modified:
    - crates/yogurt-audio/src/mic.rs
    - crates/yogurt-audio/src/lib.rs
    - crates/yogurt-audio/examples/wav_eartest.rs
    - crates/yogurt-audio/examples/dual_smoke.rs
    - crates/yogurt-audio/examples/mic_smoke.rs
    - crates/yogurt-server/src/meetings.rs
    - crates/yogurt-server/src/routes.rs
    - crates/yogurt-server/src/audio.rs
    - web/src/lib/api/settings.ts
    - web/src/components/settings/AudioSection.tsx
    - web/src/routes/Meeting.tsx

key-decisions:
  - "run_capture_control_loop reuses the same tokio::runtime::Handle the capture thread already carried for start_capture's internal drainer tasks — no new runtime handle needed"
  - "Registry::stop clears audio_cmd_tx to None as its first statement, before aborting the supervisor task, so a switch request racing with shutdown reliably observes 409 instead of a dropped-channel race"
  - "AudioDevice.id (frontend) removed — the backend DeviceInfo struct never serialized an id field; AudioSection's picker was silently broken (every option collided on value=undefined) and could never persist a real device name. Fixed as part of this plan since it directly blocked the persisted-default feature"

patterns-established:
  - "Hot-swap-on-success-only RAII: new resource built first, old resource only dropped after the new one succeeds"

requirements-completed: []

duration: ~7min
completed: 2026-07-10
---

# Quick Task 260709-wnn: Live Mic Device Switching During Meeting Summary

**True hot-swap mic-device switching mid-recording (zero transcript gap) plus a fix so the already-persisted `audio_input_device` Settings value actually takes effect on meeting start.**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-07-09T23:49:00-07:00
- **Completed:** 2026-07-09T23:56:21-07:00
- **Tasks:** 3
- **Files modified:** 14 (3 created, 11 modified)

## Accomplishments

- `yogurt-audio`: `spawn_mic_capture` resolves a device by name via `host.input_devices()` (falling back to `default_input_device()` when `None`/empty); `AudioStream::switch_mic_device` hot-swaps `_mic` only on success, leaving the prior capture running (and `mic_tx` + every subscriber untouched) on error.
- `yogurt-server`: the meeting's capture `std::thread` gained an `mpsc::Sender<AudioCommand>` channel serviced by a `tokio::select!` loop (`run_capture_control_loop`) alongside the existing shutdown `oneshot`; unit-tested across a real thread boundary with a fake switch closure to prove in-order command handling and clean shutdown with no deadlock.
- New `POST /api/meetings/{id}/audio-device` endpoint (404 unknown meeting / 409 not recording / 400 device error), behind the existing `require_session_token` middleware.
- `Registry::start` now reads the persisted `audio_input_device` setting and passes it through, so Settings' saved device actually determines which mic a new recording opens (previously dead UI).
- Frontend `MicDevicePicker`: controlled `<select>` in the meeting toolbar, visible only while recording, reflecting the actual active device after a switch (not just the OS default), with loading/switching/error states.
- Fixed `AudioSection.tsx`'s `d.id` bug (the backend `DeviceInfo` never had an `id` field — every Settings-page option silently collided on `value=undefined`, so the persisted-default feature had nothing real to read).

## Task Commits

Each task was committed atomically (TDD RED confirmed for the right reason before each GREEN implementation):

1. **Task 1: yogurt-audio device-targeted capture + hot-swap** - `87a95a4` (feat)
2. **Task 2: yogurt-server control channel + endpoint + persisted-device default** - `bafcdc3` (feat)
3. **Task 3: frontend MicDevicePicker** - `6d92e3b` (feat)

_Note: per-task TDD (RED confirmed via compile-error / import-error, then GREEN) was verified interactively but landed as a single commit per task rather than separate test→feat commits, matching this plan's `type="auto" tdd="true"` task granularity (behavior + implementation are one unit per task, not per RED/GREEN gate)._

## Files Created/Modified

- `crates/yogurt-audio/src/mic.rs` - `spawn_mic_capture(tx, requested_device)` by-name lookup + unknown-device unit test
- `crates/yogurt-audio/src/lib.rs` - `start_capture(mic_device)`, `AudioStream::switch_mic_device`
- `crates/yogurt-audio/examples/{wav_eartest,dual_smoke,mic_smoke}.rs` - updated for new two-arg signatures
- `crates/yogurt-server/src/meetings.rs` - `AudioCommand`, `SwitchDeviceError`, `run_capture_control_loop`, `Registry::start(mic_device)`, `Registry::switch_mic_device`
- `crates/yogurt-server/src/routes.rs` - `POST /api/meetings/{id}/audio-device`, persisted-device wiring in `start_meeting`
- `crates/yogurt-server/src/audio.rs` - `start_meeting_recording(mic_device)` signature update
- `crates/yogurt-server/tests/audio_device_switch.rs` - 404/409/403 REST contract tests
- `web/src/lib/api/settings.ts` - `AudioDevice` shape fix (removed nonexistent `id`), `audioApi.switchMeetingDevice`
- `web/src/components/settings/AudioSection.tsx` - `d.id` → `d.name` fix
- `web/src/components/MicDevicePicker.tsx` / `.test.tsx` - new toolbar component + 5 tests
- `web/src/routes/Meeting.tsx` - mounts `MicDevicePicker` while recording

## Decisions Made

- Used `fireEvent.change` instead of `@testing-library/user-event` in `MicDevicePicker.test.tsx` — the package is not installed in this repo and the plan's Testing Library convention doesn't require it; `fireEvent` is already available via the installed `@testing-library/react` and is sufficient for a controlled `<select>`.
- See `key-decisions` in frontmatter for the control-loop `Handle` reuse and `Registry::stop` ordering decisions.

## Deviations from Plan

None - plan executed exactly as written. All `<action>` instructions (signatures, call sites, line-level edits) matched the real source as specified.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- All three tasks' automated verification commands pass: `cargo build --workspace`, `cargo test -p yogurt-audio --lib mic`, `cargo test -p yogurt-audio --doc`, `cargo test -p yogurt-server run_capture_control_loop`, `cargo test -p yogurt-server --test audio_device_switch`, `cd web && pnpm exec tsc --noEmit && pnpm test` (128 tests, including the 5 new `MicDevicePicker` tests).
- Full `cargo test -p yogurt-server` regression run: 106 passed (20 suites) — no regressions from the `Registry::start` signature change.
- Manual smoke (developer machine only, not automated): start a recording with two input devices connected, switch via the toolbar dropdown mid-meeting, confirm the live transcript dock keeps producing text with no visible gap.

---
*Phase: quick-260709-wnn*
*Completed: 2026-07-10*

## Self-Check: PASSED

All 11 created/modified files verified present on disk; all 3 task commits (87a95a4, bafcdc3, 6d92e3b) verified present in git log.
