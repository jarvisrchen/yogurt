# AUD-6: mute the mic mid-meeting without touching system audio

Design, not yet implemented.
Ticket: `docs/TODO.md` AUD-6.

Richard wants to step away mid-meeting and talk to someone without that conversation landing in the transcript, without stopping and restarting the recording.
Mic-only: he'd already be muted in the meeting app, so `Channel::System` (the other side of the call) must keep capturing the whole time.
Only `Channel::Mic` needs to stop feeding the pipeline while he's talking to someone off to the side, plus a control to flip it and a visible "currently paused" state.

## Where the mute point lives

`FrameChunker::feed` (`crates/yogurt-audio/src/mic.rs`) is the one place every mic sample passes through before it reaches the broadcast channel that `yogurt-stt` subscribes to - the same seam `chunker_emits_exactly_frame_samples_per_frame` already tests. Gate the broadcast there:

```
cpal IOProc (unchanged, real-time) → SPSC ring → drainer → downmix → FrameChunker::feed
                                                                        └─ muted? → drop the chunk, else → tx.send(frame)
```

The buffer still drains and the monotonic clock still advances while muted - only the `tx.send` is skipped - so there's no discontinuity or timestamp jump when unmuting, just a real gap in the audio that reached the pipeline, which is the whole point.

The cpal callback and the SPSC ring are untouched. That thread is documented real-time-safe (no mutex, no allocation, no broadcast send) and this change doesn't go near it - the mute flag is only read on the tokio drainer side.

## What changes

### 1. `crates/yogurt-audio/src/mic.rs`

- `FrameChunker` gains a `muted: Arc<AtomicBool>` field. `feed` checks it per-chunk before `tx.send`.
- `MicCapture` gains the same `Arc<AtomicBool>` (constructed in `spawn_mic_capture`, passed into the chunker) and a `pub fn set_muted(&self, muted: bool)`.
- New unit test alongside the existing `FrameChunker` tests: feed samples with `muted = true`, assert nothing arrives on `rx`; unmute, feed again, assert frames resume. No hardware needed - same shape as the current tests.

### 2. `crates/yogurt-audio/src/lib.rs`

- `AudioStream::set_mic_muted(&self, muted: bool)` delegates to `self._mic.set_muted(muted)`.

**Simplification:** `switch_mic_device` builds a brand-new `MicCapture` on a hot-swap, so mute state resets to unmuted if the mic device is switched while muted. That combo (switch devices *and* be mid-mute) is rare and low-stakes - worth a `ponytail:` comment rather than threading the flag through the swap.

### 3. `crates/yogurt-server/src/meetings.rs`

- New `AudioCommand::SetMicMuted { muted: bool, reply: oneshot::Sender<Result<(), String>> }`, serviced by `run_capture_control_loop` alongside the existing `SwitchMicDevice` arm - same `tokio::select!` loop, no new machinery.
- `Meeting` gains `mic_muted: Mutex<bool>` (same shape as the existing `stt_engine` field), default `false`, so the true state survives a page reload or a second tab without adding a new sync channel.
- `Registry::set_mic_muted(&self, id: &MeetingId, muted: bool) -> Result<(), SwitchDeviceError>` - same lookup → send → 5s-timeout → error-mapping shape as `Registry::switch_mic_device`, reusing `SwitchDeviceError`'s existing variants (`NotFound`, `NotRecording`).
- Extend `run_capture_control_loop_services_commands_then_exits_cleanly` to also drive a `SetMicMuted` command through the real loop.

### 4. `crates/yogurt-server/src/routes.rs`

- `POST /api/meetings/:id/mic-muted`, body `{"muted": bool}`, same status-code mapping `switch_meeting_audio_device` already uses (`404` not found, `409` not recording, `200` on success).
- `active_recording` (`GET /api/meetings/active`) response gains `"mic_muted": bool`, read the same way `"stt"` is today. This is what makes a reload or a second tab show the true state - no WS event needed, the frontend already polls this every 5s.

### 5. Frontend

- New `web/src/components/MicMuteToggle.tsx`, mounted between the mic-picker row and the notes card in `Meeting.tsx` (`web/src/routes/Meeting.tsx`).
- Full-width `<Button>` (`web/src/components/Button.tsx`), `secondary` variant unmuted, new `warn` variant (solid strawberry, white text) muted - matching the app's existing warn tone (`MetaPill tone="warn"`, the error banner) instead of hand-rolled styles.
- **Always mounted**, not gated on `recording` - a core in-meeting action should stay a big, findable target rather than disappearing. `disabled` (with an explanatory tooltip) while not recording, since muting only makes sense mid-meeting.
- `M` hotkey via the existing `useKeyboardShortcut` hook (`web/src/hooks/useKeyboardShortcut.ts`) - no modifier, for reflex speed; `ignoreWhenTyping: true` so it doesn't fire while notes/title/chat have focus; `enabled: recording` so it can't fire before there's anything to mute.
- Seeded from `activeRecording.data?.mic_muted` (the existing 5s poll), mutates via `POST /api/meetings/:id/mic-muted`, invalidates the `["meetings","active"]` query on success so the toggle doesn't wait out the poll interval.
- `MicMuteToggle.test.tsx` covers both variants, disabled-while-not-recording, click and hotkey wiring (including the hotkey correctly not firing while typing or while not recording).

**Revision (2026-09-01):** shipped first as a small icon button in the mic-picker toolbar row (matching `MicDevicePicker`'s density), gated on `recording` like the row itself. Richard flagged it as a core in-meeting action that needed to be bigger and easier to hit, plus a quick keyboard toggle - landed as the always-visible full-width button + `M` hotkey described above. Verified in the running app (not just tests): unmuted/muted/disabled states, click and the `M` hotkey both toggle correctly.

## What is deliberately not in scope

- **Cross-tab / cross-client live sync via WebSocket.** The existing 5s `active_recording` poll is enough for "see at a glance" - this is a local single-user app, and a WS event would be new plumbing for a case (two tabs open on the same meeting, watching the mute state converge sub-5s) nobody asked for.
- **Preserving mute across a mid-mute device switch.** See the simplification note above.
- **Muting `Channel::System`, or any UI for it.** Explicitly out per the ticket - the other side of the call must keep recording.
- **Physically stopping the cpal stream while muted.** Muting only gates the software send path; the OS-level mic stream keeps running so unmuting is instant and carries no re-open cost, unlike a real device switch.

## Tests

- `crates/yogurt-audio/src/mic.rs`: `FrameChunker` mute/unmute unit test (above).
- `crates/yogurt-server/src/meetings.rs`: extend the `run_capture_control_loop` test to cover `SetMicMuted`.
- `web/src/components/MicMuteToggle.test.tsx`: renders correct icon/label for muted vs. unmuted, mutation fires the right request, error surfaces like `MicDevicePicker`'s does.
- Manual handover: start a recording, say something on mic, hit mute, say something else on mic while the other side (system audio, e.g. Chrome playing a clip) keeps talking - transcript should show the system side continuing and the mic gap, unmute, confirm mic resumes.
