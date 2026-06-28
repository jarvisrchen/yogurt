//! macOS Screen Recording **and** Microphone permission detection.
//!
//! Screen-recording detection is backed by the public CoreGraphics
//! `CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess` C
//! functions (stable on macOS 10.15+). CoreGraphics.framework is auto-linked
//! into every macOS binary — no extra `#[link]` attribute is required.
//!
//! Microphone detection is backed by `AVCaptureDevice.authorizationStatus(for:)`
//! / `AVCaptureDevice.requestAccess(for:completionHandler:)`, called via the
//! `objc2` + `objc2-av-foundation` + `block2` typed bindings (DD-01 of
//! `260628-g71-PLAN.md`). Unlike CoreGraphics, AVFoundation is an
//! Objective-C class API with no C-callable shim, so a typed binding is the
//! idiomatic Rust path.

use serde::{Deserialize, Serialize};

/// macOS TCC permission state — surfaced to the UI for the §5.10 onboarding
/// and §5.11 recovery flows.
///
/// `NotDetermined` represents "the user has never been asked yet" — distinct
/// from `Denied` ("the user explicitly said no, must go through System
/// Settings to recover"). The two states want different UI: NotDetermined →
/// "click Grant"; Denied → "open System Settings" (per DD-02).
///
/// The screen-recording path never produces `NotDetermined` today —
/// `CGPreflightScreenCaptureAccess` only returns a bool — and that's fine;
/// the enum is a single shared wire-shape used by both microphone and
/// screen-recording responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    /// Granted. Audio capture will work.
    Granted,
    /// Explicitly denied. UI should show the §5.11 recovery card with a
    /// `x-apple.systempreferences:` deep link to the relevant Privacy pane.
    Denied,
    /// The user has not yet been asked. Only produced by the microphone
    /// path — the UI should show a "Grant" button that calls the
    /// corresponding `request_*` function, which fires the macOS system
    /// dialog asynchronously.
    NotDetermined,
    /// Not applicable on this platform (non-macOS). Mic capture still
    /// works; screen capture is unsupported entirely.
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
        macos::check_screen()
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
        macos::request_screen()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

/// Detect Microphone permission **without** prompting.
///
/// On macOS, calls `AVCaptureDevice.authorizationStatus(for: .audio)` and
/// maps the four `AVAuthorizationStatus` variants:
///
/// | AVAuthorizationStatus | PermissionStatus |
/// |-----------------------|------------------|
/// | `Authorized` (3)      | `Granted`        |
/// | `Denied` (2)          | `Denied`         |
/// | `Restricted` (1)      | `Denied`         |
/// | `NotDetermined` (0)   | `NotDetermined`  |
///
/// `Restricted` collapses into `Denied` because the recovery UX is the
/// same (user must change settings — for `Restricted`, that often means
/// the device owner / MDM admin, but we can't tell which from this
/// vantage). Non-macOS returns [`PermissionStatus::NotRequired`].
///
/// Used by `<Welcome />` Step 2 (Microphone) to decide between "Grant
/// Microphone" (NotDetermined) and "Open System Settings" (Denied), and by
/// `<MicPermissionDenied />` on the Library route to render the recovery
/// card.
///
/// Never panics. Thread-safe.
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

/// Trigger the macOS Microphone permission dialog if permission has not
/// yet been granted. On non-macOS, no-op.
///
/// **Fire-and-forget semantics (mirrors `request_screen_recording_permission`):**
/// `AVCaptureDevice.requestAccess(for:completionHandler:)` returns
/// immediately and the completion handler fires asynchronously after the
/// user dismisses the dialog. We pass a no-op completion handler — the
/// SPA's 2-second polling loop on `GET /api/audio/permission` picks up
/// the new state on the next tick.
///
/// Idempotent: calling when already `Granted` is a cheap no-op. Calling
/// when already `Denied` likewise returns immediately without re-prompting
/// the user (macOS only ever asks the user once per app+TCC entry — after
/// that, the user must go through System Settings).
///
/// Returns the *current* permission state (likely still `NotDetermined`
/// immediately after firing the dialog — the user hasn't clicked Allow or
/// Deny yet).
///
/// Never panics. Thread-safe.
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

    pub fn check_screen() -> PermissionStatus {
        // SAFETY: `CGPreflightScreenCaptureAccess` is a thread-safe C
        // function with no arguments; it returns a bool and has no
        // preconditions.
        if unsafe { CGPreflightScreenCaptureAccess() } {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }

    pub fn request_screen() -> PermissionStatus {
        // SAFETY: same contract as `check_screen`. This call may trigger
        // the system dialog and returns immediately with whatever the
        // *current* (likely still pending) state is — the actual grant
        // arrives after user interaction + relaunch.
        let granted = unsafe { CGRequestScreenCaptureAccess() };
        if granted {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }

    // ── Microphone (AVFoundation) ──────────────────────────────────────
    //
    // The two functions below call Objective-C class methods on
    // `AVCaptureDevice`. They are wrapped in `unsafe { … }` because the
    // objc2 method bindings carry `unsafe fn` signatures by default
    // (Objective-C makes no static-safety guarantees the Rust borrow
    // checker can see). Both calls are safe to invoke in practice:
    //
    //   * `authorizationStatusForMediaType:` is a const, idempotent
    //     class-method query that reads TCC state.
    //   * `requestAccessForMediaType:completionHandler:` is the
    //     documented entry point for triggering the system permission
    //     dialog. The completion-handler block is owned by `RcBlock`
    //     and dropped only after Apple's runtime releases its last
    //     reference, so the block survives the async callback.
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

    fn map_av_status(status: AVAuthorizationStatus) -> PermissionStatus {
        // Use the named constants rather than literal integers — same
        // mapping, but the intent is self-documenting and survives any
        // (extremely unlikely) future AVFoundation re-numbering.
        match status {
            AVAuthorizationStatus::Authorized => PermissionStatus::Granted,
            AVAuthorizationStatus::Denied => PermissionStatus::Denied,
            AVAuthorizationStatus::Restricted => PermissionStatus::Denied,
            AVAuthorizationStatus::NotDetermined => PermissionStatus::NotDetermined,
            // AVAuthorizationStatus is an NSInteger newtype, so the
            // compiler can't prove exhaustiveness; future macOS could
            // theoretically add a variant. Treat anything unknown as
            // Denied — the recovery UX is the safe default.
            _ => PermissionStatus::Denied,
        }
    }

    pub fn check_microphone() -> PermissionStatus {
        // SAFETY: `AVMediaTypeAudio` is an extern static initialized by
        // the dynamic linker when AVFoundation.framework loads — we
        // dereference its `Option<&'static AVMediaType>` defensively.
        // `authorizationStatusForMediaType:` is a thread-safe class
        // method with no side effects beyond a read of TCC state.
        let status = unsafe {
            let media_type = match AVMediaTypeAudio {
                Some(t) => t,
                // Should be unreachable on macOS — the symbol is part of
                // AVFoundation and the framework is linked at process
                // start. If somehow nil, treat as NotDetermined so the UI
                // shows the Grant button rather than the harsher
                // "Denied → open System Settings" recovery card.
                None => return PermissionStatus::NotDetermined,
            };
            AVCaptureDevice::authorizationStatusForMediaType(media_type)
        };
        map_av_status(status)
    }

    pub fn request_microphone() -> PermissionStatus {
        // SAFETY: see check_microphone for the AVMediaTypeAudio dance.
        // The completion handler is a no-op `RcBlock<dyn Fn(Bool)>` —
        // its only contract is "accept a bool and return". Apple's
        // runtime owns the lifetime of the block for the duration of
        // the async callback (the block2 crate's RcBlock makes the
        // refcounting transparent).
        unsafe {
            let media_type = match AVMediaTypeAudio {
                Some(t) => t,
                None => return PermissionStatus::NotDetermined,
            };
            let handler = RcBlock::new(|_granted: Bool| {});
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
        }
        // Return the *current* status (almost always still
        // NotDetermined — the user hasn't dismissed the dialog yet).
        // The SPA's 2s poll picks up the eventual grant/deny on the
        // next tick.
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
        // Can't assert which — depends on the user's TCC state. Just assert
        // it doesn't panic and returns one of the valid macOS variants.
        // We accept `NotDetermined` even though the current CGPreflight
        // path can't produce it, so that a future code path producing
        // NotDetermined for screen recording does not become a
        // test-suite tripwire.
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
        // TCC state is user-dependent (Granted on a dev box that has
        // approved this binary, NotDetermined on a fresh CI runner,
        // Denied if previously revoked). All four macOS variants are
        // valid; we just assert no panic and a known variant.
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
