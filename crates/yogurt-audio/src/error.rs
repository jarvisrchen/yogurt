use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("macOS Screen Recording permission is required for system audio capture")]
    PermissionDenied,

    #[error("microphone device unavailable: {0}")]
    MicUnavailable(String),

    #[error("output device unavailable: {0}")]
    OutputUnavailable(String),

    #[error("system audio capture failed to start: {0}")]
    SystemCaptureFailed(String),

    #[error("another yogurt process is already recording")]
    AlreadyRecording,

    /// The marker's PID is dead but the OS may still be reclaiming its SCK/mic
    /// resources, so we fail fast instead of hanging inside SCK.
    #[error(
        "a previous recording session ended abnormally (force-killed) and macOS may still be \
         reclaiming its audio resources; wait a minute or two and try again"
    )]
    OrphanedSession,

    #[error("system audio capture is only supported on macOS 13+")]
    UnsupportedPlatform,

    #[error("cpal error: {0}")]
    Cpal(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, AudioError>;
