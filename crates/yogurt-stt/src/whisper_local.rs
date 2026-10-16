//! Local STT via whisper.cpp, implementing the same [`Stt`] trait as [`crate::deepgram::DeepgramStt`].
//!
//! One `WhisperContext` (the model weights) is shared by `Arc`, and a fresh `WhisperState` is created per decode because `create_state` is cheap and pooling complicates `spawn_blocking` lifetimes.
//!
//! Two sampling configurations run against it:
//!
//! - Preview (`fast=true`): greedy, fired every 1 s on the utterance in flight from `Segmenter::pending`, emitting `is_final: false`.
//!   `set_no_context(true)` keeps the partial decoder from carrying cross-segment hallucinations.
//! - Final (`fast=false`): beam search on full utterances from the VAD segmenter, emitting `is_final: true`.
//!   `set_no_context(false)` lets each segment reference prior context.
//!
//! ## Runtime safety invariant
//!
//! `whisper_full` is synchronous C++ that blocks for hundreds of ms to seconds, which would starve tokio's worker threads.
//! Every decode site (mic final, system final, partial ticker) must stay inside `tokio::task::spawn_blocking`.
//! Removing one deadlocks the WS broadcast pump under load.
//!
//! ## Partial ticker reads the segmenter
//!
//! The ticker has no audio buffer of its own: `Segmenter::buffer` already is the in-flight utterance, which only grows in speech and is bounded by `MAX_SEGMENT_SAMPLES`.
//! A separate rolling window made partials shrink once an utterance outran it, and was only fed from the mic channel.
//!
//! The segmenters therefore live behind `Arc<Mutex<_>>`, shared with the pump.
//! It is a `std::sync::Mutex` so that holding it across an await is a compile error (the future stops being `Send`) rather than a stall of the audio pump.

use crate::vad::{Segmenter, SegmenterEvent};
use crate::{AudioChunk, AudioRx, Channel, Stt, TranscriptEvent, TranscriptTx};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Whisper.cpp adapter. Owns the model context and drives the preview and final decode pipeline.
pub struct WhisperLocal {
    /// Kept for diagnostics and error messages.
    #[allow(dead_code)]
    model_path: PathBuf,
    /// Heavyweight (~80 MB for tiny.en, ~3 GB for large-v3), shared across worker tasks by `Arc::clone`.
    ctx: Arc<WhisperContext>,
}

impl WhisperLocal {
    /// Load a ggml whisper model from disk.
    ///
    /// **Blocking:** it mmaps the model and runs ggml's table init on the calling thread, so async callers must wrap it in `tokio::task::spawn_blocking`.
    /// It is not wrapped here because tests call it from synchronous setup and the Settings path should not need an async context just to load a model.
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
        // ggml writes to stderr through its own log callback, which `set_print_*(false)` in `decode` does not control.
        // Left alone that is 45 lines per load and 17 per decode, and the partial ticker decodes every second.
        // This redirects it into `tracing`, where the default filter `whisper_rs=warn` still surfaces warnings.
        // Must run before the first ggml call; `Once`-guarded inside whisper-rs.
        whisper_rs::install_logging_hooks();
        let ctx = WhisperContext::new_with_params(&path_str, WhisperContextParameters::default())
            .context("loading whisper model")?;
        Ok(Self {
            model_path,
            ctx: Arc::new(ctx),
        })
    }

    /// Settings "Test" action: load the model and decode a 1 s silent clip.
    /// This proves Metal init and inference work on this machine, which otherwise only fails when a meeting starts.
    /// An empty transcript is the expected result.
    ///
    /// **Blocking:** async callers must use `tokio::task::spawn_blocking`.
    pub fn self_test(model_path: PathBuf) -> Result<()> {
        let whisper = Self::load(model_path)?;
        // Long enough for whisper.cpp to run a real inference pass.
        let silence = vec![0i16; 16_000];
        Self::decode(&whisper.ctx, &silence, /* fast */ false)?;
        Ok(())
    }

    /// Run whisper.cpp on 16 kHz mono PCM and return the joined transcript and a confidence.
    /// `fast` selects greedy preview over beam-search final.
    ///
    /// **Blocks the calling thread.** Call it from `tokio::task::spawn_blocking`.
    fn decode(ctx: &WhisperContext, pcm_i16: &[i16], fast: bool) -> Result<(String, Option<f32>)> {
        // whisper-rs wants f32 in [-1.0, 1.0].
        let mut f32_buf = vec![0.0f32; pcm_i16.len()];
        whisper_rs::convert_integer_to_float_audio(pcm_i16, &mut f32_buf)
            .context("pcm conversion")?;

        // `create_state` allocates working buffers (KV cache), not the model weights.
        let mut state = ctx.create_state().context("create whisper state")?;

        let mut params = if fast {
            FullParams::new(SamplingStrategy::Greedy { best_of: 1 })
        } else {
            FullParams::new(SamplingStrategy::BeamSearch {
                beam_size: 5,
                patience: 1.0,
            })
        };

        // English only.
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        // Partials re-decode the same overlapping audio, so they must not carry context.
        params.set_no_context(fast);

        state.full(params, &f32_buf).context("whisper decode")?;

        let n_segments = state.full_n_segments();
        let mut out = String::new();
        // Confidence is 1 - the worst (most silence-like) segment's no-speech probability.
        let mut max_no_speech = 0.0f32;
        for i in 0..n_segments {
            // Lossy so invalid UTF-8 never reaches a browser DOM.
            if let Some(seg) = state.get_segment(i) {
                if let Ok(s) = seg.to_str_lossy() {
                    out.push_str(&s);
                }
                max_no_speech = max_no_speech.max(seg.no_speech_probability());
            }
        }
        let confidence = if n_segments > 0 {
            Some(1.0 - max_no_speech)
        } else {
            None
        };
        Ok((out.trim().to_string(), confidence))
    }
}

#[async_trait]
impl Stt for WhisperLocal {
    /// Run the pipeline for the lifetime of the audio stream, returning when the upstream audio broadcast closes.
    ///
    /// Spawns a final decoder per channel, a partial ticker that decodes one channel per second round-robin, and the audio pump.
    async fn start(&self, mut audio_rx: AudioRx, txn: TranscriptTx) -> Result<()> {
        let ctx_mic = self.ctx.clone();
        let ctx_sys = self.ctx.clone();
        let ctx_partial = self.ctx.clone();

        // Shared with the partial ticker, which reads each segmenter's in-flight utterance.
        let mic_seg = Arc::new(Mutex::new(Segmenter::new(16_000)));
        let sys_seg = Arc::new(Mutex::new(Segmenter::new(16_000)));

        // Small queue: under sustained overload `try_send` drops rather than blocking the audio pump, like the deepgram adapter.
        let (mic_seg_tx, mut mic_seg_rx) = mpsc::channel::<(Vec<i16>, u64, u64)>(8);
        let (sys_seg_tx, mut sys_seg_rx) = mpsc::channel::<(Vec<i16>, u64, u64)>(8);

        let txn_mic = txn.clone();
        tokio::spawn(async move {
            while let Some((pcm, start_ms, _end_ms)) = mic_seg_rx.recv().await {
                let ctx = ctx_mic.clone();
                // Must run on a blocking thread.
                let (text, confidence) = tokio::task::spawn_blocking(move || {
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
                    confidence,
                });
            }
        });

        let txn_sys = txn.clone();
        tokio::spawn(async move {
            while let Some((pcm, start_ms, _end_ms)) = sys_seg_rx.recv().await {
                let ctx = ctx_sys.clone();
                // Must run on a blocking thread.
                let (text, confidence) = tokio::task::spawn_blocking(move || {
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
                    confidence,
                });
            }
        });

        // Partial ticker: every 1 s, decode the in-flight utterance on one channel, round-robin among channels that have one.
        //
        // Decoding both channels per tick does not fit: a full 25 s buffer decodes in ~585 ms on large-v3-turbo,
        // so two would exceed the 1 s tick and `interval`'s default `Burst` behavior would spin catching up.
        // `examples/partial_decode_cost.rs` re-measures this. If a model pushes the 25 s row past ~1 s, raise the interval, not shrink the buffer.
        // Two simultaneous speakers each refresh every 2 s. A single speaker still refreshes every 1 s because an idle channel's `pending()` is `None`.
        // The ticker loops forever, so tie its lifetime to this `start()` future.
        // Dropping a plain JoinHandle detaches, which would keep the multi-GB model alive and burn a decode per second after the meeting ended.
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
            // Skip the immediate first tick so the first decode fires after 1 s of audio.
            ticker.tick().await;
            let mut next_source = 0usize;
            loop {
                ticker.tick().await;

                // Probe each channel once from where the last tick left off.
                // `next_source` advances on every probe so two active channels alternate.
                let mut picked = None;
                for _ in 0..sources.len() {
                    let (seg, channel) = &sources[next_source];
                    next_source = (next_source + 1) % sources.len();
                    // Scoped so the std guard is released before the decode.
                    let snapshot = {
                        let seg = seg.lock().unwrap_or_else(|e| e.into_inner());
                        seg.pending()
                            // At least 1 s of audio is worth a decode.
                            .filter(|(pcm, _)| pcm.len() >= 16_000)
                            .map(|(pcm, start_ms)| (pcm.to_vec(), start_ms))
                    };
                    if let Some((pcm, start_ms)) = snapshot {
                        picked = Some((pcm, start_ms, *channel));
                        break;
                    }
                }
                // Both channels are between utterances, so skip the decode.
                let Some((pcm, start_ms, channel)) = picked else {
                    continue;
                };

                let ctx = ctx_partial.clone();
                // Must run on a blocking thread.
                let (text, confidence) = tokio::task::spawn_blocking(move || {
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
                    // The utterance's own start, so the timestamp does not jump when the final replaces the partial.
                    ts_ms: start_ms,
                    channel,
                    text,
                    is_final: false,
                    confidence,
                });
            }
        }));

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
                                // The decoder pump only cares about Segment; `match` makes a new variant a compile error.
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
                    // Debug level: a burst while the model loads is not actionable.
                    tracing::debug!(?n, "whisper_local audio rx lagged; dropping");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("audio channel closed; whisper_local exiting");
                    break;
                }
            }
        }

        drop(mic_seg_tx);
        drop(sys_seg_tx);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_local_is_object_safe() {
        fn _assert_dyn(_: &dyn Stt) {}
        // The assertion is on the trait bound, checked at compile time.
        let _f: fn(&dyn Stt) = _assert_dyn;
    }
}
