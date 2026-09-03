//! Meeting-detection watcher (MTG-11).
//!
//! Polls [`yogurt_audio::detect::detect_meeting`] on a fixed interval and
//! holds the result in [`DetectState`] so `GET /api/meetings/detected` can
//! answer without doing a window-server round trip per request.
//!
//! ## What it will and will not do
//!
//! **It never starts a recording.** Detection only makes the UI offer a
//! prompt; the user clicks, and the click goes through the exact same
//! `/meeting/new` path as "+ New meeting". Auto-starting on a heuristic
//! would mean the first false positive silently records a room, which is
//! not a thing a privacy-first app gets to do by default.
//!
//! **It does stop a recording**, once, under a narrow rule: while a
//! detected meeting window is on screen and a recording is running, the
//! two are linked; when that window has been gone for
//! [`MISSING_TICKS_BEFORE_STOP`] consecutive polls, the recording stops.
//! Stopping is the safe direction — the failure mode is a call you have
//! to restart, not a room you did not know was being taped — and without
//! it an unattended recording runs until someone notices.
//!
//! The link is inferred rather than passed through from the UI: any
//! recording that is live while a detection is live is the recording for
//! that call. That is one rule, no plumbing through create/start, and it
//! is also true of a meeting the user started by hand mid-call — which is
//! the behavior they want anyway ("the call ended, stop recording").
//! Turning the setting off disables both halves.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use uuid::Uuid;
use yogurt_audio::detect::DetectedMeeting;

use crate::state::AppState;

/// How often to look at the window list. Long enough to be invisible in
/// `top`, short enough that the prompt shows up while you are still
/// staring at the "join" screen.
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Consecutive polls a linked window must be absent before the recording
/// is stopped. Three ticks (~15s) rides out a window that is merely
/// being reopened, resized onto another display, or briefly reported as
/// off-screen during a screen-share handoff.
pub const MISSING_TICKS_BEFORE_STOP: u8 = 3;

/// The settings key the watcher reads each tick, so toggling the setting
/// takes effect without a restart.
pub const SETTING_KEY: &str = "general.meeting_detection";

/// Live detection state. Shared between the watcher task and the
/// `/api/meetings/detected` handlers.
#[derive(Debug, Default)]
pub struct DetectState {
    /// The meeting window seen on the most recent poll.
    current: Option<DetectedMeeting>,
    /// Window id the user dismissed. Cleared when a different window
    /// shows up, so dismissing this call does not mute the next one.
    dismissed: Option<u32>,
    /// `(recording, window)` pair the watcher is holding open.
    linked: Option<(Uuid, u32)>,
    /// Polls the linked window has been missing for.
    missing_ticks: u8,
}

impl DetectState {
    /// What the prompt should show, or `None` when there is nothing to
    /// offer: no meeting seen, the user dismissed this one, or a
    /// recording is already running (MTG-12 — "start recording?" is
    /// noise once the answer is obviously yes). Folding the
    /// already-recording check in here, rather than leaving it to each
    /// caller, means every path to the prompt gets it for free.
    pub fn prompt(&self, recording: bool) -> Option<&DetectedMeeting> {
        if recording {
            return None;
        }
        let current = self.current.as_ref()?;
        if self.dismissed == Some(current.window_id) {
            return None;
        }
        Some(current)
    }

    /// Suppress the prompt for whatever meeting is on screen right now.
    pub fn dismiss_current(&mut self) {
        self.dismissed = self.current.as_ref().map(|m| m.window_id);
    }

    /// Fold one poll into the state and say whether a recording should
    /// now be stopped.
    ///
    /// Pure apart from `self`, which is the point: the link/unlink/stop
    /// decision is the only real logic in this module, and it is driven
    /// by a window list and a registry that a unit test cannot conjure.
    /// [`tick`] supplies both and acts on the answer.
    fn advance(&mut self, found: Option<DetectedMeeting>, active: Option<Uuid>) -> Option<Uuid> {
        // A different window means a different call: un-dismiss.
        let same_window = match (&self.current, &found) {
            (Some(a), Some(b)) => a.window_id == b.window_id,
            _ => false,
        };
        if !same_window {
            self.dismissed = None;
        }
        self.current = found;

        // Link: a recording running while a meeting window is on screen
        // is the recording for that call.
        if self.linked.is_none() {
            if let (Some(id), Some(m)) = (active, self.current.as_ref()) {
                tracing::debug!(
                    meeting = %id, window = m.window_id, app = %m.app,
                    "linked recording to detected meeting"
                );
                self.linked = Some((id, m.window_id));
                self.missing_ticks = 0;
            }
        }

        let (meeting_id, window_id) = self.linked?;

        // The user stopped it themselves — nothing left to hold.
        if active != Some(meeting_id) {
            self.linked = None;
            self.missing_ticks = 0;
            return None;
        }

        if self
            .current
            .as_ref()
            .is_some_and(|m| m.window_id == window_id)
        {
            self.missing_ticks = 0;
            return None;
        }

        self.missing_ticks = self.missing_ticks.saturating_add(1);
        if self.missing_ticks < MISSING_TICKS_BEFORE_STOP {
            return None;
        }
        self.linked = None;
        self.missing_ticks = 0;
        Some(meeting_id)
    }
}

/// Is meeting detection enabled? Defaults to `true` — the feature only
/// ever offers a prompt, so it is discoverable by default and the
/// Settings toggle is there to silence it.
pub fn enabled(db: &yogurt_db::Db) -> bool {
    yogurt_db::settings::get(db, SETTING_KEY)
        .ok()
        .flatten()
        .is_none_or(|v| v == "true")
}

/// Spawn the watcher. Runs for the life of the process.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        // The window list is a snapshot, not a queue: a tick we were too
        // busy to service is worthless, so skip it rather than burst.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            tick(&state).await;
        }
    });
}

/// One poll. Split out from [`spawn`] so it can be driven directly in
/// tests without waiting on wall-clock intervals.
pub async fn tick(state: &AppState) {
    if !enabled(&state.db) {
        // Drop everything we were holding — including any link, so
        // turning the setting off can never stop a recording later.
        *state.detect.lock().await = DetectState::default();
        return;
    }

    // `detect_meeting` is a blocking window-server call.
    let found = tokio::task::spawn_blocking(yogurt_audio::detect::detect_meeting)
        .await
        .unwrap_or_default();
    let active = state.meetings.active_recording().await;

    // Scoped so the state lock is released before `stop` takes the
    // registry locks.
    let to_stop = state.detect.lock().await.advance(found, active);

    if let Some(id) = to_stop {
        tracing::info!(meeting = %id, "detected meeting window closed — stopping recording");
        if let Err(e) = state.meetings.stop(&id).await {
            tracing::warn!(meeting = %id, error = %e, "auto-stop after meeting window closed failed");
        }
    }
}

/// Shared handle type stored on [`AppState`].
pub type SharedDetectState = Arc<Mutex<DetectState>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: u32) -> Option<DetectedMeeting> {
        Some(DetectedMeeting {
            window_id: id,
            app: "Zoom".into(),
            title: "Zoom Meeting".into(),
        })
    }

    #[test]
    fn recording_stops_only_after_the_window_is_gone_for_three_polls() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();

        // Detected, then the user starts recording: the two link up.
        assert_eq!(st.advance(window(7), None), None);
        assert_eq!(st.advance(window(7), Some(rec)), None);
        assert_eq!(st.linked, Some((rec, 7)));

        // Call ends. Two polls of grace, stop on the third.
        assert_eq!(st.advance(None, Some(rec)), None);
        assert_eq!(st.advance(None, Some(rec)), None);
        assert_eq!(st.advance(None, Some(rec)), Some(rec));
        // ...and only once.
        assert_eq!(st.advance(None, Some(rec)), None);
    }

    #[test]
    fn a_window_that_flickers_back_resets_the_countdown() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        st.advance(window(7), Some(rec));

        assert_eq!(st.advance(None, Some(rec)), None);
        assert_eq!(st.advance(None, Some(rec)), None);
        // Back on screen — the two missed polls must not carry over, or a
        // long call would eventually stop itself.
        assert_eq!(st.advance(window(7), Some(rec)), None);
        assert_eq!(st.advance(None, Some(rec)), None);
        assert_eq!(st.advance(None, Some(rec)), None);
        assert_eq!(st.advance(None, Some(rec)), Some(rec));
    }

    #[test]
    fn a_recording_started_before_any_detection_is_never_stopped() {
        // Nothing was ever detected, so there is nothing to infer a link
        // from — a hand-started recording must outlive any number of polls.
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        for _ in 0..10 {
            assert_eq!(st.advance(None, Some(rec)), None);
        }
        assert_eq!(st.linked, None);
    }

    #[test]
    fn stopping_by_hand_drops_the_link() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        st.advance(window(7), Some(rec));
        // User hits stop while still in the call.
        assert_eq!(st.advance(window(7), None), None);
        assert_eq!(st.linked, None);
    }

    #[test]
    fn dismissing_hides_this_call_but_not_the_next_one() {
        let mut st = DetectState::default();
        st.advance(window(7), None);
        st.dismiss_current();
        assert_eq!(st.prompt(false), None);

        // Same window, still dismissed.
        st.advance(window(7), None);
        assert_eq!(st.prompt(false), None);

        // New call, new window id — prompt again.
        st.advance(window(8), None);
        assert_eq!(st.prompt(false).map(|m| m.window_id), Some(8));
    }

    #[test]
    fn no_prompt_while_a_recording_is_already_running() {
        // MTG-12: a meeting-looking window is on screen, but a recording
        // is already in progress — "start recording?" would be noise.
        let mut st = DetectState::default();
        st.advance(window(7), None);
        assert_eq!(st.prompt(false).map(|m| m.window_id), Some(7));
        assert_eq!(st.prompt(true), None);
    }
}
