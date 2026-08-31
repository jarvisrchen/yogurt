# AUD-1: live partials shrink mid-sentence, and never appear for system audio

Design for the fix.
Ticket: `docs/TODO.md` AUD-1.
Both symptoms live in the local whisper.cpp partial ticker (`crates/yogurt-stt/src/whisper_local.rs`).
Deepgram is unaffected and unchanged.

## The one root cause

The ticker keeps its **own** notion of "the audio to preview", separate from the segmenter's:

```rust
// whisper_local.rs, main pump - a buffer the ticker owns
let mut buf = partial_buf_writer.lock().await;
buf.extend_from_slice(&samples);
let max = 16_000 * 5;
if buf.len() > max {
    let excess = buf.len() - max;
    buf.drain(..excess);   // <- the bug
}
```

That buffer is a fixed 5-second rolling window over the raw mic stream.
It knows nothing about utterance boundaries, and the ticker emits each re-decode as a whole-line replacement.

Both reported symptoms fall out of that single design choice:

| Symptom | Why the shared buffer causes it |
|---|---|
| Partial **shrinks** once you pass 5 s | The window drops the earliest words while the line is replaced wholesale, so the re-decode is of *less* audio than the previous one |
| **No partial at all** on system audio | The buffer is fed only from the `Channel::Mic` arm of the pump; `Channel::System` has no equivalent, so it only ever renders VAD finals |

Meanwhile `Segmenter` already maintains exactly the buffer the ticker wants.
`Segmenter::buffer` is the in-flight utterance: it only ever grows while `in_speech`, it is cleared the moment a `Segment` is emitted, and it is bounded.
It is the same PCM the eventual final is decoded from.

One correction found while verifying: it was *not* actually bounded at 25 s.
`MAX_SEGMENT_MS` gates on `speech_ms`, which counts only voice frames, while the buffer collects every frame we are in speech for.
Speech with pauses shorter than `SILENCE_HANG_MS` never flushes on silence and advances `speech_ms` at well under wall-clock rate - measured at a 37% duty cycle the buffer reaches **46 s** before `speech_ms` reaches 25 s.
That already broke `MAX_SEGMENT_MS`'s own stated promise ("so Whisper sees bounded input") for finals on `main`; exposing the buffer to the ticker would have extended it to partials, where a >30 s input costs a second whisper encoder pass and lands the decode outside the 1 s tick.
So `MAX_SEGMENT_SAMPLES` now caps the buffer directly, and a test pins it.

**The fix is to delete the duplicate and read the segmenter's buffer.**
Growth then becomes structural rather than something the ticker has to remember to do, and the reset-on-final is free.

## What changes

### 1. `vad.rs` - expose the in-flight utterance

```rust
pub fn pending(&self) -> Option<(&[i16], u64)> {
    if !self.in_speech || self.buffer.is_empty() {
        return None;
    }
    Some((&self.buffer, self.speech_start_ms))
}
```

`None` between utterances is load-bearing: it is what stops the ticker burning a decode on silence.

### 2. `whisper_local.rs` - ticker reads the segmenters

The two `Segmenter`s move behind `Arc<Mutex<_>>` so the pump and the ticker share them.
`partial_buf` and its rolling-window drain are deleted outright.

`std::sync::Mutex`, not `tokio::sync::Mutex`: nothing awaits while holding it, and with the std guard "await while holding" is a *compile error* (the future stops being `Send`) rather than a 600 ms stall of the audio pump.
The pump's critical section is a VAD pass over at most a few 30 ms frames.

### 3. `whisper_local.rs` - the ticker round-robins mic and system

Each tick picks the **first channel, starting from wherever it left off, that has an utterance in flight**, and decodes exactly that one.

```
tick 1  mic in flight, sys idle   -> decode mic
tick 2  mic in flight, sys idle   -> decode mic       (sys skipped, nothing pending)
tick 3  both in flight            -> decode sys
tick 4  both in flight            -> decode mic
```

This is the answer to the objection that froze this in v1 ("enabling the ticker for `Channel::System` doubles whisper.cpp pressure").
It does not double it. **Pressure is unchanged: one greedy decode per second, exactly as today.**
What is traded is refresh *rate*, and only while both people are mid-utterance at once - then each side refreshes every 2 s instead of 1 s.
One person talking, which is the overwhelmingly common case, still refreshes at 1 s because the idle channel returns `None` and is skipped for free.

### 4. Partials carry the real utterance start, not `ts_ms: 0`

The hardcoded `ts_ms: 0` is what renders the `00:00:00` in-flight line in the ticket's screenshot.
The comment defending it says a real timestamp "would falsely suggest each re-decode is a new event", but that is not how the frontend works: `mergeEvent` in `web/src/lib/ws.ts` replaces the trailing partial on a channel *regardless of `ts_ms`* - the timestamp is not a key.

Deepgram already stamps its partials with the utterance start (`UtteranceState.start_ms`), the same value its final carries.
Now that the segmenter hands us `speech_start_ms`, local matches that convention: the timestamp no longer jumps when a final replaces its partial, and partials stop polluting the `data-transcript-ts-sec` scroll anchors with a spurious 0.

## Cost: measured, not assumed

The ticket asked to confirm a 25 s cumulative decode fits the 1 s tick budget before committing.
Measured on this machine against `large-v3-turbo` (the model actually selected in `~/.yogurt/db.sqlite`), greedy/`no_context`, best of 3, real speech rendered from `scripts/eval/conversation.txt`:

| Audio in buffer | Greedy decode | Words out |
|---:|---:|---:|
| 1 s | 342 ms | 2 |
| 5 s | 366 ms | 16 |
| 10 s | 413 ms | 32 |
| 15 s | 453 ms | 43 |
| 20 s | 533 ms | 54 |
| **25 s** (`MAX_SEGMENT_MS`) | **584 ms** | 65 |
| 30 s | 636 ms | 78 |

Reproduce with `cargo run --release -p yogurt-stt --features local-stt --example partial_decode_cost -- large-v3-turbo <wav>`; the example's header comment has the two commands that render the wav.
It is committed precisely because this is a per-machine, per-model answer that a comment cannot carry on its own.

Cost is roughly `330 ms + 10 ms per second of audio`.

The flat part is the reason this works: whisper.cpp pads every input to a 30 s mel window, so the **encoder cost does not depend on how much audio you hand it**. Only the decoder loop scales, with token count, and `large-v3-turbo` has just 4 decoder layers.
A cumulative 25 s buffer costs 60% more than the 5 s window it replaces in exchange for never shrinking, and still lands at 58% of the 1 s tick budget.

The same numbers are why round-robin is the right shape for system audio: the ceiling is one ~585 ms decode per tick either way. Decoding *both* channels every tick would be ~1.17 s, over budget, and `tokio::time::interval` defaults to `MissedTickBehavior::Burst` - it would spin trying to catch up.

## Bonus: this makes the echo deduper work on local STT

`EchoDeduper` in `meetings.rs` retains the latest **system partial** as comparison material, specifically so "a mic final that beats the matching system FINAL to arrival still matches the system partial text already streamed".

On local STT that field was dead - system partials did not exist, so `sys_partial` was permanently `None`.
Emitting them activates a dedupe path that was already written and already tested, which should reduce the duplicate `Me`/`Them` lines visible in the ticket's first screenshot when recording on speakers.
That is the separate echo-bleed issue, not fixed here, but it gets meaningfully better as a side effect.

## What is deliberately not in scope

- **Append-only partial deltas.** The ticket names this as the fallback if cumulative decode blew the tick budget. It did not (584 ms of 1000 ms), so the wire format stays whole-line replacement and the frontend needs no change at all.
- **Adaptive tick interval as the buffer grows.** Also a ticket suggestion, also unnecessary at the measured cost. If a future model has a fat decoder and 25 s pushes past the budget, the knob to reach for is the tick interval, not dropping audio.
- **Echo-bleed duplicate lines.** Its own ticket.
- **Frontend changes.** `mergeEvent` is already per-channel and `TranscriptLine` already renders `Them` partials at opacity 0.7. Zero diff under `web/`.

## Tests

`crates/yogurt-stt/tests/vad_segmenter.rs` gets the regression coverage, because the root cause is now expressible as a segmenter contract:

- `pending()` is `None` before speech, and `None` again after the segment is emitted.
- `pending()` **grows monotonically** across successive pushes inside one utterance - this is AUD-1's shrink symptom, stated as an assertion.
- `pending()` keeps growing past 5 s, the old window size, up to `MAX_SEGMENT_MS`.
- `pending()` reports the same `start_ms` the emitted `Segment` carries, which is what makes the partial-to-final timestamp continuous.
- The buffer is bounded in **wall-clock** samples, not just voice time: speech with sub-`SILENCE_HANG_MS` pauses must not grow it past `MAX_SEGMENT_SAMPLES`. Verified to fail at 46.4 s before the cap was added.

End-to-end verification is the manual handover (see the PR body): play audio through the speakers so both channels go live at once, and watch that `Me` and `Them` both stream a growing dim line.
