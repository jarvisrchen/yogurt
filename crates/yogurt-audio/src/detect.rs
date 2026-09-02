//! Meeting detection (MTG-11) — "is a call happening right now?"
//!
//! ## Why this lives in `yogurt-audio`
//!
//! It is not audio. It lives here because the signal comes from
//! `ScreenCaptureKit`, and `yogurt-audio` is the crate that already
//! depends on `screencapturekit` and already carries the macOS `cfg`
//! gating plus the Screen Recording permission check. Detection costs
//! zero new dependencies and zero new TCC prompts: the Screen Recording
//! grant yogurt already needs for system-audio loopback is the same
//! grant that makes `SCWindow::title()` non-`None`.
//!
//! ## Why windows and not the alternatives
//!
//! The MTG-11 ticket left the signal open between a calendar read,
//! window/app detection, and a system-audio heuristic:
//!
//! - **Calendar (EventKit)** needs a *new* TCC permission, fires on
//!   events you decline or skip, and misses every ad-hoc call.
//! - **System audio** would mean holding an `SCStream` open around the
//!   clock just to listen for speech — capturing audio while not
//!   recording, which is exactly what the AGENTS.md privacy constraint
//!   exists to prevent.
//! - **Windows** is a read of already-authorized metadata, costs one
//!   cheap call every few seconds, and needs nothing new.
//!
//! Nothing here leaves the machine, and no window title is ever
//! persisted — the detected title is held in memory and handed to the
//! local UI so the prompt can say *which* meeting it saw.
//!
//! ## Accuracy
//!
//! Matching is a deliberately small allow-list of (bundle id, title
//! shape) pairs, because "Zoom is running" is not "you are in a call".
//! The title shapes are the part that distinguishes the two, and they
//! are vendor UI strings that can change without notice.
//!
//! **Only the Google Meet rule has been checked against a live call**
//! (2026-09-01). The Zoom, Teams, and Slack shapes below are inferred
//! from their documented window naming and have not been observed
//! firing. An earlier Meet rule was written the same way and was wrong
//! twice over - it assumed the `com.google.Chrome` bundle id (an
//! installed Chrome app reports `com.google.Chrome.app.<hash>`) and a
//! `"Meet - "` title prefix (the real one is
//! `"Google Meet - Meet - <code>"`). Treat the unverified rules with
//! that in mind, and capture the real title with `yogurt ctl windows`
//! before trusting any of them. A miss means
//! the user clicks "+ New meeting" like they do today; a false positive
//! means a dismissable prompt. Neither starts a recording on its own —
//! see `yogurt_server::detect`.
//!
//! Known blind spot: a browser window's title is its *active tab's*
//! title, so a Google Meet call sitting in a background tab is invisible
//! here. Seeing it would mean reading tab state, which needs
//! Accessibility permission — a bigger grant than this feature is worth.
//!
//! `yogurt ctl windows` dumps the live window list with each row's
//! verdict; that is the tool for retuning these patterns when a vendor
//! renames a window.

/// A meeting-looking window observed on screen.
///
/// `Deserialize` (CLI-4): `yogurt ctl status`/`detect` parse this straight
/// back out of the server's `GET /api/meetings/detected` JSON body.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DetectedMeeting {
    /// `CGWindowID` of the matched window. Stable for the window's
    /// lifetime, which is what lets the caller tell "still the same
    /// call" from "a second call started".
    pub window_id: u32,
    /// Human-facing app name for the prompt, e.g. `"Zoom"`.
    pub app: String,
    /// The matched window title, e.g. `"Meet - abc-defg-hij"`.
    pub title: String,
}

/// One entry in the allow-list: which app, and what its window title
/// looks like *while a call is live* (as opposed to merely open).
struct AppRule {
    /// Matched exactly, or against `<bundle>.app.<hash>` — a site
    /// installed as a Chrome app ("PWA") runs under its own generated
    /// bundle id, so a plain `com.google.Chrome` comparison misses the
    /// installed Google Meet entirely. That is not an exotic setup; it
    /// is what the "Install Google Meet" button produces.
    bundle: &'static str,
    /// Display name for the prompt.
    label: &'static str,
    matches: fn(&str) -> bool,
}

/// Does `actual` name this rule's browser, whether it is the browser
/// proper or one of its installed-app profiles?
fn bundle_matches(rule: &str, actual: &str) -> bool {
    actual == rule
        || (actual.len() > rule.len()
            && actual.starts_with(rule)
            && actual[rule.len()..].starts_with(".app."))
}

/// Vendors write these titles with an en dash as often as a hyphen;
/// normalize so one pattern covers both.
fn norm(title: &str) -> String {
    title.replace(['\u{2013}', '\u{2014}'], "-")
}

const RULES: &[AppRule] = &[
    // Zoom's idle window is "Zoom Workplace" / "Zoom"; the in-call
    // window is titled "Zoom Meeting" (or "Zoom Webinar").
    AppRule {
        bundle: "us.zoom.xos",
        label: "Zoom",
        matches: |t| t.starts_with("Zoom Meeting") || t.starts_with("Zoom Webinar"),
    },
    // Browsers, including their installed-app profiles. Verified
    // against a live call on 2026-09-01: the installed Google Meet app
    // reports bundle `com.google.Chrome.app.kjgfgld…` and title
    // "Google Meet - Meet - wbj-wqgz-pju 🔊", while the landing page is
    // "Google Meet" and the join-in-progress state is
    // "Google Meet - Meet". Matching a prefix like "Meet - " gets all
    // three wrong; matching the meeting *code* separates "in a call"
    // from "Meet is open" regardless of which of those title shapes a
    // given Chrome version emits.
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
    // Teams (new client and classic). The idle window is
    // "Chat | Microsoft Teams"; a call window leads with the meeting
    // subject and the "Meeting" / "Call" noun.
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
    // Slack huddles open their own window titled "Huddle …".
    AppRule {
        bundle: "com.tinyspeck.slackmacgap",
        label: "Slack huddle",
        matches: |t| t.starts_with("Huddle"),
    },
];

/// Is `tok` a Google Meet meeting code — three, four, then three
/// lowercase letters (`wbj-wqgz-pju`)?
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

/// A meeting code in the title is the signal that a Meet call is
/// actually joined. Meet only puts one there once you are in the call,
/// so this distinguishes a live call from an open tab without depending
/// on the surrounding title text, which differs between a plain tab, an
/// installed app, and Chrome versions.
fn has_meet_code(title: &str) -> bool {
    title
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .any(is_meet_code)
}

fn is_teams_meeting_title(t: &str) -> bool {
    t.contains("| Microsoft Teams") && (t.contains("Meeting") || t.contains("Call"))
}

/// Match one window against the allow-list. Pure — this is the part
/// under test; the SCK enumeration around it is not.
pub fn match_window(bundle_id: &str, title: &str) -> Option<&'static str> {
    let title = norm(title);
    RULES
        .iter()
        .find(|r| bundle_matches(r.bundle, bundle_id) && (r.matches)(&title))
        .map(|r| r.label)
}

/// Enumerate on-screen windows and return the first that looks like a
/// live meeting, or `None`.
///
/// Cheap enough to poll (`SCShareableContent::get()` is a synchronous
/// window-server round trip, single-digit milliseconds), but it IS
/// blocking — callers on an async runtime must wrap it in
/// `spawn_blocking`.
#[cfg(target_os = "macos")]
pub fn detect_meeting() -> Option<DetectedMeeting> {
    use screencapturekit::shareable_content::SCShareableContent;

    // No Screen Recording grant means no window titles (macOS redacts
    // them), so every rule would miss. Bail before the SCK call rather
    // than reporting a confident "no meeting".
    if crate::permission::has_screen_recording_permission()
        == crate::permission::PermissionStatus::Denied
    {
        return None;
    }

    let content = SCShareableContent::get().ok()?;
    content.windows().into_iter().find_map(|w| {
        // Layer 0 is the normal document layer; anything else is menu
        // bars, wallpaper, notification overlays and friends.
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

/// Non-macOS stub — window enumeration is `ScreenCaptureKit`-only.
#[cfg(not(target_os = "macos"))]
pub fn detect_meeting() -> Option<DetectedMeeting> {
    None
}

/// One on-screen window plus its [`match_window`] verdict - `None` when
/// nothing in `RULES` matched.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowVerdict {
    pub verdict: Option<&'static str>,
    pub bundle: String,
    pub title: String,
}

/// Enumerate on-screen windows with each one's [`match_window`] verdict.
/// CLI-4: backs `yogurt ctl windows`, which replaced the
/// `yogurt-audio/examples/meeting_windows.rs` this function's body used to
/// live in - that was the tool for retuning `RULES` when a vendor renames
/// a window; `yogurt ctl windows` is now.
///
/// Blocking, same caveat as [`detect_meeting`]. Returns an empty vec on a
/// SCK enumeration failure - callers that care about "denied" vs
/// "genuinely no meeting-looking windows" should check
/// [`crate::permission::has_screen_recording_permission`] first (`yogurt
/// ctl windows` does, so it never reads as an empty "no meetings").
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

/// Non-macOS stub, mirroring [`detect_meeting`].
#[cfg(not(target_os = "macos"))]
pub fn scan_windows() -> Vec<WindowVerdict> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::match_window;

    /// Bundle id of Google Meet installed as a Chrome app, as captured
    /// from a live call. The hash is Chrome's, derived from the app, so
    /// it is stable — but nothing here depends on that: `bundle_matches`
    /// keys off the `com.google.Chrome.app.` prefix, so any installed
    /// app under any Chrome profile is covered.
    const MEET_PWA: &str = "com.google.Chrome.app.kjgfgldnnfoeklkmfkjfagphfepbbdan";

    #[test]
    fn live_call_titles_match() {
        for (bundle, title, want) in [
            ("us.zoom.xos", "Zoom Meeting", "Zoom"),
            ("us.zoom.xos", "Zoom Meeting ID: 123", "Zoom"),
            // Captured from a real joined call on 2026-09-01. The
            // trailing speaker glyph appears while audio is playing.
            (
                MEET_PWA,
                "Google Meet - Meet - wbj-wqgz-pju \u{1f50a}",
                "Google Meet",
            ),
            (MEET_PWA, "Google Meet - Meet - wbj-wqgz-pju", "Google Meet"),
            // Same call in a plain tab rather than the installed app.
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
        // The whole point of matching on titles: an app being open is
        // not a call. Every one of these is a false positive if the
        // rules ever degrade to bundle-id-only matching.
        for (bundle, title) in [
            ("us.zoom.xos", "Zoom Workplace"),
            ("us.zoom.xos", "Zoom"),
            // The two pre-join states of the installed Meet app, both
            // captured live. Neither carries a meeting code.
            (MEET_PWA, "Google Meet"),
            (MEET_PWA, "Google Meet - Meet"),
            ("com.google.Chrome", "Google Meet"),
            ("com.google.Chrome", "Inbox (12) - Gmail"),
            // A different Chrome installed app must not inherit Meet's
            // rule just by sharing the `.app.` prefix.
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
        // A hyphenated word in an unrelated page title must not read as
        // a meeting code, or every blog post becomes a meeting.
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
}
