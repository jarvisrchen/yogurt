//! Yogurt audio capture — 16 kHz mono i16 PCM, two channels (mic + system).
//!
//! Phase 2 scope: capture only. STT consumption is Phase 3.
//!
//! # Format contract
//!
//! Every [`Frame`] emitted by this crate is **16 kHz mono i16 PCM, 320 samples
//! (20 ms) per frame**. Phase 3 STT engines (Deepgram, whisper.cpp) consume
//! this format directly — never resample at the STT boundary. See the
//! crate-level [`README.md`](https://github.com/jarvisrchen/yogurt/blob/main/crates/yogurt-audio/README.md)
//! for the full contract.
//!
//! # Platform support
//!
//! - macOS 13+ (full surface): mic + system loopback.
//! - Other platforms: mic only via `cpal`; [`Channel::System`] returns
//!   [`AudioError::UnsupportedPlatform`].

#![deny(rust_2018_idioms, missing_debug_implementations)]

pub mod detect;
mod echo;
mod error;
mod frame;
#[cfg(target_os = "macos")]
mod lock;
mod mic;
pub mod permission;
mod resample;
mod ring;
mod system;

#[cfg(any(test, feature = "synthetic"))]
pub mod synthetic;

pub use echo::{play_test_tone, MicEcho};
pub use error::{AudioError, Result};
pub use frame::{Channel, Frame, FRAME_SAMPLES, SAMPLE_RATE_HZ};
pub use mic::{list_input_devices, list_output_devices, spawn_mic_capture, DeviceInfo, MicCapture};
pub use permission::{
    has_microphone_permission, has_screen_recording_permission, request_microphone_permission,
    request_screen_recording_permission, PermissionStatus,
};
pub use system::{spawn_system_capture, SystemCapture};

use tokio::sync::broadcast;

/// Per-channel broadcast capacity (AUDIO-04: ≥ 256 frames). 256 frames ×
/// 20 ms/frame ≈ 5 seconds of buffered audio per channel before a slow
/// consumer starts seeing `Lagged(n)` from `Receiver::recv()` and dropping
/// late frames (correct STT semantics — wall-clock-bound, not retry-bound).
pub const BROADCAST_CAPACITY: usize = 256;

/// Live capture handle. Holds both producers as owned fields — dropping
/// `AudioStream` stops the cpal mic stream and the SCK system stream via
/// RAII (D-26 — no explicit `.stop()` API, no leaked SCK handles, no
/// orphan tasks).
///
/// ## Subscribing
///
/// Phase 3 fans both receivers in via `tokio::select!` (D-20). Subscribe
/// after `start_capture()` returns:
///
/// ```no_run
/// # async fn ex() -> yogurt_audio::Result<()> {
/// let stream = yogurt_audio::start_capture(None)?;
/// let mut mic_rx = stream.subscribe_mic();
/// let mut sys_rx = stream.subscribe_system();
/// loop {
///     tokio::select! {
///         Ok(frame) = mic_rx.recv() => { /* STT route to "Me" */ }
///         Ok(frame) = sys_rx.recv() => { /* STT route to "Them" */ }
///     }
/// }
/// # }
/// ```
pub struct AudioStream {
    _mic: MicCapture,
    _system: SystemCapture,
    /// Cleared on drop so the next `start_capture()` doesn't mistake a
    /// clean shutdown for an orphaned session (see `lock` module).
    #[cfg(target_os = "macos")]
    _capture_lock: lock::CaptureLock,
    /// Public so consumers can clone/subscribe and so `Channel::Mic` STT
    /// sessions can register late (after `start_capture` returns).
    pub mic_tx: broadcast::Sender<Frame>,
    /// Public for the same reason — Phase 3 system-channel consumers.
    pub system_tx: broadcast::Sender<Frame>,
    /// `Some` while the mic is being echoed to an output device.
    echo: Option<MicEcho>,
    /// The output device the caller last asked for (`""` = system default),
    /// remembered so `switch_mic_device` can reattach echo.
    echo_requested_device: String,
    /// The buffer size (frames) the caller last asked for, reused on reattach.
    echo_buffer: u32,
}

impl std::fmt::Debug for AudioStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioStream")
            .field("mic_receivers", &self.mic_tx.receiver_count())
            .field("system_receivers", &self.system_tx.receiver_count())
            .finish_non_exhaustive()
    }
}

impl AudioStream {
    /// Subscribe a new consumer to the mic channel. Each subscription gets
    /// its own backpressure window; slow consumers drop frames via
    /// `RecvError::Lagged` without affecting other subscribers (D-19).
    pub fn subscribe_mic(&self) -> broadcast::Receiver<Frame> {
        self.mic_tx.subscribe()
    }

    /// Subscribe a new consumer to the system audio channel.
    pub fn subscribe_system(&self) -> broadcast::Receiver<Frame> {
        self.system_tx.subscribe()
    }

    /// Hot-swap the mic producer in place, leaving `mic_tx`, system audio,
    /// and all existing broadcast subscribers untouched.
    ///
    /// Spawns the replacement capture FIRST and only swaps `_mic` on
    /// success — on error, the previous device keeps capturing and the
    /// error is returned to the caller, so a bad device id never
    /// interrupts a live recording.
    pub fn switch_mic_device(&mut self, device_name: Option<&str>) -> Result<String> {
        let new_mic = spawn_mic_capture(self.mic_tx.clone(), device_name)?;
        let resolved = new_mic.device_name.clone();
        let was_echoing = self.echo.is_some();
        self.echo = None;
        self._mic = new_mic;

        if was_echoing {
            let requested = self.echo_requested_device.clone();
            let buffer = self.echo_buffer;
            let dev = Some(requested.as_str()).filter(|s| !s.is_empty());
            if let Err(e) = self.open_echo(dev, buffer) {
                tracing::warn!(
                    error = %e,
                    "failed to reattach mic echo after device switch; echo disabled"
                );
            }
        }
        Ok(resolved)
    }

    /// AUD-6: pause (`true`) or resume (`false`) mic audio reaching the STT
    /// pipeline. `Channel::System` is untouched.
    ///
    /// ponytail: resets to unmuted after `switch_mic_device` hot-swaps in a
    /// fresh `MicCapture` — muting and switching devices in the same beat
    /// is a rare, low-stakes combo, not worth threading the flag through.
    pub fn set_mic_muted(&self, muted: bool) {
        self._mic.set_muted(muted);
    }

    /// Start (or hot-swap) echoing the mic to `device` (`None`/`""` =
    /// system default output), buffered at `buffer` frames. Returns the
    /// resolved device name.
    pub fn start_echo(&mut self, device: Option<&str>, buffer: u32) -> Result<String> {
        self.open_echo(device, buffer)
    }

    /// Shared by `start_echo` and `switch_mic_device`'s reattach path.
    fn open_echo(&mut self, device: Option<&str>, buffer: u32) -> Result<String> {
        self.echo_requested_device = device.unwrap_or("").to_string();
        self.echo_buffer = buffer;

        // Stop before start so two output streams never overlap on the same device.
        self._mic.set_echo_active(false);
        self.echo = None;

        let consumer = self._mic.echo_consumer_handle();
        let muted = self._mic.muted_handle();
        let echo = MicEcho::start(device, buffer, self._mic.sample_rate, muted, consumer)?;
        let name = echo.device_name().to_string();
        self._mic.set_echo_active(true);
        self.echo = Some(echo);
        Ok(name)
    }

    /// Stop echoing. Idempotent.
    pub fn stop_echo(&mut self) {
        self._mic.set_echo_active(false);
        self.echo = None;
    }

    /// The device currently receiving the echo, if any.
    pub fn echo_device(&self) -> Option<String> {
        self.echo.as_ref().map(|e| e.device_name().to_string())
    }
}

/// Open both capture streams (mic + system audio) and return an
/// [`AudioStream`] handle. Dropping the handle stops both streams.
///
/// **Permission gate (D-25):** fast-fails with
/// [`AudioError::PermissionDenied`] *before* opening the microphone if
/// Screen Recording is denied. This prevents the user from seeing a mic
/// recording indicator when system capture is going to fail anyway —
/// the §5.11 recovery card should render instead.
///
/// **Meeting-relative clock (AUDIO-05):** each producer's `FrameChunker`
/// captures `Instant::now()` at construction time. Both chunkers are
/// instantiated synchronously inside this function (the spawn calls are
/// sequential and quick — well under a millisecond), so drift between
/// mic and system channel `monotonic_ms` baselines is bounded by spawn-
/// order skew (microseconds). The AUDIO-05 < 50 ms drift requirement is
/// trivially met.
///
/// **Broadcast capacity (AUDIO-04):** both channels are created with
/// [`BROADCAST_CAPACITY`] = 256 frames (~5 seconds).
pub fn start_capture(mic_device: Option<&str>) -> Result<AudioStream> {
    if has_screen_recording_permission() == PermissionStatus::Denied {
        return Err(AudioError::PermissionDenied);
    }

    #[cfg(target_os = "macos")]
    let _capture_lock = lock::acquire()?;

    let (mic_tx, _) = broadcast::channel::<Frame>(BROADCAST_CAPACITY);
    let (system_tx, _) = broadcast::channel::<Frame>(BROADCAST_CAPACITY);

    // CR-03: open SCK FIRST. The mic is opened second, after SCK has
    // successfully started, so a permission-denied or transient SCK
    // failure never flashes the macOS orange microphone indicator. The
    // old order (mic-first) violated D-25 ("the user never hears a
    // recording indicator if system capture is going to fail") because
    // a brief mic-open → SCK-fail → mic-drop sequence still toggles
    // the indicator. SCK is the slow / failure-prone resource; we want
    // it opened first so the cheap, safe mic open never runs when SCK
    // is going to fail anyway.
    let _system = spawn_system_capture(system_tx.clone())?;
    let _mic = spawn_mic_capture(mic_tx.clone(), mic_device)?;

    Ok(AudioStream {
        _mic,
        _system,
        #[cfg(target_os = "macos")]
        _capture_lock,
        mic_tx,
        system_tx,
        echo: None,
        echo_requested_device: String::new(),
        echo_buffer: 512,
    })
}
