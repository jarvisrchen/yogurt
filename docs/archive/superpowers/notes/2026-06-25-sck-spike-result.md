# SCK audio-only loopback spike — result

**Date:** 2026-06-25
**Phase / Task:** Phase 2 (audio capture) · Task 2.0
**Machine:** Apple Silicon (aarch64), macOS 15.6 (24G84)
**Toolchain:** rustc/cargo stable 1.96 via rustup (rust-toolchain.toml = "stable")
**`screencapturekit` version tested:** **8.0.0** (NOT 0.3 — see "Deviation from plan" below)

## Outcome

**PASS.**

The `screencapturekit` 8.x crate, configured for audio-only loopback with
`with_captures_audio(true)` + `with_excludes_current_process_audio(true)`,
delivers a steady stream of CoreMedia audio sample buffers from system audio
on this Mac. Buffers contain real, non-silent PCM whenever system audio is
actually playing.

## Empirical evidence (one of several runs)

5-second capture window with `afplay` of `/System/Library/Sounds/Glass.aiff`
+ `Funk.aiff` looping in the background:

| Metric | Observed |
|---|---|
| Audio callbacks fired | 250 (≈ 50 Hz, consistent with 20 ms frames) |
| Total audio bytes | 1,920,000 |
| Non-zero bytes | 1,426,700 (74.3%) |
| Buffers per callback | 2 (= stereo, matches `.with_channel_count(2)`) |
| Bytes per buffer | 3,840 → 960 samples × 4 bytes (f32) → confirms 48 kHz f32 stereo as documented in `screencapturekit/src/stream/configuration/audio.rs` |
| Permission dialog | Already granted to this terminal — `CGPreflightScreenCaptureAccess()` returned `1` |

The zero-byte stretches inside the run line up with the silent intervals
between `Glass.aiff` and `Funk.aiff` playback — i.e. SCK is faithfully
reporting silence vs. content, not arbitrarily dropping bytes.

## Deviation from plan — `screencapturekit` is on **8.0**, not 0.3

The phase plan (`docs/superpowers/plans/2026-06-25-yogurt-phase-2-audio-capture.md`,
Task 2.0 Step 1) was written expecting `screencapturekit = "0.3"`. The
crate has since had several major bumps (current: **8.0.0**). The plan's
explicit guidance — _"if only 0.2.x exists, stop and re-check before
proceeding (the public API differs substantially)"_ — applies in reverse:
the public API at 8.x is substantially different from the 0.3 sketch in
the plan. The spike was re-written against the 8.x API; see "API quirks"
below for the exact patterns Plan 02 Task 2.6 will need.

This is recorded as a **planning bug auto-fix (Rule 1)** in the
`02-01-SUMMARY.md` deviation log. **`Cargo.toml` `[workspace.dependencies]`
must pin `screencapturekit = "8"`, not `"0.3"`, in Task 2.1.**

## What worked

- `SCShareableContent::get()` returned 2 displays cleanly.
- `SCContentFilter::create().with_display(display).with_excluding_windows(&[]).build()`
  built a valid filter.
- `SCStreamConfiguration::new().with_width(2).with_height(2).with_captures_audio(true).with_excludes_current_process_audio(true).with_sample_rate(48000).with_channel_count(2)`
  produced a working audio-only configuration. (We still pass `with_width`
  and `with_height` ≥ 2 because SCK requires a valid video config even when
  we never register a Screen output handler — see "API quirks" #1.)
- `SCStream::new(&filter, &config)` succeeded; `add_output_handler(handler, SCStreamOutputType::Audio)`
  attached the audio handler; `start_capture()` returned `Ok(())` immediately.
- `did_output_sample_buffer` fired on a background dispatch queue
  approximately every 20 ms with `of_type == SCStreamOutputType::Audio`.
- `sample.audio_buffer_list()` returned `Some(AudioBufferList)`; `.iter()`
  yielded 2 `AudioBuffer`s per callback, each `data()` returning a
  3,840-byte slice (= 960 f32 samples per buffer per 20 ms tick).
- `stop_capture()` returned `Ok(())` and the producer thread shut down
  cleanly without leaking.

## What didn't work (and what to fix in Plan 02)

### 1. **Swift Concurrency runtime not auto-rpath'd into the produced binary**

The `screencapturekit` 8.x `build.rs` emits
`cargo:rustc-link-arg=-Wl,-rpath,...` to point at Xcode's
`libswift_Concurrency.dylib`. **These flags do not propagate from a
transitive dependency to the final binary's `LC_RPATH` table.** A binary
built normally fails at load time:

```
dyld[…]: Library not loaded: @rpath/libswift_Concurrency.dylib
  Reason: no LC_RPATH's found
```

Worked around for the spike by exporting
`DYLD_FALLBACK_LIBRARY_PATH=/usr/lib/swift` before invoking the binary.

**Plan 02 fix (Task 2.6 implementer):** add a `build.rs` to `yogurt-audio`
(or to whichever binary crate ends up linking SCK — `yogurt-cli`) that
emits its own `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift` so the
final binary's `LC_RPATH` table contains the system Swift dylib path.
For development builds on machines where `/usr/lib/swift` is missing,
the Xcode toolchain path
(`$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx`)
also works — but adding *both* paths via DYLD_FALLBACK caused the macOS
`objc[]: Class … is implemented in both /usr/lib/swift/… and …xctoolchain/…`
warnings + a (spurious) `"user declined TCCs"` failure from `SCShareableContent::get()`.
The fix is to bake **only `/usr/lib/swift`** into the binary's rpath table
and skip the Xcode toolchain fallback.

### 2. **Initial DYLD path mistake caused a misleading TCC error**

When the spike first ran with both Xcode and system swift dirs on
`DYLD_FALLBACK_LIBRARY_PATH`, two copies of `libswift_Concurrency.dylib`
loaded into the same process. The `objc[]` runtime complained about
duplicate classes, and **then** `SCShareableContent::get()` returned
`No shareable content available: Content unavailable: The user declined
TCCs for application, window, display capture` — even though
`CGPreflightScreenCaptureAccess()` was returning `true` at that same
instant. **Root cause is the duplicate Swift runtime, not TCC.** This is
a subtle landmine future Plan 02 / Phase 7 implementers may hit when
debugging permission flows — record it here so the next person doesn't
spend an hour blaming TCC.

### 3. **Crate module paths are private — must use the prelude**

The Task 2.0 sketch in the plan reached into
`screencapturekit::cm::sample_buffer::CMSampleBuffer` and other internal
paths directly. In 8.x those modules are private (`mod sample_buffer;`).
Everything we need is re-exported by `screencapturekit::prelude::*`:

- `SCShareableContent`, `SCContentFilter`, `SCStreamConfiguration`,
  `SCStream`, `SCStreamOutputType`, `SCStreamOutputTrait`,
  `CMSampleBuffer` (via the `CMSampleBufferSCExt` / `CMSampleBufferExt`
  extension traits).

Plan 02 Task 2.6 should `use screencapturekit::prelude::*;` and only
reach into specific submodules (`stream::configuration::audio::AudioSampleRate`,
`stream::configuration::audio::AudioChannelCount`) when typed enums are
preferable to raw `i32` arguments.

### 4. **Spike binary's runtime PATH dependency on `xcode-select`**

When the build.rs runs without a working `xcode-select`, the Swift
bridge compile-step fails entirely (`swift build` not found). This is
not a new constraint, but worth flagging: the repo already implicitly
requires Xcode Command Line Tools for development builds.

## API quirks (for Plan 02 Task 2.6 implementer)

1. **Audio-only `SCStream` still needs a valid video config**: SCK requires
   `set_width(W)` / `set_height(H)` to be ≥ 2 even when no Screen handler
   is registered. Trying to omit the video config or set width=0 produces
   an opaque SCK error at `start_capture()`. Use `.with_width(2).with_height(2)`
   as the canonical "we only want audio" sentinel.

2. **Closure handlers must be `Fn + Send + Sync + 'static`**: SCK invokes
   delegates concurrently from arbitrary dispatch queues. The `2.0`
   release notes call this out explicitly. The `AudioCounter` struct in
   the spike used `Arc<AtomicUsize>` for exactly this reason — Plan 02
   Task 2.6's broadcast-sender handle (`tokio::sync::broadcast::Sender<Frame>`)
   is `Send + Sync` by construction, so the production wrapping is straightforward.

3. **48 kHz f32 stereo is the SCK output format regardless of
   `with_sample_rate(16000)`**: the spike requested `with_sample_rate(48000).with_channel_count(2)`
   and observed exactly that. Setting `with_sample_rate(16000)` would
   ask SCK to do the downsample itself; per the
   `AudioSampleRate` enum docs the supported set is `{8000, 16000, 24000, 48000}`,
   so SCK can do it. **Recommendation for Plan 02**: stick with the
   plan's existing decision (D-14 / D-17) to take SCK's 48 kHz f32 stereo
   output and resample with `rubato` 0.16 in `yogurt-audio/src/resample.rs`
   ourselves. Reasons: (a) `rubato`'s `SincFixedIn` is the well-tuned reference
   resampler; (b) the same code path serves both the cpal mic input (which
   we have to resample anyway because mic hardware is usually 48 kHz f32);
   (c) keeps SCK doing only what it does best (raw capture).

4. **Audio buffers are interleaved**: per-callback, `audio_buffer_list().iter()`
   yields 2 buffers (one per channel), **NOT** one interleaved buffer.
   Plan 02 downmix needs to average L+R sample-by-sample, not stride
   across one buffer. Concretely: `let l = &buf0.data()[..]; let r = &buf1.data()[..];`
   then iterate `(l_sample + r_sample) / 2.0`.

5. **`stop_capture()` is synchronous and blocks ~tens of ms**: the spike
   observed a clean shutdown without orphaned threads. Plan 02's RAII
   `Drop for AudioStream` design (D-26) is sound.

## Decision

**Proceed with in-process SCK (Task 2.5 / 2.6 path A).**

The crate's audio surface is good enough for v1. The Swift sidecar
fallback (Path B) is not needed. Plan 02 Task 2.1 must pin
`screencapturekit = { version = "8", features = ["macos_13_0"] }` (NOT
`"0.3"`), and Task 2.6 must apply the API patterns documented in this note.

## Notes for Plan 02 Task 2.6 implementer

- Pin `screencapturekit = "8"` with at least the `macos_13_0` feature
  (for `with_captures_audio` / audio outputs). `macos_15_0` enables
  microphone capture via SCK if you'd rather replace `cpal` later
  (out of scope for v1 — D-13 says cpal stays).
- Use `screencapturekit::prelude::*` exclusively; do not reach into
  private modules.
- Audio-only config sentinel: `width = 2, height = 2`.
- Always set `with_excludes_current_process_audio(true)` per D-16 / AUDIO-03.
- Treat the callback's `audio_buffer_list()` as **two parallel mono
  buffers (L, R)**, not one interleaved buffer. Downmix to mono via
  `(L + R) * 0.5`.
- The producer thread is owned by SCK; on `Drop` of `SCStream`, SCK
  tears it down. No explicit thread-join needed (matches D-26).
- Add a build.rs to `yogurt-audio` (or `yogurt-cli`) emitting
  `cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift` so the binary loads
  Swift Concurrency without `DYLD_FALLBACK_LIBRARY_PATH` set by the
  user. **Do not** add the Xcode toolchain path as a second rpath
  fallback — that triggers the duplicate-class warning + spurious TCC
  denial documented above.

## Reproducibility

The spike crate was throwaway and has been deleted per plan instructions.
To reproduce:

1. Create a new Cargo bin crate (outside the workspace).
2. Add `screencapturekit = { version = "8", features = ["macos_13_0"] }`
   and `anyhow = "1"`.
3. Copy the captured spike source from this commit's history (see the
   `crates/yogurt-audio/spike/` tree at this commit's parent — but it
   was deleted in the same commit, so it lives only in the spike branch
   if you preserved it).
4. Build: `cargo build --release`.
5. Run with `/usr/lib/swift` on `DYLD_FALLBACK_LIBRARY_PATH`, with
   audio playing, in a terminal that has Screen Recording permission.
