//! Live recap: while a meeting records, a few short bullets of what was just discussed, every few minutes.
//!
//! One task per recording, kept through a quick Stop and resume, buffers final segments and sends only those since the last update,
//! with the last few bullets as context, so each call stays about the same size however long the
//! meeting runs. Bullets live only on the in-memory `Meeting` and are dropped once Enhance has saved
//! the summary, so they never reach SQLite, the markdown export, or Yogurt Cloud.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use uuid::Uuid;
use yogurt_llm::{ChatMessage, ChatRequest};
use yogurt_stt::TranscriptEvent;

use crate::meetings::Meeting;
use crate::ws::WsEvent;
use crate::AppState;

pub const SETTING_KEY: &str = "general.live_recap";
const TICK: Duration = Duration::from_secs(180);
/// Silence and small talk below this many new words skip their tick and cost nothing.
const WORD_FLOOR: usize = 100;
/// Caps the context so a call is the same size in minute 5 and minute 90.
const CONTEXT_BULLETS: usize = 20;
const MAX_NEW_BULLETS: usize = 4;
/// About two busy windows. A provider that keeps failing would otherwise grow the retried backlog,
/// and the next prompt, without bound.
const MAX_PENDING_WORDS: usize = 2_000;
/// Finals landing this soon after Resume are dropped. A local whisper segment runs up to
/// `MAX_SEGMENT_MS` and its final lands a few seconds after it closes, so without this the words
/// said while paused could ride in on one that straddles Resume.
/// ponytail: wall-clock, so it also drops up to 30s of speech said after Resume. The transcript's
/// own `ts_ms` cannot stand in, because Deepgram restarts it on every reconnect.
const RESUME_GRACE: Duration = Duration::from_millis(yogurt_stt::vad::MAX_SEGMENT_MS + 5_000);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bullet {
    pub ts_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Idle,
    Updating,
    Error,
}

/// What `GET /recap` returns and every `live_recap` frame carries.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RecapState {
    pub bullets: Vec<Bullet>,
    pub status: Status,
    pub error: Option<String>,
    /// Unix ms of the last update that succeeded.
    pub updated_at: Option<i64>,
    pub paused: bool,
}

#[derive(Debug, Default)]
pub struct Recap {
    state: RecapState,
    /// Final segments since the last update. Shared with the routes so Pause can drop them.
    pending: Vec<TranscriptEvent>,
    /// Bumped by `close`, so a call that began before Enhance saved the summary is discarded when it lands.
    epoch: u64,
    resumed_at: Option<tokio::time::Instant>,
    /// A task is alive. It exits after the last update once the meeting stops recording.
    running: bool,
    recording: bool,
    /// Update now, or the last update at Stop. Asked for while a call runs, it waits for that call.
    wanted: bool,
}

type Update =
    Pin<Box<dyn Future<Output = (u64, Vec<TranscriptEvent>, anyhow::Result<Vec<Bullet>>)> + Send>>;

/// Starts the recap task, or keeps the one still finishing the last session's final update, so a
/// quick Stop and resume never has two tasks sharing one buffer. `rx` must be subscribed before
/// `Registry::start`, so no final segment slips past.
pub fn spawn(state: AppState, m: Arc<Meeting>, rx: broadcast::Receiver<TranscriptEvent>) {
    let start = {
        let mut r = m.recap.lock().unwrap();
        r.recording = true;
        !std::mem::replace(&mut r.running, true)
    };
    if start {
        tokio::spawn(run(state, m, rx));
    }
}

/// Runs one last update with no word floor, then the task exits. Not awaited, so the post-meeting
/// page opens as fast as before.
pub fn stop(m: &Meeting) {
    let mut r = m.recap.lock().unwrap();
    r.recording = false;
    r.wanted = true;
    drop(r);
    m.recap_refresh.notify_one();
}

/// Update now, which skips the word floor.
fn request_update(m: &Meeting) {
    m.recap.lock().unwrap().wanted = true;
    m.recap_refresh.notify_one();
}

/// Called once Enhance has saved the summary the recap stood in for.
pub fn close(m: &Meeting) {
    update(m, |r| {
        r.epoch += 1;
        r.pending.clear();
        r.state = RecapState {
            paused: r.state.paused,
            ..Default::default()
        };
    });
}

pub(crate) fn snapshot(m: &Meeting) -> RecapState {
    m.recap.lock().unwrap().state.clone()
}

fn enabled(state: &AppState) -> bool {
    crate::detect::setting_enabled(&state.db, SETTING_KEY)
}

/// Applies `f` under the lock, then sends the new state as a `live_recap` frame.
fn update(m: &Meeting, f: impl FnOnce(&mut Recap)) -> RecapState {
    let snapshot = {
        let mut r = m.recap.lock().unwrap();
        f(&mut r);
        r.state.clone()
    };
    if let Ok(v) = serde_json::to_value(WsEvent::LiveRecap(snapshot.clone())) {
        let _ = m.events_tx.send(v);
    }
    snapshot
}

async fn run(state: AppState, m: Arc<Meeting>, mut rx: broadcast::Receiver<TranscriptEvent>) {
    let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + TICK, TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut call: Option<Update> = None;
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(ev) => accept(&state, &m, ev),
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "live recap lagged, segments dropped");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    m.recap.lock().unwrap().running = false;
                    return;
                }
            },
            _ = ticker.tick() => {
                if call.is_none() {
                    drain(&state, &m, &mut rx);
                    call = begin(&state, &m, WORD_FLOOR);
                }
            }
            _ = m.recap_refresh.notified() => {}
            (epoch, window, res) = async { call.as_mut().expect("guarded").await }, if call.is_some() => {
                call = None;
                finish(&m, epoch, window, res);
            }
        }
        // Under the lock `spawn` and `stop` take, so a resume either finds this task running or
        // starts a new one, and the last update always runs before the exit. Loops because an asked-for
        // update can find nothing to send, and the exit must still follow.
        while call.is_none() {
            let wanted = {
                let mut r = m.recap.lock().unwrap();
                let wanted = std::mem::take(&mut r.wanted);
                if !wanted && !r.recording {
                    r.running = false;
                    return;
                }
                wanted
            };
            if !wanted {
                break;
            }
            drain(&state, &m, &mut rx);
            call = begin(&state, &m, 1);
        }
    }
}

/// `select!` picks a ready branch at random, so segments already queued are taken in before an update.
fn drain(state: &AppState, m: &Meeting, rx: &mut broadcast::Receiver<TranscriptEvent>) {
    while let Ok(ev) = rx.try_recv() {
        accept(state, m, ev);
    }
}

/// Speech while paused or with the setting off is dropped here, so no later update can send it.
fn accept(state: &AppState, m: &Meeting, ev: TranscriptEvent) {
    let text = ev.text.trim();
    if !ev.is_final || text.is_empty() || text.starts_with("[stt ") {
        return;
    }
    let on = enabled(state);
    let mut r = m.recap.lock().unwrap();
    let settling = r.resumed_at.is_some_and(|t| t.elapsed() < RESUME_GRACE);
    if on && !r.state.paused && !settling {
        r.pending.push(ev);
    }
}

fn words(segments: &[TranscriptEvent]) -> usize {
    segments
        .iter()
        .map(|ev| ev.text.split_whitespace().count())
        .sum()
}

/// Starts an update when at least `floor` new words are buffered.
fn begin(state: &AppState, m: &Meeting, floor: usize) -> Option<Update> {
    let on = enabled(state);
    let (epoch, earlier, window) = {
        let mut r = m.recap.lock().unwrap();
        if !on || r.state.paused {
            r.pending.clear();
            return None;
        }
        let words = words(&r.pending);
        if words == 0 || words < floor {
            return None;
        }
        let skip = r.state.bullets.len().saturating_sub(CONTEXT_BULLETS);
        let earlier = r.state.bullets[skip..]
            .iter()
            .map(|b| format!("- [{}] {}", mmss(b.ts_ms), b.text))
            .collect::<Vec<_>>()
            .join("\n");
        (r.epoch, earlier, std::mem::take(&mut r.pending))
    };
    update(m, |r| r.state.status = Status::Updating);
    let state = state.clone();
    Some(Box::pin(async move {
        let res = summarize(&state, &earlier, &window).await;
        (epoch, window, res)
    }))
}

async fn summarize(
    state: &AppState,
    earlier: &str,
    window: &[TranscriptEvent],
) -> anyhow::Result<Vec<Bullet>> {
    let segments: Vec<_> = window.iter().map(crate::meetings::segment_json).collect();
    let lines = crate::api::chat::transcript_lines(&serde_json::Value::Array(segments).to_string());
    let prompt = state.prompts.render_live_recap(earlier, &lines)?;
    // Unlike Enhance, no mock fallback: an empty recap would look like nothing worth noting was said.
    let llm = crate::llm_openai::resolve_configured(state)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No LLM provider is set up. Add one in Settings > Model"))?;
    let model = llm.model_name();
    let timeout = llm.response_timeout();
    let req = ChatRequest {
        messages: vec![ChatMessage::user(prompt)],
        stream: false,
    };
    let reply = match tokio::time::timeout(timeout, llm.complete(req)).await {
        Ok(Ok(r)) => r.content,
        Ok(Err(e)) => anyhow::bail!("{model} failed: {e}"),
        Err(_) => anyhow::bail!("{model} did not respond within {}s", timeout.as_secs()),
    };
    let lo = window.iter().map(|ev| ev.ts_ms / 1000).min().unwrap_or(0);
    let hi = window.iter().map(|ev| ev.ts_ms / 1000).max().unwrap_or(0);
    Ok(parse_bullets(
        &crate::enhance::strip_model_reasoning(&reply),
        lo,
        hi,
    ))
}

/// A failed window goes back in front of the buffer so the next tick retries it, oldest words
/// dropped past [`MAX_PENDING_WORDS`].
fn finish(m: &Meeting, epoch: u64, window: Vec<TranscriptEvent>, res: anyhow::Result<Vec<Bullet>>) {
    if let Err(e) = &res {
        tracing::warn!(event = "live_recap_failed", meeting_id = %m.id, error = %e, "live recap update failed");
    }
    update(m, |r| {
        if r.epoch != epoch {
            return;
        }
        match res {
            Ok(bullets) => {
                r.state.bullets.extend(bullets);
                r.state.status = Status::Idle;
                r.state.error = None;
                r.state.updated_at = Some(crate::routes::now_unix_ms());
            }
            Err(e) => {
                r.state.status = Status::Error;
                r.state.error = Some(e.to_string());
                if !r.state.paused {
                    let newer = std::mem::replace(&mut r.pending, window);
                    r.pending.extend(newer);
                    let mut newest = 0;
                    let over = r.pending.iter().rposition(|ev| {
                        newest += ev.text.split_whitespace().count();
                        newest > MAX_PENDING_WORDS
                    });
                    // Keeps the newest segment even when it alone is over the cap.
                    let cut = over.map_or(0, |i| (i + 1).min(r.pending.len() - 1));
                    r.pending.drain(..cut);
                }
            }
        }
    });
}

/// Keeps `- [mm:ss] fact` lines, at most [`MAX_NEW_BULLETS`]. A time outside `lo_s..=hi_s`, the
/// window the model saw, clamps to `lo_s` so a bullet never points at a moment it did not summarize.
fn parse_bullets(reply: &str, lo_s: u64, hi_s: u64) -> Vec<Bullet> {
    reply
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix(['-', '*'])?.trim_start();
            let (stamp, text) = rest.strip_prefix('[')?.split_once(']')?;
            let secs = parse_stamp(stamp)?;
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            let secs = if (lo_s..=hi_s).contains(&secs) {
                secs
            } else {
                lo_s
            };
            Some(Bullet {
                ts_ms: secs * 1000,
                text: text.to_string(),
            })
        })
        .take(MAX_NEW_BULLETS)
        .collect()
}

/// `mm:ss` (minutes may exceed 59, as `transcript_lines` writes them) or `h:mm:ss`.
fn parse_stamp(s: &str) -> Option<u64> {
    let parts: Vec<u64> = s
        .trim()
        .split(':')
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    match parts[..] {
        [m, s] if s < 60 => Some(m * 60 + s),
        [h, m, s] if m < 60 && s < 60 => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
}

fn mmss(ts_ms: u64) -> String {
    let s = ts_ms / 1000;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// `GET /api/meetings/{id}/recap`. A meeting the registry does not hold has no recap.
pub async fn get_recap(State(state): State<AppState>, Path(id): Path<Uuid>) -> Json<RecapState> {
    Json(match state.meetings.get(&id).await {
        Some(m) => m.recap.lock().unwrap().state.clone(),
        None => RecapState::default(),
    })
}

#[derive(Deserialize)]
pub struct PatchRecap {
    paused: bool,
}

/// `PATCH /api/meetings/{id}/recap` with `{"paused": bool}`. Pausing drops what is buffered, since
/// the words that prompted it were likely just said.
pub async fn patch_recap(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchRecap>,
) -> Response {
    let Some(m) = state.meetings.get(&id).await else {
        return not_found();
    };
    let snapshot = update(&m, |r| {
        if r.state.paused == body.paused {
            return;
        }
        r.state.paused = body.paused;
        r.pending.clear();
        if !body.paused {
            r.resumed_at = Some(tokio::time::Instant::now());
        }
    });
    Json(snapshot).into_response()
}

/// `POST /api/meetings/{id}/recap/refresh`: Update now, which skips the word floor.
pub async fn refresh_recap(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let Some(m) = state.meetings.get(&id).await else {
        return not_found();
    };
    if m.task.lock().await.is_none() {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "meeting is not currently recording" })),
        )
            .into_response();
    }
    request_update(&m);
    (StatusCode::ACCEPTED, Json(json!({ "status": "queued" }))).into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "meeting not found" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use yogurt_llm::{ChatChunk, ChatResponse, LlmClient};
    use yogurt_stt::Channel;

    /// Replies in order, then `(none)`. `gate` holds each call until the test releases it.
    #[derive(Default)]
    struct ScriptLlm {
        replies: std::sync::Mutex<VecDeque<Result<&'static str, &'static str>>>,
        prompts: std::sync::Mutex<Vec<String>>,
        gate: Option<tokio::sync::Notify>,
    }

    impl ScriptLlm {
        fn prompts(&self) -> Vec<String> {
            self.prompts.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl LlmClient for ScriptLlm {
        fn model_name(&self) -> String {
            "script".into()
        }

        // Paused test time auto-advances past any real timeout while a call is gated.
        fn response_timeout(&self) -> Duration {
            Duration::from_secs(86_400)
        }

        async fn complete(&self, req: ChatRequest) -> anyhow::Result<ChatResponse> {
            self.prompts
                .lock()
                .unwrap()
                .push(req.messages[0].content.clone());
            if let Some(g) = &self.gate {
                g.notified().await;
            }
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok("(none)"));
            match reply {
                Ok(content) => Ok(ChatResponse {
                    content: content.into(),
                    model: "script".into(),
                }),
                Err(e) => Err(anyhow::anyhow!(e)),
            }
        }

        async fn stream(
            &self,
            _req: ChatRequest,
        ) -> anyhow::Result<futures_util::stream::BoxStream<'static, anyhow::Result<ChatChunk>>>
        {
            unreachable!("the recap never streams")
        }
    }

    struct Harness {
        state: AppState,
        m: Arc<Meeting>,
        llm: Arc<ScriptLlm>,
        _tmp: tempfile::TempDir,
    }

    impl Harness {
        async fn new(replies: &[Result<&'static str, &'static str>], gated: bool) -> Self {
            Self::build(replies, gated, true).await
        }

        /// `scripted: false` leaves no provider set, so `resolve` falls back to the mock.
        async fn build(
            replies: &[Result<&'static str, &'static str>],
            gated: bool,
            scripted: bool,
        ) -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let storage =
                Arc::new(crate::storage::Storage::init_at(&tmp.path().join("db.sqlite")).unwrap());
            let session = Arc::new(
                crate::session::load_or_create(&tmp.path().join("session-token")).unwrap(),
            );
            let mut state = AppState::in_memory(
                crate::Mode::Release,
                storage,
                session,
                7878,
                tmp.path().join("notes"),
            )
            .unwrap();
            let llm = Arc::new(ScriptLlm {
                replies: std::sync::Mutex::new(replies.iter().copied().collect()),
                gate: gated.then(Default::default),
                ..Default::default()
            });
            if scripted {
                state.llm_override = Some(llm.clone());
            }
            let m = state.meetings.create().await;
            let h = Self {
                state,
                m,
                llm,
                _tmp: tmp,
            };
            h.start();
            h
        }

        fn start(&self) {
            spawn(
                self.state.clone(),
                self.m.clone(),
                self.m.transcript_tx.subscribe(),
            );
        }

        fn say(&self, at_s: u64, word: &str, n: usize) {
            let text = vec![word; n].join(" ");
            self.m
                .transcript_tx
                .send(TranscriptEvent {
                    ts_ms: at_s * 1000,
                    channel: Channel::System,
                    text,
                    is_final: true,
                    confidence: None,
                })
                .unwrap();
        }

        fn recap(&self) -> RecapState {
            self.m.recap.lock().unwrap().state.clone()
        }

        fn calls(&self) -> usize {
            self.llm.prompts.lock().unwrap().len()
        }

        fn set_enabled(&self, on: bool) {
            yogurt_db::settings::set(
                &self.state.db,
                SETTING_KEY,
                if on { "true" } else { "false" },
            )
            .unwrap();
        }

        async fn set_paused(&self, paused: bool) {
            let res = patch_recap(
                State(self.state.clone()),
                Path(self.m.id),
                Json(PatchRecap { paused }),
            )
            .await;
            assert_eq!(res.status(), StatusCode::OK);
        }
    }

    /// One second past the next tick, so the tick always fires first.
    async fn tick() {
        tokio::time::sleep(TICK + Duration::from_secs(1)).await;
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_the_word_floor_then_appends_bullets() {
        let h = Harness::new(
            &[Ok(
                "- [00:10] Pro tier at $18\n- [00:40] Finance caps the discount",
            )],
            false,
        )
        .await;
        let mut frames = h.m.events_tx.subscribe();
        h.say(10, "pricing", 50);
        tick().await;
        assert_eq!(h.calls(), 0, "50 words is below the floor");

        h.say(40, "discount", 60);
        tick().await;
        assert_eq!(h.calls(), 1);
        let prompt = &h.llm.prompts()[0];
        assert!(prompt.contains("[00:10] them: pricing"), "{prompt}");
        assert!(prompt.contains("[00:40] them: discount"), "{prompt}");

        let s = h.recap();
        assert_eq!(
            s.bullets,
            vec![
                Bullet {
                    ts_ms: 10_000,
                    text: "Pro tier at $18".into()
                },
                Bullet {
                    ts_ms: 40_000,
                    text: "Finance caps the discount".into()
                },
            ]
        );
        assert_eq!(s.status, Status::Idle);
        assert!(s.updated_at.is_some());

        let mut last = None;
        while let Ok(f) = frames.try_recv() {
            if f["type"] == "live_recap" {
                last = Some(f);
            }
        }
        let got = get_recap(State(h.state.clone()), Path(h.m.id)).await.0;
        let mut want = serde_json::to_value(&got).unwrap();
        want["type"] = "live_recap".into();
        assert_eq!(last, Some(want), "the last frame matches GET");
    }

    #[tokio::test(start_paused = true)]
    async fn update_now_skips_the_floor_but_needs_new_words() {
        let h = Harness::new(&[Ok("- [00:03] Hello")], false).await;
        request_update(&h.m);
        settle().await;
        assert_eq!(h.calls(), 0, "nothing new to send");

        h.say(3, "hello", 5);
        request_update(&h.m);
        settle().await;
        assert_eq!(h.calls(), 1);
        assert_eq!(h.recap().bullets.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_failure_keeps_bullets_and_retries_its_window() {
        let h = Harness::new(&[Ok("- [00:05] Alpha"), Err("boom")], false).await;
        h.say(5, "alpha", 120);
        tick().await;
        h.say(200, "bravo", 120);
        tick().await;
        let s = h.recap();
        assert_eq!(s.status, Status::Error);
        assert!(s.error.as_deref().unwrap().contains("boom"), "{s:?}");
        assert_eq!(s.bullets.len(), 1, "earlier bullets survive");

        h.say(400, "charlie", 120);
        tick().await;
        let prompt = h.llm.prompts().pop().unwrap();
        assert!(prompt.contains("- [00:05] Alpha"), "context: {prompt}");
        assert!(prompt.contains("them: bravo"), "retried window: {prompt}");
        assert!(prompt.contains("them: charlie"), "{prompt}");
        assert_eq!(h.recap().status, Status::Idle);
        assert_eq!(h.recap().error, None);
    }

    #[tokio::test(start_paused = true)]
    async fn speech_with_the_setting_off_is_never_sent() {
        let h = Harness::new(&[], false).await;
        h.set_enabled(false);
        h.say(5, "secret", 120);
        tick().await;
        assert_eq!(h.calls(), 0);

        h.set_enabled(true);
        h.say(200, "open", 120);
        tick().await;
        assert_eq!(h.calls(), 1);
        assert!(!h.llm.prompts()[0].contains("secret"));
    }

    #[tokio::test(start_paused = true)]
    async fn one_call_in_flight_at_a_time() {
        let h = Harness::new(&[Ok("- [00:05] First")], true).await;
        h.say(5, "alpha", 120);
        tick().await;
        assert_eq!(h.calls(), 1);
        assert_eq!(h.recap().status, Status::Updating);

        h.say(200, "bravo", 120);
        request_update(&h.m);
        tick().await;
        assert_eq!(h.calls(), 1, "no second call while one runs");

        h.llm.gate.as_ref().unwrap().notify_one();
        settle().await;
        assert_eq!(h.recap().bullets.len(), 1);
        h.llm.gate.as_ref().unwrap().notify_one();
        tick().await;
        assert_eq!(h.calls(), 2);
        assert!(h.llm.prompts()[1].contains("them: bravo"));
    }

    #[tokio::test(start_paused = true)]
    async fn paused_speech_never_reaches_a_prompt() {
        let h = Harness::new(&[], false).await;
        h.say(5, "before", 120);
        h.set_paused(true).await;
        h.say(20, "secret", 120);
        tick().await;
        assert_eq!(h.calls(), 0);

        stop(&h.m);
        settle().await;
        assert_eq!(h.calls(), 0, "no last update while paused");
        h.start();
        assert!(h.recap().paused, "the pause survives a stop and resume");
        h.say(300, "secret", 120);
        tick().await;
        assert_eq!(h.calls(), 0);

        h.set_paused(false).await;
        tokio::time::sleep(RESUME_GRACE).await;
        h.say(500, "after", 120);
        tick().await;
        h.say(700, "closing", 3);
        stop(&h.m);
        settle().await;
        let prompts = h.llm.prompts();
        assert_eq!(
            prompts.len(),
            2,
            "a tick, then the last update with no floor"
        );
        assert!(prompts[0].contains("them: after"));
        assert!(prompts[1].contains("them: closing"));
        for p in &prompts {
            assert!(!p.contains("secret") && !p.contains("before"), "{p}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn close_discards_a_late_update() {
        let h = Harness::new(&[Ok("- [00:05] Late")], true).await;
        h.say(5, "alpha", 5);
        stop(&h.m);
        settle().await;
        assert_eq!(h.calls(), 1, "the last update is in flight");
        close(&h.m);
        h.llm.gate.as_ref().unwrap().notify_one();
        settle().await;
        assert!(h.recap().bullets.is_empty());
        assert_eq!(h.recap().status, Status::Idle);
    }

    #[tokio::test(start_paused = true)]
    async fn a_final_landing_right_after_resume_is_dropped() {
        let h = Harness::new(&[], false).await;
        h.say(5, "before", 120);
        h.set_paused(false).await;
        settle().await;
        assert_eq!(
            h.m.recap.lock().unwrap().pending.len(),
            1,
            "a no-op resume keeps the buffer"
        );

        h.set_paused(true).await;
        h.set_paused(false).await;
        h.say(50, "secret", 120);
        tokio::time::sleep(RESUME_GRACE).await;
        h.say(70, "open", 120);
        tick().await;
        let prompts = h.llm.prompts();
        assert_eq!(prompts.len(), 1);
        assert!(!prompts[0].contains("secret"), "{}", prompts[0]);
        assert!(prompts[0].contains("them: open"));
    }

    #[tokio::test(start_paused = true)]
    async fn update_now_during_a_call_runs_once_it_lands() {
        let h = Harness::new(&[Ok("- [00:05] First"), Ok("- [03:20] Second")], true).await;
        h.say(5, "alpha", 120);
        tick().await;
        h.say(200, "bravo", 5);
        request_update(&h.m);
        settle().await;
        assert_eq!(h.calls(), 1);

        let gate = h.llm.gate.as_ref().unwrap();
        gate.notify_one();
        settle().await;
        assert_eq!(h.calls(), 2, "the asked-for update follows the tick's");
        assert!(h.llm.prompts()[1].contains("them: bravo"));
        gate.notify_one();
        settle().await;
        assert_eq!(h.recap().bullets.len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_failures_keep_only_the_newest_words() {
        let h = Harness::new(&[Err("down"), Err("down"), Err("down"), Err("down")], false).await;
        for i in 0..4 {
            h.say(i * 200, &format!("w{i}"), 800);
            tick().await;
        }
        let pending = words(&h.m.recap.lock().unwrap().pending);
        assert!(pending <= MAX_PENDING_WORDS, "{pending}");
        let last = h.llm.prompts().pop().unwrap();
        assert!(!last.contains("them: w0"), "the oldest window was dropped");
    }

    #[tokio::test(start_paused = true)]
    async fn no_provider_is_an_error_rather_than_an_empty_update() {
        let h = Harness::build(&[], false, false).await;
        h.say(5, "hello", 5);
        stop(&h.m);
        settle().await;
        let s = h.recap();
        assert_eq!(s.status, Status::Error);
        assert!(s.error.unwrap().contains("Settings > Model"));
        assert_eq!(s.updated_at, None);
    }

    #[tokio::test(start_paused = true)]
    async fn the_task_exits_after_the_last_update_and_a_resume_starts_a_new_one() {
        let h = Harness::new(&[Ok("- [00:05] One"), Ok("- [03:20] Two")], false).await;
        h.say(5, "alpha", 5);
        stop(&h.m);
        settle().await;
        assert!(!h.m.recap.lock().unwrap().running);
        h.start();
        h.say(200, "bravo", 120);
        tick().await;
        assert_eq!(h.recap().bullets.len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn the_task_exits_at_stop_with_nothing_to_send() {
        let h = Harness::new(&[], false).await;
        stop(&h.m);
        settle().await;
        assert!(!h.m.recap.lock().unwrap().running);
        assert_eq!(h.calls(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_resume_after_enhance_cannot_revive_the_old_recap() {
        let h = Harness::new(&[Ok("- [00:05] Stale")], true).await;
        h.say(5, "alpha", 5);
        stop(&h.m);
        settle().await;
        close(&h.m);
        h.start();
        h.llm.gate.as_ref().unwrap().notify_one();
        settle().await;
        assert!(h.recap().bullets.is_empty());
    }

    #[tokio::test]
    async fn update_now_needs_a_recording() {
        let h = Harness::new(&[], false).await;
        let res = refresh_recap(State(h.state.clone()), Path(h.m.id)).await;
        assert_eq!(res.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn parses_bullets_and_clamps_times_to_the_window() {
        let reply = "Here you go:\n- [01:05] Dana proposes $18\n* [1:02:03] Late\n- [00:01] Too early\n- [02:00]\n- no stamp\n- [9:99] bad";
        assert_eq!(
            parse_bullets(reply, 60, 4000),
            vec![
                Bullet {
                    ts_ms: 65_000,
                    text: "Dana proposes $18".into()
                },
                Bullet {
                    ts_ms: 3_723_000,
                    text: "Late".into()
                },
                Bullet {
                    ts_ms: 60_000,
                    text: "Too early".into()
                },
            ]
        );
        assert!(parse_bullets("(none)", 0, 10).is_empty());
        let many = "- [00:01] a\n- [00:02] b\n- [00:03] c\n- [00:04] d\n- [00:05] e";
        assert_eq!(parse_bullets(many, 0, 10).len(), MAX_NEW_BULLETS);
        let thinking = crate::enhance::strip_model_reasoning(
            "<think>\n- [00:01] not this\n</think>\n- [00:02] this",
        );
        assert_eq!(parse_bullets(&thinking, 0, 10)[0].text, "this");
    }
}
