//! Meeting-detection watcher.
//!
//! Polls [`yogurt_audio::detect::detect_meeting`] on a fixed interval and holds the result in
//! [`DetectState`] so `GET /api/meetings/detected` needs no window-server round trip.
//!
//! It never starts a recording. A false positive must not silently record a room, so detection
//! only offers a prompt and the user's click goes through the normal `/meeting/new` path.
//!
//! It does stop a recording, which is the safe direction: the failure is a call to restart,
//! not an unnoticed recording. Every auto-stop goes through [`AppState::stop_meeting`] and is
//! gated by `general.meeting_auto_stop`.
//!
//! Window signal: any recording live while a detection is live is linked to that call. Once the
//! window has been gone for [`MISSING_TICKS_BEFORE_STOP`] polls, the recording stops.
//!
//! Mic signal (preferred): while recording, [`mic_poll`] asks CoreAudio which processes hold the
//! mic. Once an allowlisted meeting app (`yogurt_audio::detect::meeting_app_for_process`) has
//! held it and then released it for [`MIC_RELEASED_TICKS`] polls, the recording stops. A
//! recording no meeting app ever held the mic for is never stopped this way. When the OS cannot
//! report mic users, the window signal stays in charge. When a meeting app has held the mic, the
//! window signal is off, because a hidden or switched browser tab looks like a closed call.
//! Safari captures in a WebKit process that never matches, so Safari calls stay on the window
//! signal.
//!
//! Silence: [`silence_tick`] stops the recording once neither channel has had a peak above
//! `SILENCE_PEAK` for [`SilencePolicy::DEFAULT`]`.stop_after` (5 minutes). `GET
//! /api/meetings/active` reports `auto_stop_at` from the 4 minute mark so the UI can warn. It
//! only reads streams already open for a recording, so it never captures while idle.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use uuid::Uuid;
use yogurt_audio::detect::DetectedMeeting;

use crate::state::AppState;

pub const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Three ticks (~15s) rides out a window that is reopened, moved to another display, or
/// briefly off-screen during a screen-share handoff.
pub const MISSING_TICKS_BEFORE_STOP: u8 = 3;

pub const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

/// Settings keys are read each tick, so toggles need no restart.
pub const SETTING_KEY: &str = "general.meeting_detection";

pub const FOCUS_SETTING_KEY: &str = "general.meeting_detection_focus";

/// Gates both the window auto-stop and the silence auto-stop.
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Advance {
    /// Linked recording whose window has been gone long enough to stop.
    pub stop: Option<Uuid>,
    /// A new meeting window appeared with nothing recording: raise the app.
    pub raise: bool,
}

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
    /// Microphone-release watch for the current recording.
    mic: MicWatch,
    /// The OS can say which processes hold the mic. Once a meeting app has
    /// held it, the mic signal replaces the window-gone stop, which misfires
    /// when the user merely switches browser tabs.
    mic_supported: bool,
}

/// Consecutive polls with no meeting app on the mic before the stop.
pub const MIC_RELEASED_TICKS: u8 = 2;

/// Remembers whether a meeting app has held the mic during the current
/// recording, and stops it once the app lets go.
#[derive(Debug, Default)]
pub struct MicWatch {
    recording: Option<Uuid>,
    seen: bool,
    released_ticks: u8,
}

impl MicWatch {
    /// `holding` is `None` when the OS cannot report mic users. A recording
    /// no meeting app ever held the mic for (an in-person meeting) is never
    /// stopped here.
    pub fn advance(&mut self, active: Option<Uuid>, holding: Option<bool>) -> Option<Uuid> {
        if self.recording != active {
            *self = Self {
                recording: active,
                ..Self::default()
            };
        }
        let id = active?;
        match holding? {
            true => {
                self.seen = true;
                self.released_ticks = 0;
                None
            }
            false if self.seen => {
                self.released_ticks += 1;
                if self.released_ticks < MIC_RELEASED_TICKS {
                    return None;
                }
                *self = Self::default();
                Some(id)
            }
            false => None,
        }
    }
}

impl DetectState {
    /// `None` when no meeting is seen, the user dismissed it, or a recording is running.
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

    pub fn dismiss_current(&mut self) {
        self.dismissed = self.current.as_ref().map(|m| m.window_id);
    }

    /// Pure apart from `self` so the link/unlink/stop decision is unit-testable without a
    /// window list or registry. [`tick`] supplies both and acts on the answer.
    fn advance(&mut self, found: Option<DetectedMeeting>, active: Option<Uuid>) -> Advance {
        let same_window = match (&self.current, &found) {
            (Some(a), Some(b)) => a.window_id == b.window_id,
            _ => false,
        };
        let raise = !same_window && found.is_some() && active.is_none();
        if !same_window {
            self.dismissed = None;
        }
        self.current = found;

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
            stop: (!(self.mic_supported && self.mic.seen)).then_some(meeting_id),
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

pub fn enabled(db: &yogurt_db::Db) -> bool {
    setting_enabled(db, SETTING_KEY)
}

pub fn auto_stop_enabled(db: &yogurt_db::Db) -> bool {
    setting_enabled(db, AUTO_STOP_SETTING_KEY)
}

pub fn focus_enabled(db: &yogurt_db::Db) -> bool {
    setting_enabled(db, FOCUS_SETTING_KEY)
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(POLL_INTERVAL);
        // A missed tick is worthless, so delay rather than burst.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            mic_poll(&state).await;
            tick(&state).await;
            silence_tick(&state, now_ms(), SilencePolicy::DEFAULT).await;
        }
    });
}

/// One poll, split from [`spawn`] so tests can drive it without wall-clock waits.
pub async fn tick(state: &AppState) {
    if !enabled(&state.db) {
        // Drops any link too, so turning the setting off can never stop a recording later.
        let mut st = state.detect.lock().await;
        *st = DetectState {
            mic: std::mem::take(&mut st.mic),
            mic_supported: st.mic_supported,
            ..DetectState::default()
        };
        return;
    }

    // A blind poll must not advance state, or it could start the stop countdown for a live call.
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

    // The state lock must drop before `stop` takes the registry locks.
    let Advance {
        stop: to_stop,
        raise,
    } = state.detect.lock().await.advance(found, active);

    if raise && focus_enabled(&state.db) {
        // By app, never by URL: a URL adds a tab to the frontmost Chrome window, which hides a
        // Meet call there and un-detects it. Without the installed app the notification click covers it.
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

async fn mic_poll(state: &AppState) {
    static IN_FLIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    use std::sync::atomic::Ordering::SeqCst;

    if state.meetings.active_recording().await.is_none() {
        state.detect.lock().await.mic = MicWatch::default();
        return;
    }
    if IN_FLIGHT.swap(true, SeqCst) {
        return;
    }
    let query = tokio::task::spawn_blocking(|| {
        // Cleared on drop so a panic in `mic_users` cannot disable the stop.
        struct Clear;
        impl Drop for Clear {
            fn drop(&mut self) {
                IN_FLIGHT.store(false, SeqCst);
            }
        }
        let _clear = Clear;
        yogurt_audio::mic_usage::mic_users()
    });
    match tokio::time::timeout(QUERY_TIMEOUT, query).await {
        Ok(Ok(users)) => mic_tick(state, users, std::process::id() as i32).await,
        _ => tracing::warn!("microphone usage query skipped: wedged or panicked"),
    }
}

pub async fn mic_tick(
    state: &AppState,
    users: Option<Vec<yogurt_audio::mic_usage::MicUser>>,
    own_pid: i32,
) {
    let active = state.meetings.active_recording().await;
    let stop = {
        let mut st = state.detect.lock().await;
        st.mic_supported = users.is_some();
        if !auto_stop_enabled(&state.db) {
            st.mic = MicWatch::default();
            return;
        }
        let holding = users
            .map(|u| !yogurt_audio::mic_usage::meeting_apps_holding_mic(&u, own_pid).is_empty());
        st.mic.advance(active, holding)
    };
    if let Some(id) = stop {
        tracing::info!(meeting = %id, "meeting app released the microphone, stopping recording");
        if let Err(e) = state.stop_meeting(&id).await {
            tracing::warn!(meeting = %id, error = %e, "auto-stop after microphone release failed");
        }
    }
}

pub async fn silence_tick(state: &AppState, now_ms: u64, policy: SilencePolicy) {
    let enabled = auto_stop_enabled(&state.db);
    let Some(id) = state.meetings.active_recording().await else {
        return;
    };
    let Some(m) = state.meetings.get(&id).await else {
        return;
    };
    if !enabled {
        // Silence from while the setting was off must not count once it is on.
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

        assert_eq!(st.advance(window(7), None).stop, None);
        assert_eq!(st.advance(window(7), Some(rec)).stop, None);
        assert_eq!(st.linked, Some((rec, 7)));

        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, Some(rec));
        assert_eq!(st.advance(None, Some(rec)).stop, None);
    }

    #[test]
    fn a_window_that_flickers_back_resets_the_countdown() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        st.advance(window(7), Some(rec));

        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        // The two missed polls must not carry over, or a long call would eventually stop itself.
        assert_eq!(st.advance(window(7), Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, None);
        assert_eq!(st.advance(None, Some(rec)).stop, Some(rec));
    }

    #[test]
    fn a_recording_started_before_any_detection_is_never_stopped() {
        // A hand-started recording with no detection must outlive any number of polls.
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
        assert_eq!(st.advance(window(7), None).stop, None);
        assert_eq!(st.linked, None);
    }

    #[test]
    fn dismissing_hides_this_call_but_not_the_next_one() {
        let mut st = DetectState::default();
        st.advance(window(7), None);
        st.dismiss_current();
        assert_eq!(st.prompt(false), None);

        st.advance(window(7), None);
        assert_eq!(st.prompt(false), None);

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
        assert!(!st.advance(window(7), None).raise);
        assert!(!st.advance(window(7), None).raise);

        assert!(st.advance(window(8), None).raise);
        assert!(!st.advance(None, None).raise);
    }

    #[test]
    fn no_raise_while_a_recording_is_running() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        assert!(!st.advance(window(7), Some(rec)).raise);
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

        m.last_audible_ms.store(start + 40, at);
        silence_tick(&state, start + 60, POLICY).await;
        assert_eq!(state.meetings.active_recording().await, Some(id));

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

    #[test]
    fn mic_watch_stops_after_two_released_ticks_once_a_meeting_app_was_seen() {
        let mut w = MicWatch::default();
        let rec = Uuid::now_v7();
        assert_eq!(w.advance(Some(rec), Some(true)), None);
        assert_eq!(w.advance(Some(rec), Some(false)), None);
        assert_eq!(w.advance(Some(rec), Some(false)), Some(rec));
        assert_eq!(w.advance(Some(rec), Some(false)), None);
    }

    #[test]
    fn mic_watch_ignores_a_one_tick_blip() {
        let mut w = MicWatch::default();
        let rec = Uuid::now_v7();
        w.advance(Some(rec), Some(true));
        assert_eq!(w.advance(Some(rec), Some(false)), None);
        assert_eq!(w.advance(Some(rec), Some(true)), None);
        assert_eq!(w.advance(Some(rec), Some(false)), None);
    }

    #[test]
    fn mic_watch_never_stops_a_recording_no_meeting_app_held() {
        let mut w = MicWatch::default();
        let rec = Uuid::now_v7();
        for _ in 0..10 {
            assert_eq!(w.advance(Some(rec), Some(false)), None);
        }
    }

    #[test]
    fn mic_watch_resets_for_a_new_recording() {
        let mut w = MicWatch::default();
        let (a, b) = (Uuid::now_v7(), Uuid::now_v7());
        w.advance(Some(a), Some(true));
        // Recording A ends by hand, B starts with no meeting app.
        assert_eq!(w.advance(None, Some(false)), None);
        for _ in 0..5 {
            assert_eq!(w.advance(Some(b), Some(false)), None);
        }
    }

    #[test]
    fn unsupported_mic_api_keeps_the_window_fallback() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        st.advance(window(7), Some(rec));
        st.mic_supported = false;
        st.advance(None, Some(rec));
        st.advance(None, Some(rec));
        assert_eq!(st.advance(None, Some(rec)).stop, Some(rec));
    }

    #[test]
    fn tab_switch_does_not_stop_once_a_meeting_app_held_the_mic() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        st.advance(window(7), Some(rec));
        st.mic_supported = true;
        st.mic.advance(Some(rec), Some(true));
        for _ in 0..5 {
            assert_eq!(st.advance(None, Some(rec)).stop, None);
        }
    }

    #[test]
    fn supported_mic_without_a_meeting_app_still_allows_the_window_stop() {
        let mut st = DetectState::default();
        let rec = Uuid::now_v7();
        st.advance(window(7), Some(rec));
        st.mic_supported = true;
        st.advance(None, Some(rec));
        st.advance(None, Some(rec));
        assert_eq!(st.advance(None, Some(rec)).stop, Some(rec));
    }

    fn chrome_helper() -> Vec<yogurt_audio::mic_usage::MicUser> {
        vec![yogurt_audio::mic_usage::MicUser {
            pid: 99,
            bundle_id: "com.google.Chrome.helper".into(),
        }]
    }

    #[tokio::test]
    async fn mic_tick_stops_and_stamps_after_the_app_releases_the_mic() {
        let (state, id, _tmp) = recording_state().await;
        mic_tick(&state, Some(chrome_helper()), 1).await;
        mic_tick(&state, Some(vec![]), 1).await;
        assert_eq!(state.meetings.active_recording().await, Some(id));
        mic_tick(&state, Some(vec![]), 1).await;
        assert_eq!(state.meetings.active_recording().await, None);
        let row = state.meeting_repo.get(&id.to_string()).unwrap().unwrap();
        assert!(row.ended_at.is_some());
    }

    #[tokio::test]
    async fn mic_tick_does_nothing_when_auto_stop_is_off() {
        let (state, id, _tmp) = recording_state().await;
        yogurt_db::settings::set(&state.db, AUTO_STOP_SETTING_KEY, "false").unwrap();
        mic_tick(&state, Some(chrome_helper()), 1).await;
        for _ in 0..3 {
            mic_tick(&state, Some(vec![]), 1).await;
        }
        assert_eq!(state.meetings.active_recording().await, Some(id));
    }
}
