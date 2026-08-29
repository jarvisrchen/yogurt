---
phase: quick
plan: 260701-vjb
subsystem: onboarding
tags: [welcome, permissions, screen-recording, settings-links]
requires: []
provides:
  - "POST /api/audio/screen-recording/request endpoint"
  - "requestScreenRecordingPermission() API wrapper"
  - "Actionable Welcome Steps 1, 3, and 4"
affects: [welcome-onboarding]
key-files:
  created: []
  modified:
    - crates/yogurt-server/src/audio.rs
    - crates/yogurt-server/src/routes.rs
    - crates/yogurt-server/tests/audio_api.rs
    - web/src/lib/api/audio.ts
    - web/src/routes/Welcome.tsx
decisions:
  - "Step 1 always shows both Grant button and System Settings link because CGPreflight cannot distinguish never-asked from denied"
  - "Step 4 state derived from steps 1-3 but does not gate ready (Deepgram cloud is the seeded default)"
metrics:
  duration: "~5 minutes"
  completed: 2026-07-01
---

# Quick Task 260701-vjb: Make Welcome Onboarding Steps Actionable Summary

New POST /api/audio/screen-recording/request endpoint fires the TCC prompt without starting capture, and Welcome Steps 1/3/4 now offer Grant buttons and Settings links instead of dead cards.

## What Was Done

### Task 1: Backend endpoint (99fc11f)

Added `request_screen_recording` handler in `crates/yogurt-server/src/audio.rs`, mirroring `request_microphone` exactly.
It fires `yogurt_audio::request_screen_recording_permission()` fire-and-forget and returns the combined `PermissionResponse` snapshot.
Registered `POST /api/audio/screen-recording/request` in `audio_routes` behind `require_session_token`.
Updated the module doc-comment to list four endpoints and removed the stale claim that the prompt fires implicitly via `start_capture` (that path deadlocked first-run users since the CTA is gated on the permission).
Added `it_request_screen_recording_returns_combined_snapshot` integration test.

### Task 2: Frontend wrapper + actionable steps (88fd68d)

Added `requestScreenRecordingPermission()` to `web/src/lib/api/audio.ts`, mirroring the mic wrapper.
In `Welcome.tsx`:

- Step 1 (not granted): primary "Grant Screen Recording" button firing the `screenRequest` mutation, plus a secondary "Open System Settings" link to the Privacy_ScreenCapture pane. Both always shown together since CGPreflight cannot distinguish never-asked from denied.
- Step 3 (current): "Set up in Settings →" link below the provider chips.
- Step 4: state derived (`granted && micGranted && hasProvider` → current), with a "Choose in Settings →" link when current. `ready` computation unchanged.

## Deviations from Plan

None - plan executed exactly as written.

## Verification

- `cargo test -p yogurt-server --test audio_api`: 7 passed (including the new screen-recording test).
- `cd web && pnpm exec tsc --noEmit`: no errors.
- `pnpm test`: 123 tests passed across 21 files.
- Manual curl smoke skipped (no running server in this environment); the integration test exercises the same path end to end.

## Known Stubs

None.

## Self-Check: PASSED
