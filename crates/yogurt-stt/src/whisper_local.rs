//! Local STT via whisper.cpp (whisper-rs 0.16 bindings).
//!
//! Drop-in alternative to [`crate::deepgram::DeepgramStt`] — implements the
//! same [`Stt`] trait so the meeting supervisor can swap engines per
//! provider preference at runtime.
//!
//! ## Streaming strategy (dual `whisper_state` pattern)
//!
//! whisper.cpp's `WhisperState` is the per-decoder scratch space; the
//! parent `WhisperContext` (the model weights) is shared by `Arc`. We
//! create state fresh per decode call rather than pooling — `create_state`
//! is cheap, and pooling complicates lifetimes for `spawn_blocking`.
//!
//! We run TWO sampling configurations against the same context:
//!
//! 1. **Greedy / fast — preview** (`fast=true`): `SamplingStrategy::Greedy
//!    { best_of: 1 }` on the utterance currently in flight, fired every
//!    1 s. The audio comes straight from `Segmenter::pending` rather than
//!    from a buffer the ticker keeps itself — see AUD-1 below. Used
//!    for the "still listening" indicator (`is_final: false` events). Per
//!    PRD §13 the partial quality is openly worse than Deepgram — it's the
//!    privacy escape hatch, not the daily driver. `set_no_context(true)`
//!    so the partial decoder doesn't carry cross-segment hallucinations.
//!
//! 2. **BeamSearch / settled — final** (`fast=false`): `BeamSearch
//!    { beam_size: 5, patience: 1.0 }` on full utterances handed up by the
//!    VAD segmenter. These emit `is_final: true` events and are what the
//!    transcript dock locks in place. `set_no_context(false)` so each
//!    decoded segment can reference prior context.
//!
//! ## Runtime safety invariant (LOCAL-05)
//!
//! `whisper.cpp` is synchronous C++ — its `whisper_full` call blocks for
//! hundreds of ms to several seconds depending on model size + audio
//! length. Calling it directly from a tokio task would starve the
//! runtime's worker threads (no other futures progress while the C++ is
//! running). EVERY decode site in this file is wrapped in
//! `tokio::task::spawn_blocking`. Grep proof must show ≥3 occurrences:
//!   - mic-channel final decoder task
//!   - system-channel final decoder task
//!   - partial-window ticker task
//!
//! Anyone refactoring this file: if you remove a `spawn_blocking`, you
//! WILL deadlock the WS broadcast pump under load. The invariant is
//! covered in Plan 08-03's bench acceptance — don't regress it.
//!
//! ## Why the partial ticker has no buffer of its own (AUD-1)
//!
//! It used to. The ticker kept a rolling 5 s window over the raw mic
//! stream, re-decoded it every second, and emitted the result as a
//! whole-line replacement. Once an utterance ran past five seconds the
//! earliest words scrolled out of the window, so each replacement decoded
//! *less* audio than the one before and the on-screen partial visibly
//! shrank mid-sentence. The window was also fed only from the
//! `Channel::Mic` arm of the pump, so system audio never got a partial at
//! all.
//!
//! Both were the same mistake: a second, worse notion of "the audio to
//! preview", parallel to the one `Segmenter` already maintains.
//! `Segmenter::buffer` *is* the in-flight utterance — it only grows while
//! in speech, it is cleared the instant a `Segment` is emitted, and
//! `MAX_SEGMENT_SAMPLES` bounds it. The ticker now reads it through
//! `Segmenter::pending`, which makes "partials only ever grow" structural
//! rather than something this file has to remember to do.
//!
//! The segmenters therefore live behind `Arc<Mutex<_>>`, shared with the
//! pump. `std::sync::Mutex`, deliberately, not the tokio one: nothing
//! awaits while holding it, and with the std guard an accidental
//! await-while-held is a *compile* error (the future stops being `Send`)
//! instead of a 600 ms stall of the audio pump.
//!
//! ## Why the segmenter is a placeholder in 08-01
//!
//! Plan 08-02 lands the real `vad.rs`. Plan 08-01 ships the adapter
//! against the placeholder's type signatures so the trait impl exists
//! today. The pump body is fully wired against the placeholder API —
//! Plan 08-02 only needs to replace `vad.rs`.

use crate::vad::{Segmenter, SegmenterEvent};
use crate::{AudioChunk, AudioRx, Channel, Stt, TranscriptEvent, TranscriptTx};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Whisper.cpp adapter — owns the model context and drives a dual-state
/// (greedy/preview + beam/settled) decode pipeline against the Phase 3
/// `Stt` trait.
pub struct WhisperLocal {
    /// Held for diagnostics + error messages; the loaded `ctx` is the
    /// thing actually used at decode time.
    #[allow(dead_code)]
    model_path: PathBuf,
    /// Heavyweight (~80 MB for tiny.en, ~3 GB for large-v3) — share by
    /// `Arc::clone` across mic / system / partial worker tasks. Each
    /// task calls `ctx.create_state()` per decode, so there's no
    /// interior mutability concern on the context itself.
    ctx: Arc<WhisperContext>,
}

impl WhisperLocal {
    /// Load a ggml whisper model from disk.
    ///
    /// **Blocking:** `WhisperContext::new_with_params` mmaps the model
    /// file and runs ggml's quantization-table init on the calling
    /// thread. For multi-GB models this is hundreds of ms. Callers in
    /// the async runtime MUST wrap this in `tokio::task::spawn_blocking`
    /// — the meeting supervisor (Plan 08-03 `meetings/start.rs`) does
    /// exactly that. We don't wrap inside `load` because:
    ///   1. Test code (smoke test, Plan 08-02 download tests) calls this
    ///      from synchronous setup blocks.
    ///   2. Wrapping here would force callers into an async context just
    ///      to load a model, which is the wrong shape for the
    ///      Settings → SQLite "preferred model" path.
    pub fn load(model_path: PathBuf) -> Result<Self> {
        if !model_path.exists() {
            return Err(anyhow!(
                "whisper model not found at {} — run the Settings → Download flow first",
                model_path.display()
            ));
        }
        let path_str = model_path
            .to_str()
            .context("model path is not utf-8")?
            .to_string();
        // whisper.cpp and ggml write straight to stderr through ggml's own log
        // callback, which `params.set_print_*(false)` in `decode` does NOT
        // control - those only govern whisper's transcript printing. Left
        // alone it is 45 lines on model load and another 17 on EVERY decode,
        // and the partial ticker decodes once a second, so a live meeting
        // buries the terminal. This redirects that stream into `tracing`,
        // where the subscriber's filter decides (see `yogurt-cli`'s default:
        // `whisper_rs=warn`, so warnings and errors still surface and
        // `RUST_LOG=whisper_rs=debug` brings the rest back).
        //
        // Must run before the first ggml call. `Once`-guarded inside
        // whisper-rs, so calling it per load is free.
        whisper_rs::install_logging_hooks();
        let ctx = WhisperContext::new_with_params(&path_str, WhisperContextParameters::default())
            .context("loading whisper model")?;
        Ok(Self {
            model_path,
            ctx: Arc::new(ctx),
        })
    }

    /// Run whisper.cpp on a PCM segment, returning the joined transcript text.
    ///
    /// Inputs:
    ///   - `ctx`: shared model context (cloned from the adapter).
    ///   - `pcm_i16`: 16 kHz mono linear-16 PCM samples.
    ///   - `fast`: `true` selects the greedy/preview strategy; `false`
    ///     selects beam-search/settled.
    ///
    /// **This function blocks the calling thread.** Always call it from
    /// inside `tokio::task::spawn_blocking`.
    fn decode(ctx: &WhisperContext, pcm_i16: &[i16], fast: bool) -> Result<String> {
        // whisper-rs wants f32 in [-1.0, 1.0]. The helper is a single
        // SIMD-friendly loop in whisper-rs; no allocation other than our
        // destination buffer.
        let mut f32_buf = vec![0.0f32; pcm_i16.len()];
        whisper_rs::convert_integer_to_float_audio(pcm_i16, &mut f32_buf)
            .context("pcm conversion")?;

        // Fresh state per decode — `create_state` allocates the working
        // buffers (KV cache etc.) but NOT the model weights. Cheap
        // relative to the decode itself.
        let mut state = ctx.create_state().context("create whisper state")?;

        // Dual-state proof: both Greedy and BeamSearch must be visible
        // in this file (acceptance criterion).
        let mut params = if fast {
            FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
        } else {
            FullParams::new(SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: 1.0,
            })
        };

        // English-only — matches Phase 3 + the v1 PRD §5 transcript scope.
        params.set_language(Some("en"));
        // Silence whisper.cpp's own stdout — we surface our own structured
        // events via TranscriptTx instead.
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        // Partials: don't carry context across windows (we're decoding the
        // same audio repeatedly, with overlap). Finals: do carry context
        // so adjacent segments cohere.
        params.set_no_context(fast);

        state.full(params, &f32_buf).context("whisper decode")?;

        let n_segments = state.full_n_segments();
        let mut out = String::new();
        for i in 0..n_segments {
            // whisper-rs 0.16 replaced `full_get_segment_text(i)` with the
            // typed `get_segment(i)?.to_str_lossy()` chain. Lossy is
            // appropriate here: invalid UTF-8 should never escape into a
            // browser DOM (Phase 4's ammonia sanitizer would also catch it
            // but we don't want to rely on that for a transcript line).
            if let Some(seg) = state.get_segment(i) {
                if let Ok(s) = seg.to_str_lossy() {
                    out.push_str(&s);
                }
            }
        }
        Ok(out.trim().to_string())
    }
}

#[async_trait]
impl Stt for WhisperLocal {
    /// Run the local STT pipeline for the lifetime of the audio stream.
    ///
    /// Spawns three worker tasks (all guarding their decode calls with
    /// `spawn_blocking`) plus the main audio pump:
    ///
    /// 1. **Mic final decoder** — drains `mic_seg_rx`, runs
    ///    `decode(.., fast=false)` per VAD segment, emits Final.
    /// 2. **System final decoder** — same but for the system-audio side.
    /// 3. **Partial ticker** — every 1 s snapshots the utterance in
    ///    flight on ONE channel (round-robin, skipping channels with
    ///    nothing pending), runs `decode(.., fast=true)`, emits Partial.
    /// 4. **Audio pump** — `audio_rx.recv()` → split by `Channel` →
    ///    `Segmenter::push` (which fires `SegmenterEvent::Segment` once
    ///    Plan 08-02 lands the real VAD).
    ///
    /// Returns when the upstream audio broadcast closes.
    async fn start(&self, mut audio_rx: AudioRx, txn: TranscriptTx) -> Result<()> {
        let ctx_mic = self.ctx.clone();
        let ctx_sys = self.ctx.clone();
        let ctx_partial = self.ctx.clone();

        // Shared with the partial ticker, which reads each segmenter's
        // in-flight utterance via `Segmenter::pending`. See the AUD-1 note
        // in the module docs for why the ticker has no buffer of its own.
        let mic_seg = Arc::new(Mutex::new(Segmenter::new(16_000)));
        let sys_seg = Arc::new(Mutex::new(Segmenter::new(16_000)));

        // Per-channel segment queue. Sized small (8) — under sustained
        // overload `try_send` drops; that's deliberate vs blocking the
        // audio pump (matches the deepgram adapter's backpressure stance).
        let (mic_seg_tx, mut mic_seg_rx) = mpsc::channel::<(Vec<i16>, u64, u64)>(8);
        let (sys_seg_tx, mut sys_seg_rx) = mpsc::channel::<(Vec<i16>, u64, u64)>(8);

        // ------------------------------------------------------------------
        // Worker 1: mic-channel FINAL decoder (beam search, is_final = true)
        // ------------------------------------------------------------------
        let txn_mic = txn.clone();
        tokio::spawn(async move {
            while let Some((pcm, start_ms, _end_ms)) = mic_seg_rx.recv().await {
                let ctx = ctx_mic.clone();
                // LOCAL-05 invariant: whisper.cpp call MUST be on a
                // blocking thread, not on the tokio worker pool.
                let text = tokio::task::spawn_blocking(move || {
                    Self::decode(&ctx, &pcm, /* fast */ false)
                })
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                let _ = txn_mic.send(TranscriptEvent {
                    ts_ms: start_ms,
                    channel: Channel::Mic,
                    text,
                    is_final: true,
                });
            }
        });

        // ------------------------------------------------------------------
        // Worker 2: system-channel FINAL decoder (beam search, is_final = true)
        // ------------------------------------------------------------------
        let txn_sys = txn.clone();
        tokio::spawn(async move {
            while let Some((pcm, start_ms, _end_ms)) = sys_seg_rx.recv().await {
                let ctx = ctx_sys.clone();
                // LOCAL-05 invariant.
                let text = tokio::task::spawn_blocking(move || {
                    Self::decode(&ctx, &pcm, /* fast */ false)
                })
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                let _ = txn_sys.send(TranscriptEvent {
                    ts_ms: start_ms,
                    channel: Channel::System,
                    text,
                    is_final: true,
                });
            }
        });

        // ------------------------------------------------------------------
        // Worker 3: partial ticker (greedy, is_final = false)
        //
        // Tick every 1 s; decode the in-flight utterance on ONE channel,
        // chosen round-robin among the channels that actually have one.
        //
        // v1 ran this mic-only "to halve whisper.cpp pressure", which left
        // the far end with no still-listening indicator at all (AUD-1).
        // Round-robin covers both channels without paying that price:
        // pressure is unchanged at one greedy decode per tick. What gives
        // instead is refresh *rate*, and only while both people are
        // mid-utterance simultaneously — then each side refreshes every 2 s
        // rather than every 1 s. One person talking, the common case, still
        // refreshes at 1 s, because the idle channel's `pending()` is `None`
        // and costs nothing to skip.
        //
        // Decoding both channels every tick was the alternative and does not
        // fit: measured on large-v3-turbo, a full 25 s `MAX_SEGMENT_MS`
        // buffer decodes in ~585 ms, so two would be ~1.17 s against a 1 s
        // tick — and `interval`'s default `MissedTickBehavior::Burst` would
        // then spin trying to catch up. `examples/partial_decode_cost.rs`
        // re-measures that on your machine and model; if a future model
        // pushes the 25 s row past ~1 s, the knob is this interval, not a
        // smaller buffer (that is the shrinking bug all over again).
        // ------------------------------------------------------------------
        // The ticker loops forever on its own — tie its lifetime to this
        // `start()` future (which the meeting supervisor aborts on stop) or
        // it keeps a WhisperCtx clone (multi-GB model) alive and burns a
        // decode every second long after the meeting ended. A plain
        // JoinHandle drop detaches; this guard aborts instead, covering
        // both the normal audio-closed exit AND the caller aborting us.
        struct AbortOnDrop(tokio::task::JoinHandle<()>);
        impl Drop for AbortOnDrop {
            fn drop(&mut self) {
                self.0.abort();
            }
        }

        let sources = [
            (mic_seg.clone(), Channel::Mic),
            (sys_seg.clone(), Channel::System),
        ];
        let txn_partial = txn.clone();
        let _ticker_guard = AbortOnDrop(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_millis(1000));
            // Skip the immediate first tick — we want to fire AFTER 1 s
            // of buffered audio, not at t=0 with an empty buffer.
            ticker.tick().await;
            let mut next_source = 0usize;
            loop {
                ticker.tick().await;

                // Probe each channel once, starting where the last tick left
                // off, and take the first with an utterance long enough to be
                // worth a decode. `next_source` advances on every probe, not
                // just on a hit, so two active channels alternate rather than
                // one starving the other.
                let mut picked = None;
                for _ in 0..sources.len() {
                    let (seg, channel) = &sources[next_source];
                    next_source = (next_source + 1) % sources.len();
                    // Scoped so the guard is released before the decode
                    // below — and it is a std guard, so holding it across
                    // that `.await` would not compile in the first place.
                    let snapshot = {
                        let seg = seg.lock().unwrap_or_else(|e| e.into_inner());
                        seg.pending()
                            // Need at least 1 s of audio to bother decoding.
                            .filter(|(pcm, _)| pcm.len() >= 16_000)
                            .map(|(pcm, start_ms)| (pcm.to_vec(), start_ms))
                    };
                    if let Some((pcm, start_ms)) = snapshot {
                        picked = Some((pcm, start_ms, *channel));
                        break;
                    }
                }
                // Nothing in flight on either channel — everyone is between
                // utterances. Skip the decode entirely.
                let Some((pcm, start_ms, channel)) = picked else {
                    continue;
                };

                let ctx = ctx_partial.clone();
                // LOCAL-05 invariant.
                let text = tokio::task::spawn_blocking(move || {
                    Self::decode(&ctx, &pcm, /* fast */ true)
                })
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                let _ = txn_partial.send(TranscriptEvent {
                    // The utterance's own start, matching what the final for
                    // this same segment will carry — so the timestamp does
                    // not jump when the final replaces the partial in place.
                    // This mirrors the deepgram adapter, which stamps its
                    // partials with `UtteranceState.start_ms`. (It used to be
                    // a hardcoded 0, which rendered every in-flight line as
                    // `00:00:00`; `mergeEvent` on the frontend keys off
                    // channel and is_final, never ts_ms, so nothing depended
                    // on the zero.)
                    ts_ms: start_ms,
                    channel,
                    text,
                    is_final: false,
                });
            }
        }));

        // ------------------------------------------------------------------
        // Main pump: split incoming AudioChunk by channel and feed the
        // matching segmenter. Mirror the deepgram pump's Lagged/Closed
        // semantics. The partial ticker reads the same segmenters, so there
        // is nothing extra to maintain here.
        // ------------------------------------------------------------------
        loop {
            match audio_rx.recv().await {
                Ok(chunk) => {
                    let AudioChunk {
                        channel,
                        samples,
                        ts_ms: _,
                    } = chunk;
                    match channel {
                        Channel::Mic => {
                            let tx_seg = mic_seg_tx.clone();
                            let mut seg = mic_seg.lock().unwrap_or_else(|e| e.into_inner());
                            seg.push(&samples, |e| {
                                // Plan 08-02 added SpeechStart for UI
                                // indicators; the decoder pump only
                                // cares about Segment.  Using `match`
                                // so adding further variants is a
                                // compile error rather than a silent
                                // drop.
                                match e {
                                    SegmenterEvent::Segment {
                                        pcm,
                                        start_ms,
                                        end_ms,
                                    } => {
                                        let _ = tx_seg.try_send((pcm, start_ms, end_ms));
                                    }
                                    SegmenterEvent::SpeechStart { .. } => {}
                                }
                            });
                        }
                        Channel::System => {
                            let tx_seg = sys_seg_tx.clone();
                            let mut seg = sys_seg.lock().unwrap_or_else(|e| e.into_inner());
                            seg.push(&samples, |e| match e {
                                SegmenterEvent::Segment {
                                    pcm,
                                    start_ms,
                                    end_ms,
                                } => {
                                    let _ = tx_seg.try_send((pcm, start_ms, end_ms));
                                }
                                SegmenterEvent::SpeechStart { .. } => {}
                            });
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // CLI-2: debug for the same reason as the audio adapter's
                    // lag in `meetings.rs` - a model-load-time burst, not an
                    // actionable warning.
                    tracing::debug!(?n, "whisper_local audio rx lagged; dropping");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("audio channel closed; whisper_local exiting");
                    break;
                }
            }
        }

        // Drop the segment senders so the worker tasks unwind cleanly.
        drop(mic_seg_tx);
        drop(sys_seg_tx);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time + runtime smoke: WhisperLocal must be `dyn Stt`-safe so
    /// the meeting supervisor can box it behind `Box<dyn Stt>` interchangeably
    /// with `DeepgramStt`.
    #[test]
    fn whisper_local_is_object_safe() {
        fn _assert_dyn(_: &dyn Stt) {}
        // We don't actually construct one (would need a real model file);
        // the assertion is on the trait bound, which is checked at compile.
        let _f: fn(&dyn Stt) = _assert_dyn;
    }
}
