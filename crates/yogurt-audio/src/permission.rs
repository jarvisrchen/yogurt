use serde::{Deserialize, Serialize};

/// `NotDetermined` (never asked) wants a "Grant" button, `Denied` wants "open System Settings".
/// Screen recording never yields `NotDetermined` because `CGPreflightScreenCaptureAccess` returns a bool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    NotRequired,
}

/// Detects Screen Recording permission without prompting.
pub fn has_screen_recording_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::check_screen()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

/// Fires the Screen Recording dialog if not yet granted.
/// The grant only takes effect after the process restarts, so the returned status is usually still `Denied`.
pub fn request_screen_recording_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::request_screen()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

/// Detects Microphone permission without prompting.
/// `Restricted` maps to `Denied` because the recovery path is the same.
pub fn has_microphone_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::check_microphone()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

/// Fires the Microphone dialog if not yet granted.
/// The completion handler is a no-op because the SPA polls `GET /api/audio/permission`.
/// Returns the current status, usually still `NotDetermined`.
pub fn request_microphone_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::request_microphone()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::PermissionStatus;

    // Plain Rust binaries do not link CoreGraphics by default, so library tests need this.
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn check_screen() -> PermissionStatus {
        // SAFETY: thread-safe C function with no arguments or preconditions.
        if unsafe { CGPreflightScreenCaptureAccess() } {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }

    pub fn request_screen() -> PermissionStatus {
        // SAFETY: same contract as `check_screen`.
        let granted = unsafe { CGRequestScreenCaptureAccess() };
        if granted {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }

    // The objc2 bindings are `unsafe fn` by default. `RcBlock` keeps the handler alive until the async callback finishes.
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    fn map_av_status(status: AVAuthorizationStatus) -> PermissionStatus {
        match status {
            AVAuthorizationStatus::Authorized => PermissionStatus::Granted,
            AVAuthorizationStatus::Denied => PermissionStatus::Denied,
            AVAuthorizationStatus::Restricted => PermissionStatus::Denied,
            AVAuthorizationStatus::NotDetermined => PermissionStatus::NotDetermined,
            // NSInteger newtype, so the compiler cannot prove exhaustiveness.
            _ => PermissionStatus::Denied,
        }
    }

    pub fn check_microphone() -> PermissionStatus {
        // SAFETY: `AVMediaTypeAudio` is an extern static set by the dynamic linker.
        // `authorizationStatusForMediaType:` is a thread-safe read of TCC state.
        let status = unsafe {
            let media_type = match AVMediaTypeAudio {
                Some(t) => t,
                // Unreachable in practice. NotDetermined shows the Grant button instead of the Denied recovery card.
                None => return PermissionStatus::NotDetermined,
            };
            AVCaptureDevice::authorizationStatusForMediaType(media_type)
        };
        map_av_status(status)
    }

    pub fn request_microphone() -> PermissionStatus {
        // SAFETY: see `check_microphone`.
        unsafe {
            let media_type = match AVMediaTypeAudio {
                Some(t) => t,
                None => return PermissionStatus::NotDetermined,
            };
            let handler = RcBlock::new(|_granted: Bool| {});
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
        }
        check_microphone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn it_returns_not_required_on_non_macos() {
        assert_eq!(
            has_screen_recording_permission(),
            PermissionStatus::NotRequired
        );
        assert_eq!(
            request_screen_recording_permission(),
            PermissionStatus::NotRequired
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn it_returns_granted_or_denied_on_macos() {
        let status = has_screen_recording_permission();
        assert!(
            matches!(
                status,
                PermissionStatus::Granted
                    | PermissionStatus::Denied
                    | PermissionStatus::NotDetermined
            ),
            "macOS should never return NotRequired, got {status:?}"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn it_returns_known_microphone_status_on_macos() {
        let status = has_microphone_permission();
        assert!(
            matches!(
                status,
                PermissionStatus::Granted
                    | PermissionStatus::Denied
                    | PermissionStatus::NotDetermined
            ),
            "macOS mic check should never return NotRequired, got {status:?}"
        );
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn it_returns_not_required_on_non_macos_microphone() {
        assert_eq!(has_microphone_permission(), PermissionStatus::NotRequired);
        assert_eq!(
            request_microphone_permission(),
            PermissionStatus::NotRequired
        );
    }
}
