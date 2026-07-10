---
phase: quick-260709-wnn
verified: 2026-07-10T00:10:00Z
status: human_needed
score: 5/5 must-haves verified (code + automated tests); 1 item needs human hardware verification
overrides_applied: 0
human_verification:
  - test: "Start a recording with two real input devices connected (e.g. built-in mic + AirPods/USB interface). Mid-recording, use the new MicDevicePicker dropdown in the meeting toolbar to switch devices."
    expected: "The live transcript dock keeps producing text with no visible gap across the switch (no STT session restart, no audio dropout); the dropdown's displayed value updates to the newly active device; unplugging/selecting an invalid device leaves the prior mic capturing and shows an inline error instead of ending the recording."
    why_human: "Requires real macOS Screen Recording/mic TCC permission and real audio hardware. This sandbox cannot open cpal/ScreenCaptureKit streams (no existing test in this crate calls Registry::start with a real device either), so the zero-transcript-gap hot-swap claim and the fail-safe-leaves-prior-device-capturing claim are only verified at the unit level (mocked switch closure in run_capture_control_loop_services_commands_then_exits_cleanly, and the spawn-first-swap-on-Ok-only logic in AudioStream::switch_mic_device) — not end-to-end with real audio."
---

# Quick Task 260709-wnn: Live Mic Device Switching During Meeting Verification Report

**Task Goal:** Add live mic/audio-source switching during an active meeting — a toolbar control that changes the captured microphone mid-recording with zero gap in the transcript, plus making the already-persisted Settings `audio_input_device` value actually take effect on meeting start.

**Verified:** 2026-07-10T00:10:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User can pick a different mic mid-recording from a toolbar dropdown; switch takes effect with no gap in transcript/system-audio capture | ✓ VERIFIED (code + unit test) / needs human for real-hardware zero-gap claim | `MicDevicePicker.tsx` mounted in `Meeting.tsx` toolbar (`{meetingId && recording && <MicDevicePicker meetingId={meetingId} />}`, line 277-279); `AudioStream::switch_mic_device` (lib.rs:111-116) reuses `mic_tx` unchanged so subscribers never resubscribe; `run_capture_control_loop_services_commands_then_exits_cleanly` unit test proves the real `tokio::select!` bridge services swap commands in order with no deadlock across a real thread boundary. Real-hardware, real-transcript "no visible gap" claim requires a live mic — routed to human verification. |
| 2 | Picker reflects actual active device after a successful switch, not just OS default | ✓ VERIFIED | `MicDevicePicker.tsx` `activeDevice` state set only in mutation `onSuccess` (line 33); `effectiveValue` prefers `activeDevice` over `is_default` (line 52-56); `MicDevicePicker.test.tsx` test 3 ("reflects the resolved active device after a successful switch") asserts `select` value becomes "AirPods Pro" after switch — ran and passed. |
| 3 | Meeting started after picking a device in Settings opens that device by default | ✓ VERIFIED | `routes.rs` `start_meeting` (line 201-217) reads `load_general(&state.db)`, derives `mic_device` from `g.audio_input_device`, passes to `state.meetings.start(&id, stt_settings, mic_device)`; `Registry::start` forwards `mic_device.as_deref()` into `yogurt_audio::start_capture` (meetings.rs:333). Also fixed the previously-dead `AudioSection.tsx` `d.id` bug (never existed on wire type `DeviceInfo`) so Settings can actually persist a real device name — confirmed fix at `AudioSection.tsx:45` (`d.name` now used as key/value). |
| 4 | A device that fails to open leaves the previous mic capturing and surfaces an error, not killing the recording | ✓ VERIFIED (code-level) | `AudioStream::switch_mic_device` spawns the replacement `MicCapture` FIRST via `spawn_mic_capture` and only assigns `self._mic = new_mic` on `Ok` (lib.rs:112-115) — on `Err` the prior `_mic` is untouched and the error propagates. `spawn_mic_capture_unknown_device_returns_mic_unavailable` unit test (mic.rs:397-405) proves the by-name lookup returns `Err(AudioError::MicUnavailable(_))` for a nonexistent device instead of panicking. Frontend surfaces the error inline without touching `activeDevice` (`MicDevicePicker.tsx` error branch, `MicDevicePicker.test.tsx` test 5 passed). |
| 5 | Switch request on a non-recording meeting returns 409; on unknown meeting id returns 404 | ✓ VERIFIED | `crates/yogurt-server/tests/audio_device_switch.rs::it_returns_404_for_unknown_meeting` and `::it_returns_409_when_meeting_is_not_recording` are real integration tests spinning up the actual axum server via `run_with_config` and hitting the real HTTP endpoint with `reqwest` — both ran and passed (plus a bonus `it_rejects_unauthenticated_switch_requests` → 403 WR-08 regression test, also passed). |

**Score:** 5/5 truths supported by code + automated tests. Truth #1's "zero gap in live transcript" claim under real hardware conditions cannot be exercised in this sandbox and is routed to human verification (see below), consistent with the plan's own documented sandbox limitation.

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/yogurt-audio/src/mic.rs` | `spawn_mic_capture(tx, requested_device)` by-name lookup + unit test | ✓ VERIFIED | Signature matches plan exactly (lines 175-178); unknown-device unit test present and passing. |
| `crates/yogurt-audio/src/lib.rs` | `start_capture(mic_device)` + `AudioStream::switch_mic_device` | ✓ VERIFIED | Both present exactly as specified (lines 111-116, 138). Doctest updated to `start_capture(None)` and passes. |
| `crates/yogurt-server/src/meetings.rs` | `AudioCommand`, `run_capture_control_loop`, `Registry::start(mic_device)`, `Registry::switch_mic_device`, `SwitchDeviceError` | ✓ VERIFIED | All present (lines 116-130, 264-269, 534-564, 577-596). `audio_cmd_tx` cleared on `stop()` first statement (line 474) as specified. |
| `crates/yogurt-server/src/routes.rs` | `POST /api/meetings/{id}/audio-device` + persisted-device wiring | ✓ VERIFIED | Route registered under `require_session_token` layer (lines 69-72); handler + `SwitchDeviceRequest` present (lines 227-261); `start_meeting` reads `audio_input_device` (lines 201-217). |
| `crates/yogurt-server/tests/audio_device_switch.rs` | 404/409/403 REST regression tests | ✓ VERIFIED | 3 real integration tests against a spawned server; all pass (`cargo test -p yogurt-server --test audio_device_switch` → 3 passed). |
| `web/src/components/MicDevicePicker.tsx` | in-meeting mic dropdown | ✓ VERIFIED | Present, controlled `<select>`, loading/switching/error states, matches plan spec closely. |
| `web/src/routes/Meeting.tsx` | mounts `MicDevicePicker` in toolbar | ✓ VERIFIED | Mounted at line 277-279, gated on `meetingId && recording`. |

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `meetings.rs Registry::start` | `yogurt_audio::start_capture` | `mic_device.as_deref()` forwarded | ✓ WIRED | `start_capture(mic_device.as_deref())` at meetings.rs:333. |
| `meetings.rs run_capture_control_loop` | `AudioStream::switch_mic_device` | in-thread call inside `tokio::select!` | ✓ WIRED | `stream.switch_mic_device(opt)` inside the closure passed to `run_capture_control_loop` (meetings.rs:357-360); loop body itself calls `switch(&device_name)` inside the select arm (line 589). |
| `routes.rs switch_meeting_audio_device` | `meetings.rs Registry::switch_mic_device` | `state.meetings.switch_mic_device(&id, body.device_id)` | ✓ WIRED | routes.rs:241. |
| `MicDevicePicker.tsx` | `POST /api/meetings/:id/audio-device` | `audioApi.switchMeetingDevice` mutation, `response.device` drives value | ✓ WIRED | settings.ts:144-151 (`switchMeetingDevice`); `MicDevicePicker.tsx` `onSuccess: (data) => setActiveDevice(data.device)` (line 33). |
| `routes.rs start_meeting` | `yogurt_db::settings::General.audio_input_device` | `load_general(&state.db)` before `Registry::start` | ✓ WIRED | routes.rs:201-217. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|---------------------|--------|
| `MicDevicePicker.tsx` | `devices.data` | `audioApi.devices()` → `GET /api/audio/devices` → `yogurt_audio::list_input_devices()` (real cpal enumeration, not static) | Yes | ✓ FLOWING |
| `MicDevicePicker.tsx` | `activeDevice` | `switchMeetingDevice` mutation response `data.device`, itself the resolved name from a real `AudioStream::switch_mic_device` call server-side | Yes (mocked in unit tests; real call chain traced through server code) | ✓ FLOWING |

### Behavioral Spot-Checks / Test Execution (run directly by verifier, not trusted from SUMMARY)

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| yogurt-audio mic unit tests (incl. new unknown-device test) | `cargo test -p yogurt-audio --lib mic` | 7 passed | ✓ PASS |
| yogurt-audio doctest | `cargo test -p yogurt-audio --doc` | 1 passed | ✓ PASS |
| yogurt-audio examples build with new signatures | `cargo build -p yogurt-audio --examples` | builds clean | ✓ PASS |
| Capture-thread control-loop unit test | `cargo test -p yogurt-server run_capture_control_loop` | 1 passed | ✓ PASS |
| New REST contract tests | `cargo test -p yogurt-server --test audio_device_switch` | 3 passed | ✓ PASS |
| Full yogurt-server regression | `cargo test -p yogurt-server` | 106 passed (20 suites), no regressions | ✓ PASS |
| Full workspace build | `cargo build --workspace` | builds clean | ✓ PASS |
| Frontend typecheck | `cd web && pnpm exec tsc --noEmit` | no errors | ✓ PASS |
| Frontend test suite (incl. 5 new MicDevicePicker tests) | `cd web && pnpm test -- --run` | 128 passed (22 files) | ✓ PASS |

### Requirements Coverage

No `requirements:` declared in PLAN frontmatter (`requirements: []`); this is a quick task, not tied to REQUIREMENTS.md entries. N/A.

### Anti-Patterns Found

Scanned all 9 core files modified in this phase for `TBD|FIXME|XXX|TODO|HACK|PLACEHOLDER|placeholder|coming soon|not yet implemented`. No blocker-level debt markers found in the files this phase touched. (One pre-existing, unrelated `TODO` comment in `Meeting.tsx` line 53 about a future Phase 5 transcript-state lift predates this phase and was not introduced by it.)

### Human Verification Required

### 1. Real-hardware mid-meeting mic switch with zero transcript gap

**Test:** Start a recording with two real input devices connected (e.g. built-in mic + AirPods or a USB interface). Mid-recording, use the new toolbar dropdown to switch devices; also try switching to a device that's been unplugged.
**Expected:** The live transcript dock keeps producing text with no visible gap across the switch; the dropdown's displayed value updates to the newly active device; switching to an invalid/unplugged device leaves the prior mic still capturing and surfaces an inline error instead of ending the recording.
**Why human:** This sandbox has no real audio hardware and cannot obtain macOS Screen Recording/mic TCC permission, so `Registry::start` (and therefore `AudioStream::switch_mic_device` against a live `cpal::Stream`) cannot be exercised end-to-end here — this is the same limitation the plan itself documents ("no existing test in this crate calls `Registry::start` successfully either"). The concurrency mechanics (control-loop ordering, no-deadlock shutdown, spawn-first-swap-on-success-only) are unit-tested; the actual "no audible/transcript gap" behavior with real audio is not.

### Gaps Summary

No code-level gaps found. All artifacts exist, are substantive, are wired end-to-end, and all automated verification commands specified in the plan (`cargo build --workspace`, the four `cargo test` invocations, `pnpm exec tsc --noEmit`, `pnpm test`) were re-run directly by the verifier (not trusted from SUMMARY.md) and passed. The only open item is the plan's own explicitly-flagged manual/hardware smoke test, which per this verifier's process must route to human verification rather than being marked `passed` outright.

---

_Verified: 2026-07-10T00:10:00Z_
_Verifier: Claude (gsd-verifier)_
