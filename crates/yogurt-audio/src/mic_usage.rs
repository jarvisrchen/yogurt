//! Which processes currently hold the microphone open (MTG-17).
//!
//! CoreAudio exposes one audio object per client process. Each reports its
//! pid, its bundle id, and whether it is running input right now. A call app
//! releases the mic within about a second of hanging up, which is a much
//! faster and less ambiguous "the call ended" signal than a window title.
//!
//! The process-object properties are not annotated with an availability in
//! the SDK header and are not expected on older macOS releases. An OSStatus
//! error on the process list is reported as `None` so the caller can fall
//! back to another signal.
//!
//! Safari captures in a WebKit process whose bundle id never matches a
//! meeting app, so Safari calls stay on the window-title fallback.

use crate::detect::meeting_app_for_process;

/// A process that is running audio input.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MicUser {
    pub pid: i32,
    pub bundle_id: String,
}

/// Meeting-app labels among `users`, ignoring `own_pid`. Pure.
pub fn meeting_apps_holding_mic(users: &[MicUser], own_pid: i32) -> Vec<&'static str> {
    users
        .iter()
        .filter(|u| u.pid != own_pid)
        .filter_map(|u| meeting_app_for_process(&u.bundle_id))
        .collect()
}

/// Every process currently running input, or `None` when the process list
/// is unavailable (older macOS, non-macOS, or a CoreAudio error).
#[cfg(target_os = "macos")]
pub fn mic_users() -> Option<Vec<MicUser>> {
    use std::ffi::{c_char, c_void};

    #[repr(C)]
    struct Addr {
        selector: u32,
        scope: u32,
        element: u32,
    }

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(
            id: u32,
            addr: *const Addr,
            qsize: u32,
            q: *const c_void,
            out: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            id: u32,
            addr: *const Addr,
            qsize: u32,
            q: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringGetCString(s: *const c_void, buf: *mut c_char, size: isize, enc: u32) -> u8;
        fn CFRelease(p: *const c_void);
    }

    const SYSTEM_OBJECT: u32 = 1;
    const UTF8: u32 = 0x0800_0100;
    let fcc = |s: &[u8; 4]| u32::from_be_bytes(*s);
    let addr = |sel: &[u8; 4]| Addr {
        selector: fcc(sel),
        scope: fcc(b"glob"),
        element: 0,
    };

    fn read_u32(id: u32, a: &Addr) -> Option<u32> {
        let mut v = 0u32;
        let mut size = 4u32;
        // SAFETY: `v` is a valid 4-byte buffer and `size` says so.
        let st = unsafe {
            AudioObjectGetPropertyData(
                id,
                a,
                0,
                std::ptr::null(),
                &mut size,
                (&mut v as *mut u32).cast(),
            )
        };
        (st == 0).then_some(v)
    }

    let list = addr(b"prs#");
    let mut size = 0u32;
    // SAFETY: out-pointer is a valid u32.
    let st = unsafe {
        AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &list, 0, std::ptr::null(), &mut size)
    };
    if st != 0 {
        return None;
    }
    let mut ids = vec![0u32; size as usize / 4];
    // SAFETY: `ids` holds `size` bytes.
    let st = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            &list,
            0,
            std::ptr::null(),
            &mut size,
            ids.as_mut_ptr().cast(),
        )
    };
    if st != 0 {
        return None;
    }
    ids.truncate(size as usize / 4);

    let (running, pid_a, bundle_a) = (addr(b"piri"), addr(b"ppid"), addr(b"pbid"));
    let mut out = Vec::new();
    for id in ids {
        // A process can exit between the list and these reads; skip it.
        if read_u32(id, &running) != Some(1) {
            continue;
        }
        let Some(pid) = read_u32(id, &pid_a) else {
            continue;
        };
        let mut cf: *const c_void = std::ptr::null();
        let mut sz = std::mem::size_of::<*const c_void>() as u32;
        // SAFETY: `cf` is a pointer-sized buffer; CoreAudio returns a
        // retained CFString that is released below.
        let st = unsafe {
            AudioObjectGetPropertyData(
                id,
                &bundle_a,
                0,
                std::ptr::null(),
                &mut sz,
                (&mut cf as *mut *const c_void).cast(),
            )
        };
        let mut bundle_id = String::new();
        if st == 0 && !cf.is_null() {
            let mut buf = [0 as c_char; 256];
            // SAFETY: `buf` is 256 bytes; `cf` is a live CFString.
            let ok = unsafe { CFStringGetCString(cf, buf.as_mut_ptr(), 256, UTF8) };
            if ok != 0 {
                // SAFETY: CFStringGetCString NUL-terminates on success.
                bundle_id = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }
                    .to_string_lossy()
                    .into_owned();
            }
            // SAFETY: balances the retain CoreAudio gave us.
            unsafe { CFRelease(cf) };
        }
        out.push(MicUser {
            pid: pid as i32,
            bundle_id,
        });
    }
    Some(out)
}

#[cfg(not(target_os = "macos"))]
pub fn mic_users() -> Option<Vec<MicUser>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(pid: i32, bundle: &str) -> MicUser {
        MicUser {
            pid,
            bundle_id: bundle.into(),
        }
    }

    #[test]
    fn chrome_helper_counts_as_a_meeting_app() {
        let users = [user(10, "com.google.Chrome.helper")];
        assert_eq!(meeting_apps_holding_mic(&users, 1), vec!["Google Meet"]);
    }

    #[test]
    fn installed_pwa_zoom_teams_and_slack_count() {
        for (b, label) in [
            (
                "com.google.Chrome.app.kjgfgldnnfoeklkmfkjfagphfepbbdan",
                "Google Meet",
            ),
            ("us.zoom.xos", "Zoom"),
            ("com.microsoft.teams2.helper", "Microsoft Teams"),
            ("com.tinyspeck.slackmacgap.helper", "Slack huddle"),
        ] {
            assert_eq!(
                meeting_apps_holding_mic(&[user(5, b)], 1),
                vec![label],
                "{b}"
            );
        }
    }

    #[test]
    fn corespeechd_and_lookalike_prefixes_are_ignored() {
        let users = [
            user(3, "com.apple.CoreSpeech"),
            user(4, "com.google.Chromecast"),
            user(5, ""),
        ];
        assert!(meeting_apps_holding_mic(&users, 1).is_empty());
    }

    #[test]
    fn our_own_pid_is_excluded() {
        let users = [user(42, "com.google.Chrome.helper")];
        assert!(meeting_apps_holding_mic(&users, 42).is_empty());
    }

    #[test]
    #[ignore = "needs macOS 14.2+ CoreAudio process objects"]
    fn mic_users_is_supported_on_this_machine() {
        assert!(mic_users().is_some());
    }
}
