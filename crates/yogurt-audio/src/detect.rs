//! Meeting detection: is a call happening right now?
//!
//! It lives in `yogurt-audio` because the signal comes from `ScreenCaptureKit`, which this crate already depends on.
//! The Screen Recording grant needed for system-audio loopback also makes `SCWindow::title()` non-`None`,
//! so detection adds no dependency and no TCC prompt.
//!
//! Calendar (EventKit) needs a new TCC permission and misses ad-hoc calls.
//! A system-audio heuristic would hold an `SCStream` open while not recording, which the AGENTS.md privacy constraint forbids.
//! No window title is persisted. The title is held in memory and handed to the local UI.
//!
//! Matching is a small allow-list of (bundle id, title shape) pairs, because "Zoom is running" is not "you are in a call".
//! Only the Google Meet rule has been checked against a live call.
//! The Zoom, Teams, and Slack shapes are inferred from documented window naming.
//! Capture the real title with `yogurt ctl windows` before trusting them or retuning after a vendor renames a window.
//! A miss means the user clicks "+ New meeting". A false positive means a dismissable prompt.
//!
//! Blind spot: a browser window's title is its active tab's, so a Meet call in a background tab is invisible.
//! Reading tab state needs Accessibility permission, which is a bigger grant than this feature is worth.

/// `Deserialize` because `yogurt ctl status`/`detect` parse this back out of `GET /api/meetings/detected`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DetectedMeeting {
    /// `CGWindowID`. Stable for the window's lifetime, so a second call is distinguishable from the same one.
    pub window_id: u32,
    pub app: String,
    pub title: String,
}

struct AppRule {
    /// Matched exactly or as `<bundle>.app.<hash>`, because a Chrome-installed site (PWA) runs under its own generated bundle id.
    bundle: &'static str,
    label: &'static str,
    matches: fn(&str) -> bool,
}

fn bundle_matches(rule: &str, actual: &str) -> bool {
    actual == rule
        || (actual.len() > rule.len()
            && actual.starts_with(rule)
            && actual[rule.len()..].starts_with(".app."))
}

/// Vendors use en dashes as often as hyphens.
fn norm(title: &str) -> String {
    title.replace(['\u{2013}', '\u{2014}'], "-")
}

const RULES: &[AppRule] = &[
    // The idle window is "Zoom Workplace" or "Zoom".
    AppRule {
        bundle: "us.zoom.xos",
        label: "Zoom",
        matches: |t| t.starts_with("Zoom Meeting") || t.starts_with("Zoom Webinar"),
    },
    // Verified live: the installed Meet app reports `com.google.Chrome.app.<hash>` and title
    // "Google Meet - Meet - wbj-wqgz-pju". The landing page and join-in-progress titles
    // lack a meeting code, so matching the code separates "in a call" from "Meet is open".
    AppRule {
        bundle: "com.google.Chrome",
        label: "Google Meet",
        matches: has_meet_code,
    },
    AppRule {
        bundle: "com.apple.Safari",
        label: "Google Meet",
        matches: has_meet_code,
    },
    AppRule {
        bundle: "company.thebrowser.Browser",
        label: "Google Meet",
        matches: has_meet_code,
    },
    AppRule {
        bundle: "com.microsoft.edgemac",
        label: "Google Meet",
        matches: has_meet_code,
    },
    // The idle window is "Chat | Microsoft Teams".
    AppRule {
        bundle: "com.microsoft.teams2",
        label: "Microsoft Teams",
        matches: is_teams_meeting_title,
    },
    AppRule {
        bundle: "com.microsoft.teams",
        label: "Microsoft Teams",
        matches: is_teams_meeting_title,
    },
    // Huddles open their own window titled "Huddle ...".
    AppRule {
        bundle: "com.tinyspeck.slackmacgap",
        label: "Slack huddle",
        matches: |t| t.starts_with("Huddle"),
    },
];

/// Three, four, then three lowercase letters, like `wbj-wqgz-pju`.
fn is_meet_code(tok: &str) -> bool {
    let mut parts = tok.split('-');
    let (Some(a), Some(b), Some(c), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    (a.len(), b.len(), c.len()) == (3, 4, 3)
        && [a, b, c]
            .iter()
            .all(|s| s.bytes().all(|ch| ch.is_ascii_lowercase()))
}

/// Meet only puts a code in the title once you are in the call, whatever the tab, app, or Chrome version.
fn has_meet_code(title: &str) -> bool {
    title
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .any(is_meet_code)
}

fn is_teams_meeting_title(t: &str) -> bool {
    t.contains("| Microsoft Teams") && (t.contains("Meeting") || t.contains("Call"))
}

pub fn match_window(bundle_id: &str, title: &str) -> Option<&'static str> {
    let title = norm(title);
    RULES
        .iter()
        .find(|r| bundle_matches(r.bundle, bundle_id) && (r.matches)(&title))
        .map(|r| r.label)
}

/// Keys on the bundle id alone, and a rule bundle also matches its dotted children,
/// so `com.google.Chrome.helper` (which captures in a browser call) counts as Chrome.
pub fn meeting_app_for_process(bundle_id: &str) -> Option<&'static str> {
    RULES
        .iter()
        .find(|r| {
            bundle_id == r.bundle
                || bundle_id
                    .strip_prefix(r.bundle)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
        .map(|r| r.label)
}

/// Blocking, a window-server round trip, so async callers wrap it in `spawn_blocking`.
#[cfg(target_os = "macos")]
pub fn detect_meeting() -> Option<DetectedMeeting> {
    use screencapturekit::shareable_content::SCShareableContent;

    // Without the grant macOS redacts titles, so bail rather than report a confident "no meeting".
    if crate::permission::has_screen_recording_permission()
        == crate::permission::PermissionStatus::Denied
    {
        return None;
    }

    let content = SCShareableContent::get().ok()?;
    content.windows().into_iter().find_map(|w| {
        // Layer 0 is the document layer. Other layers are menu bars, wallpaper, and overlays.
        if !w.is_on_screen() || w.window_layer() != 0 {
            return None;
        }
        let title = w.title()?;
        let app = w.owning_application()?;
        let label = match_window(&app.bundle_identifier(), &title)?;
        Some(DetectedMeeting {
            window_id: w.window_id(),
            app: label.to_string(),
            title,
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wedged {
    TimedOut,
    /// [`MAX_STUCK`] earlier queries are still hanging.
    Busy,
}

/// Queries still running. The caller waits out each query or its timeout before the next, so every one counted here is stuck.
static QUERIES_IN_FLIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// ponytail: each stuck query parks a thread until its reply comes, which may be never, so detection pauses at this many. Raise it if long sessions hit it.
const MAX_STUCK: usize = 4;

/// The window server occasionally never completes the `SCShareableContent::get()` call (seen mid-call on macOS 26).
/// A lost reply strands only that call, so the next query starts fresh beside it, up to [`MAX_STUCK`] stuck threads.
pub fn detect_meeting_bounded(
    timeout: std::time::Duration,
) -> Result<Option<DetectedMeeting>, Wedged> {
    bounded(detect_meeting, timeout)
}

fn bounded<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
    timeout: std::time::Duration,
) -> Result<T, Wedged> {
    use std::sync::atomic::Ordering;
    if QUERIES_IN_FLIGHT
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
            (n < MAX_STUCK).then_some(n + 1)
        })
        .is_err()
    {
        return Err(Wedged::Busy);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Released on drop so a panic in `f` cannot count as stuck forever.
        struct Clear;
        impl Drop for Clear {
            fn drop(&mut self) {
                QUERIES_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
            }
        }
        let clear = Clear;
        let v = f();
        drop(clear);
        let _ = tx.send(v);
    });
    rx.recv_timeout(timeout).map_err(|_| Wedged::TimedOut)
}

#[cfg(not(target_os = "macos"))]
pub fn detect_meeting() -> Option<DetectedMeeting> {
    None
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowVerdict {
    pub verdict: Option<&'static str>,
    pub bundle: String,
    pub title: String,
}

/// Backs `yogurt ctl windows`. Blocking, and returns empty on an SCK failure,
/// so callers should check the Screen Recording permission first to tell "denied" from "no windows".
#[cfg(target_os = "macos")]
pub fn scan_windows() -> Vec<WindowVerdict> {
    use screencapturekit::shareable_content::SCShareableContent;

    let Ok(content) = SCShareableContent::get() else {
        return Vec::new();
    };
    content
        .windows()
        .into_iter()
        .filter_map(|w| {
            if !w.is_on_screen() || w.window_layer() != 0 {
                return None;
            }
            let title = w.title()?;
            let app = w.owning_application()?;
            let bundle = app.bundle_identifier();
            let verdict = match_window(&bundle, &title);
            Some(WindowVerdict {
                verdict,
                bundle,
                title,
            })
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn scan_windows() -> Vec<WindowVerdict> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::{bounded, match_window, Wedged, MAX_STUCK};

    /// Google Meet installed as a Chrome app, captured from a live call.
    const MEET_PWA: &str = "com.google.Chrome.app.kjgfgldnnfoeklkmfkjfagphfepbbdan";

    #[test]
    fn live_call_titles_match() {
        for (bundle, title, want) in [
            ("us.zoom.xos", "Zoom Meeting", "Zoom"),
            ("us.zoom.xos", "Zoom Meeting ID: 123", "Zoom"),
            // The trailing speaker glyph appears while audio is playing.
            (
                MEET_PWA,
                "Google Meet - Meet - wbj-wqgz-pju \u{1f50a}",
                "Google Meet",
            ),
            (MEET_PWA, "Google Meet - Meet - wbj-wqgz-pju", "Google Meet"),
            ("com.google.Chrome", "Meet - wbj-wqgz-pju", "Google Meet"),
            ("com.apple.Safari", "Meet - abc-defg-hij", "Google Meet"),
            (
                "com.microsoft.teams2",
                "Weekly sync | Microsoft Teams Meeting",
                "Microsoft Teams",
            ),
            (
                "com.tinyspeck.slackmacgap",
                "Huddle in #eng",
                "Slack huddle",
            ),
        ] {
            assert_eq!(
                match_window(bundle, title),
                Some(want),
                "{bundle} / {title}"
            );
        }
    }

    #[test]
    fn merely_open_apps_do_not_match() {
        for (bundle, title) in [
            ("us.zoom.xos", "Zoom Workplace"),
            ("us.zoom.xos", "Zoom"),
            // Pre-join states, neither carries a meeting code.
            (MEET_PWA, "Google Meet"),
            (MEET_PWA, "Google Meet - Meet"),
            ("com.google.Chrome", "Google Meet"),
            ("com.google.Chrome", "Inbox (12) - Gmail"),
            // Sharing the `.app.` prefix must not inherit Meet's rule.
            (
                "com.google.Chrome.app.kjbdgfilnfhdoflbpgamdcdgpehopbep",
                "Google Calendar - Week of August 31, 2026",
            ),
            ("com.microsoft.teams2", "Chat | Microsoft Teams"),
            ("com.tinyspeck.slackmacgap", "yogurt - Slack"),
            ("com.apple.finder", "Downloads"),
        ] {
            assert_eq!(match_window(bundle, title), None, "{bundle} / {title}");
        }
    }

    #[test]
    fn meeting_code_shape_is_specific() {
        for title in [
            "abc-defg-hi",      // wrong last segment length
            "abcd-defg-hij",    // wrong first segment length
            "abc-defg-hij-klm", // too many segments
            "ABC-DEFG-HIJ",     // codes are lowercase
            "abc-def1-hij",     // codes are letters only
            "how-to-cook",
        ] {
            assert_eq!(match_window("com.google.Chrome", title), None, "{title}");
        }
    }

    #[test]
    fn a_hung_query_times_out_and_the_next_one_runs_until_too_many_are_stuck() {
        use std::time::Duration;
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let release_rx = std::sync::Arc::new(std::sync::Mutex::new(release_rx));
        for i in 1..=MAX_STUCK {
            let rx = release_rx.clone();
            let hung = bounded(
                move || rx.lock().unwrap().recv().ok(),
                Duration::from_millis(20),
            );
            assert_eq!(hung, Err(Wedged::TimedOut));
            if i < MAX_STUCK {
                assert_eq!(
                    bounded(|| 1, Duration::from_secs(1)),
                    Ok(1),
                    "a stuck query must not block the next"
                );
            }
        }
        assert_eq!(bounded(|| 1, Duration::from_secs(1)), Err(Wedged::Busy));

        for _ in 0..MAX_STUCK {
            release_tx.send(()).unwrap();
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match bounded(|| 1, Duration::from_secs(1)) {
                Ok(1) => break,
                Err(Wedged::Busy) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                other => panic!("unexpected: {other:?}"),
            }
        }

        // A panicking query must not leave the flag set forever.
        let _ = bounded(
            || -> i32 { panic!("query blew up") },
            Duration::from_secs(1),
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match bounded(|| 1, Duration::from_secs(1)) {
                Ok(1) => break,
                Err(_) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                other => panic!("flag stuck after a panic: {other:?}"),
            }
        }
    }
}
