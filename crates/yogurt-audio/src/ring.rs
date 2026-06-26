//! Lock-free SPSC ring between the real-time audio callback and the tokio
//! drainer task that owns rubato + `broadcast::Sender<Frame>`.
//!
//! ## Why this exists (BL-01 / BL-03)
//!
//! cpal's CoreAudio IOProc and SCK's audio-delegate are **real-time threads**:
//! macOS's audio HAL refuses to deliver the next ~10 ms buffer if the previous
//! callback misses its deadline. The original Phase 2 code called
//! `tokio::sync::broadcast::Sender::send()` directly from the callback —
//! `send()` is not async but acquires the channel's internal tail mutex,
//! which can block under multi-subscriber contention.
//!
//! The fix is a lock-free single-producer / single-consumer ring (`rtrb`):
//! - **Producer side** (audio thread): `push_slice` is wait-free as long as
//!   the ring has room. On overflow we increment a `dropped` counter rather
//!   than block.
//! - **Consumer side** (drainer): a dedicated tokio task wakes every 20 ms
//!   (one frame period), drains everything the ring contains, hands the
//!   slice to a [`SampleSink`], and may safely block on
//!   `broadcast::Sender::send`.
//!
//! ## What flows through the ring
//!
//! Pre-downmixed **mono f32 samples at the source rate** (48 kHz typically).
//! The audio thread does the cheap work — bytes → f32, channel mean — and
//! pushes mono samples. The drainer does the expensive work — rubato sinc
//! resample 48 kHz → 16 kHz, i16 quantize, frame chunking, broadcast.
//!
//! This is what makes BL-03's mutex go away entirely: the resampler now
//! lives on the drainer task and has no concurrent callers, so no
//! synchronization is needed.

use crate::frame::SAMPLE_RATE_HZ;
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// Ring capacity in mono f32 samples. Sized to ≈ 1.3 s at 48 kHz so the
/// drainer task's worst-case wake latency (tokio scheduler stalls, GC
/// pauses on a busy machine) cannot make the producer overflow under
/// nominal load. 48 000 × 1.3 ≈ 64 000.
pub(crate) const RING_CAPACITY: usize = 64_000;

/// Producer half — owned by the audio thread. Never blocks; on overflow
/// it increments [`RingStats::dropped_samples`].
pub(crate) struct AudioRingProducer {
    inner: Producer<f32>,
    stats: Arc<RingStats>,
}

impl AudioRingProducer {
    /// Push a contiguous mono f32 slice. Returns the count actually written.
    /// On a full ring, the trailing samples are dropped and the
    /// `dropped_samples` counter is incremented; this is the intended
    /// real-time-safe behaviour (per CoreAudio's no-blocking contract).
    pub(crate) fn push_slice(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        // `write_chunk_uninit` lets us push without a Vec allocation. Its
        // return is `Ok(write_chunk)` carrying the count we may write — at
        // most `min(samples.len(), slots_available())`. Anything past that
        // is intentionally dropped.
        match self.inner.write_chunk_uninit(samples.len()) {
            Ok(chunk) => {
                let n_written = chunk.fill_from_iter(samples.iter().copied());
                let n_dropped = samples.len().saturating_sub(n_written);
                if n_dropped > 0 {
                    self.stats
                        .dropped_samples
                        .fetch_add(n_dropped as u64, Ordering::Relaxed);
                }
            }
            Err(rtrb::chunks::ChunkError::TooFewSlots(slots)) => {
                // Partial write: take what we can, drop the rest.
                if slots == 0 {
                    self.stats
                        .dropped_samples
                        .fetch_add(samples.len() as u64, Ordering::Relaxed);
                    return;
                }
                if let Ok(chunk) = self.inner.write_chunk_uninit(slots) {
                    let n_written = chunk.fill_from_iter(samples.iter().take(slots).copied());
                    let n_dropped = samples.len().saturating_sub(n_written);
                    self.stats
                        .dropped_samples
                        .fetch_add(n_dropped as u64, Ordering::Relaxed);
                }
            }
        }
    }
}

/// Consumer half — owned by the drainer task. Drained in bulk via
/// [`Self::drain_to`].
pub(crate) struct AudioRingConsumer {
    inner: Consumer<f32>,
    stats: Arc<RingStats>,
}

impl AudioRingConsumer {
    /// Drain all currently-available samples into `out` (appended).
    /// Returns the count drained.
    pub(crate) fn drain_to(&mut self, out: &mut Vec<f32>) -> usize {
        let available = self.inner.slots();
        if available == 0 {
            return 0;
        }
        // `read_chunk` is the bulk pop API; iterate it directly to avoid
        // an intermediate Vec allocation.
        match self.inner.read_chunk(available) {
            Ok(chunk) => {
                let (first, second) = chunk.as_slices();
                out.extend_from_slice(first);
                out.extend_from_slice(second);
                let n = first.len() + second.len();
                chunk.commit_all();
                n
            }
            Err(_) => 0,
        }
    }

    /// Read the dropped-sample counter without resetting it. The drainer
    /// uses this to emit a `tracing::warn!` when overflow occurred.
    pub(crate) fn dropped_samples(&self) -> u64 {
        self.stats.dropped_samples.load(Ordering::Relaxed)
    }
}

/// Shared overflow counter. Producer increments, consumer reads.
#[derive(Debug, Default)]
pub(crate) struct RingStats {
    /// Samples the producer had to drop because the ring was full.
    /// Non-zero = the drainer is falling behind, which usually means the
    /// tokio runtime is stalled or a broadcast subscriber is mis-sized.
    pub(crate) dropped_samples: AtomicU64,
}

/// Build the producer/consumer pair plus the shared stats handle.
pub(crate) fn ring() -> (AudioRingProducer, AudioRingConsumer) {
    let (tx, rx) = RingBuffer::<f32>::new(RING_CAPACITY);
    let stats = Arc::new(RingStats::default());
    (
        AudioRingProducer {
            inner: tx,
            stats: Arc::clone(&stats),
        },
        AudioRingConsumer { inner: rx, stats },
    )
}

/// How often the drainer wakes to consume samples and forward them to the
/// broadcast channel. One frame period (20 ms) keeps end-to-end latency
/// tight; on a slower wake the ring just buffers more samples.
pub(crate) const DRAINER_TICK: std::time::Duration = std::time::Duration::from_millis(20);

/// Maximum samples we expect to see per drainer tick (20 ms × 48 kHz = 960
/// samples per channel; pad 4× for slow wakes / scheduler delays).
pub(crate) const DRAINER_SCRATCH_HINT: usize = (SAMPLE_RATE_HZ as usize * 4 * 4) / 50;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_round_trips_samples() {
        let (mut tx, mut rx) = ring();
        let input: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        tx.push_slice(&input);
        let mut out: Vec<f32> = Vec::new();
        let n = rx.drain_to(&mut out);
        assert_eq!(n, input.len());
        assert_eq!(out, input);
    }

    #[test]
    fn it_increments_dropped_counter_on_overflow() {
        let (mut tx, mut rx) = ring();
        // Push WAY more than capacity in one shot.
        let oversized: Vec<f32> = vec![0.42; RING_CAPACITY + 5_000];
        tx.push_slice(&oversized);
        // Drain whatever fit.
        let mut out: Vec<f32> = Vec::with_capacity(RING_CAPACITY);
        let drained = rx.drain_to(&mut out);
        assert_eq!(drained, RING_CAPACITY);
        // The overage must be reflected in the dropped counter.
        assert_eq!(rx.dropped_samples(), 5_000);
    }

    #[test]
    fn it_does_not_increment_dropped_counter_under_nominal_flow() {
        let (mut tx, mut rx) = ring();
        // Push small chunks and drain between pushes — should never overflow.
        for _ in 0..50 {
            let chunk: Vec<f32> = vec![1.0; 480]; // one rubato chunk
            tx.push_slice(&chunk);
            let mut out: Vec<f32> = Vec::new();
            rx.drain_to(&mut out);
        }
        assert_eq!(rx.dropped_samples(), 0);
    }

    #[test]
    fn audio_callback_push_does_not_block_under_full_ring() {
        // Producer-side proxy for "audio thread is real-time-safe". Fill
        // the ring to capacity, then time a push of one frame's worth of
        // samples. push_slice must return promptly (well under 100µs)
        // even when the ring is full — it just bumps the dropped counter.
        let (mut tx, _rx) = ring();
        let bulk: Vec<f32> = vec![0.0; RING_CAPACITY];
        tx.push_slice(&bulk); // fills the ring

        let probe: Vec<f32> = vec![0.0; 960];
        let t0 = std::time::Instant::now();
        tx.push_slice(&probe); // ring is full → all 960 dropped
        let elapsed = t0.elapsed();
        assert!(
            elapsed < std::time::Duration::from_micros(100),
            "push_slice on full ring took {elapsed:?}, expected < 100µs (real-time-safety regression)"
        );
    }
}
