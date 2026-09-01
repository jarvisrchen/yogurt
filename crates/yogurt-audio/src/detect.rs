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
//! are vendor UI strings that can change without notice. A miss means
//! the user clicks "+ New meeting" like they do today; a false positive
//! means a dismissable prompt. Neither starts a recording on its own —
//! see `yogurt_server::detect`.
//!
//! Known blind spot: a browser window's title is its *active tab's*
//! title, so a Google Meet call sitting in a background tab is invisible
//! here. Seeing it would mean reading tab state, which needs
//! Accessibility permission — a bigger grant than this feature is worth.
//!
//! `cargo run -p yogurt-audio --example meeting_windows` dumps the live
//! window list with each row's verdict; that is the tool for retuning
//! these patterns when a vendor renames a window.

/// A meeting-looking window observed on screen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
    bundle_id: &'static str,
    /// Display name for the prompt — the SCK application name is used
    /// when this is `None`.
    label: &'static str,
    matches: fn(&str) -> bool,
}

/// Case-insensitive `starts_with` over the ASCII prefix, tolerant of the
/// en dash vendors like to use in place of a hyphen.
fn norm(title: &str) -> String {
    title.replace(['\u{2013}', '\u{2014}'], "-")
}

const RULES: &[AppRule] = &[
    // Zoom's idle window is "Zoom Workplace" / "Zoom"; the in-call
    // window is titled "Zoom Meeting" (or "Zoom Webinar").
    AppRule {
        bundle_id: "us.zoom.xos",
        label: "Zoom",
        matches: |t| t.starts_with("Zoom Meeting") || t.starts_with("Zoom Webinar"),
    },
    // Browser tabs. A Chrome/Safari/Arc window title IS the tab title
    // (verified against the live SCK window list), so the Google Meet
    // in-call title "Meet - abc-defg-hij" is visible. The bare landing
    // page is titled "Google Meet", which deliberately does NOT match.
    AppRule {
        bundle_id: "com.google.Chrome",
        label: "Google Meet",
        matches: is_browser_meeting_title,
    },
    AppRule {
        bundle_id: "com.apple.Safari",
        label: "Google Meet",
        matches: is_browser_meeting_title,
    },
    AppRule {
        bundle_id: "company.thebrowser.Browser",
        label: "Google Meet",
        matches: is_browser_meeting_title,
    },
    AppRule {
        bundle_id: "com.microsoft.edgemac",
        label: "Google Meet",
        matches: is_browser_meeting_title,
    },
    // Teams (new client and classic). The idle window is
    // "Chat | Microsoft Teams"; a call window leads with the meeting
    // subject and the "Meeting" / "Call" noun.
    AppRule {
        bundle_id: "com.microsoft.teams2",
        label: "Microsoft Teams",
        matches: is_teams_meeting_title,
    },
    AppRule {
        bundle_id: "com.microsoft.teams",
        label: "Microsoft Teams",
        matches: is_teams_meeting_title,
    },
    // Slack huddles open their own window titled "Huddle …".
    AppRule {
        bundle_id: "com.tinyspeck.slackmacgap",
        label: "Slack huddle",
        matches: |t| t.starts_with("Huddle"),
    },
];

fn is_browser_meeting_title(t: &str) -> bool {
    // "Meet - abc-defg-hij" / "Meet - Weekly sync". Requires the
    // separator so the plain "Google Meet" landing page is not a hit.
    t.starts_with("Meet - ")
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
        .find(|r| r.bundle_id == bundle_id && (r.matches)(&title))
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

#[cfg(test)]
mod tests {
    use super::match_window;

    #[test]
    fn live_call_titles_match() {
        for (bundle, title, want) in [
            ("us.zoom.xos", "Zoom Meeting", "Zoom"),
            ("us.zoom.xos", "Zoom Meeting ID: 123", "Zoom"),
            ("com.google.Chrome", "Meet - abc-defg-hij", "Google Meet"),
            // Chrome renders Meet titles with an en dash.
            (
                "com.google.Chrome",
                "Meet \u{2013} abc-defg-hij",
                "Google Meet",
            ),
            ("com.apple.Safari", "Meet - Weekly sync", "Google Meet"),
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
            ("com.google.Chrome", "Google Meet"),
            ("com.google.Chrome", "Inbox (12) - Gmail"),
            ("com.microsoft.teams2", "Chat | Microsoft Teams"),
            ("com.tinyspeck.slackmacgap", "yogurt - Slack"),
            ("com.apple.finder", "Downloads"),
        ] {
            assert_eq!(match_window(bundle, title), None, "{bundle} / {title}");
        }
    }
}
