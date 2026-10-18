//! Lock-free SPSC ring between the real-time audio callback and the tokio drainer task.
//! `broadcast::Sender::send` takes an internal mutex that can block the callback, so the callback pushes
//! pre-downmixed mono f32 at the source rate and drops samples on overflow instead of blocking.

use crate::frame::SAMPLE_RATE_HZ;
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// About 1.3 s at 48 kHz, so scheduler stalls in the drainer do not overflow under nominal load.
pub(crate) const RING_CAPACITY: usize = 64_000;

/// Never blocks; overflow increments [`RingStats::dropped_samples`].
pub(crate) struct AudioRingProducer {
    inner: Producer<f32>,
    stats: Arc<RingStats>,
}

impl AudioRingProducer {
    /// Drops the trailing samples when the ring is full.
    pub(crate) fn push_slice(&mut self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
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

pub(crate) struct AudioRingConsumer {
    inner: Consumer<f32>,
    stats: Arc<RingStats>,
}

impl AudioRingConsumer {
    /// Appends all available samples to `out` and returns the count.
    pub(crate) fn drain_to(&mut self, out: &mut Vec<f32>) -> usize {
        let available = self.inner.slots();
        if available == 0 {
            return 0;
        }
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

    pub(crate) fn dropped_samples(&self) -> u64 {
        self.stats.dropped_samples.load(Ordering::Relaxed)
    }

    pub(crate) fn pop(&mut self) -> Option<f32> {
        self.inner.pop().ok()
    }
}

#[derive(Debug, Default)]
pub(crate) struct RingStats {
    /// Non-zero means the drainer is falling behind.
    pub(crate) dropped_samples: AtomicU64,
}

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

/// One frame period. A slower wake just buffers more samples in the ring.
pub(crate) const DRAINER_TICK: std::time::Duration = std::time::Duration::from_millis(20);

/// 20 ms at 48 kHz is 960 samples, padded 4x for slow wakes.
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
        let oversized: Vec<f32> = vec![0.42; RING_CAPACITY + 5_000];
        tx.push_slice(&oversized);
        let mut out: Vec<f32> = Vec::with_capacity(RING_CAPACITY);
        let drained = rx.drain_to(&mut out);
        assert_eq!(drained, RING_CAPACITY);
        assert_eq!(rx.dropped_samples(), 5_000);
    }

    #[test]
    fn it_does_not_increment_dropped_counter_under_nominal_flow() {
        let (mut tx, mut rx) = ring();
        for _ in 0..50 {
            let chunk: Vec<f32> = vec![1.0; 480];
            tx.push_slice(&chunk);
            let mut out: Vec<f32> = Vec::new();
            rx.drain_to(&mut out);
        }
        assert_eq!(rx.dropped_samples(), 0);
    }

    #[test]
    fn audio_callback_push_does_not_block_under_full_ring() {
        // A full ring must make push_slice return promptly, not block.
        let (mut tx, _rx) = ring();
        let bulk: Vec<f32> = vec![0.0; RING_CAPACITY];
        tx.push_slice(&bulk);

        let probe: Vec<f32> = vec![0.0; 960];
        let t0 = std::time::Instant::now();
        tx.push_slice(&probe);
        let elapsed = t0.elapsed();
        assert!(
            elapsed < std::time::Duration::from_micros(100),
            "push_slice on full ring took {elapsed:?}, expected < 100µs (real-time-safety regression)"
        );
    }
}
