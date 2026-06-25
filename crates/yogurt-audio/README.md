# yogurt-audio

macOS audio capture for [yogurt](../../README.md). Two synchronized 16 kHz
mono `i16` PCM streams:

- `Channel::Mic` — default input device via `cpal` / CoreAudio (lands in Plan 02).
- `Channel::System` — system audio loopback via `screencapturekit` 8.x /
  ScreenCaptureKit framework. macOS 13+ (lands in Plan 02).

## Format contract (load-bearing for Phase 3)

| | |
|---|---|
| Sample rate | 16,000 Hz (`SAMPLE_RATE_HZ`) |
| Channels   | 1 (mono) — system loopback is L+R averaged before emit |
| Sample fmt | `i16` (signed 16-bit LE) |
| Frame size | 320 samples / 20 ms (`FRAME_SAMPLES`) |
| Frame tag  | `Channel::{Mic, System}` |
| Clock      | `monotonic_ms` since `start_capture()` returned |

Phase 3 STT engines (Deepgram, whisper.cpp) consume this format directly —
no resampling at the STT boundary.

## Permissions (macOS)

System loopback requires macOS **Screen Recording** permission (the same TCC
gate as screenshot apps). Call `has_screen_recording_permission()` before
`start_capture()` and surface `PermissionStatus::Denied` to the user as the
PRD §5.11 recovery screen.

The OS prompts for permission on the first call to `request_screen_recording_permission()`.
**TCC limitation:** the binary must be restarted once after the user grants
permission before the grant takes effect — surface PRD §5.10's "restart once"
copy.

## Non-macOS platforms

- `Channel::System` returns `AudioError::UnsupportedPlatform` on non-macOS
  targets — both the type surface and `cargo build` stay green on Linux CI.
- `Channel::Mic` works everywhere (`cpal` is cross-platform).
- `has_screen_recording_permission()` returns `PermissionStatus::NotRequired`
  on non-macOS.

## Plan 02 scope

This crate is being scaffolded across Phase 2's two GSD plans:

| Plan | Scope |
|---|---|
| 02-01 | Type surface (`Frame`, `Channel`, `AudioError`, format consts), permission detection, synthetic sine-wave test fixture. |
| 02-XX | Real mic capture (`cpal`), real system capture (`screencapturekit` 8.x), resampler (`rubato`), broadcast plumbing, REST endpoints. |

See the phase context at `.planning/phases/02-audio-capture-highest-risk/`.

## Phase 2 spike result

The mandatory `screencapturekit` 8.x audio-loopback spike (Plan 02-01 Task 1)
**passed** on Apple Silicon macOS 15.6 — 74% non-zero bytes captured over a
5-second window with system audio playing. See
[`docs/superpowers/notes/2026-06-25-sck-spike-result.md`](../../docs/superpowers/notes/2026-06-25-sck-spike-result.md)
for empirical evidence, the SCK 8.x API quirks Plan 02 must work around,
and the Swift-runtime rpath gotcha future implementers will hit.
