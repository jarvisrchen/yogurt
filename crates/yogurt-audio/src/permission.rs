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

/// The app System Settings lists for both grants: the terminal that ran yogurt, or yogurt itself.
pub fn permission_owner() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::owner()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::PermissionStatus;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

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

    pub fn owner() -> Option<String> {
        type ResponsibleFor = unsafe extern "C" fn(libc::pid_t) -> libc::pid_t;
        // Private symbol with no public equivalent, so look it up at runtime instead of linking it.
        // SAFETY: dlsym with RTLD_DEFAULT and a NUL-terminated name has no preconditions.
        let sym = unsafe {
            libc::dlsym(
                libc::RTLD_DEFAULT,
                c"responsibility_get_pid_responsible_for_pid".as_ptr(),
            )
        };
        if sym.is_null() {
            return None;
        }
        // SAFETY: the symbol has had this signature since macOS 10.14.
        let responsible_for: ResponsibleFor = unsafe { std::mem::transmute(sym) };
        // SAFETY: plain syscalls; proc_pidpath writes at most `buf.len()` bytes.
        let pid = unsafe { responsible_for(libc::getpid()) };
        if pid <= 0 {
            return None;
        }
        let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        if n <= 0 {
            return None;
        }
        buf.truncate(n as usize);
        owner_name(Path::new(OsStr::from_bytes(&buf)))
    }

    /// System Settings shows the outermost bundle, so a helper inside `Foo.app` still reads as "Foo".
    pub(super) fn owner_name(path: &Path) -> Option<String> {
        let bundle = path.components().find_map(|c| {
            c.as_os_str()
                .to_str()?
                .strip_suffix(".app")
                .map(str::to_owned)
        });
        bundle.or_else(|| Some(path.file_name()?.to_str()?.to_owned()))
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
    #[cfg(target_os = "macos")]
    fn it_names_the_outermost_app_bundle_or_the_bare_binary() {
        use std::path::Path;
        let name = |p: &str| macos::owner_name(Path::new(p));
        assert_eq!(
            name("/Applications/Ghostty.app/Contents/MacOS/ghostty").as_deref(),
            Some("Ghostty")
        );
        assert_eq!(
            name("/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper.app/Contents/MacOS/Code Helper").as_deref(),
            Some("Visual Studio Code")
        );
        assert_eq!(
            name("/opt/homebrew/Cellar/yogurt/1.4.0/bin/yogurt").as_deref(),
            Some("yogurt")
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
