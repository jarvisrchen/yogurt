//! macOS system-audio loopback capture via `screencapturekit` 8.x
//! (Path A from the Plan 02-01 spike: PASS — in-process SCK is sufficient
//! for v1, the Swift sidecar fallback (Path B) is not needed).
//!
//! ## Pipeline
//!
//! `SCStream` (audio-only, 48 kHz f32 stereo) →
//! `SCStreamOutputTrait::did_output_sample_buffer` →
//! `audio_buffer_list()` (parallel L/R buffers, **not** interleaved) →
//! `Downmix` (mean-mix + resample to 16 kHz mono i16) →
//! `FrameChunker` (320-sample frames) →
//! `broadcast::Sender<Frame>`.
//!
//! ## Critical configuration (AUDIO-03)
//!
//! `with_excludes_current_process_audio(true)` is set from the first commit.
//! Without it, yogurt's own UI sounds (or any audio the binary plays back)
//! would loop back into the system capture stream, contaminating the
//! transcript. This is **load-bearing for privacy and correctness**.
//!
//! ## API quirks (from the Plan 02-01 spike note)
//!
//! 1. Audio-only `SCStream` still needs a valid video config: `width ≥ 2`,
//!    `height ≥ 2`. Omitting them produces an opaque error at
//!    `start_capture()`.
//! 2. Each callback's `audio_buffer_list()` yields **two parallel mono
//!    buffers** (L, R), not one interleaved buffer. Downmix iterates the
//!    parallel channels and averages sample-by-sample.
//! 3. Handlers must be `Fn + Send + Sync + 'static`. The
//!    `Arc<Mutex<(Downmix, FrameChunker)>>` wrapper satisfies this; cpal's
//!    mic path uses the same shape.

use crate::{
    error::{AudioError, Result},
    frame::{Channel, Frame},
    mic::FrameChunker,
};
use tokio::sync::broadcast;

/// Live system-audio capture handle. Dropping it stops the underlying
/// `SCStream` via RAII (D-26).
pub struct SystemCapture {
    #[cfg(target_os = "macos")]
    _inner: macos::Inner,
}

impl std::fmt::Debug for SystemCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemCapture")
            .finish_non_exhaustive()
    }
}

/// Spawn a system-audio loopback capture. Returns a [`SystemCapture`] whose
/// `Drop` impl stops the underlying SCK stream.
///
/// **Permission gate:** on macOS, returns [`AudioError::PermissionDenied`]
/// immediately if Screen Recording is not granted — the user never hears
/// a recording indicator if capture is going to fail (D-25). On non-macOS,
/// returns [`AudioError::UnsupportedPlatform`].
#[cfg(target_os = "macos")]
pub fn spawn_system_capture(tx: broadcast::Sender<Frame>) -> Result<SystemCapture> {
    let inner = macos::start(tx)?;
    Ok(SystemCapture { _inner: inner })
}

/// Non-macOS stub. System audio loopback requires `ScreenCaptureKit`.
#[cfg(not(target_os = "macos"))]
pub fn spawn_system_capture(_tx: broadcast::Sender<Frame>) -> Result<SystemCapture> {
    Err(AudioError::UnsupportedPlatform)
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use crate::permission::{has_screen_recording_permission, PermissionStatus};
    use crate::resample::Downmix;
    use screencapturekit::prelude::*;
    use std::sync::{Arc, Mutex};

    /// SCK output is fixed-format 48 kHz f32 stereo per the spike's
    /// empirical observation (and matches the `AudioSampleRate` /
    /// `AudioChannelCount` defaults).
    const SCK_INPUT_RATE: u32 = 48_000;
    const SCK_INPUT_CHANNELS: u16 = 2;

    /// Internal handle: holds the SCStream so Drop stops capture.
    ///
    /// `SCStream` itself owns the SCK dispatch-queue producer thread; its
    /// own Drop tears it down. We don't call `stop_capture()` explicitly —
    /// the spike confirmed this RAII shutdown is clean (no orphan threads).
    pub(super) struct Inner {
        _stream: SCStream,
    }

    impl std::fmt::Debug for Inner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Inner").finish_non_exhaustive()
        }
    }

    pub(super) fn start(tx: broadcast::Sender<Frame>) -> Result<Inner> {
        // 1. Permission gate (D-25). Fast-fail before we touch SCK.
        if has_screen_recording_permission() == PermissionStatus::Denied {
            return Err(AudioError::PermissionDenied);
        }

        // 2. Get shareable content + pick the first display.
        let content = SCShareableContent::get().map_err(|e| {
            AudioError::SystemCaptureFailed(format!("SCShareableContent::get(): {e}"))
        })?;
        let displays = content.displays();
        let display = displays.first().ok_or_else(|| {
            AudioError::SystemCaptureFailed("no displays available for SCK content filter".into())
        })?;

        // 3. Build content filter (display-rooted, no window exclusions).
        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();

        // 4. Build audio-only config.
        // - width/height must be ≥ 2 (API quirk #1).
        // - captures_audio = true: enable audio output.
        // - excludes_current_process_audio = true: AUDIO-03 load-bearing —
        //   prevents yogurt's own UI sounds from looping into capture.
        // - sample_rate = 48 000, channel_count = 2: let SCK do what it does
        //   best (raw capture); we resample with rubato per D-14/D-17.
        let config = SCStreamConfiguration::new()
            .with_width(2)
            .with_height(2)
            .with_captures_audio(true)
            .with_excludes_current_process_audio(true)
            .with_sample_rate(SCK_INPUT_RATE as i32)
            .with_channel_count(SCK_INPUT_CHANNELS as i32);

        // 5. Construct shared Downmix + FrameChunker.
        let downmix = Downmix::new(SCK_INPUT_RATE, SCK_INPUT_CHANNELS)?;
        let chunker = FrameChunker::new(Channel::System, tx);
        let state = Arc::new(Mutex::new((downmix, chunker)));

        // 6. Build stream + register audio handler.
        let mut stream = SCStream::new(&filter, &config);

        let state_for_handler = Arc::clone(&state);
        let handler =
            move |sample_buffer: CMSampleBuffer, of_type: SCStreamOutputType| {
                if of_type != SCStreamOutputType::Audio {
                    return;
                }
                // Pull the audio buffer list (parallel L/R buffers).
                let Some(abl) = sample_buffer.audio_buffer_list() else {
                    return;
                };

                // Collect each channel's f32 samples. SCK gives us 4-byte f32
                // samples per buffer; each buffer is ONE channel (parallel,
                // not interleaved — API quirk #2 from spike note).
                let mut per_channel: Vec<Vec<f32>> =
                    Vec::with_capacity(abl.num_buffers());
                for buf in abl.iter() {
                    let bytes = buf.data();
                    // Each f32 sample is 4 bytes, little-endian on Apple Silicon.
                    let sample_count = bytes.len() / 4;
                    let mut samples = Vec::with_capacity(sample_count);
                    for chunk in bytes.chunks_exact(4) {
                        let mut a = [0_u8; 4];
                        a.copy_from_slice(chunk);
                        samples.push(f32::from_le_bytes(a));
                    }
                    per_channel.push(samples);
                }
                if per_channel.is_empty() {
                    return;
                }

                // Interleave parallel channels into a single [L0, R0, L1, R1, …]
                // slice that Downmix::push can consume (it expects interleaved
                // input with `input_channels` items per frame).
                let frames = per_channel.iter().map(|c| c.len()).min().unwrap_or(0);
                let ch_count = per_channel.len();
                let mut interleaved = Vec::with_capacity(frames * ch_count);
                for i in 0..frames {
                    for ch in &per_channel {
                        interleaved.push(ch[i]);
                    }
                }

                if let Ok(mut guard) = state_for_handler.lock() {
                    let (dx, chunker) = &mut *guard;
                    let out = dx.push(&interleaved);
                    if !out.is_empty() {
                        chunker.feed(&out);
                    }
                }
            };

        let _handler_id = stream
            .add_output_handler(handler, SCStreamOutputType::Audio)
            .ok_or_else(|| {
                AudioError::SystemCaptureFailed(
                    "SCStream::add_output_handler returned None".into(),
                )
            })?;

        // 7. Start capture.
        stream
            .start_capture()
            .map_err(|e| AudioError::SystemCaptureFailed(format!("SCStream::start_capture: {e}")))?;

        tracing::info!(
            sample_rate = SCK_INPUT_RATE,
            channels = SCK_INPUT_CHANNELS,
            "system audio capture running"
        );

        Ok(Inner { _stream: stream })
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn it_returns_unsupported_platform_off_macos() {
        let (tx, _rx) = broadcast::channel::<Frame>(8);
        let err = spawn_system_capture(tx).unwrap_err();
        assert!(matches!(err, AudioError::UnsupportedPlatform));
    }
}
