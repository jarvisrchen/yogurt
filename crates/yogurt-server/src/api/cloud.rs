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
use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;
use tokio::sync::MutexGuard;
use yogurt_cloud::push::{self, LocalMeeting, Plan};
use yogurt_cloud::{config, Error as CloudError, Project, Session};
use yogurt_db::keys::ProcessLock;
use yogurt_db::MeetingRepo;

use crate::AppState;

const ACCOUNT: &str = "yogurt-cloud";
/// The last pass that succeeded, kept apart from the last failure so a failed pass never hides when the cloud was last in step.
const LAST_PUSH_KEY: &str = "cloud.last_push";
const LAST_ERROR_KEY: &str = "cloud.last_error";
const INTERVAL_KEY: &str = "cloud.push_interval";
const INTERVALS: [(&str, i64); 3] = [("15m", 15 * 60_000), ("1h", 3_600_000), ("1d", 86_400_000)];
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
        "not_connected" | "already_connected" | "device_removed" => StatusCode::CONFLICT,
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

fn local_meetings(repo: &MeetingRepo) -> anyhow::Result<Vec<LocalMeeting>> {
    let stored: HashMap<String, Option<String>> = repo.cloud_hashes()?.into_iter().collect();
    let mut out = Vec::with_capacity(stored.len());
    for (id, hash) in stored {
        if let Some(body) = repo.meeting_bundle(&id)? {
            out.push(LocalMeeting::new(id, hash, body));
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

fn record_failure(state: &AppState, e: &CloudApiError, now: i64) {
    record(
        state,
        LAST_ERROR_KEY,
        &json!({ "at": now, "error": e.message, "code": e.code }),
    );
}

fn interval_ms(name: &str) -> Option<i64> {
    INTERVALS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, ms)| *ms)
}

/// `off`, `15m`, `1h`, or `1d`. Anything else stored reads as `off`.
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
            "Auto-push interval must be off, 15m, 1h, or 1d.",
        ));
    }
    if load(&state)?.is_none() {
        return Err(CloudApiError::coded("not_connected"));
    }
    yogurt_db::settings::set(&state.db, INTERVAL_KEY, &body.interval)
        .map_err(CloudApiError::internal)?;
    status(State(state)).await
}

struct Pending {
    meetings: usize,
    not_pushed: usize,
}

/// The meeting being recorded does not count, and a local deletion not yet sent does.
async fn pending(state: &AppState) -> ApiResult<Pending> {
    let recording = recording_id(state).await;
    let repo = state.meeting_repo.clone();
    let (local, tombstones) =
        blocking(move || Ok((local_meetings(&repo)?, repo.cloud_tombstones()?.len()))).await?;
    let changed = local
        .iter()
        .filter(|m| Some(m.id.as_str()) != recording.as_deref())
        .filter(|m| m.cloud_hash.as_deref() != Some(&m.hash))
        .count();
    Ok(Pending {
        meetings: local.len(),
        not_pushed: changed + tombstones,
    })
}

async fn status(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let Some(conn) = load(&state)? else {
        return Ok(Json(json!({ "connected": false })));
    };
    let pending = pending(&state).await?;
    Ok(Json(json!({
        "connected": true,
        "address": conn.address,
        "email": conn.email,
        "machine_name": conn.machine_name,
        "meetings": pending.meetings,
        "not_pushed": pending.not_pushed,
        "last_push": recorded(&state, LAST_PUSH_KEY),
        "last_error": recorded(&state, LAST_ERROR_KEY),
        "push_interval": push_interval(&state),
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
        record(&state, LAST_PUSH_KEY, &Value::Null);
        record(&state, LAST_ERROR_KEY, &Value::Null);
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

/// One pass. `local` is every meeting on this Mac, or nothing when the whole device is being marked deleted.
async fn run_pass(
    state: &AppState,
    link: &mut Link,
    local: Vec<LocalMeeting>,
    skip: Option<String>,
) -> ApiResult<Plan> {
    let repo = state.meeting_repo.clone();
    let Link {
        conn,
        project,
        session,
    } = link;
    let plan = push::run(
        project,
        session,
        &conn.device_id.clone(),
        &local,
        skip.as_deref(),
        |renewed| {
            conn.refresh_token = renewed.refresh_token.clone();
            save(state, conn).map_err(|e| e.message)
        },
        |hashes| {
            hashes
                .iter()
                .try_for_each(|(id, hash)| repo.set_cloud_hash(id, hash))
                .map_err(|e| format!("{e:#}"))
        },
    )
    .await?;
    // The deletion markers went out with this pass.
    let repo = state.meeting_repo.clone();
    blocking(move || repo.clear_cloud_tombstones()).await?;
    // A lagging index only delays search, so its failure never fails the pass.
    match push::catch_up_search(project, session).await {
        Ok(0) => {}
        Ok(left) => tracing::info!(left, "search index still catching up"),
        Err(e) => tracing::warn!(code = e.code(), error = %e, "search index catch-up failed"),
    }
    Ok(plan)
}

/// The shared pass behind the Push button and the timer. `now` stamps the success or failure record.
async fn push_locked(state: &AppState, held: &Held<'_>, now: i64) -> ApiResult<Value> {
    let pass = async {
        let recording = recording_id(state).await;
        let repo = state.meeting_repo.clone();
        let local = blocking(move || local_meetings(&repo)).await?;
        let mut link = link(state, held).await?;
        run_pass(state, &mut link, local, recording).await
    };
    match pass.await {
        Ok(plan) => {
            let v =
                json!({ "at": now, "changed": plan.changed.len(), "deleted": plan.deleted.len() });
            record(state, LAST_PUSH_KEY, &v);
            record(state, LAST_ERROR_KEY, &Value::Null);
            Ok(v)
        }
        Err(e) => {
            if e.code != "not_connected" {
                record_failure(state, &e, now);
            }
            Err(e)
        }
    }
}

async fn push_now(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    detached(async move {
        let held = acquire(&state).await?;
        push_locked(&state, &held, now_ms()).await.map(Json)
    })
    .await
}

/// Whether the timer should push now.
fn due(
    interval_ms: i64,
    last_push: Option<i64>,
    last_error: Option<(i64, &str)>,
    now: i64,
) -> bool {
    match last_error {
        Some((_, code)) if USER_FIXABLE.contains(&code) => return false,
        Some((at, _)) if now - at < RETRY_AFTER_FAILURE_MS => return false,
        _ => {}
    }
    last_push.is_none_or(|at| now - at >= interval_ms)
}

/// One wake of the timer. It takes both locks first and decides under them, so a push that just finished counts.
/// With nothing pending it only stamps the last push, so an idle laptop never touches the network or rewrites `keys.json`.
async fn auto_tick(state: &AppState, now: i64) {
    let Some(interval_ms) = interval_ms(push_interval(state)) else {
        return;
    };
    let state = state.clone();
    let result = detached(async move {
        let held = acquire(&state).await?;
        if load(&state)?.is_none() {
            return Ok(());
        }
        let last_push = recorded(&state, LAST_PUSH_KEY)["at"].as_i64();
        let err = recorded(&state, LAST_ERROR_KEY);
        let last_error = err["at"]
            .as_i64()
            .map(|at| (at, err["code"].as_str().unwrap_or("")));
        if !due(interval_ms, last_push, last_error, now) {
            return Ok(());
        }
        if pending(&state).await?.not_pushed == 0 {
            record(
                &state,
                LAST_PUSH_KEY,
                &json!({ "at": now, "changed": 0, "deleted": 0 }),
            );
            record(&state, LAST_ERROR_KEY, &Value::Null);
            return Ok(());
        }
        let v = push_locked(&state, &held, now).await?;
        tracing::info!(changed = %v["changed"], deleted = %v["deleted"], "auto-push finished");
        Ok(())
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
    detached(async move {
        let held = acquire(&state).await?;
        let linked = link(&state, &held).await;
        let mut linked = match linked {
            Ok(l) => Some(l),
            Err(e) if body.mark_deleted => {
                if e.code != "not_connected" {
                    record_failure(&state, &e, now_ms());
                }
                return Err(e);
            }
            Err(_) => None,
        };
        if let (true, Some(l)) = (body.mark_deleted, linked.as_mut()) {
            if let Err(e) = run_pass(&state, l, Vec::new(), None).await {
                record_failure(&state, &e, now_ms());
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
        record(&state, LAST_PUSH_KEY, &Value::Null);
        record(&state, LAST_ERROR_KEY, &Value::Null);
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
        let (state, tmp) = idle_state();
        state
            .meeting_repo
            .create(yogurt_db::NewMeeting {
                title: "unpushed".into(),
                started_at_unix_ms: None,
                id: None,
            })
            .unwrap();
        (state, tmp)
    }

    /// Like [`state`] with no meeting, so nothing is pending.
    fn idle_state() -> (AppState, tempfile::TempDir) {
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

    fn set_interval(state: &AppState, v: &str) {
        yogurt_db::settings::set(&state.db, INTERVAL_KEY, v).unwrap();
    }

    fn attempt_at(state: &AppState) -> Option<i64> {
        recorded(state, LAST_ERROR_KEY)["at"].as_i64()
    }

    #[test]
    fn due_rules() {
        let i = 15 * MIN;
        assert!(due(i, None, None, T0));
        assert!(!due(i, Some(T0), None, T0 + 14 * MIN));
        assert!(due(i, Some(T0), None, T0 + 15 * MIN));
        assert!(!due(
            i,
            Some(T0),
            Some((T0 + 15 * MIN, "network")),
            T0 + 19 * MIN
        ));
        assert!(due(
            i,
            Some(T0),
            Some((T0 + 15 * MIN, "network")),
            T0 + 20 * MIN
        ));
        for code in USER_FIXABLE {
            assert!(!due(i, Some(T0), Some((T0, code)), T0 + 24 * 60 * MIN));
        }
    }

    #[tokio::test]
    async fn interval_is_validated_and_defaults_off() {
        let (state, _tmp) = state();
        assert_eq!(push_interval(&state), "off");
        for ok in ["15m", "1h", "1d", "off"] {
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
        assert_eq!(attempt_at(&state), None, "off never pushes");

        set_interval(&state, "15m");
        auto_tick(&state, T0 + 14 * MIN).await;
        assert_eq!(attempt_at(&state), None, "not yet due");
        auto_tick(&state, T0 + 15 * MIN).await;
        assert_eq!(
            attempt_at(&state),
            Some(T0 + 15 * MIN),
            "due after 15 minutes"
        );
        assert_eq!(recorded(&state, LAST_ERROR_KEY)["code"], "network");

        auto_tick(&state, T0 + 19 * MIN).await;
        assert_eq!(
            attempt_at(&state),
            Some(T0 + 15 * MIN),
            "a failure waits out the retry gap"
        );
        auto_tick(&state, T0 + 20 * MIN).await;
        assert_eq!(attempt_at(&state), Some(T0 + 20 * MIN), "then retries");

        record(
            &state,
            LAST_ERROR_KEY,
            &json!({ "at": T0 + 20 * MIN, "error": "x", "code": "session_expired" }),
        );
        auto_tick(&state, T0 + 24 * 60 * MIN).await;
        assert_eq!(
            attempt_at(&state),
            Some(T0 + 20 * MIN),
            "a dead session is not retried"
        );

        record(&state, LAST_ERROR_KEY, &Value::Null);
        state.keys.delete(ACCOUNT).unwrap();
        auto_tick(&state, T0 + 24 * 60 * MIN).await;
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

    #[tokio::test]
    async fn an_idle_laptop_stamps_the_push_without_touching_the_network() {
        let (state, _tmp) = idle_state();
        set_interval(&state, "15m");
        auto_tick(&state, T0 + 15 * MIN).await;
        assert_eq!(attempt_at(&state), None, "no pass, so nothing could fail");
        assert_eq!(recorded(&state, LAST_PUSH_KEY)["at"], T0 + 15 * MIN);
        assert_eq!(recorded(&state, LAST_PUSH_KEY)["changed"], 0);
    }

    /// A mock Supabase that takes one push, and `state` connected to it. Search catch-up is left for each test to mount.
    async fn mock_cloud(state: &AppState) -> wiremock::MockServer {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let cloud = MockServer::start().await;
        let answer = |p: &str, status: u16, body: Value| {
            Mock::given(method("POST"))
                .and(path(p))
                .respond_with(ResponseTemplate::new(status).set_body_json(body))
        };
        let session = json!({ "access_token": "a", "refresh_token": "r2", "expires_at": now_ms() / 1000 + 3600, "user": { "id": "u" } });
        answer("/auth/v1/token", 200, session).mount(&cloud).await;
        answer("/rest/v1/rpc/yogurt_device_meetings", 200, json!([]))
            .mount(&cloud)
            .await;
        answer("/rest/v1/rpc/yogurt_start_push", 200, json!(7))
            .mount(&cloud)
            .await;
        answer("/rest/v1/yogurt_versions", 201, Value::Null)
            .mount(&cloud)
            .await;
        let mut conn = load(state).unwrap().unwrap();
        conn.supabase_url = cloud.uri();
        save(state, &conn).unwrap();
        cloud
    }

    fn catch_up(status: u16, body: Value) -> wiremock::Mock {
        wiremock::Mock::given(wiremock::matchers::path(
            "/rest/v1/rpc/yogurt_search_catch_up",
        ))
        .respond_with(wiremock::ResponseTemplate::new(status).set_body_json(body))
    }

    #[tokio::test]
    async fn a_failed_search_catch_up_leaves_the_push_successful() {
        let (state, _tmp) = state();
        let cloud = mock_cloud(&state).await;
        catch_up(500, json!({ "message": "boom" }))
            .expect(1)
            .mount(&cloud)
            .await;
        let Json(v) = push_now(State(state.clone())).await.unwrap();
        assert_eq!(v["changed"], 1);
        assert_eq!(recorded(&state, LAST_PUSH_KEY)["changed"], 1);
        assert_eq!(recorded(&state, LAST_ERROR_KEY), Value::Null);
        assert_eq!(pending(&state).await.unwrap().not_pushed, 0);
    }

    #[tokio::test]
    async fn a_pass_with_nothing_to_push_still_catches_search_up_until_none_is_left() {
        let (state, _tmp) = idle_state();
        let cloud = mock_cloud(&state).await;
        catch_up(200, json!(4))
            .up_to_n_times(2)
            .expect(2)
            .mount(&cloud)
            .await;
        catch_up(200, json!(0)).expect(1).mount(&cloud).await;
        let Json(v) = push_now(State(state.clone())).await.unwrap();
        assert_eq!(v["changed"], 0);
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
