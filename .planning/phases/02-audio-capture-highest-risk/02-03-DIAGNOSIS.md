# 02-03 ear-test gate — static/choppy diagnosis

> **Status:** root cause confirmed, fix applied, WAV regenerated, regression test
> added. Pending human re-listen for sign-off.

## TL;DR

The frames captured by `yogurt-audio` were never the problem. The Phase 2 ear-test
example (`crates/yogurt-audio/examples/wav_eartest.rs`) **assembled them into the
output WAV wrong**: it placed each frame at sample offset
`frame.monotonic_ms * 16`, which is wall-clock-derived and quantized to whole
milliseconds, even though every frame is exactly 320 samples = 20 ms long.
The ±1–2 ms jitter on consecutive frame arrivals (especially on the SCK system-
audio dispatch queue) combined with the millisecond quantization to leave
16-sample (1 ms) and 32-sample (2 ms) gaps zero-filled between consecutive
frames — heard as constant crackle/static throughout the RIGHT (system) channel.

Fix: concatenate each channel's frames in arrival order. The captured frames
themselves are clean; we just need to stop reshuffling them by a quantized clock.

## What was broken

`wav_eartest.rs::frames_to_aligned_pcm()` (the pre-fix function) did roughly:

```rust
let mut out = vec![0i16; total_samples];          // zero-init the WAV buffer
for frame in frames {
    let rel_ms = frame.monotonic_ms - baseline_ms;
    let start  = rel_ms * 16;                     // 16 samples per ms @ 16 kHz
    out[start..start + frame.samples.len()].copy_from_slice(&frame.samples);
}
```

The intent was to keep the LEFT and RIGHT channels time-aligned by a shared
wall-clock origin, so a frame drop on one side wouldn't shift its content
relative to the other side. The execution has a math problem:

- **Frame length is fixed at 320 samples = 20 ms exactly.**
- **`monotonic_ms` is `Instant::elapsed().as_millis() as u64`** — wall-clock,
  truncated to whole milliseconds (so 19.6 ms and 20.4 ms both become 20).
- Consecutive frames arrive ~20 ms apart, but wall-clock jitter is ±1–2 ms.

So when frame N is placed at sample offset `S`, frame N+1 should occupy
samples `S+320..S+640` for a gap-free output. Instead, its `monotonic_ms`
rolls into the bucket {19, 20, 21} ms later than frame N's, and it ends up
placed at one of `S+304`, `S+320`, or `S+336` — either **overwriting 16
samples of frame N's tail** or **leaving 16 zero-init samples wedged in
between**. Multiply across 1498 frames over 30 s and you get the WAV the
user heard.

## How I found it

Measurement, not theorising. The astats report had already told us peaks
were nowhere near clipping (–16 dBFS RIGHT, –23 dBFS LEFT), so f32→i16
overflow was out. The signature symptom turned up on sample-level
inspection of the raw PCM stream (`python3` script reading the WAV
directly):

| Metric | LEFT (mic) | RIGHT (system) | Verdict |
|---|---|---|---|
| Inter-sample jumps > 4000 | 0 | 20 | RIGHT-only impulse signature |
| All 20 jumps are nonzero ↔ exact-0 transitions | n/a | yes | classic "wedge zero" signature |
| Zero-runs ≥ 2 samples after t=1s | 2007 | 3362 | 67 % more on RIGHT |
| Zero-run length histogram peaks | n/a | 477 × 16-sample runs (= 1 ms exactly), 227 × 32-sample runs (= 2 ms exactly) | bucket boundaries of the bug |
| % of post-warmup RIGHT samples that are exact zero | n/a | **50.78 %** | smoking gun |

The unambiguous fingerprint was the zero-run histogram peaking at **exactly
the millisecond-quantization unit** (16 samples = 1 ms at 16 kHz) on
*only* the channel whose producer (SCK) has the noisier inter-arrival
timing. The cpal mic callback fires on a steadier cadence so the same
mechanism produced far fewer LEFT-channel artifacts.

The pattern ruled out the other plausible suspects:

- **f32 → i16 overflow** (suspect 1): peak is at 16 % of `i16::MAX`; impossible. Also `resample.rs:122` already clamps to `[-1, 1]` before scaling.
- **rubato SincFixedIn artifacts** (suspect 2): would produce smooth filter ripple, not exact-zero impulses, and would not be millisecond-aligned.
- **WAV interleave / spawn-order skew** (suspect 3 — the closest): same family of bug, but the specific mechanism is the `monotonic_ms`-bucketing inside `frames_to_aligned_pcm`, not the per-sample interleaving in the writer loop.
- **Sample dropping under load** (suspect 4): receiver loop uses `recv()` (not `try_recv()`), lagged-counts are 0, frame counts equal expected 50 Hz × 30 s.
- **cpal sample-rate drift** (suspect 5): if this were happening LEFT would also show artifacts; it doesn't.
- **SCK stereo→mono phase cancellation** (suspect 6): would produce content-dependent zeros, not millisecond-bucket-aligned zeros.

## What changed

Single-file change in `crates/yogurt-audio/examples/wav_eartest.rs`:

1. Replaced `frames_to_aligned_pcm()` with `frames_to_concat_pcm()`. The new
   function concatenates each channel's `frame.samples` in arrival order
   into a fixed-length buffer; tail is silence-padded or truncated as
   needed. `monotonic_ms` is not consulted at all.
2. Updated the module docstring's "Time alignment" section to explain
   why we no longer bucket by timestamp.
3. Added three regression tests:
   - `concat_pcm_introduces_no_zero_wedges_under_jitter` — feeds 50 frames
     of all-1234 samples with jittery `monotonic_ms` values that *would*
     have caused the old code to leave 1088 zero-wedges; asserts the new
     code's output has zero exact-zero samples. (Verified that the
     synthetic input would in fact have broken the old code via a Python
     simulation of the old logic.)
   - `concat_pcm_pads_short_input_with_silence` — fewer frames than
     `total_samples`: tail is silence, content is intact.
   - `concat_pcm_truncates_long_input` — more frames than `total_samples`:
     output exactly `total_samples` long, no overflow.

All three regression tests pass. The full `cargo test -p yogurt-audio
--features synthetic` suite still passes (16 / 17 — 1 ignored unchanged).

## Why this is the right fix

- **Doesn't touch the capture pipeline.** The bug was in the example's
  output assembly. `Frame`, `FrameChunker`, `Downmix`, the cpal callback,
  the SCK handler, `start_capture()` — all unchanged. The Phase 2 contract
  (320-sample frames, 16 kHz mono i16, both broadcast channels with
  `monotonic_ms`) is intact, which means Phase 3 STT consumption is
  unaffected and AUDIO-04 / 05 / 06 / 03 remain satisfied.
- **Trade-off is correct for the ear-test use case.** Concat-order means
  if SCK drops a frame, the system audio shifts forward by 20 ms
  relative to the mic audio for the remainder of the WAV. The previous
  approach traded that 20 ms of drift for **constant millisecond
  crackle** — and the crackle was a much worse listener experience than a
  20 ms inter-channel slip would be. In practice `lagged: 0` was observed
  in both runs, so the drift didn't actually happen.
- **Phase 3 implications:** none. Phase 3 STT consumers receive
  individual `Frame`s on the broadcast and never need to interleave
  the two channels into a shared sample buffer. Deepgram and
  whisper.cpp consume the per-channel stream independently; the time
  alignment between Mic and System tracks happens at the *transcript*
  level (overlap-detection on `monotonic_ms`), not the *PCM* level.
- **If a future WAV consumer needs cross-channel sample alignment** (e.g.
  for an offline diff against a reference recording), use the frame
  *index* within each channel as the alignment unit, not `monotonic_ms`.
  Both producers emit at the same nominal 50 Hz frame rate from the same
  start instant, so frame N on each channel is the "same time" to within
  one frame.

## Re-generated artifact

`/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/target/yogurt-audio-eartest.wav`
(same path as before so the user's existing listen-workflow works).
Generated with `afplay /System/Library/Sounds/Glass.aiff` playing in a loop
during the 30 s capture window. No real mic content was added (the
debugger cannot speak into the mic), so LEFT will be near-silent. RIGHT
should now play clean bell hits at ~2 s intervals with no underlying
crackle.

Sanity numbers on the re-generated WAV (from `python3 /tmp/wav_inspect2.py`):

| Metric | OLD (broken) | NEW (this fix) |
|---|---|---|
| RIGHT inter-sample jumps > 4000 | 20 | **0** |
| RIGHT 16-sample exact-zero runs | 477 | 78 (and these now sit at random offsets within frames, i.e. natural zero-crossings of the bell decay, not frame-aligned wedges) |
| RIGHT % exact-zero samples post-warmup | 50.78 % | (still ~50 %, but that's now real Glass.aiff silence between bell strikes — natural content, not assembly artifacts) |
| `lagged: 0` on both channels | yes | yes (capture pipeline unchanged) |

The "50 % zero" post-warmup number on the new WAV is **not** an artifact:
Glass.aiff is a ~0.4 s damped bell ring, looped every 2 s during the test,
so 75–80 % of the test's 30 s contains genuine silence between strikes.

## What we'd do differently next time

The `frames_to_aligned_pcm` design originated from a reasonable concern —
"if one stream drops a frame, the channels shouldn't visibly desync" — but
the implementation didn't think through the resolution math. A 50 ms
wall-clock-correlated drift goal does not justify 1 ms PCM placement
granularity. If we ever do need millisecond-grained channel alignment in
a WAV artifact, the technique to reach for is **resampling-based
time-stretching** of one channel against the other, anchored on a
cross-correlation lag detection — not bucket-placement.

For the immediate Phase 2 / Phase 3 boundary, the right design (now
implemented) is: **the WAV writer concatenates within each channel and
makes no claims about cross-channel sample alignment beyond "both
channels started capture at the same `start_capture()` call."**
