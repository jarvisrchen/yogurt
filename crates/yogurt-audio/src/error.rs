//! Typed audio errors. Each variant maps to a distinct user-facing recovery
//! in the Phase 7 onboarding / §5.11 plan — keep them distinguishable.

use thiserror::Error;

/// All errors `yogurt-audio` can surface.
#[derive(Debug, Error)]
pub enum AudioError {
    /// macOS Screen Recording permission has not been granted. Phase 7 renders
    /// this as the §5.11 "Yogurt can't hear the call yet" recovery card.
    #[error("macOS Screen Recording permission is required for system audio capture")]
    PermissionDenied,

    /// The selected microphone device disappeared (unplugged, switched).
    #[error("microphone device unavailable: {0}")]
    MicUnavailable(String),

    /// SCK refused to start — usually a transient OS-level issue.
    #[error("system audio capture failed to start: {0}")]
    SystemCaptureFailed(String),

    /// We're not on macOS. Mic still works; system loopback does not.
    #[error("system audio capture is only supported on macOS 13+")]
    UnsupportedPlatform,

    /// Wrapped cpal error (from device enumeration / stream setup).
    #[error("cpal error: {0}")]
    Cpal(String),

    /// Wrapped IO error (sidecar stdout reads, etc.).
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience `Result` alias used throughout the crate.
pub type Result<T> = std::result::Result<T, AudioError>;
