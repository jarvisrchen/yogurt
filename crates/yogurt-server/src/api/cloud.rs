//! Yogurt Cloud sync: connect this machine to the user's own cloud, push meetings, disconnect.
//! The whole connection is one `keys.json` account, so connect and disconnect are atomic.
//! Nothing here returns, logs, or stores a secret outside that file. The password is used once to sign in and dropped.
//!
//! Every operation that touches the refresh token runs in a spawned task and holds [`Held`] to the end: `AppState::cloud_lock` in this process and a lock file beside `keys.json` across processes.
//! Another yogurt on the same data dir may have rotated the refresh token, so each operation reads the connection fresh from disk.
//! hyper drops a handler future when the client disconnects, and a dropped future would lose a rotated refresh token that was not saved yet.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use std::time::Duration;
use tokio::sync::{watch, MutexGuard};
use yogurt_cloud::push::{self, LocalMeeting, Pass, Plan, Progress};
use yogurt_cloud::{config, Error as CloudError, Project, Session};
use yogurt_db::keys::ProcessLock;
use yogurt_db::MeetingRepo;

use crate::cloud_push::Pending;
use crate::ws::{self, WsEvent};
use crate::AppState;

const ACCOUNT: &str = "yogurt-cloud";
/// The last pass that succeeded, kept apart from the last failure so a failed pass never hides when the cloud was last in step.
const LAST_PUSH_KEY: &str = "cloud.last_push";
const LAST_ERROR_KEY: &str = "cloud.last_error";
/// The last [`HISTORY_LEN`] pushes, newest first, for the Settings history.
const HISTORY_KEY: &str = "cloud.push_history";
const HISTORY_LEN: usize = 20;
const INTERVAL_KEY: &str = "cloud.push_interval";
/// `meeting` pushes after each meeting and keeps a daily push as the backstop for edits made later.
const INTERVALS: [(&str, i64); 4] = [
    ("meeting", 86_400_000),
    ("15m", 15 * 60_000),
    ("1h", 3_600_000),
    ("1d", 86_400_000),
];
/// How long the `meeting` setting waits after a request, so a stop and the notes that finish soon after go out in one push.
const SETTLE_MS: i64 = 60_000;
/// Failures only the user can fix. The timer stays quiet until a sign-in, a connect, or a manual push clears the error.
const USER_FIXABLE: [&str; 4] = [
    "session_expired",
    "device_removed",
    "not_owner",
    "passcode_not_set",
];
/// How often the auto-push task wakes to compare the clock with the setting.
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// After a failed pass the next automatic attempt waits this long, so an outage or a dead cloud is not hammered every wake.
const RETRY_AFTER_FAILURE_MS: i64 = 5 * 60_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/cloud", get(status))
        .route("/api/cloud/check", post(check))
        .route("/api/cloud/connect", post(connect))
        .route("/api/cloud/sign-in", post(sign_in))
        .route("/api/cloud/push", post(push_now))
        .route("/api/cloud/push-interval", post(set_push_interval))
        .route("/api/cloud/disconnect", post(disconnect))
}

/// The `yogurt-cloud` keys.json value. Deliberately not `Debug`, because it holds the refresh token.
#[derive(Clone, Serialize, Deserialize)]
struct Connection {
    address: String,
    email: String,
    supabase_url: String,
    publishable_key: String,
    refresh_token: String,
    device_id: String,
    machine_name: String,
}

/// `{"error": <message>, "code": <stable id>}` with a status chosen from the code.
#[derive(Debug)]
pub(crate) struct CloudApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl CloudApiError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: status_for(code),
            code,
            message: message.into(),
        }
    }
    /// An error whose text is fixed by its code.
    fn coded(code: &'static str) -> Self {
        Self::new(code, message_for(code, ""))
    }
    fn internal(e: impl std::fmt::Display) -> Self {
        Self::new("internal", e.to_string())
    }
}

fn status_for(code: &str) -> StatusCode {
    match code {
        "bad_address" | "bad_request" => StatusCode::BAD_REQUEST,
        "bad_credentials" | "session_expired" => StatusCode::UNAUTHORIZED,
        "not_owner" => StatusCode::FORBIDDEN,
        "passcode_not_set" => StatusCode::PRECONDITION_FAILED,
        "not_connected" | "already_connected" | "device_removed" | "push_running" => {
            StatusCode::CONFLICT
        }
        "bad_config" | "network" | "cloud_error" => StatusCode::BAD_GATEWAY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// The text the UI shows for each code. Anything not listed falls back to the error's own message, which never carries a secret.
fn message_for(code: &str, fallback: &str) -> String {
    match code {
        "session_expired" => "Your Cloud session expired. Sign in again.",
        "bad_credentials" => "Wrong email or password.",
        "not_owner" => "This login does not own that Yogurt Cloud",
        "passcode_not_set" => "Choose a PIN in the webapp first",
        "not_connected" => "This Mac is not connected to a Yogurt Cloud.",
        "already_connected" => "This Mac is already connected. Disconnect first.",
        "cancelled" => "The push was stopped.",
        "device_removed" => {
            "This machine was removed from the Yogurt Cloud. Disconnect, then connect again."
        }
        _ => fallback,
    }
    .to_string()
}

impl From<CloudError> for CloudApiError {
    fn from(e: CloudError) -> Self {
        let code = e.code();
        Self::new(code, message_for(code, &e.to_string()))
    }
}

/// While connecting, a 42501 means the login is not the owner. Anywhere else it can be the one-hour push window.
fn owner_error(e: CloudError) -> CloudApiError {
    match &e {
        CloudError::Http { code, .. } if code == "42501" => CloudApiError::coded("not_owner"),
        _ => e.into(),
    }
}

impl IntoResponse for CloudApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "error": self.message, "code": self.code })),
        )
            .into_response()
    }
}

type ApiResult<T> = Result<T, CloudApiError>;

async fn detached<T: Send + 'static>(
    f: impl Future<Output = ApiResult<T>> + Send + 'static,
) -> ApiResult<T> {
    tokio::spawn(f).await.map_err(CloudApiError::internal)?
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn load(state: &AppState) -> ApiResult<Option<Connection>> {
    let raw = state
        .keys
        .get_fresh(ACCOUNT)
        .map_err(CloudApiError::internal)?;
    raw.map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(|_| {
            CloudApiError::internal(
                "The Yogurt Cloud entry in keys.json is unreadable. Disconnect and connect again.",
            )
        })
}

fn save(state: &AppState, conn: &Connection) -> ApiResult<()> {
    let raw = serde_json::to_string(conn).map_err(CloudApiError::internal)?;
    state
        .keys
        .set(ACCOUNT, &raw)
        .map_err(CloudApiError::internal)
}

/// The in-process mutex and the cross-process file lock, released together.
struct Held<'a> {
    _mutex: MutexGuard<'a, ()>,
    _file: ProcessLock,
}

async fn acquire(state: &AppState) -> ApiResult<Held<'_>> {
    let mutex = state.cloud_lock.lock().await;
    let keys = state.keys.clone();
    let file = blocking(move || keys.lock_exclusive()).await?;
    Ok(Held {
        _mutex: mutex,
        _file: file,
    })
}

struct Link {
    conn: Connection,
    project: Project,
    session: Session,
}

/// Refreshes the session and persists the rotated refresh token before anything else uses it.
/// The guard parameter makes holding both locks a compile-time requirement.
async fn link(state: &AppState, _held: &Held<'_>) -> ApiResult<Link> {
    let mut conn = load(state)?.ok_or_else(|| CloudApiError::coded("not_connected"))?;
    let project = Project::new(&conn.supabase_url, &conn.publishable_key);
    let session = project.refresh(&conn.refresh_token).await?;
    conn.refresh_token = session.refresh_token.clone();
    save(state, &conn)?;
    Ok(Link {
        conn,
        project,
        session,
    })
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> ApiResult<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(CloudApiError::internal)?
        .map_err(|e| CloudApiError::internal(format!("{e:#}")))
}

/// Every meeting's hash and size, built one bundle at a time so the library is never in memory at once.
fn local_meetings(repo: &MeetingRepo) -> anyhow::Result<Vec<LocalMeeting>> {
    let stored = repo.cloud_hashes()?;
    let mut out = Vec::with_capacity(stored.len());
    for (id, hash) in stored {
        if let Some(body) = repo.meeting_bundle(&id)? {
            out.push(LocalMeeting::new(id, hash, &body));
        }
    }
    Ok(out)
}

async fn recording_id(state: &AppState) -> Option<String> {
    state
        .meetings
        .active_recording()
        .await
        .map(|id| id.to_string())
}

fn recorded(state: &AppState, key: &str) -> Value {
    yogurt_db::settings::get(&state.db, key)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

fn record(state: &AppState, key: &str, value: &Value) {
    if let Err(e) = yogurt_db::settings::set(&state.db, key, &value.to_string()) {
        tracing::warn!(error = %e, "could not record cloud sync state");
    }
}

/// A new connection starts with nothing leaving this Mac until the user picks an interval again.
fn reset_interval(state: &AppState) {
    if let Err(e) = yogurt_db::settings::set(&state.db, INTERVAL_KEY, "off") {
        tracing::warn!(error = %e, "could not reset the auto-push interval");
    }
}

/// An idle timer finds nothing to send every wake, so a run of `up_to_date` entries collapses into the newest one instead of pushing real runs out.
fn add_history(state: &AppState, entry: Value) {
    let mut history = match recorded(state, HISTORY_KEY) {
        Value::Array(h) => h,
        _ => Vec::new(),
    };
    let idle = |e: &Value| e["outcome"] == "up_to_date";
    if idle(&entry) && history.first().is_some_and(idle) {
        history[0] = entry;
    } else {
        history.insert(0, entry);
    }
    history.truncate(HISTORY_LEN);
    record(state, HISTORY_KEY, &Value::Array(history));
}

fn clear_records(state: &AppState) {
    for key in [LAST_PUSH_KEY, LAST_ERROR_KEY, HISTORY_KEY] {
        record(state, key, &Value::Null);
    }
}

fn record_success(state: &AppState, trigger: &str, now: i64, changed: usize, deleted: usize) {
    record(
        state,
        LAST_PUSH_KEY,
        &json!({ "at": now, "changed": changed, "deleted": deleted }),
    );
    record(state, LAST_ERROR_KEY, &Value::Null);
    let outcome = if changed + deleted == 0 {
        "up_to_date"
    } else {
        "pushed"
    };
    add_history(
        state,
        json!({ "at": now, "trigger": trigger, "outcome": outcome, "changed": changed, "deleted": deleted }),
    );
}

fn record_failure(state: &AppState, trigger: &str, e: &CloudApiError, now: i64) {
    record(
        state,
        LAST_ERROR_KEY,
        &json!({ "at": now, "error": e.message, "code": e.code }),
    );
    add_history(
        state,
        json!({
            "at": now,
            "trigger": trigger,
            "outcome": "failed",
            "error": e.message,
            "code": e.code,
            "needs_you": USER_FIXABLE.contains(&e.code),
        }),
    );
}

fn interval_ms(name: &str) -> Option<i64> {
    INTERVALS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, ms)| *ms)
}

/// `off`, `meeting`, `15m`, `1h`, or `1d`. Anything else stored reads as `off`.
fn push_interval(state: &AppState) -> &'static str {
    let stored = yogurt_db::settings::get(&state.db, INTERVAL_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();
    INTERVALS
        .iter()
        .find(|(n, _)| *n == stored)
        .map_or("off", |(n, _)| n)
}

#[derive(Deserialize)]
struct IntervalBody {
    interval: String,
}

async fn set_push_interval(
    State(state): State<AppState>,
    Json(body): Json<IntervalBody>,
) -> ApiResult<Json<Value>> {
    if body.interval != "off" && interval_ms(&body.interval).is_none() {
        return Err(CloudApiError::new(
            "bad_request",
            "Auto-push interval must be off, meeting, 15m, 1h, or 1d.",
        ));
    }
    if load(&state)?.is_none() {
        return Err(CloudApiError::coded("not_connected"));
    }
    yogurt_db::settings::set(&state.db, INTERVAL_KEY, &body.interval)
        .map_err(CloudApiError::internal)?;
    status(State(state)).await
}

/// The meeting being recorded does not count, and a local deletion not yet sent does.
async fn pending(state: &AppState) -> ApiResult<Pending> {
    let recording = recording_id(state).await;
    let repo = state.meeting_repo.clone();
    let (local, tombstones) =
        blocking(move || Ok((local_meetings(&repo)?, repo.cloud_tombstones()?.len()))).await?;
    let sizes: Vec<usize> = local
        .iter()
        .filter(|m| Some(m.id.as_str()) != recording.as_deref())
        .filter(|m| m.cloud_hash.as_deref() != Some(&m.hash))
        .map(|m| m.size)
        .chain(std::iter::repeat_n(0, tombstones))
        .collect();
    Ok(Pending {
        meetings: local.len(),
        not_pushed: sizes.len(),
        bytes: sizes.iter().sum(),
        estimate_secs: push::estimate(&sizes, state.cloud_push.pacing())
            .as_secs_f64()
            .ceil() as u64,
    })
}

/// For display only: the Library and Settings ask on every visit, and an exact answer builds every bundle. The timer decides with [`pending`] instead.
async fn counted(state: &AppState) -> ApiResult<Pending> {
    let repo = state.meeting_repo.clone();
    let recording = recording_id(state).await.unwrap_or_default();
    let fingerprint = blocking(move || repo.cloud_fingerprint()).await? + &recording;
    if let Some(p) = state.cloud_push.counted(&fingerprint) {
        return Ok(p);
    }
    let p = pending(state).await?;
    state.cloud_push.remember(fingerprint, p);
    Ok(p)
}

async fn status(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let Some(conn) = load(&state)? else {
        return Ok(Json(json!({ "connected": false })));
    };
    let push = state.cloud_push.status(now_ms());
    let left = push["total"].as_u64().filter(|t| *t > 0);
    let pending = match left {
        // Building every bundle again would cost a poll a second while a large pass runs, and the job already knows what is left.
        Some(total) => {
            let repo = state.meeting_repo.clone();
            let meetings = blocking(move || Ok(repo.cloud_hashes()?.len())).await?;
            let sent = push["sent"].as_u64().unwrap_or(0);
            Pending {
                meetings,
                not_pushed: total.saturating_sub(sent) as usize,
                ..Pending::default()
            }
        }
        None => counted(&state).await?,
    };
    Ok(Json(json!({
        "connected": true,
        "push": push,
        "address": conn.address,
        "email": conn.email,
        "machine_name": conn.machine_name,
        "meetings": pending.meetings,
        "not_pushed": pending.not_pushed,
        "not_pushed_bytes": pending.bytes,
        "push_estimate_secs": pending.estimate_secs,
        "last_push": recorded(&state, LAST_PUSH_KEY),
        "last_error": recorded(&state, LAST_ERROR_KEY),
        "push_history": recorded(&state, HISTORY_KEY),
        "push_interval": push_interval(&state),
        // Never in the past, and null when auto-push is off or paused on a failure only the user can fix.
        "next_push": TimerInputs::read(&state)
            .and_then(|t| t.next_push())
            .map(|at| at.max(now_ms())),
    })))
}

#[derive(Deserialize)]
struct CheckBody {
    address: String,
}

/// Lets the connect form say "Found a Yogurt Cloud" before the user types a password.
async fn check(Json(body): Json<CheckBody>) -> ApiResult<Json<Value>> {
    let address = config::normalize_address(&body.address)?;
    config::fetch(&address).await?;
    Ok(Json(json!({ "address": address })))
}

#[derive(Deserialize)]
struct ConnectBody {
    address: String,
    email: String,
    password: String,
    machine_name: String,
}

/// Fails unless the login owns the cloud and the owner has chosen a PIN.
async fn require_ready(project: &Project, session: &Session) -> ApiResult<()> {
    let st = project
        .rpc(session, "yogurt_status", &json!({}))
        .await
        .map_err(owner_error)?;
    if st["is_owner"].as_bool() != Some(true) {
        return Err(CloudApiError::coded("not_owner"));
    }
    if st["passcode_set"].as_bool() != Some(true) {
        return Err(CloudApiError::coded("passcode_not_set"));
    }
    Ok(())
}

async fn connect(
    State(state): State<AppState>,
    Json(body): Json<ConnectBody>,
) -> ApiResult<Json<Value>> {
    let machine_name = body.machine_name.trim().to_string();
    let email = body.email.trim().to_string();
    if machine_name.is_empty() || machine_name.chars().count() > 80 {
        return Err(CloudApiError::new(
            "bad_request",
            "Machine name must be 1 to 80 characters.",
        ));
    }
    if email.is_empty() || body.password.is_empty() {
        return Err(CloudApiError::new(
            "bad_request",
            "Enter the Cloud login email and password.",
        ));
    }
    detached(async move {
        let _held = acquire(&state).await?;
        if load(&state).is_ok_and(|c| c.is_some()) {
            return Err(CloudApiError::coded("already_connected"));
        }
        let address = config::normalize_address(&body.address)?;
        let cfg = config::fetch(&address).await?;
        let project = Project::new(&cfg.supabase_url, &cfg.publishable_key);
        let session = project.sign_in(&email, &body.password).await?;
        require_ready(&project, &session).await?;

        let device_id = uuid::Uuid::now_v7().to_string();
        project
            .insert(
                &session,
                "yogurt_devices",
                &json!({ "id": device_id, "name": machine_name }),
            )
            .await
            .map_err(owner_error)?;
        save(
            &state,
            &Connection {
                address,
                email,
                supabase_url: cfg.supabase_url,
                publishable_key: cfg.publishable_key,
                refresh_token: session.refresh_token,
                device_id,
                machine_name,
            },
        )?;
        let repo = state.meeting_repo.clone();
        blocking(move || repo.clear_cloud_hashes()).await?;
        clear_records(&state);
        reset_interval(&state);
        status(State(state.clone())).await
    })
    .await
}

#[derive(Deserialize)]
struct SignInBody {
    password: String,
}

/// Replaces a dead refresh token after the session expired or was revoked. The email comes from the stored connection.
async fn sign_in(
    State(state): State<AppState>,
    Json(body): Json<SignInBody>,
) -> ApiResult<Json<Value>> {
    detached(async move {
        let _held = acquire(&state).await?;
        let mut conn = load(&state)?.ok_or_else(|| CloudApiError::coded("not_connected"))?;
        let project = Project::new(&conn.supabase_url, &conn.publishable_key);
        let session = project.sign_in(&conn.email, &body.password).await?;
        require_ready(&project, &session).await?;
        conn.refresh_token = session.refresh_token;
        save(&state, &conn)?;
        if USER_FIXABLE
            .iter()
            .any(|c| recorded(&state, LAST_ERROR_KEY)["code"] == *c)
        {
            record(&state, LAST_ERROR_KEY, &Value::Null);
        }
        status(State(state.clone())).await
    })
    .await
}

struct PassHooks<'a> {
    state: &'a AppState,
    conn: &'a mut Connection,
    repo: std::sync::Arc<MeetingRepo>,
    /// Whether this pass belongs to the background job, whose progress clients hear about.
    job: bool,
}

impl push::Hooks for PassHooks<'_> {
    fn bundle(&mut self, id: &str) -> Result<Option<Value>, String> {
        self.repo.meeting_bundle(id).map_err(|e| format!("{e:#}"))
    }

    fn refreshed(&mut self, renewed: &Session) -> Result<(), String> {
        self.conn.refresh_token = renewed.refresh_token.clone();
        save(self.state, self.conn).map_err(|e| e.message)
    }

    fn stored(&mut self, hashes: &[(String, String)]) -> Result<(), String> {
        hashes
            .iter()
            .try_for_each(|(id, hash)| self.repo.set_cloud_hash(id, hash))
            .map_err(|e| format!("{e:#}"))
    }

    fn progress(&mut self, p: &Progress) {
        let now = now_ms();
        if self.job && self.state.cloud_push.update(p, now) {
            ws::send_event(
                &self.state.app_events_tx,
                WsEvent::CloudPushProgress {
                    sent: p.sent,
                    total: p.total,
                    retry_in: p.retry_in.map(|d| d.as_secs_f64().ceil() as u64),
                    left_secs: self.state.cloud_push.left_secs(now),
                },
            );
        }
    }
}

/// One pass. `local` is every meeting on this Mac, or nothing when the whole device is being marked deleted.
async fn run_pass(
    state: &AppState,
    link: &mut Link,
    local: Vec<LocalMeeting>,
    skip: Option<String>,
    cancel: &mut watch::Receiver<bool>,
    job: bool,
) -> ApiResult<Plan> {
    let Link {
        conn,
        project,
        session,
    } = link;
    let device_id = conn.device_id.clone();
    let pass = Pass {
        project,
        device_id: &device_id,
        local: &local,
        skip: skip.as_deref(),
        pacing: state.cloud_push.pacing(),
    };
    let mut hooks = PassHooks {
        state,
        conn,
        repo: state.meeting_repo.clone(),
        job,
    };
    let plan = pass.run(session, cancel, &mut hooks).await;
    // A failed pass may have committed its earlier batches, and a pass with nothing to send may follow one that left the index behind, so the index catches up either way.
    if !matches!(plan, Err(CloudError::Cancelled)) {
        push::catch_up_quietly(project, session).await;
    }
    let plan = plan?;
    // The deletion markers went out with this pass.
    let repo = state.meeting_repo.clone();
    blocking(move || repo.clear_cloud_tombstones()).await?;
    Ok(plan)
}

/// The pass behind the Push button and the timer. `now` stamps the success or failure record.
async fn push_locked(
    state: &AppState,
    held: &Held<'_>,
    cancel: &mut watch::Receiver<bool>,
    trigger: &str,
    now: i64,
) -> ApiResult<Plan> {
    let pass = async {
        let recording = recording_id(state).await;
        let repo = state.meeting_repo.clone();
        let local = blocking(move || local_meetings(&repo)).await?;
        let mut link = link(state, held).await?;
        run_pass(state, &mut link, local, recording, cancel, true).await
    };
    match pass.await {
        Ok(plan) => {
            record_success(state, trigger, now, plan.changed.len(), plan.deleted.len());
            Ok(plan)
        }
        Err(e) => {
            if !matches!(e.code, "not_connected" | "cancelled") {
                record_failure(state, trigger, &e, now);
            }
            Err(e)
        }
    }
}

/// Runs to the end of the pass, then tells clients how it went if they were following it.
async fn push_task(
    state: AppState,
    mut cancel: watch::Receiver<bool>,
    trigger: &'static str,
    now: i64,
) {
    let result = match acquire(&state).await {
        Ok(held) => push_locked(&state, &held, &mut cancel, trigger, now).await,
        Err(e) => {
            record_failure(&state, trigger, &e, now);
            Err(e)
        }
    };
    let announced = state.cloud_push.finish();
    match &result {
        Ok(plan) => tracing::info!(
            changed = plan.changed.len(),
            deleted = plan.deleted.len(),
            "cloud push finished"
        ),
        Err(e) => tracing::warn!(code = e.code, "cloud push stopped"),
    }
    if announced {
        let ev = match result {
            Ok(plan) => WsEvent::CloudPushComplete {
                pushed: plan.changed.len(),
                deleted: plan.deleted.len(),
            },
            Err(e) if e.code == "cancelled" => WsEvent::CloudPushCancelled,
            Err(e) => WsEvent::CloudPushError {
                error: e.message,
                code: e.code.to_string(),
            },
        };
        ws::send_event(&state.app_events_tx, ev);
    }
}

/// Starts the one background push, or says why not. The result of the push itself arrives as frames and in `GET /api/cloud`.
/// `trigger` is `manual`, `auto`, or `meeting`, shown in the history.
fn start_push(state: &AppState, trigger: &'static str, now: i64) -> ApiResult<()> {
    if load(state)?.is_none() {
        return Err(CloudApiError::coded("not_connected"));
    }
    let cancel = state
        .cloud_push
        .begin()
        .ok_or_else(|| CloudApiError::new("push_running", "A push is already running."))?;
    tokio::spawn(push_task(state.clone(), cancel, trigger, now));
    Ok(())
}

async fn push_now(State(state): State<AppState>) -> ApiResult<(StatusCode, Json<Value>)> {
    start_push(&state, "manual", now_ms())?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "started": true }))))
}

/// When the timer may next push, or `None` while a failure only the user can fix stands.
/// `requested` is when a meeting last stopped or finished its notes. The wake that pushes comes up to [`CHECK_EVERY`] later.
fn next_push(
    interval_ms: i64,
    last_push: Option<i64>,
    last_error: Option<(i64, &str)>,
    requested: Option<i64>,
) -> Option<i64> {
    let retry = match last_error {
        Some((_, code)) if USER_FIXABLE.contains(&code) => return None,
        Some((at, _)) => at + RETRY_AFTER_FAILURE_MS,
        None => i64::MIN,
    };
    let auto = last_push.map_or(i64::MIN, |at| at + interval_ms);
    let meeting = settled(last_push, requested).unwrap_or(i64::MAX);
    Some(auto.min(meeting).max(retry))
}

/// When a meeting request not yet pushed has settled.
fn settled(last_push: Option<i64>, requested: Option<i64>) -> Option<i64> {
    requested
        .filter(|r| last_push.is_none_or(|p| *r > p))
        .map(|r| r + SETTLE_MS)
}

/// The trigger the timer should push for now, if any.
fn due(
    interval_ms: i64,
    last_push: Option<i64>,
    last_error: Option<(i64, &str)>,
    requested: Option<i64>,
    now: i64,
) -> Option<&'static str> {
    (next_push(interval_ms, last_push, last_error, requested)? <= now).then(|| {
        if settled(last_push, requested).is_some_and(|at| at <= now) {
            "meeting"
        } else {
            "auto"
        }
    })
}

/// What the timer decides on. `requested` is when a meeting last stopped or finished its notes, read only for the `meeting` setting.
struct TimerInputs {
    interval_ms: i64,
    last_push: Option<i64>,
    last_error: Option<(i64, String)>,
    requested: Option<i64>,
}

impl TimerInputs {
    /// `None` when auto-push is off.
    fn read(state: &AppState) -> Option<Self> {
        let setting = push_interval(state);
        let err = recorded(state, LAST_ERROR_KEY);
        Some(Self {
            interval_ms: interval_ms(setting)?,
            last_push: recorded(state, LAST_PUSH_KEY)["at"].as_i64(),
            last_error: err["at"]
                .as_i64()
                .map(|at| (at, err["code"].as_str().unwrap_or("").to_owned())),
            requested: (setting == "meeting")
                .then(|| state.cloud_push.requested())
                .flatten(),
        })
    }

    fn next_push(&self) -> Option<i64> {
        next_push(
            self.interval_ms,
            self.last_push,
            self.error(),
            self.requested,
        )
    }

    fn due(&self, now: i64) -> Option<&'static str> {
        due(
            self.interval_ms,
            self.last_push,
            self.error(),
            self.requested,
            now,
        )
    }

    fn error(&self) -> Option<(i64, &str)> {
        self.last_error
            .as_ref()
            .map(|(at, code)| (*at, code.as_str()))
    }
}

/// One wake of the timer. It takes both locks first and decides under them, so a push that just finished counts.
/// With nothing pending it only stamps the last push, so an idle laptop never touches the network or rewrites `keys.json`.
async fn auto_tick(state: &AppState, now: i64) {
    if push_interval(state) == "off" || state.cloud_push.is_running() {
        return;
    }
    let state = state.clone();
    let result = detached(async move {
        let _held = acquire(&state).await?;
        if load(&state)?.is_none() {
            return Ok(());
        }
        let Some(trigger) = TimerInputs::read(&state).and_then(|t| t.due(now)) else {
            return Ok(());
        };
        if pending(&state).await?.not_pushed == 0 {
            record_success(&state, trigger, now, 0, 0);
            return Ok(());
        }
        start_push(&state, trigger, now)
    })
    .await;
    if let Err(e) = result {
        tracing::warn!(code = e.code, "auto-push failed");
    }
}

/// `clock` returns wall-clock milliseconds, and only tests substitute it.
async fn drive(state: AppState, period: Duration, clock: impl Fn() -> i64) {
    let mut ticker = tokio::time::interval(period);
    // A missed tick is worthless, so delay rather than burst.
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        auto_tick(&state, clock()).await;
    }
}

pub fn spawn_auto_push(state: AppState) {
    tokio::spawn(drive(state, CHECK_EVERY, now_ms));
}

#[derive(Deserialize, Default)]
struct DisconnectBody {
    #[serde(default)]
    mark_deleted: bool,
}

/// With `mark_deleted`, one last push marks every meeting from this machine deleted first.
/// If that push fails nothing is disconnected, so the user can retry or disconnect without it.
/// The refresh token is revoked on the way out, when the cloud can be reached.
async fn disconnect(
    State(state): State<AppState>,
    body: Option<Json<DisconnectBody>>,
) -> ApiResult<Json<Value>> {
    let Json(body) = body.unwrap_or_default();
    state.cloud_push.cancel();
    detached(async move {
        let held = acquire(&state).await?;
        let linked = link(&state, &held).await;
        let mut linked = match linked {
            Ok(l) => Some(l),
            Err(e) if body.mark_deleted => {
                if e.code != "not_connected" {
                    record_failure(&state, "disconnect", &e, now_ms());
                }
                return Err(e);
            }
            Err(_) => None,
        };
        if let (true, Some(l)) = (body.mark_deleted, linked.as_mut()) {
            let (_stay, mut never) = watch::channel(false);
            if let Err(e) = run_pass(&state, l, Vec::new(), None, &mut never, false).await {
                record_failure(&state, "disconnect", &e, now_ms());
                return Err(e);
            }
        }
        if let Some(l) = &linked {
            let _ = l.project.logout(&l.session).await;
        }
        state
            .keys
            .delete(ACCOUNT)
            .map_err(CloudApiError::internal)?;
        let repo = state.meeting_repo.clone();
        blocking(move || repo.clear_cloud_hashes()).await?;
        clear_records(&state);
        reset_interval(&state);
        Ok(Json(json!({ "connected": false })))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const MIN: i64 = 60_000;
    const T0: i64 = 1_000_000_000;

    /// Connected to a cloud nobody listens on, so an attempted pass fails fast with a `network` error stamped with its `now`.
    fn state() -> (AppState, tempfile::TempDir) {
        with_meetings(1)
    }

    fn with_meetings(n: usize) -> (AppState, tempfile::TempDir) {
        let (state, tmp) = idle_state();
        for i in 0..n {
            state
                .meeting_repo
                .create(yogurt_db::NewMeeting {
                    title: format!("unpushed {i}"),
                    started_at_unix_ms: None,
                    id: None,
                })
                .unwrap();
        }
        (state, tmp)
    }

    /// Like [`state`] with no meeting, so nothing is pending.
    fn idle_state() -> (AppState, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let storage =
            Arc::new(crate::storage::Storage::init_at(&tmp.path().join("db.sqlite")).unwrap());
        let session =
            Arc::new(crate::session::load_or_create(&tmp.path().join("session-token")).unwrap());
        let mut state = AppState::in_memory(
            crate::Mode::Release,
            storage,
            session,
            7878,
            tmp.path().join("notes"),
        )
        .unwrap();
        state.cloud_push = std::sync::Arc::new(crate::cloud_push::PushJob::new(fast(2)));
        save(
            &state,
            &Connection {
                address: "http://127.0.0.1:1".into(),
                email: "o@example.com".into(),
                supabase_url: "http://127.0.0.1:1".into(),
                publishable_key: "k".into(),
                refresh_token: "r".into(),
                device_id: "d".into(),
                machine_name: "m".into(),
            },
        )
        .unwrap();
        record(
            &state,
            LAST_PUSH_KEY,
            &json!({ "at": T0, "changed": 0, "deleted": 0 }),
        );
        (state, tmp)
    }

    /// Two meetings per request and millisecond waits, so a test sees several batches and every retry in well under a second.
    fn fast(rows: usize) -> push::Pacing {
        push::Pacing {
            rows,
            bytes: usize::MAX,
            pause: Duration::from_millis(20),
            retries: vec![Duration::from_millis(10); 3],
        }
    }

    /// Waits for the background push to end.
    async fn settle(state: &AppState) {
        tokio::time::timeout(Duration::from_secs(20), async {
            while state.cloud_push.is_running() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("the push ended");
    }

    async fn push_and_settle(state: &AppState) {
        start_push(state, "manual", now_ms()).unwrap();
        settle(state).await;
    }

    fn set_interval(state: &AppState, v: &str) {
        yogurt_db::settings::set(&state.db, INTERVAL_KEY, v).unwrap();
    }

    fn attempt_at(state: &AppState) -> Option<i64> {
        recorded(state, LAST_ERROR_KEY)["at"].as_i64()
    }

    #[test]
    fn next_push_is_when_due_turns_true() {
        let day = 24 * 60 * MIN;
        let net = Some((T0 + 15 * MIN, "network"));
        assert_eq!(next_push(day, Some(T0), None, None), Some(T0 + day));
        assert_eq!(
            next_push(15 * MIN, Some(T0), net, None),
            Some(T0 + 20 * MIN)
        );
        assert_eq!(
            next_push(day, Some(T0), None, Some(T0 + MIN)),
            Some(T0 + 2 * MIN)
        );
        assert_eq!(
            next_push(day, Some(T0 + 2 * MIN), None, Some(T0 + MIN)),
            Some(T0 + 2 * MIN + day)
        );
        assert_eq!(
            next_push(day, Some(T0), Some((T0, "session_expired")), None),
            None
        );
        for (last_error, requested) in [
            (None, None),
            (net, None),
            (None, Some(T0 + MIN)),
            (net, Some(T0 + MIN)),
        ] {
            let at = next_push(day, Some(T0), last_error, requested).unwrap();
            assert_eq!(due(day, Some(T0), last_error, requested, at - 1), None);
            assert!(due(day, Some(T0), last_error, requested, at).is_some());
        }
    }

    #[test]
    fn due_rules() {
        let i = 15 * MIN;
        assert_eq!(due(i, None, None, None, T0), Some("auto"));
        assert_eq!(due(i, Some(T0), None, None, T0 + 14 * MIN), None);
        assert_eq!(due(i, Some(T0), None, None, T0 + 15 * MIN), Some("auto"));
        let net = Some((T0 + 15 * MIN, "network"));
        assert_eq!(due(i, Some(T0), net, None, T0 + 19 * MIN), None);
        assert_eq!(due(i, Some(T0), net, None, T0 + 20 * MIN), Some("auto"));
        for code in USER_FIXABLE {
            let err = Some((T0, code));
            assert_eq!(due(i, Some(T0), err, None, T0 + 24 * 60 * MIN), None);
            assert_eq!(
                due(i, Some(T0), err, Some(T0 + MIN), T0 + 24 * 60 * MIN),
                None
            );
        }

        let day = 24 * 60 * MIN;
        let asked = Some(T0 + MIN);
        assert_eq!(
            due(day, Some(T0), None, asked, T0 + MIN + 59_000),
            None,
            "settling"
        );
        assert_eq!(
            due(day, Some(T0), None, asked, T0 + 2 * MIN),
            Some("meeting")
        );
        assert_eq!(
            due(day, Some(T0 + 2 * MIN), None, asked, T0 + 3 * MIN),
            None,
            "already pushed"
        );
        assert_eq!(
            due(day, Some(T0), net, asked, T0 + 19 * MIN),
            None,
            "waits out a failure"
        );
        assert_eq!(
            due(day, Some(T0), net, asked, T0 + 20 * MIN),
            Some("meeting"),
            "then retries"
        );
        assert_eq!(
            due(day, Some(T0), None, None, T0 + day),
            Some("auto"),
            "daily backstop"
        );
    }

    #[tokio::test]
    async fn interval_is_validated_and_defaults_off() {
        let (state, _tmp) = state();
        assert_eq!(push_interval(&state), "off");
        for ok in ["meeting", "15m", "1h", "1d", "off"] {
            let Json(status) = set_push_interval(
                State(state.clone()),
                Json(IntervalBody {
                    interval: ok.into(),
                }),
            )
            .await
            .unwrap();
            assert_eq!(push_interval(&state), ok);
            assert_eq!(status["push_interval"], ok);
        }
        let bad = set_push_interval(
            State(state.clone()),
            Json(IntervalBody {
                interval: "5m".into(),
            }),
        )
        .await;
        assert_eq!(bad.unwrap_err().code, "bad_request");
        assert_eq!(push_interval(&state), "off");
    }

    #[tokio::test]
    async fn auto_tick_follows_the_setting_the_connection_and_the_clock() {
        let (state, _tmp) = state();

        auto_tick(&state, T0 + 99 * 60 * MIN).await;

        settle(&state).await;
        assert_eq!(attempt_at(&state), None, "off never pushes");

        set_interval(&state, "15m");
        auto_tick(&state, T0 + 14 * MIN).await;
        settle(&state).await;
        assert_eq!(attempt_at(&state), None, "not yet due");
        auto_tick(&state, T0 + 15 * MIN).await;
        settle(&state).await;
        assert_eq!(
            attempt_at(&state),
            Some(T0 + 15 * MIN),
            "due after 15 minutes"
        );
        assert_eq!(recorded(&state, LAST_ERROR_KEY)["code"], "network");

        auto_tick(&state, T0 + 19 * MIN).await;

        settle(&state).await;
        assert_eq!(
            attempt_at(&state),
            Some(T0 + 15 * MIN),
            "a failure waits out the retry gap"
        );
        auto_tick(&state, T0 + 20 * MIN).await;
        settle(&state).await;
        assert_eq!(attempt_at(&state), Some(T0 + 20 * MIN), "then retries");

        record(
            &state,
            LAST_ERROR_KEY,
            &json!({ "at": T0 + 20 * MIN, "error": "x", "code": "session_expired" }),
        );
        auto_tick(&state, T0 + 24 * 60 * MIN).await;
        settle(&state).await;
        assert_eq!(
            attempt_at(&state),
            Some(T0 + 20 * MIN),
            "a dead session is not retried"
        );

        record(&state, LAST_ERROR_KEY, &Value::Null);
        state.keys.delete(ACCOUNT).unwrap();
        auto_tick(&state, T0 + 24 * 60 * MIN).await;
        settle(&state).await;
        assert_eq!(attempt_at(&state), None, "not connected, nothing to push");
    }

    #[tokio::test(start_paused = true)]
    async fn the_timer_wakes_every_period_and_rereads_the_setting() {
        let (state, _tmp) = state();
        let start = tokio::time::Instant::now();
        let clock = move || T0 + start.elapsed().as_millis() as i64;
        let task = tokio::spawn(drive(state.clone(), Duration::from_secs(60), clock));
        let settle = || async {
            for _ in 0..50 {
                tokio::task::yield_now().await;
            }
        };

        settle().await;
        assert_eq!(
            attempt_at(&state),
            None,
            "the first wake finds the setting off"
        );

        set_interval(&state, "1h");
        tokio::time::advance(Duration::from_secs(30 * 60)).await;
        settle().await;
        assert_eq!(attempt_at(&state), None, "30 minutes into a 1h interval");

        tokio::time::advance(Duration::from_secs(30 * 60)).await;
        let mut at = None;
        for _ in 0..200 {
            settle().await;
            at = attempt_at(&state);
            if at.is_some() {
                break;
            }
            tokio::task::spawn_blocking(|| std::thread::sleep(Duration::from_millis(5)))
                .await
                .unwrap();
        }
        let at = at.expect("an attempt after the hour");
        assert!(
            (T0 + 60 * MIN..T0 + 62 * MIN).contains(&at),
            "stamped near the hour mark: {}",
            at - T0
        );
        task.abort();
    }

    fn history(state: &AppState) -> Vec<Value> {
        recorded(state, HISTORY_KEY)
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    #[tokio::test]
    async fn history_keeps_failures_collapses_idle_wakes_and_caps() {
        let (state, _tmp) = state();
        push_and_settle(&state).await;
        let h = history(&state);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0]["outcome"], "failed");
        assert_eq!(h[0]["trigger"], "manual");
        assert_eq!(h[0]["code"], "network");
        assert_eq!(h[0]["needs_you"], false);

        record_success(&state, "auto", T0 + MIN, 0, 0);
        record_success(&state, "auto", T0 + 2 * MIN, 0, 0);
        let h = history(&state);
        assert_eq!(h.len(), 2, "idle wakes collapse into one row");
        assert_eq!(h[0]["outcome"], "up_to_date");
        assert_eq!(h[0]["at"], T0 + 2 * MIN);
        assert_eq!(
            h[1]["outcome"], "failed",
            "the failure survives the idle wakes"
        );

        for i in 0..HISTORY_LEN {
            record_success(&state, "manual", T0 + i as i64, 3, 1);
        }
        let h = history(&state);
        assert_eq!(h.len(), HISTORY_LEN);
        assert_eq!(h[0]["outcome"], "pushed");
        assert_eq!(h[0]["changed"], 3);

        let Json(status) = status(State(state.clone())).await.unwrap();
        assert_eq!(
            status["push_history"].as_array().unwrap().len(),
            HISTORY_LEN
        );
    }

    #[tokio::test]
    async fn the_meeting_setting_pushes_a_minute_after_a_request() {
        let (state, _tmp) = state();
        set_interval(&state, "meeting");
        auto_tick(&state, T0 + 10 * MIN).await;
        settle(&state).await;
        assert_eq!(
            attempt_at(&state),
            None,
            "no meeting yet, and the daily backstop is not due"
        );

        state.cloud_push.request(T0 + 10 * MIN);
        auto_tick(&state, T0 + 10 * MIN + 30_000).await;
        settle(&state).await;
        assert_eq!(attempt_at(&state), None, "still settling");
        auto_tick(&state, T0 + 11 * MIN).await;
        settle(&state).await;
        assert_eq!(attempt_at(&state), Some(T0 + 11 * MIN));
        assert_eq!(history(&state)[0]["trigger"], "meeting");

        set_interval(&state, "1d");
        record(&state, LAST_ERROR_KEY, &Value::Null);
        auto_tick(&state, T0 + 20 * MIN).await;
        settle(&state).await;
        assert_eq!(
            history(&state).len(),
            1,
            "other settings ignore the request"
        );
    }

    #[tokio::test]
    async fn an_idle_laptop_stamps_the_push_without_touching_the_network() {
        let (state, _tmp) = idle_state();
        set_interval(&state, "15m");
        auto_tick(&state, T0 + 15 * MIN).await;
        settle(&state).await;
        assert_eq!(attempt_at(&state), None, "no pass, so nothing could fail");
        assert_eq!(recorded(&state, LAST_PUSH_KEY)["at"], T0 + 15 * MIN);
        assert_eq!(recorded(&state, LAST_PUSH_KEY)["changed"], 0);
    }

    /// A mock Supabase with everything a push needs except the insert and the search catch-up, which each test mounts, and `state` connected to it.
    async fn bare_cloud(state: &AppState) -> wiremock::MockServer {
        cloud_listing(state, &[]).await
    }

    /// [`bare_cloud`] whose first page of `yogurt_device_meetings` lists `listed`.
    async fn cloud_listing(state: &AppState, listed: &[String]) -> wiremock::MockServer {
        use wiremock::matchers::{method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let cloud = MockServer::start().await;
        let answer = |p: &str, body: Value| {
            Mock::given(method("POST"))
                .and(path(p))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
        };
        let session = json!({ "access_token": "a", "refresh_token": "r2", "expires_at": now_ms() / 1000 + 3600, "user": { "id": "u" } });
        answer("/auth/v1/token", session).mount(&cloud).await;
        Mock::given(method("POST"))
            .and(path("/rest/v1/rpc/yogurt_device_meetings"))
            .and(query_param("offset", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(listed)))
            .mount(&cloud)
            .await;
        answer("/rest/v1/rpc/yogurt_device_meetings", json!([]))
            .mount(&cloud)
            .await;
        answer("/rest/v1/rpc/yogurt_start_push", json!(7))
            .mount(&cloud)
            .await;
        let mut conn = load(state).unwrap().unwrap();
        conn.supabase_url = cloud.uri();
        save(state, &conn).unwrap();
        cloud
    }

    fn insert(status: u16) -> wiremock::Mock {
        wiremock::Mock::given(wiremock::matchers::path("/rest/v1/yogurt_versions"))
            .respond_with(wiremock::ResponseTemplate::new(status))
    }

    /// [`bare_cloud`] whose insert answers `insert_status`.
    async fn mock_cloud(state: &AppState, insert_status: u16) -> wiremock::MockServer {
        let cloud = bare_cloud(state).await;
        insert(insert_status).mount(&cloud).await;
        cloud
    }

    fn catch_up(status: u16, body: Value) -> wiremock::Mock {
        wiremock::Mock::given(wiremock::matchers::path(
            "/rest/v1/rpc/yogurt_search_catch_up",
        ))
        .respond_with(wiremock::ResponseTemplate::new(status).set_body_json(body))
    }

    async fn hits(cloud: &wiremock::MockServer, p: &str) -> usize {
        cloud
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == p)
            .count()
    }

    /// Frames from the app-wide stream until the push ends, as `(type, frame)`.
    async fn frames_until_end(rx: &mut tokio::sync::broadcast::Receiver<Value>) -> Vec<Value> {
        let mut out = Vec::new();
        loop {
            let f = tokio::time::timeout(Duration::from_secs(20), rx.recv())
                .await
                .expect("a frame")
                .unwrap();
            let end = f["type"] != "cloud_push_progress";
            out.push(f);
            if end {
                return out;
            }
        }
    }

    #[tokio::test]
    async fn a_failed_search_catch_up_leaves_the_push_successful() {
        let (state, _tmp) = state();
        let cloud = mock_cloud(&state, 201).await;
        catch_up(500, json!({ "message": "boom" }))
            .expect(2)
            .mount(&cloud)
            .await;
        push_and_settle(&state).await;
        assert_eq!(recorded(&state, LAST_PUSH_KEY)["changed"], 1);
        assert_eq!(recorded(&state, LAST_ERROR_KEY), Value::Null);
        assert_eq!(pending(&state).await.unwrap().not_pushed, 0);
    }

    #[tokio::test]
    async fn a_failed_push_still_catches_search_up() {
        let (state, _tmp) = state();
        let cloud = mock_cloud(&state, 500).await;
        catch_up(200, json!(0)).expect(1).mount(&cloud).await;
        push_and_settle(&state).await;
        assert_eq!(recorded(&state, LAST_ERROR_KEY)["code"], "cloud_error");
        assert_eq!(hits(&cloud, "/rest/v1/yogurt_versions").await, 4);
    }

    #[tokio::test]
    async fn a_pass_with_nothing_to_push_still_catches_search_up_until_none_is_left() {
        let (state, _tmp) = idle_state();
        let cloud = mock_cloud(&state, 201).await;
        catch_up(200, json!(4))
            .up_to_n_times(2)
            .expect(2)
            .mount(&cloud)
            .await;
        catch_up(200, json!(0)).expect(1).mount(&cloud).await;
        push_and_settle(&state).await;
        assert_eq!(recorded(&state, LAST_PUSH_KEY)["changed"], 0);
    }

    #[tokio::test]
    async fn push_returns_at_once_and_only_one_runs_at_a_time() {
        let (state, _tmp) = with_meetings(3);
        let cloud = bare_cloud(&state).await;
        wiremock::Mock::given(wiremock::matchers::path("/rest/v1/yogurt_versions"))
            .respond_with(
                wiremock::ResponseTemplate::new(201).set_delay(Duration::from_millis(150)),
            )
            .mount(&cloud)
            .await;
        catch_up(200, json!(0)).mount(&cloud).await;

        let started = std::time::Instant::now();
        let (code, Json(body)) = push_now(State(state.clone())).await.unwrap();
        assert_eq!(code, StatusCode::ACCEPTED);
        assert_eq!(body["started"], true);
        assert!(started.elapsed() < Duration::from_millis(100));

        let again = push_now(State(state.clone())).await.unwrap_err();
        assert_eq!(
            (again.code, again.status),
            ("push_running", StatusCode::CONFLICT)
        );
        let Json(st) = status(State(state.clone())).await.unwrap();
        assert!(st["push"].is_object(), "{st}");
        auto_tick(&state, T0 + 99 * 60 * MIN).await;

        settle(&state).await;
        assert_eq!(hits(&cloud, "/rest/v1/rpc/yogurt_start_push").await, 1);
        let Json(st) = status(State(state.clone())).await.unwrap();
        assert!(st["push"].is_null());
        assert_eq!(st["not_pushed"], 0);
        push_and_settle(&state).await;
    }

    #[tokio::test]
    async fn the_not_pushed_count_follows_edits_without_rebuilding_every_bundle() {
        let (state, _tmp) = with_meetings(3);
        let count = |state: AppState| async move {
            let Json(st) = status(State(state)).await.unwrap();
            st["not_pushed"].as_u64().unwrap()
        };
        assert_eq!(count(state.clone()).await, 3);
        let Json(st) = status(State(state.clone())).await.unwrap();
        assert!(st["not_pushed_bytes"].as_u64().unwrap() > 0, "{st}");
        assert!(st["push_estimate_secs"].as_u64().unwrap() > 0, "{st}");
        let id = state.meeting_repo.cloud_hashes().unwrap()[0].0.clone();
        let hash = {
            let b = state.meeting_repo.meeting_bundle(&id).unwrap().unwrap();
            push::hash_body(&b)
        };
        state.meeting_repo.set_cloud_hash(&id, &hash).unwrap();
        assert_eq!(count(state.clone()).await, 2, "a push shows at once");
        tokio::time::sleep(Duration::from_millis(5)).await;
        state
            .meeting_repo
            .patch(
                &id,
                yogurt_db::MeetingPatch {
                    notes_md: Some("edited".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(count(state.clone()).await, 3, "an edit shows at once");
    }

    #[tokio::test]
    async fn the_count_drops_to_zero_after_a_push_that_only_replaced_hashes() {
        let (state, _tmp) = with_meetings(2);
        let cloud = mock_cloud(&state, 201).await;
        catch_up(200, json!(0)).mount(&cloud).await;
        push_and_settle(&state).await;
        let count = || async {
            let Json(st) = status(State(state.clone())).await.unwrap();
            st["not_pushed"].as_u64().unwrap()
        };
        assert_eq!(count().await, 0);
        let id = state.meeting_repo.cloud_hashes().unwrap()[0].0.clone();
        tokio::time::sleep(Duration::from_millis(5)).await;
        state
            .meeting_repo
            .patch(
                &id,
                yogurt_db::MeetingPatch {
                    notes_md: Some("edited".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(count().await, 1);
        push_and_settle(&state).await;
        assert_eq!(count().await, 0);
    }

    #[tokio::test]
    async fn push_while_disconnected_is_refused_at_once() {
        let (state, _tmp) = state();
        state.keys.delete(ACCOUNT).unwrap();
        assert_eq!(
            push_now(State(state)).await.unwrap_err().code,
            "not_connected"
        );
    }

    #[tokio::test]
    async fn a_large_pass_streams_progress_then_completes_and_a_small_one_stays_quiet() {
        let (state, _tmp) = with_meetings(5);
        let cloud = mock_cloud(&state, 201).await;
        catch_up(200, json!(0)).mount(&cloud).await;
        let mut rx = state.app_events_tx.subscribe();
        start_push(&state, "manual", now_ms()).unwrap();
        let frames = frames_until_end(&mut rx).await;
        let sent: Vec<_> = frames[..frames.len() - 1]
            .iter()
            .map(|f| (f["sent"].as_u64().unwrap(), f["total"].as_u64().unwrap()))
            .collect();
        assert_eq!(sent, [(0, 5), (2, 5), (4, 5), (5, 5)]);
        assert!(frames[0]["retry_in"].is_null());
        assert!(
            frames[0]["left_secs"].is_null(),
            "no pace before a batch lands"
        );
        assert!(frames[1]["left_secs"].is_u64(), "{}", frames[1]);
        let end = frames.last().unwrap();
        assert_eq!(
            (end["type"].as_str(), end["pushed"].as_u64()),
            (Some("cloud_push_complete"), Some(5))
        );
        settle(&state).await;
        assert_eq!(recorded(&state, LAST_PUSH_KEY)["changed"], 5);
        assert_eq!(hits(&cloud, "/rest/v1/yogurt_versions").await, 3);

        let (state, _tmp) = with_meetings(2);
        let cloud = mock_cloud(&state, 201).await;
        catch_up(200, json!(0)).mount(&cloud).await;
        let mut rx = state.app_events_tx.subscribe();
        push_and_settle(&state).await;
        assert!(rx.try_recv().is_err(), "one batch sends no frames");
        assert_eq!(hits(&cloud, "/rest/v1/yogurt_versions").await, 1);
    }

    #[tokio::test]
    async fn a_throttled_batch_reports_its_wait_then_the_pass_goes_on() {
        let (state, _tmp) = with_meetings(5);
        let cloud = bare_cloud(&state).await;
        wiremock::Mock::given(wiremock::matchers::path("/rest/v1/yogurt_versions"))
            .respond_with(wiremock::ResponseTemplate::new(429).insert_header("Retry-After", "1"))
            .up_to_n_times(1)
            .mount(&cloud)
            .await;
        insert(201).mount(&cloud).await;
        catch_up(200, json!(0)).mount(&cloud).await;
        let mut rx = state.app_events_tx.subscribe();
        start_push(&state, "manual", now_ms()).unwrap();
        let frames = frames_until_end(&mut rx).await;
        let waits: Vec<_> = frames
            .iter()
            .filter_map(|f| f["retry_in"].as_u64())
            .collect();
        assert_eq!(waits, [1]);
        assert_eq!(frames.last().unwrap()["type"], "cloud_push_complete");
        settle(&state).await;
        assert_eq!(recorded(&state, LAST_ERROR_KEY), Value::Null);
    }

    #[tokio::test]
    async fn a_failure_midway_keeps_the_committed_batches_and_the_next_push_resumes() {
        let (state, _tmp) = with_meetings(6);
        let cloud = bare_cloud(&state).await;
        insert(201).up_to_n_times(2).mount(&cloud).await;
        insert(403).mount(&cloud).await;
        catch_up(200, json!(0)).mount(&cloud).await;
        let mut rx = state.app_events_tx.subscribe();
        start_push(&state, "manual", now_ms()).unwrap();
        let frames = frames_until_end(&mut rx).await;
        let end = frames.last().unwrap();
        assert_eq!(end["type"], "cloud_push_error");
        assert_eq!(end["code"], "cloud_error");
        settle(&state).await;
        assert_eq!(recorded(&state, LAST_ERROR_KEY)["code"], "cloud_error");
        assert_eq!(pending(&state).await.unwrap().not_pushed, 2);

        let repo = state.meeting_repo.clone();
        let committed: Vec<String> = repo
            .cloud_hashes()
            .unwrap()
            .into_iter()
            .filter_map(|(id, hash)| hash.map(|_| id))
            .collect();
        assert_eq!(committed.len(), 4);
        let cloud2 = cloud_listing(&state, &committed).await;
        insert(201).mount(&cloud2).await;
        catch_up(200, json!(0)).mount(&cloud2).await;
        push_and_settle(&state).await;
        assert_eq!(pending(&state).await.unwrap().not_pushed, 0);
        assert_eq!(
            hits(&cloud2, "/rest/v1/yogurt_versions").await,
            1,
            "only the two uncommitted meetings go out"
        );
    }

    #[tokio::test]
    async fn disconnect_stops_a_running_push_at_the_next_boundary() {
        let (mut state, _tmp) = with_meetings(6);
        state.cloud_push = std::sync::Arc::new(crate::cloud_push::PushJob::new(push::Pacing {
            pause: Duration::from_secs(60),
            ..fast(2)
        }));
        let cloud = mock_cloud(&state, 201).await;
        catch_up(200, json!(0)).mount(&cloud).await;
        let mut rx = state.app_events_tx.subscribe();
        start_push(&state, "manual", now_ms()).unwrap();
        loop {
            let f = rx.recv().await.unwrap();
            if f["sent"] == 2 {
                break;
            }
        }
        let started = std::time::Instant::now();
        let _ = disconnect(State(state.clone()), None).await.unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
        let end = rx.recv().await.unwrap();
        assert_eq!(end["type"], "cloud_push_cancelled");
        assert!(!state.cloud_push.is_running());
        assert!(load(&state).unwrap().is_none());
        assert_eq!(recorded(&state, LAST_ERROR_KEY), Value::Null);
    }

    #[tokio::test]
    async fn the_interval_cannot_be_set_while_disconnected() {
        let (state, _tmp) = state();
        state.keys.delete(ACCOUNT).unwrap();
        let err = set_push_interval(
            State(state.clone()),
            Json(IntervalBody {
                interval: "1h".into(),
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, "not_connected");
        assert_eq!(err.status, StatusCode::CONFLICT);
        assert_eq!(push_interval(&state), "off");
    }
}
