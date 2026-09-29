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
//!
//! **It also stops a silent recording.** Independently of window detection,
//! [`silence_tick`] stops the active recording once neither the mic nor the
//! system channel has had a peak above `SILENCE_PEAK` for
//! [`SilencePolicy::DEFAULT`]`.stop_after` (5 minutes). `GET
//! /api/meetings/active` reports `auto_stop_at` from the 4 minute mark so the
//! UI can warn and offer keep-recording. This only runs while recording, so
//! it never holds a capture stream open to listen. `general.meeting_auto_stop`
//! gates both this and the window auto-stop, and every auto-stop goes
//! through [`AppState::stop_meeting`], the same path as the manual stop.

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

/// Longest a single window-server query may take before the tick is
/// skipped. Normal answers arrive in single-digit milliseconds.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

/// The settings key the watcher reads each tick, so toggling the setting
/// takes effect without a restart.
pub const SETTING_KEY: &str = "general.meeting_detection";

/// Read each tick like [`SETTING_KEY`], so the toggle needs no restart.
pub const FOCUS_SETTING_KEY: &str = "general.meeting_detection_focus";

/// Read each tick like [`SETTING_KEY`]. Gates both the window auto-stop and
/// the silence auto-stop.
pub const AUTO_STOP_SETTING_KEY: &str = "general.meeting_auto_stop";

/// How long both channels must stay quiet before the warning and the stop.
#[derive(Debug, Clone, Copy)]
pub struct SilencePolicy {
    pub warn_after: Duration,
    pub stop_after: Duration,
}

impl SilencePolicy {
    pub const DEFAULT: Self = Self {
        warn_after: Duration::from_secs(4 * 60),
        stop_after: Duration::from_secs(5 * 60),
    };

    pub fn check(&self, last_audible_ms: u64, now_ms: u64) -> Silence {
        let quiet = Duration::from_millis(now_ms.saturating_sub(last_audible_ms));
        if quiet >= self.stop_after {
            Silence::Expired
        } else if quiet >= self.warn_after {
            Silence::Warning {
                stop_at_ms: last_audible_ms + self.stop_after.as_millis() as u64,
            }
        } else {
            Silence::Audible
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Silence {
    Audible,
    Warning { stop_at_ms: u64 },
    Expired,
}

/// Result of one [`DetectState::advance`] call.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Advance {
    /// Linked recording whose window has been gone long enough to stop.
    pub stop: Option<Uuid>,
    /// A new meeting window appeared with nothing recording: raise the app.
    pub raise: bool,
}

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
    /// recording is already running.
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
    /// now be stopped and/or the app window should be raised.
    ///
    /// Pure apart from `self`, which is the point: the link/unlink/stop
    /// decision is the only real logic in this module, and it is driven
    /// by a window list and a registry that a unit test cannot conjure.
    /// [`tick`] supplies both and acts on the answer.
    fn advance(&mut self, found: Option<DetectedMeeting>, active: Option<Uuid>) -> Advance {
        // A different window means a different call: un-dismiss.
        let same_window = match (&self.current, &found) {
            (Some(a), Some(b)) => a.window_id == b.window_id,
            _ => false,
        };
        let raise = !same_window && found.is_some() && active.is_none();
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

        let Some((meeting_id, window_id)) = self.linked else {
            return Advance { stop: None, raise };
        };

        // The user stopped it themselves — nothing left to hold.
        if active != Some(meeting_id) {
            self.linked = None;
            self.missing_ticks = 0;
            return Advance { stop: None, raise };
        }

        if self
            .current
            .as_ref()
            .is_some_and(|m| m.window_id == window_id)
        {
            self.missing_ticks = 0;
            return Advance { stop: None, raise };
        }

        self.missing_ticks = self.missing_ticks.saturating_add(1);
        if self.missing_ticks < MISSING_TICKS_BEFORE_STOP {
            return Advance { stop: None, raise };
        }
        self.linked = None;
        self.missing_ticks = 0;
        Advance {
            stop: Some(meeting_id),
            raise,
        }
    }
}

/// Absent means `true`: both settings are opt-out.
fn setting_enabled(db: &yogurt_db::Db, key: &str) -> bool {
    yogurt_db::settings::get(db, key)
        .ok()
        .flatten()
        .is_none_or(|v| v == "true")
}

/// Is meeting detection enabled? Defaults to `true` — the feature only
/// ever offers a prompt, so it is discoverable by default and the
/// Settings toggle is there to silence it.
pub fn enabled(db: &yogurt_db::Db) -> bool {
    setting_enabled(db, SETTING_KEY)
}

pub fn auto_stop_enabled(db: &yogurt_db::Db) -> bool {
    setting_enabled(db, AUTO_STOP_SETTING_KEY)
}

pub fn focus_enabled(db: &yogurt_db::Db) -> bool {
    setting_enabled(db, FOCUS_SETTING_KEY)
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
            silence_tick(&state, now_ms(), SilencePolicy::DEFAULT).await;
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

    // A blind poll must not advance state: it could otherwise start the
    // auto-stop countdown for a call that is still on screen.
    let found = match tokio::task::spawn_blocking(|| {
        yogurt_audio::detect::detect_meeting_bounded(QUERY_TIMEOUT)
    })
    .await
    {
        Ok(Ok(found)) => found,
        Ok(Err(why)) => {
            tracing::warn!(?why, "meeting detection skipped: window query wedged");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "meeting detection poll panicked");
            return;
        }
    };
    tracing::debug!(found = ?found.as_ref().map(|m| (&m.app, m.window_id)), "detection poll");
    let active = state.meetings.active_recording().await;

    // Scoped so the state lock is released before `stop` takes the
    // registry locks.
    let Advance {
        stop: to_stop,
        raise,
    } = state.detect.lock().await.advance(found, active);

    if raise && focus_enabled(&state.db) {
        // By app, never by URL: opening the URL adds a tab to the frontmost
        // Chrome window, which hides a Meet call running there and
        // un-detects it on the next poll. Without the installed app there
        // is nothing to raise; the notification click covers that case.
        tokio::task::spawn_blocking(|| {
            match std::process::Command::new("open")
                .args(["-a", "yogurt"])
                .output()
            {
                Ok(out) if out.status.success() => {}
                Ok(out) => tracing::debug!(
                    stderr = %String::from_utf8_lossy(&out.stderr).trim(),
                    "no installed yogurt app to raise"
                ),
                Err(e) => tracing::warn!(error = %e, "failed to run open -a yogurt"),
            }
        });
    }

    if let Some(id) = to_stop.filter(|_| auto_stop_enabled(&state.db)) {
        tracing::info!(meeting = %id, "detected meeting window closed — stopping recording");
        if let Err(e) = state.stop_meeting(&id).await {
            tracing::warn!(meeting = %id, error = %e, "auto-stop after meeting window closed failed");
        }
    }
}

/// Stop the active recording once both channels have been silent for
/// `policy.stop_after`. Runs independently of meeting detection.
pub async fn silence_tick(state: &AppState, now_ms: u64, policy: SilencePolicy) {
    let enabled = auto_stop_enabled(&state.db);
    let Some(id) = state.meetings.active_recording().await else {
        return;
    };
    let Some(m) = state.meetings.get(&id).await else {
        return;
    };
    if !enabled {
        // Silence that piled up while the setting was off must not count
        // once it is turned on.
        m.last_audible_ms
            .store(now_ms, std::sync::atomic::Ordering::Relaxed);
        return;
    }
    let last = m.last_audible_ms.load(std::sync::atomic::Ordering::Relaxed);
    if policy.check(last, now_ms) == Silence::Expired {
        tracing::info!(meeting = %id, "no audio on either channel, stopping recording");
        if let Err(e) = state.stop_meeting(&id).await {
            tracing::warn!(meeting = %id, error = %e, "auto-stop after silence failed");
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
        assert_eq!(st.advance(window(7), None).stop, None);
        assert_eq!(st.advance(window(7), Some(rec)).stop, None);
        assert_eq!(st.linked, Some((rec, 7)));

        // Call ends. Two polls of grace, stop on the third.
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, Some(rec));
        // ...and only once.
        assert_eq!(st.advance(None, Some(rec)).stop, None);
    }

    #[test]
    fn a_window_that_flickers_back_resets_the_countdown() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        st.advance(window(7), Some(rec));

        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        // Back on screen — the two missed polls must not carry over, or a
        // long call would eventually stop itself.
        assert_eq!(st.advance(window(7), Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, Some(rec));
    }

    #[test]
    fn a_recording_started_before_any_detection_is_never_stopped() {
        // Nothing was ever detected, so there is nothing to infer a link
        // from — a hand-started recording must outlive any number of polls.
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        for _ in 0..10 {
            assert_eq!(st.advance(None, Some(rec)).stop, None);
        }
        assert_eq!(st.linked, None);
    }

    #[test]
    fn stopping_by_hand_drops_the_link() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        st.advance(window(7), Some(rec));
        // User hits stop while still in the call.
        assert_eq!(st.advance(window(7), None).stop, None);
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
        let mut st = DetectState::default();
        st.advance(window(7), None);
        assert_eq!(st.prompt(false).map(|m| m.window_id), Some(7));
        assert_eq!(st.prompt(true), None);
    }

    #[test]
    fn a_new_window_raises_once_and_the_same_window_does_not() {
        let mut st = DetectState::default();
        assert!(st.advance(window(7), None).raise);
        // Same window on the next poll: no second raise.
        assert!(!st.advance(window(7), None).raise);
        assert!(!st.advance(window(7), None).raise);

        // A different window is a different call, so it raises again.
        assert!(st.advance(window(8), None).raise);
        // Nothing detected does not raise either.
        assert!(!st.advance(None, None).raise);
    }

    #[test]
    fn no_raise_while_a_recording_is_running() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        // A window shows up while a recording is already active.
        assert!(!st.advance(window(7), Some(rec)).raise);
        // ...and a different window mid-recording still doesn't raise.
        assert!(!st.advance(window(9), Some(rec)).raise);
    }

    #[test]
    fn silence_warns_at_four_minutes_and_expires_at_five() {
        let p = SilencePolicy::DEFAULT;
        let min = 60_000;
        assert_eq!(p.check(1_000, 1_000 + 4 * min - 1), Silence::Audible);
        assert_eq!(
            p.check(1_000, 1_000 + 4 * min),
            Silence::Warning {
                stop_at_ms: 1_000 + 5 * min
            }
        );
        assert_eq!(p.check(1_000, 1_000 + 5 * min), Silence::Expired);
    }

    async fn recording_state() -> (AppState, Uuid, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let storage =
            Arc::new(crate::storage::Storage::init_at(&tmp.path().join("db.sqlite")).unwrap());
        let session =
            Arc::new(crate::session::load_or_create(&tmp.path().join("session-token")).unwrap());
        let state = AppState::in_memory(
            crate::Mode::Release,
            storage,
            session,
            7878,
            tmp.path().join("notes"),
        )
        .unwrap();
        let m = state.meetings.create().await;
        state
            .meeting_repo
            .create(yogurt_db::NewMeeting {
                title: "t".into(),
                started_at_unix_ms: None,
                id: Some(m.id.to_string()),
            })
            .unwrap();
        // A live supervisor task is what makes the registry call it recording.
        *m.task.lock().await = Some(tokio::spawn(std::future::pending()));
        (state, m.id, tmp)
    }

    const POLICY: SilencePolicy = SilencePolicy {
        warn_after: Duration::from_millis(40),
        stop_after: Duration::from_millis(50),
    };

    #[tokio::test]
    async fn silence_tick_stops_and_stamps_only_after_the_stop_threshold() {
        let (state, id, _tmp) = recording_state().await;
        let last = state
            .meetings
            .get(&id)
            .await
            .unwrap()
            .last_audible_ms
            .load(std::sync::atomic::Ordering::Relaxed);

        silence_tick(&state, last + 49, POLICY).await;
        assert_eq!(state.meetings.active_recording().await, Some(id));

        silence_tick(&state, last + 50, POLICY).await;
        assert_eq!(state.meetings.active_recording().await, None);
        let row = state.meeting_repo.get(&id.to_string()).unwrap().unwrap();
        assert!(row.ended_at.is_some());
    }

    #[tokio::test]
    async fn audible_audio_and_keep_recording_reset_the_silence_clock() {
        let (state, id, _tmp) = recording_state().await;
        let m = state.meetings.get(&id).await.unwrap();
        let at = std::sync::atomic::Ordering::Relaxed;
        let start = m.last_audible_ms.load(at);

        // Sound at start+40 pushes the deadline out, so start+60 is fine.
        m.last_audible_ms.store(start + 40, at);
        silence_tick(&state, start + 60, POLICY).await;
        assert_eq!(state.meetings.active_recording().await, Some(id));

        // Keep-recording is the same store with "now".
        m.last_audible_ms.store(start + 100, at);
        silence_tick(&state, start + 149, POLICY).await;
        assert_eq!(state.meetings.active_recording().await, Some(id));
    }

    #[tokio::test]
    async fn silence_never_stops_when_auto_stop_is_off() {
        let (state, id, _tmp) = recording_state().await;
        yogurt_db::settings::set(&state.db, AUTO_STOP_SETTING_KEY, "false").unwrap();
        silence_tick(&state, u64::MAX / 2, POLICY).await;
        assert_eq!(state.meetings.active_recording().await, Some(id));
    }

    #[tokio::test]
    async fn enabling_auto_stop_after_long_silence_gives_a_full_window() {
        let (state, id, _tmp) = recording_state().await;
        let last = state
            .meetings
            .get(&id)
            .await
            .unwrap()
            .last_audible_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        yogurt_db::settings::set(&state.db, AUTO_STOP_SETTING_KEY, "false").unwrap();
        let ten_min = last + 600_000;
        silence_tick(&state, ten_min, POLICY).await;

        yogurt_db::settings::set(&state.db, AUTO_STOP_SETTING_KEY, "true").unwrap();
        silence_tick(&state, ten_min + 1, POLICY).await;
        assert_eq!(state.meetings.active_recording().await, Some(id));
        let m = state.meetings.get(&id).await.unwrap();
        let reset = m.last_audible_ms.load(std::sync::atomic::Ordering::Relaxed);
        assert_eq!(POLICY.check(reset, ten_min + 1), Silence::Audible);
        assert!(matches!(
            POLICY.check(reset, ten_min + 40),
            Silence::Warning { .. }
        ));
    }

    #[tokio::test]
    async fn state_stop_meeting_stamps_ended_at_once() {
        let (state, id, _tmp) = recording_state().await;
        state.stop_meeting(&id).await.unwrap();
        let first = state
            .meeting_repo
            .get(&id.to_string())
            .unwrap()
            .unwrap()
            .ended_at;
        assert!(first.is_some());
        state.stop_meeting(&id).await.unwrap();
        let again = state
            .meeting_repo
            .get(&id.to_string())
            .unwrap()
            .unwrap()
            .ended_at;
        assert_eq!(first, again);
    }
}
