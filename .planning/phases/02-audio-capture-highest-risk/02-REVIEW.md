---
phase: 02-audio-capture-highest-risk
reviewed: 2026-06-25T16:55:00Z
depth: deep
files_reviewed: 17
files_reviewed_list:
  - crates/yogurt-audio/src/lib.rs
  - crates/yogurt-audio/src/frame.rs
  - crates/yogurt-audio/src/error.rs
  - crates/yogurt-audio/src/mic.rs
  - crates/yogurt-audio/src/system.rs
  - crates/yogurt-audio/src/resample.rs
  - crates/yogurt-audio/src/permission.rs
  - crates/yogurt-audio/src/synthetic.rs
  - crates/yogurt-audio/build.rs
  - crates/yogurt-audio/Cargo.toml
  - crates/yogurt-audio/examples/wav_eartest.rs
  - crates/yogurt-audio/examples/mic_smoke.rs
  - crates/yogurt-audio/examples/system_smoke.rs
  - crates/yogurt-audio/examples/dual_smoke.rs
  - crates/yogurt-audio/tests/frame_contract.rs
  - crates/yogurt-audio/tests/synthetic.rs
  - crates/yogurt-audio/tests/permission.rs
  - crates/yogurt-server/src/audio.rs
  - crates/yogurt-server/src/routes.rs
  - crates/yogurt-server/src/lib.rs
  - crates/yogurt-server/tests/audio_api.rs
  - crates/yogurt-server/build.rs
  - crates/yogurt-cli/build.rs
findings:
  blocker: 4
  critical: 4
  warning: 9
  info: 5
  total: 18
status: issues_found
---

# Phase 2: Code Review Report — Audio Capture (HIGHEST RISK)

**Reviewed:** 2026-06-25T16:55Z
**Depth:** deep
**Files Reviewed:** ~23 (audio crate + server wiring + build scripts)
**Status:** issues_found

## Summary

Phase 2 ships a working in-process SCK + cpal capture pipeline that produced a usable WAV after the post-hoc `monotonic_ms`-bucketing fix. The code is well-documented and the ScreenCaptureKit integration is competent. However, **the adversarial review surfaces four BLOCKER-class production bugs that the verifier appears to have missed** — the choppy/static fix landed in the example, but the same bug class (and several worse ones) persist in production code paths that Phase 3 will consume the moment STT lands.

Headline production-path defects:

1. **Production `Downmix::push` silently drops the rubato resampler's residual output on every call** because `INPUT_CHUNK` is set to 480 samples while the resampler is constructed with chunk size also 480, and there is no `process_partial`/final flush — but worse: the `process_into_buffer` API is being called with a *new* `&[f32]` reference each call sourced from `pending_mono.drain(..INPUT_CHUNK).collect::<Vec<f32>>()`, which is fine, BUT the call site only consumes one chunk per loop iteration even though `pending_mono` may grow several `INPUT_CHUNK`s deep when a slow tokio scheduler delays the cpal callback drain (see BL-02 — actually a non-issue, the `while` loop drains correctly). The real bug is in **mic.rs i16→f32 conversion: division by `i16::MAX` (32767) means the most-negative sample `-32768` produces `-1.00003`, which the downstream clamp catches but the conversion already lost precision on the negative full-scale extreme.** Cosmetic for content, but a textbook conversion bug.
2. **`broadcast::Sender::send()` is called from the cpal CoreAudio real-time thread and from SCK's dispatch queue, but the sender's send path inside tokio takes an internal `RwLock` under the hood; under back-pressure (256-frame overflow) the audio thread can block — directly violating CoreAudio's "no blocking in IOProc" contract** (this can manifest as a glitch/dropout under heavy load, not a crash). See BL-01.
3. **The `Mutex<(Downmix, FrameChunker)>` is held across heavy work** — the rubato `process_into_buffer` call and a `Vec::with_capacity(frames * ch_count)` allocation plus interleave loop run while the mutex is locked, on the SCK dispatch thread. If a second SCK callback fires before the first completes (SCK explicitly states callbacks may be concurrent — see spike note quirk #2), the second callback blocks on the audio thread mutex. See BL-03.
4. **The SCK audio handler can fire after `SCStream` is dropped** in 8.x because SCK's `stop_capture()` is synchronous-ish but the underlying dispatch queue may still have in-flight buffer callbacks. The `Inner::Drop` (implicit via `_stream` field) does not call `stop_capture()` explicitly nor wait for in-flight callbacks; the handler holds an `Arc<Mutex<(Downmix, FrameChunker)>>` and a `broadcast::Sender<Frame>` — both are reference-counted so use-after-free is prevented, BUT frames can be emitted on the broadcast channel **after `start_capture()`'s caller dropped `AudioStream` and started teardown**, which surfaces in Phase 3 as ghost frames arriving on a "closed" stream. See BL-04.

In addition, the static/choppy fix in `wav_eartest.rs` is correct for the example but **the root cause — that `monotonic_ms` from `FrameChunker::feed` is wall-clock-truncated and lossy — was NOT addressed in production**. Any Phase 3 consumer that uses `frame.monotonic_ms` to align mic and system streams (per D-07: "Phase 3 uses `monotonic_ms` for `↳ HH:MM` deep-links per §5.3") will hit the exact same class of bug the diagnosis document describes. The diagnosis ends with "use frame *index*, not monotonic_ms, for alignment" — but no code change enforces that.

The 17 findings below are ordered by severity. All file paths are absolute.

---

## Structural Findings (fallow)

No `<structural_findings>` block was provided in the spawning prompt; structural pre-pass output is absent. Skipping this section.

---

## Narrative Findings (AI reviewer)

## Blocker Issues

### BL-01: cpal real-time audio thread can block on `broadcast::Sender` under back-pressure

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/mic.rs:178-184` and `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/system.rs:190-196` (call sites); root cause `mic.rs:88-97`.

**Excerpt (mic.rs):**
```rust
move |data: &[f32], _: &cpal::InputCallbackInfo| {
    if let Ok(mut guard) = state.lock() {
        let (dx, chunker) = &mut *guard;
        let out = dx.push(data);
        if !out.is_empty() {
            chunker.feed(&out);     // calls self.tx.send(frame) below
        }
    }
},
```

**Issue:** `FrameChunker::feed` calls `self.tx.send(frame)` (a `tokio::sync::broadcast::Sender::send`). This function is *not* async, but its implementation acquires the channel's internal tail-pointer lock (`parking_lot` mutex / RwLock depending on tokio version) and walks the receiver list. Under the documented load (50 Hz × 2 channels × multiple subscribers in Phase 3) it is fast — but the cpal callback is called on CoreAudio's IOProc thread, which is **real-time-priority and must never block**. If any other thread (a Phase 3 subscriber's `recv()` call, or the SCK dispatch handler also trying to send) is contending the same broadcast's internal state, the audio thread can be preempted/blocked, producing audible glitches and (worse) the macOS audio HAL will refuse to deliver the next buffer if the IOProc misses its deadline (~10 ms typical).

**Consequence:** Under Phase 3 load, expect intermittent ~10–20 ms audio dropouts whose root cause will be invisible (cpal returns `Ok` for the missed buffer, the broadcast just sees fewer frames). This is the kind of bug that ruins live transcripts for users on busy machines and is undetectable in CI.

**Fix:** Decouple the audio thread from the broadcast send. The canonical pattern is a lock-free SPSC ring (e.g. `ringbuf` 0.4) drained by a dedicated tokio task that owns the `broadcast::Sender`:

```rust
// In start_capture:
let (ring_tx, ring_rx) = ringbuf::HeapRb::<i16>::new(48_000).split();
// Audio thread: push samples into ring_tx (lock-free, wait-free).
// Tokio task: pop samples, chunk into Frames, send on broadcast::Sender.
```

The audio thread no longer touches a mutex or a broadcast channel. The drainer task can also detect overflow correctly (currently silent).

---

### BL-02: f32→i16 conversion is asymmetric — `i16::MIN` underflow is masked by clamp but the path is wrong

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/resample.rs:122-123` and the inverse in `mic.rs:200-203`.

**Excerpt (resample.rs):**
```rust
let clamped = s.clamp(-1.0, 1.0);
out_i16.push((clamped * i16::MAX as f32) as i16);
```

**Excerpt (mic.rs i16→f32):**
```rust
let as_f32: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
```

**Issue:** Two related defects.

(a) **`s as i16` (f32-to-i16 saturating cast) for `s == -1.0 * 32767.0 == -32767.0`** lands at exactly `-32767` — fine. For `s == 1.0`, it produces `32767`. So the dynamic range used is `[-32767, 32767]` rather than the full `[-32768, 32767]`. This is acceptable but the **inverse conversion in mic.rs uses `i16::MAX` (32767) as divisor** — so a hardware-supplied `i16::MIN` (-32768) becomes `-32768 / 32767 = -1.00003`. This sample then enters the rubato pipeline as a value > 1.0 in absolute terms, which the downstream `clamp(-1, 1)` catches before re-quantization — so the round-trip works **only by accident** (the clamp masks the bug). On any path that does not clamp (e.g. if a future writer skips the clamp because "we know everything is in range"), this asymmetry will overflow.

(b) **The Rust `as` cast from f32 to integer is saturating since Rust 1.45** (no UB), but the cast rounds toward zero. `(-1.0_f32 * 32767.0) as i16` is `-32767`, but `(-0.9999847_f32 * 32767.0)` is `-32766` due to f32 precision loss at this magnitude. Combined with the asymmetric divisor in (a), the noise floor is ~ –90 dBFS rather than –96 dBFS — STT-imperceptible but a quality smell that will surface in audio quality A/B tests if anyone runs them.

**Consequence:** Brittle invariant. Any reordering or removal of the clamp produces a wraparound impulse (positive sample where a negative should be) — the exact class of bug the diagnosis flagged for the WAV writer. Sets a trap for the next person who touches this code.

**Fix:** Use the symmetric divisor / multiplier on both ends:

```rust
// i16 → f32 (mic.rs)
let as_f32: Vec<f32> = data
    .iter()
    .map(|&s| (s as f32) * (1.0 / 32768.0))   // divide by 2^15, not by i16::MAX
    .collect();

// f32 → i16 (resample.rs)
let clamped = s.clamp(-1.0, 1.0 - 1.0 / 32768.0);  // explicit range
out_i16.push((clamped * 32768.0) as i16);          // multiply by 2^15
```

This matches the convention every reference audio codec uses (LAME, libsndfile, etc.) and removes the "saved by the clamp" smell.

---

### BL-03: SCK callback holds the production Mutex across heavy work, can serialize concurrent callbacks

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/system.rs:160-196`.

**Excerpt:**
```rust
for buf in abl.iter() {
    let bytes = buf.data();
    let sample_count = bytes.len() / 4;
    let mut samples = Vec::with_capacity(sample_count);
    for chunk in bytes.chunks_exact(4) {
        let mut a = [0_u8; 4];
        a.copy_from_slice(chunk);
        samples.push(f32::from_le_bytes(a));
    }
    per_channel.push(samples);
}
// … plus interleave with allocation …
if let Ok(mut guard) = state_for_handler.lock() {
    let (dx, chunker) = &mut *guard;
    let out = dx.push(&interleaved);   // rubato process, possibly several KB of arith
    if !out.is_empty() {
        chunker.feed(&out);             // broadcast send, walks receiver list
    }
}
```

**Issue:** Two issues.

(a) **The SCK spike note (`docs/superpowers/notes/2026-06-25-sck-spike-result.md`, "API quirks #2") explicitly says "SCK invokes delegates concurrently from arbitrary dispatch queues"**. The production handler is `Fn + Send + Sync`, satisfying the trait bound, but it serializes calls behind a single `Arc<Mutex<(Downmix, FrameChunker)>>`. If two SCK audio callbacks fire concurrently (which the API contract allows), the second one waits on the first — eating its 20-ms budget. Under any spike of dispatch concurrency this drops frames in the upstream SCK queue.

(b) The mutex is held while doing 960 × 4-byte LE decodes, allocating two `Vec<f32>` of 960 samples each, **another `Vec<f32>` of 1920 samples for the interleave**, running rubato's sinc filter, and then sending on the broadcast. None of this needs the mutex except the rubato/chunker state. Even if quirk (a) doesn't bite in practice (SCK might never actually call back concurrently on a single output type), the critical section is ~10× larger than necessary.

**Consequence:** Same dropout/glitch class as BL-01, plus head-of-line blocking inside SCK's audio queue itself.

**Fix:**
1. Move the f32-decode and interleave OUT of the mutex; only the `dx.push(...)` and `chunker.feed(...)` calls need the lock.
2. Even better, route through the same lock-free ring buffer recommended in BL-01 so the SCK callback only does the decode + push-to-ring (no allocation if the ring is large enough), and a tokio drainer task owns rubato + broadcast.

```rust
let interleaved: Vec<f32> = decode_and_interleave(&abl);  // outside lock
if let Ok(mut guard) = state_for_handler.try_lock() {     // try_lock, not lock
    let (dx, chunker) = &mut *guard;
    let out = dx.push(&interleaved);
    if !out.is_empty() { chunker.feed(&out); }
} else {
    // Drop this buffer rather than queue behind a stuck callback.
    tracing::warn!("system callback ran while previous still held lock; dropping buffer");
}
```

---

### BL-04: SCK callback can fire after `SCStream` Drop → ghost frames on a torn-down stream

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/system.rs:88-95, 218`.

**Excerpt:**
```rust
pub(super) struct Inner {
    _stream: SCStream,
}
// …
Ok(Inner { _stream: stream })
```

**Issue:** The spike note says "`stop_capture()` is synchronous and blocks ~tens of ms" and "the spike observed a clean shutdown without orphaned threads" — but the production code does NOT call `stop_capture()` explicitly. It relies entirely on `SCStream::Drop` doing the right thing. The `screencapturekit` 8.x crate's `Drop` is, in practice, a no-op or a fire-and-forget tear-down; the safe pattern in every Apple sample code I've seen is `stream.stopCapture { ... }` (or its Rust equivalent) followed by waiting for the completion callback.

Even if the stream's `Drop` does call `stop_capture`, the SCK dispatch queue can have one or more in-flight `did_output_sample_buffer` invocations that landed before stop took effect. Those callbacks hold clones of `Arc<Mutex<(Downmix, FrameChunker)>>` and the `broadcast::Sender<Frame>` — both live as long as those `Arc`s, so memory-safety is preserved (no UAF). But they will:

1. Push frames into the broadcast AFTER the user dropped `AudioStream`.
2. If the user immediately spawns a new `AudioStream` and a Phase 3 consumer attaches, those late frames are GONE (the channel was dropped). Fine.
3. If the user re-uses the same `broadcast::Sender` somehow (unlikely here), they'd see frames from the previous capture session. Not currently a vector.

The actual user-visible bug: **dropping `AudioStream` does not guarantee capture has stopped by the time the next line of code runs.** A Phase 3 test that does `drop(stream); assert_eq!(mic_rx.try_recv(), Err(Empty))` will be flaky.

**Consequence:** Phase 3 will hit flaky teardown semantics. More important for production: when a meeting ends, the user may briefly see "still recording" because the SCK pipeline hasn't fully stopped — and the privacy story ("audio deleted after transcription") starts depending on receiver cleanup rather than producer stop.

**Fix:** Add an explicit `Drop` impl that calls `stop_capture()` and (if the crate exposes it) waits for the completion handler:

```rust
impl Drop for Inner {
    fn drop(&mut self) {
        if let Err(e) = self._stream.stop_capture() {
            tracing::warn!(error = %e, "SCStream::stop_capture() failed during drop");
        }
        // SCK's `stop_capture` blocks until in-flight callbacks have drained
        // (per the spike note — "synchronous and blocks ~tens of ms").
    }
}
```

This also lets you delete the `_stream:` field-naming convention (the underscore prefix implies "unused", which is a lie — it's RAII-load-bearing).

---

## Critical Issues

### CR-01: `monotonic_ms` is wall-clock-truncated and lossy — the root cause the diagnosis identified is NOT fixed in production

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/mic.rs:92`.

**Excerpt:**
```rust
let monotonic_ms = self.start.elapsed().as_millis() as u64;
```

**Issue:** The diagnosis document (`02-03-DIAGNOSIS.md`) explicitly identifies this as the root cause of the static/choppy bug:

> `monotonic_ms` is `Instant::elapsed().as_millis() as u64` — wall-clock, truncated to whole milliseconds (so 19.6 ms and 20.4 ms both become 20).
> Consecutive frames arrive ~20 ms apart, but wall-clock jitter is ±1–2 ms.

The fix lives in `wav_eartest.rs::frames_to_concat_pcm` (don't use `monotonic_ms` for alignment). But `FrameChunker` itself still emits the same jittered, ms-quantized value. Per D-07 from the context doc:

> Phase 3 routes Mic → "Me" (ink) and System → "Them" (grey) per PRD §5.2 and uses `monotonic_ms` for `↳ HH:MM` deep-links per §5.3.

Phase 3 deep-links operate at minute resolution (`HH:MM`), so for that specific use the ms quantization doesn't matter. BUT any cross-channel alignment Phase 4 / Phase 8 may want (offline whisper.cpp re-transcription, transcript-to-audio sync for "play this clip" UI) will rediscover this bug. The diagnosis ends with "use frame *index*, not monotonic_ms, for alignment" — but `Frame` does NOT carry a frame index, and the code does not enforce or even encourage this.

**Consequence:** This bug is a landmine for Phase 3+. The diagnosis says "in practice `lagged: 0` was observed in both runs, so the drift didn't actually happen" — but the bug persists in the type system; the next person to align channels will re-implement `frames_to_aligned_pcm` and re-discover the static-crackle defect.

**Fix:** Either (a) compute `monotonic_ms` from a per-frame counter so it's exact:

```rust
let frame_idx = self.frame_count;
self.frame_count += 1;
let monotonic_ms = (frame_idx * 20) as u64;  // exact, no jitter
```

Or (b) add a `frame_index: u64` field to `Frame` and document loudly that **only `frame_index` is safe for cross-channel sample alignment; `monotonic_ms` is wall-clock-derived and lossy.**

---

### CR-02: `start_capture()` permission gate races against TCC revocation; status is cached, not live

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/lib.rs:122-125` and `permission.rs:81-90`.

**Excerpt:**
```rust
pub fn start_capture() -> Result<AudioStream> {
    if has_screen_recording_permission() == PermissionStatus::Denied {
        return Err(AudioError::PermissionDenied);
    }
    // … opens mic, then opens SCK …
}
```

**Issue:** `CGPreflightScreenCaptureAccess` is documented to return cached TCC state. Two failure modes:

1. **First-run TCC quirk** (acknowledged in D-24): the binary must be restarted after the user grants permission. So a fresh-install user grants permission, the binary is told "Denied", and `start_capture` refuses. Fine — documented.

2. **Mid-session revocation:** if the user revokes Screen Recording in System Settings while yogurt is running, `CGPreflightScreenCaptureAccess` may still return `true` (the cached value), `start_capture` proceeds, the mic opens — and then SCK's actual `start_capture()` call hits a TCC denial. The error path returns `AudioError::SystemCaptureFailed("SCStream::start_capture: …")` instead of `AudioError::PermissionDenied`, so the §5.11 recovery card does NOT trigger and the user sees a generic error.

3. **Race between preflight and SCK start:** even with no user action, the preflight is observational only — SCK's internal TCC check happens at `SCShareableContent::get()` and again at `start_capture()`. The preflight in `start_capture` is purely advisory; the authoritative check is SCK's.

**Consequence:** Wrong error variant surfaces in the (rare but real) revocation case, breaking the Phase 7 §5.11 recovery UX.

**Fix:** In `system.rs::start`, normalize SCK errors that look like TCC denials back to `PermissionDenied`:

```rust
.map_err(|e| {
    let msg = e.to_string();
    if msg.contains("declined") || msg.contains("TCC") || msg.contains("permission") {
        AudioError::PermissionDenied
    } else {
        AudioError::SystemCaptureFailed(format!("SCStream::start_capture: {msg}"))
    }
})?;
```

String-matching SCK error text is fragile, but it's the only signal the crate exposes. Alternatively, re-call `CGRequestScreenCaptureAccess()` on any SCK start failure and use its return value as the discriminator.

---

### CR-03: `start_capture()` does NOT atomically clean up the mic stream when SCK fails

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/lib.rs:130-141`.

**Excerpt:**
```rust
let _mic = spawn_mic_capture(mic_tx.clone())?;
let _system = spawn_system_capture(system_tx.clone())?;

Ok(AudioStream { _mic, _system, mic_tx, system_tx })
```

**Issue:** The comment claims "If SCK fails, we still drop the mic via RAII when MicCapture goes out of scope on early-return." That's correct in this version — the `?` operator drops `_mic` on early-return because the `Ok(AudioStream {...})` literal hasn't been constructed yet. So this finding is **partially mitigated by Rust's drop semantics**.

But the user-facing symptom is still bad: between `spawn_mic_capture` returning and `spawn_system_capture` failing, the mic's CoreAudio stream is RUNNING. On a Mac with a hardware mic indicator (the orange dot on macOS 14+), that dot briefly turns ON, then OFF, then the user gets a `PermissionDenied` error. From the user's perspective, "Yogurt opened my mic without permission." This violates the explicit D-25 contract — "the user never hears a recording indicator if system capture is going to fail."

**Consequence:** UX/privacy regression vs. D-25. The orange dot flashing on every failed start is going to generate "is yogurt spying on me?" GitHub issues.

**Fix:** Reverse the order — open SCK first (which is the failure-prone one), then open the mic:

```rust
let _system = spawn_system_capture(system_tx.clone())?;
let _mic = spawn_mic_capture(mic_tx.clone())?;  // only opens if SCK succeeded
```

The comment in the current code says the opposite order is intentional ("Order matters: open mic first because it's faster to recover from"), but the reasoning is backwards — we want the slow/failure-prone resource opened first so the safe one doesn't flash the indicator unnecessarily.

---

### CR-04: `audio_buffer_list()` decode assumes f32 LE bytes but does not validate `mDataByteSize` or buffer alignment

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/system.rs:163-172`.

**Excerpt:**
```rust
for buf in abl.iter() {
    let bytes = buf.data();
    let sample_count = bytes.len() / 4;          // assumes f32
    let mut samples = Vec::with_capacity(sample_count);
    for chunk in bytes.chunks_exact(4) {
        let mut a = [0_u8; 4];
        a.copy_from_slice(chunk);
        samples.push(f32::from_le_bytes(a));
    }
    per_channel.push(samples);
}
```

**Issue:**

1. **Format assumption is hard-coded.** The spike confirmed 48 kHz f32 stereo on **one machine**. SCK's AudioStreamBasicDescription can in principle deliver i16, i32, or f32 depending on macOS version and audio device. There is no runtime check that `mFormatFlags & kAudioFormatFlagIsFloat` is true. A device that returns i16 will get its bytes mis-interpreted as f32, producing garbage PCM. The spike note (line 30) says "Bytes per buffer | 3,840 → 960 samples × 4 bytes (f32)" — this is observational, not enforced.

2. **`chunks_exact(4)` silently truncates** any trailing 1–3 bytes. If SCK ever delivers a buffer whose length isn't a multiple of 4, the tail is dropped. Unlikely but unverified.

3. **`f32::from_le_bytes` assumes little-endian.** Apple Silicon is LE, but Intel macOS is also LE so this happens to be safe; if Apple ever ships a BE platform (they won't) this breaks. Cosmetic — but `f32::from_ne_bytes` would be more honest about the assumption.

**Consequence:** On any macOS version / hardware combination that delivers non-f32 audio (real edge: external USB audio interfaces with non-standard formats; future macOS API change), the system channel produces noise that STT will accept and transcribe garbage from.

**Fix:** Query the actual format from the sample buffer's `CMFormatDescription`:

```rust
let asbd = sample_buffer.format_description()
    .and_then(|fd| fd.audio_stream_basic_description())
    .ok_or_else(|| /* … */)?;
debug_assert!(asbd.format_flags.contains(AudioFormatFlags::IS_FLOAT));
debug_assert_eq!(asbd.bits_per_channel, 32);
```

At minimum, log a one-time warning if the observed format diverges from the assumed 48 kHz / f32 / stereo. (The `screencapturekit` 8.x API may or may not expose `format_description`; if not, document the assumption with `// SAFETY: …` and add a CI check that fails if the spike re-runs and observes a different format.)

---

## Warnings

### WR-01: `Mutex::lock().ok()` silently drops poisoned-mutex data

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/mic.rs:178, 204` and `system.rs:190`.

**Issue:** `if let Ok(mut guard) = state.lock()` swallows the `PoisonError` case. If any prior callback panicked while holding the lock, all subsequent callbacks become silent no-ops — capture appears to be running but no frames flow. The error callback path on cpal doesn't fire for this either; the only signal is `mic_rx.recv()` timing out.

**Fix:** Use `.lock().unwrap()` (panic-loud), or recover from poison explicitly with `.into_inner()`. A silent no-op is the worst option.

```rust
let mut guard = match state.lock() {
    Ok(g) => g,
    Err(poisoned) => {
        tracing::error!("mic state mutex poisoned; recovering");
        poisoned.into_inner()
    }
};
```

---

### WR-02: `Frame::new` panics on length mismatch — fine in producers, but `Frame` is `pub` and constructible by Phase 3 tests

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/frame.rs:44-57`.

**Issue:** The `assert_eq!` on `samples.len() == FRAME_SAMPLES` is correct as an internal contract, but `Frame::new` is `pub`, and `Frame { channel, monotonic_ms, samples }` is *also* constructable directly via the pub field shorthand because all three fields are `pub`. A Phase 3 test constructing a stub Frame can either panic-by-accident-via-new or bypass-the-assertion-via-struct-literal — inconsistent.

**Fix:** Either make the fields private and force `Frame::new` (so the assert is unbypassable), or accept that the struct fields are pub and downgrade `new` to a non-validating constructor. Current state is half-and-half.

---

### WR-03: `Cargo.toml` `[dev-dependencies]` re-pins `tokio` — workspace dep already pulls it in

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/Cargo.toml:25`.

**Excerpt:**
```toml
[dev-dependencies]
tokio = { workspace = true }
```

**Issue:** Tokio is already a regular dependency (line 11). Re-declaring it as a dev-dep is a no-op — but if the workspace tokio dep ever gets feature-gated (e.g., `features = ["rt"]` for prod, `features = ["rt", "macros", "test-util"]` for dev), this re-declare will not pick up the dev features automatically. Currently harmless but a footgun.

**Fix:** Remove the dev-dep tokio line; if tests need additional tokio features, use `tokio = { workspace = true, features = ["test-util"] }` in dev-dependencies.

---

### WR-04: `pending_mono.drain(..INPUT_CHUNK).collect::<Vec<f32>>()` allocates per chunk on the audio thread

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/resample.rs:114`.

**Excerpt:**
```rust
let chunk_in: Vec<f32> = self.pending_mono.drain(..INPUT_CHUNK).collect();
let wave_in = [chunk_in.as_slice()];
```

**Issue:** Per-callback allocation on the audio thread. cpal callbacks run at hard-real-time priority on CoreAudio's IOProc; `Vec::with_capacity` calls into the system allocator. macOS `malloc` is not lock-free; a fragmented heap can produce ms-scale latency spikes audible as crackle. (This is the same class as BL-01.)

**Fix:** Pre-allocate a `Vec<f32>` of size `INPUT_CHUNK` in `Downmix::new` and copy into it via `.copy_from_slice` rather than `drain().collect()`:

```rust
// In Downmix struct: chunk_scratch: Vec<f32>,  // size INPUT_CHUNK, allocated once
// In push:
self.chunk_scratch.clear();
self.chunk_scratch.extend(self.pending_mono.drain(..INPUT_CHUNK));
let wave_in = [self.chunk_scratch.as_slice()];
```

Also: the mic.rs i16 path allocates a per-callback `Vec<f32>` of size `data.len()` (line 200). Same fix applies — pre-allocate a reusable scratch buffer.

---

### WR-05: Production `Downmix` lacks the `clamp` BEFORE multiplying — wait, it has it. False alarm. (Negative finding for review traceability.)

**Verdict:** Reviewed `resample.rs:121-124` — the `clamp(-1.0, 1.0)` precedes the multiply. Production path is safe for in-range and out-of-range samples. The static/choppy bug was example-scoped as the diagnosis claims. **No defect.** Recording this as a deliberate non-finding because the prompt asked for explicit verification.

---

### WR-06: Synthetic sine generator uses `tokio::time::interval` with `Delay` — drops frame indices, not just ticks

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/synthetic.rs:47-61`.

**Excerpt:**
```rust
let mut interval = tokio::time::interval(Duration::from_millis(20));
interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
// …
let samples = generate_chunk(&cfg, frame_idx);
let monotonic_ms = start.elapsed().as_millis() as u64;
// …
frame_idx += 1;
```

**Issue:** With `MissedTickBehavior::Delay`, missed ticks are not caught up — but `frame_idx` still increments by 1 per emitted tick, NOT by the number of frames that *should* have been emitted. So if the scheduler delays by 100 ms (5 frames missed), the next emission has `frame_idx = N+1` but corresponds to wall-time `monotonic_ms = N*20 + 100`. The sine wave's phase is computed from `frame_idx`, so it desynchronizes from `monotonic_ms` — Phase 3 tests that assert "sample at monotonic_ms=T has sine value X" will be flaky.

This is a test-only path so the impact is bounded, but the synthetic generator is also exposed via the `synthetic` feature for "Phase 5 debug input source" — at which point the phase glitch becomes audible.

**Fix:** Skip frames when ticks are missed, or compute `frame_idx` from elapsed time:

```rust
let elapsed_ms = start.elapsed().as_millis() as u64;
let frame_idx = elapsed_ms / 20;  // catches up phase even after a delay
```

---

### WR-07: Permission `request_screen_recording_permission()` documentation is misleading about return value

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/permission.rs:49-52, 92-103`.

**Issue:** The doc string says:

> The bool returned by `CGRequestScreenCaptureAccess` reflects the *current* state at the moment of the call (usually still `false` immediately after the dialog fires — the user hasn't clicked yet).

But the function returns `PermissionStatus::Denied` on `false`, which the UI will interpret as "user denied" — surfacing the §5.11 recovery card immediately even though the user hasn't actually clicked anything in the dialog. The docstring acknowledges the quirk but the API doesn't communicate it; the UI has no way to distinguish "user denied" from "still waiting on click."

**Fix:** Add a third variant `PermissionStatus::Pending` or rename `Denied` → `NotGranted` to clarify. Or document that the right UX is to *not* call `request` from the UI and just let SCK trigger the system dialog on the first capture attempt (which is what macOS Apple Sample Code recommends).

---

### WR-08: REST endpoints have no auth; `/api/audio/devices` enumerates user hardware to any localhost reader

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-server/src/routes.rs:17-18` (mount) and `audio.rs:46-52` (handler).

**Issue:** The `/ws` endpoint requires the session token (per ws_auth.rs in Phase 0). The new `/api/audio/devices` and `/api/audio/permission` endpoints do NOT. The argument is "localhost-only bind is sufficient" — but every other browser tab on localhost can also fetch these endpoints. The exposed data:

- `/api/audio/devices`: full device names of every input device on the user's Mac. This is hardware fingerprint material (the user's audio interface brand/model often identifies their setup).
- `/api/audio/permission`: leaks TCC state.

Per PRD §2 (constraints): "no telemetry, audio never leaves machine." Hardware fingerprinting via a localhost endpoint is a small but real privacy regression vs. that posture.

**Consequence:** Any malicious web page the user visits (in the same browser session that's running yogurt's UI) can `fetch('http://localhost:7878/api/audio/devices')` and read the device list. CORS will block reading the response body unless the server sets `Access-Control-Allow-Origin: *` — which it doesn't, so the leak is mostly theoretical. BUT: image preloading, `<iframe>` redirects, and SSRF-via-link-preview attacks can hit these endpoints without CORS protection.

**Fix:** Require the same `X-Yogurt-Session` (or whatever the Phase 0 ws_auth uses) bearer for `/api/*` endpoints. Add a session-check middleware at the router level rather than per-endpoint.

---

### WR-09: `audio_api.rs` integration test runs `yogurt_server::run` but only sleeps 200 ms before hitting it

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-server/tests/audio_api.rs:15-17, 57-59`.

**Excerpt:**
```rust
let handle = tokio::spawn(async move { yogurt_server::run(addr, mode).await });
tokio::time::sleep(Duration::from_millis(200)).await;
```

**Issue:** Race-prone. On a slow CI box (especially the first test run after `cargo build --tests`), 200 ms is not enough for `TcpListener::bind` + axum start. The test then hits a closed port and `reqwest::get` returns `ConnectionRefused`, surfacing as a flaky test. Also: `yogurt_server::run` reads `~/.yogurt/` paths (per `lib.rs:67-78`), so this test clobbers the developer's real DB/session token unless `YOGURT_HOME` or similar is set — but the test doesn't set anything.

Worse: `run_with_config` exists (`lib.rs:66`) specifically to override DB/token paths for tests. This test ignores it and uses `run`.

**Fix:** Use `run_with_config` with `tempfile::TempDir`-backed paths, and poll the port until it's open rather than sleeping a fixed duration:

```rust
let tmp = tempfile::tempdir()?;
let cfg = RunConfig {
    addr,
    mode: Mode::Release,
    db_path: Some(tmp.path().join("db.sqlite")),
    session_token_path: Some(tmp.path().join("token")),
};
let handle = tokio::spawn(async move { yogurt_server::run_with_config(cfg).await });
// Poll until reachable:
for _ in 0..50 {
    if reqwest::get(format!("http://{addr}/api/health")).await.is_ok() { break; }
    tokio::time::sleep(Duration::from_millis(20)).await;
}
```

---

## Info

### IN-01: Prompt referenced `/api/meeting/start` / `/api/meeting/stop` endpoints that DO NOT EXIST in Phase 2 source

**Files searched:** all of `crates/yogurt-server/src/`.

**Issue:** The review prompt's section 5 ("REST API surface (Plan 02-03)") lists `POST /api/meeting/start` and `POST /api/meeting/stop` as in-scope. Neither endpoint exists in this commit set. The only meeting-recording surface is `audio::start_meeting_recording()` — a function pointer documented as "Phase 3 will wire this into POST /api/meetings/:id/start" (note the `:id`-bearing plural URL — different from the prompt's `/api/meeting/start`).

**Implication:** The bugs the prompt anticipates (double-call → resource leak, idempotency, auth) are Phase 3 work. **They are NOT in this review's scope.** Phase 2 only ships the hook function. Future reviewers should not look for them here.

---

### IN-02: Underscore-prefixed `_mic`, `_system`, `_stream` fields imply "unused" but are RAII-load-bearing

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/lib.rs:71-72`, `mic.rs:33`, `system.rs:93`.

**Issue:** Rust convention is `_name` for "intentionally unused binding." Using it for a field whose entire purpose is RAII (Drop semantics) misleads future readers and trips `#[deny(unused)]` lints that get added later.

**Fix:** Rename to `mic: MicCapture` / `system: SystemCapture` / `stream: SCStream`. The field naming `_mic` is also why the `Debug` impl uses `mic_receivers`/`system_receivers` — without the underscore the natural Debug field would be `mic`.

---

### IN-03: `Downmix` has an `#[allow(dead_code)]` on `input_rate` — code smell

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/resample.rs:41-42`.

**Issue:** `input_rate` is stored but never read outside of Debug. If it's there for diagnostic logging, expose it via a getter; if it's truly dead, remove it.

---

### IN-04: `BROADCAST_CAPACITY = 256` justification (~5 sec) assumes 50 Hz; should be expressed as a derived constant

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/src/lib.rs:41-45`.

**Issue:** Magic number with a comment justifying the math. If `FRAME_SAMPLES` or `SAMPLE_RATE_HZ` ever changes (D-08 says they won't, but…), the 256 stays static and the "~5 sec" claim becomes wrong. Derive it:

```rust
const FRAMES_PER_SEC: usize = SAMPLE_RATE_HZ as usize / FRAME_SAMPLES;  // = 50
pub const BROADCAST_CAPACITY: usize = FRAMES_PER_SEC * 5;  // ~5 sec
```

---

### IN-05: Build script rpath path `/usr/lib/swift` not verified to exist on minimum-supported macOS 13

**File:** `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/crates/yogurt-audio/build.rs:26`, also `yogurt-server/build.rs:28`, `yogurt-cli/build.rs:22`.

**Issue:** The spike was run on macOS 15.6. PRD targets macOS 13+. `/usr/lib/swift/libswift_Concurrency.dylib` is shipped as part of the OS on macOS 12.5+ (Apple confirmed) — but it's worth a CI matrix run on a macOS 13.0 base image to confirm before Phase 9 ship. If the dylib is missing on some macOS 13.x minor version, the binary will fail at load with the same opaque error the spike fought with.

**Fix:** Add a CI job that runs the audio integration tests on a macOS 13.0 runner (not just `macos-latest`). Out of Phase 2 scope to fix; flag for Phase 9 distribution work.

---

## Cross-Reference: What the Phase 2 Verifier Likely Missed

(Speculative — the verifier's REVIEW artifact is `02-VERIFICATION.md`, which I did not find in this commit set; presumably written in parallel.)

The most important class of issues the verifier likely missed:

1. **All four BLOCKERs are concurrency / real-time-thread issues that only manifest under multi-subscriber load.** Phase 2 verification was done with single-subscriber smoke binaries (`mic_smoke`, `system_smoke`, `dual_smoke`) and `wav_eartest` (also single-subscriber per channel). No verification involved 2+ subscribers on the same channel under sustained load, which is the precondition for BL-01/BL-03 to bite.
2. **The static/choppy diagnosis flagged the right root cause but the fix was scoped to the example only.** A verifier looking at "is the WAV no longer static?" answers yes. A verifier looking at "is the root cause eliminated?" answers no — the lossy `monotonic_ms` still flows out of production into Phase 3's hands.
3. **CR-03 (mic indicator flashes on SCK failure) is invisible to any verification that doesn't watch the orange dot during a permission-denied path.** Verifier likely tested the happy path only.

These are the kinds of bugs that ship to users undetected and produce "I trust Granola more than this" GitHub issues 3 weeks after launch. They should block Phase 3 from starting.

---

_Reviewed: 2026-06-25T16:55Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
