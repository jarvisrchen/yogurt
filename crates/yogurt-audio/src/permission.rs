//! macOS Screen Recording permission detection.
//!
//! Backed by the public CoreGraphics `CGPreflightScreenCaptureAccess` /
//! `CGRequestScreenCaptureAccess` C functions (stable on macOS 10.15+).
//! CoreGraphics.framework is auto-linked into every macOS binary — no
//! extra `#[link]` attribute is required.

use serde::{Deserialize, Serialize};

/// Screen Recording permission state — surfaced to the UI for the §5.11
/// recovery flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    /// Granted. System audio capture will work.
    Granted,
    /// Explicitly denied or never asked. UI should show the §5.11 recovery card.
    Denied,
    /// Not applicable on this platform (non-macOS). Mic capture still works.
    NotRequired,
}

/// Detect Screen Recording permission **without** prompting.
///
/// On macOS, calls `CGPreflightScreenCaptureAccess()`. On other platforms,
/// returns [`PermissionStatus::NotRequired`].
///
/// Call this from the UI on app boot to decide whether to render the
/// §5.10 onboarding step or the §5.11 recovery card.
pub fn has_screen_recording_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::check()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

/// Trigger the macOS Screen Recording permission dialog if permission has
/// not yet been granted. On non-macOS, no-op.
///
/// **TCC limitation:** the binary must be restarted after the user grants
/// permission before the grant takes effect. After calling this, surface
/// PRD §5.10's "Restart once after granting — a macOS quirk, not us"
/// copy in the UI.
///
/// The bool returned by `CGRequestScreenCaptureAccess` reflects the *current*
/// state at the moment of the call (usually still `false` immediately after
/// the dialog fires — the user hasn't clicked yet). The post-grant grant is
/// observable only after the next process launch.
pub fn request_screen_recording_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::request()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::PermissionStatus;

    // CoreGraphics functions for TCC screen-recording status. These are stable
    // public API on macOS 10.15+. CoreGraphics.framework is part of the
    // ApplicationServices umbrella framework; it is NOT linked by default
    // into a plain Rust binary — the `#[link]` attribute below makes the
    // linker pull it in. (Once `yogurt-server` or `yogurt-cli` transitively
    // links anything that depends on AppKit / SCK, the framework would be
    // pulled in anyway, but our library tests must declare it themselves.)
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn check() -> PermissionStatus {
        // SAFETY: `CGPreflightScreenCaptureAccess` is a thread-safe C
        // function with no arguments; it returns a bool and has no
        // preconditions.
        if unsafe { CGPreflightScreenCaptureAccess() } {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }

    pub fn request() -> PermissionStatus {
        // SAFETY: same contract as `check`. This call may trigger the system
        // dialog and returns immediately with whatever the *current* (likely
        // still pending) state is — the actual grant arrives after user
        // interaction + relaunch.
        let granted = unsafe { CGRequestScreenCaptureAccess() };
        if granted {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
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
        // Can't assert which — depends on the user's TCC state. Just assert
        // it doesn't panic and returns one of the two valid macOS variants.
        let status = has_screen_recording_permission();
        assert!(
            matches!(status, PermissionStatus::Granted | PermissionStatus::Denied),
            "macOS should never return NotRequired, got {status:?}"
        );
    }
}
