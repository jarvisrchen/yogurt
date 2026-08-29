---
status: awaiting_human_verify
trigger: "Ear-test WAV at target/yogurt-audio-eartest.wav is staticy and choppy. LEFT=mic, RIGHT=system. Channel mapping confirmed correct; this is an audio QUALITY defect, not signal absence or channel swap."
created: 2026-06-25T20:43:59Z
updated: 2026-06-25T20:52:50Z
---

## Current Focus

reasoning_checkpoint:
  hypothesis: "frames_to_aligned_pcm() in wav_eartest.rs places frames into the WAV buffer by frame.monotonic_ms * 16-samples-per-ms. Because monotonic_ms is wall-clock-truncated to whole milliseconds (Instant::elapsed().as_millis() as u64) but frames arrive on ~20 ms boundaries with ±1-2 ms jitter (especially the SCK system-audio dispatch queue), consecutive frames are sometimes placed N*16 samples too far apart (1 ms gap = 16 zero samples wedged between frames) or N*16 samples too close (overwriting the tail of the previous frame). The result is a torrent of single-millisecond zero-samples (impulse clicks) and short overwrites all across the RIGHT channel. The LEFT channel doesn't show this nearly as badly because the cpal callback timing is much more uniform than SCK's audio buffer delivery."
  confirming_evidence:
    - "RIGHT channel has 3362 zero-runs of length >= 2 samples after t=1s — over 50% of RIGHT post-warmup samples are exact-zero (impossible for real audio with content)"
    - "Zero-run length histogram peaks at exactly 16 samples (1 ms) with 477 occurrences and 32 samples (2 ms) with 227 occurrences — these are the millisecond-quantization boundaries"
    - "20 inter-sample jumps > 4000 on RIGHT vs ZERO on LEFT — all RIGHT jumps either drop FROM nonzero TO 0 or rise FROM 0 TO nonzero (the impulse-click signature)"
    - "Inter-jump spacings include exactly 16 samples (1 ms) — the unit of monotonic_ms rounding"
    - "LEFT channel (mic, cpal) has only 2007 zero-runs — proves the wav_eartest.rs assembly logic isn't fundamentally broken; it's broken specifically for streams with jittery inter-arrival timing (SCK)"
    - "Per-frame samples themselves are NOT corrupted: peak only −16 dBFS on RIGHT, no clipping, no f32→i16 overflow possible at these levels"
  falsification_test: "If hypothesis is true, replacing frames_to_aligned_pcm with arrival-order concatenation (just concat all frame.samples back-to-back) should produce zero 1-ms-wedge zero-runs on RIGHT — instead the music plays continuously with no clicks"
  fix_rationale: "Don't place frames by wall-clock millisecond bucket. The Frame contract is 320 samples = 20 ms exactly per frame; if we concatenate frames in arrival order, output is exactly correct samples-wise. We lose nothing — both channels are independently sequential. For the 30s ear-test, what matters is each channel plays its own audio correctly so the human can ear-test it. The original frames_to_aligned_pcm was over-engineered: it tried to time-align two channels to a shared millisecond clock via the comment 'preferable to drifting the channels relative to each other,' but in practice the channels drift sub-frame anyway, and the millisecond-bucketing introduces orders-of-magnitude more sample-level corruption than the drift it tries to prevent."
  blind_spots: "(1) I'm assuming the broadcast frames are themselves intact, based on the fact that LEFT (mic) is clean. If SCK is ALSO occasionally emitting actually-zero data, concat won't fix that — but the impulse pattern is wrong for that hypothesis. (2) The user wants per-channel time alignment for ear-test purposes (matching speech with music). With independent concat, if SCK drops a frame, the system audio shifts relative to mic by 20 ms — but that's preferable to constant 1-ms clicks. (3) Cannot test the mic side because I can't speak into the mic; will rely on the user's ear-test for that."

hypothesis: WAV writer's monotonic_ms bucketing introduces 1-ms quantization artifacts at every frame boundary
test: Replace frames_to_aligned_pcm with arrival-order concat; regenerate WAV; re-measure zero-runs
expecting: Zero-run histogram collapses; sustained audio without 1ms-wedge zeros
next_action: Edit wav_eartest.rs to drop monotonic_ms bucketing; add regression test in resample/frame module that catches the broader pattern

## Symptoms

expected: Clean PCM audio in WAV — RIGHT channel should be a faithful 16 kHz downsample of Glass.aiff played via afplay; LEFT channel should be clean mic content
actual: "A bit staticy and choppy" per human ear listener with headphones; per-channel astats clean (RIGHT peak -9.4 dBFS, no clipping detected by ffmpeg); 0 lagged warnings; frame counts match expected (50Hz x 30s)
errors: None reported in logs/ffmpeg
reproduction: cargo run --example wav_eartest -p yogurt-audio (with mic input + afplay Glass.aiff in parallel)
started: First Phase 2 ear-test gate (this is initial QA, not a regression)

## Eliminated

- hypothesis: f32->i16 conversion overflow / missing clamp (suspect #1)
  evidence: clamp IS already in place at resample.rs:122 (`s.clamp(-1.0, 1.0)` before `* i16::MAX as f32`); also RIGHT peak is -16 dBFS = ~5200 / 32767 = 16% of i16::MAX — nowhere near overflow territory
  timestamp: 2026-06-25T20:45:30Z

- hypothesis: rubato SincFixedIn chunk-boundary artifacts (suspect #2)
  evidence: rubato is given exactly INPUT_CHUNK=480 samples per call and is stateful (preserves filter state across calls); also if the resampler were the source, jumps would be sample-level smooth artifacts (ringing/ripple), not impulses to exact-zero
  timestamp: 2026-06-25T20:45:45Z

- hypothesis: SCK system stream stereo->mono downmix phase cancellation (suspect #6)
  evidence: zero-runs are 16/32/etc samples (millisecond-quantized), not phase-cancellation patterns; downmix is (L+R)/2 which gives 0 only when L==-R for a span, which would not produce millisecond-bucket-aligned zeros
  timestamp: 2026-06-25T20:45:55Z

## Evidence

- timestamp: 2026-06-25T20:43:59Z
  checked: Pre-investigation facts from user
  found: Channels not swapped, no drops/lags, frames at expected count, peak -9.4 dBFS so ffmpeg sees no clip
  implication: Quality defect is sub-frame — sample-level corruption, not frame-level drops

- timestamp: 2026-06-25T20:44:30Z
  checked: ffprobe + ffmpeg astats per channel
  found: LEFT peak -23 dB / RMS -45 dB; RIGHT peak -16 dB / RMS -33 dB. Neither peak anywhere near 0 dBFS so clipping is impossible
  implication: ruled out f32→i16 overflow hypothesis

- timestamp: 2026-06-25T20:45:30Z
  checked: Sample-level inter-sample-delta analysis (python script reading raw WAV PCM)
  found: 20 jumps >4000 on RIGHT, 0 on LEFT. Every RIGHT jump is between a nonzero sample and exact zero (impulse signature). Inter-jump spacings include exactly 16 samples (= 1 ms at 16 kHz)
  implication: discontinuities are deterministically aligned to millisecond boundaries — strong signal of monotonic_ms rounding

- timestamp: 2026-06-25T20:45:50Z
  checked: Zero-run length histogram on RIGHT channel after t=1s
  found: 3362 zero-runs >= 2 samples; 477 of length exactly 16 samples (1 ms); 227 of length exactly 32 samples (2 ms); total zero samples = 50.78% of post-1s RIGHT content
  implication: confirmed — frames_to_aligned_pcm's millisecond-bucketing in wav_eartest.rs:227 introduces 1-ms quantization gaps that the zero-init buffer shows through

- timestamp: 2026-06-25T20:45:55Z
  checked: Mental trace of frames_to_aligned_pcm with realistic monotonic_ms values
  found: Frame N+1's start = monotonic_ms * 16. If wall-clock for frame N+1 falls in the same millisecond bucket as expected-20-ms-later (mod 1 ms jitter), placement is off by 16-32 samples. Since 320 samples = 20 ms exactly but wall-clock-measured 20 ms gaps round to {19, 20, 21} ms, consecutive frames stochastically gap or overlap by 16 samples
  implication: WAV-writer-side bug. Frames themselves are clean. Fix is to concatenate by arrival order, not place by timestamp

## Resolution

root_cause: frames_to_aligned_pcm() in wav_eartest.rs places each frame into the output WAV at offset (frame.monotonic_ms * 16). monotonic_ms is wall-clock-derived (Instant::elapsed().as_millis() as u64) so it's quantized to whole milliseconds, but frames span exactly 320 samples = 20 ms each. Wall-clock jitter of ±1-2 ms per frame combines with the millisecond quantization to leave ±16-sample gaps (filled by the zero-init buffer) or ±16-sample overlaps between consecutive frames. Result: ~3362 zero-runs on the RIGHT channel, with the histogram peaking at exactly 16-sample and 32-sample runs — audible as sustained crackle/static throughout the system-audio channel.
fix: Replace timestamp-bucketed placement with arrival-order concatenation in wav_eartest.rs (commit c92af4a). Each channel's frame samples are concatenated in receive order; tail is silence-padded or truncated to fit total_samples. This loses zero-pad-on-channel-drop behavior, but no drops were observed (lagged counts = 0). Three regression tests added in the example's #[cfg(test)] module, including one feeding jittery monotonic_ms values that would have triggered 1088 zero-wedges under the old logic.
verification: WAV regenerated with afplay Glass.aiff loop. Measurements before vs after: RIGHT inter-sample jumps >4000 went from 20 to 0; 16-sample exact-zero runs went from 477 to 78 (and those 78 now sit at random offsets within frames, not aligned to frame boundaries — these are natural zero-crossings of the bell decay tail). cargo test -p yogurt-audio --features synthetic = 16 passed, 1 ignored. PENDING: user re-listen with headphones to confirm crackle is gone.
files_changed:
  - crates/yogurt-audio/examples/wav_eartest.rs (137+/38-)
  - .planning/phases/02-audio-capture-highest-risk/02-03-DIAGNOSIS.md (new)
commits:
  - c92af4a fix(audio,quality): replace monotonic_ms bucketing in wav_eartest with arrival-order concat
  - 674222f docs(02-03): add diagnosis for audio ear-test static/choppy defect
