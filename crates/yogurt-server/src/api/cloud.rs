//! Yogurt Cloud sync: connect this machine to the user's own cloud, push meetings, disconnect.
//! The whole connection is one `keys.json` account, so connect and disconnect are atomic.
//! Nothing here returns, logs, or stores a secret outside that file. The password is used once to sign in and dropped.
//!
//! Every operation that touches the refresh token runs in a spawned task and holds `AppState::cloud_lock` to the end.
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
use tokio::sync::MutexGuard;
use yogurt_cloud::push::{self, LocalMeeting, Plan};
use yogurt_cloud::{config, Error as CloudError, Project, Session};
use yogurt_db::MeetingRepo;

use crate::AppState;

const ACCOUNT: &str = "yogurt-cloud";
/// The last pass that succeeded, kept apart from the last failure so a failed pass never hides when the cloud was last in step.
const LAST_PUSH_KEY: &str = "cloud.last_push";
const LAST_ERROR_KEY: &str = "cloud.last_error";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/cloud", get(status))
        .route("/api/cloud/check", post(check))
        .route("/api/cloud/connect", post(connect))
        .route("/api/cloud/sign-in", post(sign_in))
        .route("/api/cloud/push", post(push_now))
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
        "passcode_not_set" => "Choose a passcode in the webapp first",
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
    let raw = state.keys.get(ACCOUNT).map_err(CloudApiError::internal)?;
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

struct Link {
    conn: Connection,
    project: Project,
    session: Session,
}

/// Refreshes the session and persists the rotated refresh token before anything else uses it.
/// The guard parameter makes holding `AppState::cloud_lock` a compile-time requirement.
async fn link(state: &AppState, _held: &MutexGuard<'_, ()>) -> ApiResult<Link> {
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

fn record_failure(state: &AppState, e: &CloudApiError) {
    record(
        state,
        LAST_ERROR_KEY,
        &json!({ "at": now_ms(), "error": e.message, "code": e.code }),
    );
}

async fn status(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let Some(conn) = load(&state)? else {
        return Ok(Json(json!({ "connected": false })));
    };
    let recording = recording_id(&state).await;
    let repo = state.meeting_repo.clone();
    let (local, tombstones) =
        blocking(move || Ok((local_meetings(&repo)?, repo.cloud_tombstones()?.len()))).await?;
    let changed = local
        .iter()
        .filter(|m| Some(m.id.as_str()) != recording.as_deref())
        .filter(|m| m.cloud_hash.as_deref() != Some(&m.hash))
        .count();
    Ok(Json(json!({
        "connected": true,
        "address": conn.address,
        "email": conn.email,
        "machine_name": conn.machine_name,
        "meetings": local.len(),
        "not_pushed": changed + tombstones,
        "last_push": recorded(&state, LAST_PUSH_KEY),
        "last_error": recorded(&state, LAST_ERROR_KEY),
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

/// Fails unless the login owns the cloud and the owner has chosen a passcode.
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
        let _held = state.cloud_lock.lock().await;
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
        let _held = state.cloud_lock.lock().await;
        let mut conn = load(&state)?.ok_or_else(|| CloudApiError::coded("not_connected"))?;
        let project = Project::new(&conn.supabase_url, &conn.publishable_key);
        let session = project.sign_in(&conn.email, &body.password).await?;
        require_ready(&project, &session).await?;
        conn.refresh_token = session.refresh_token;
        save(&state, &conn)?;
        if recorded(&state, LAST_ERROR_KEY)["code"] == "session_expired" {
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
    Ok(plan)
}

async fn push_now(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    detached(async move {
        let held = state.cloud_lock.lock().await;
        let pass = async {
            let recording = recording_id(&state).await;
            let repo = state.meeting_repo.clone();
            let local = blocking(move || local_meetings(&repo)).await?;
            let mut link = link(&state, &held).await?;
            run_pass(&state, &mut link, local, recording).await
        };
        match pass.await {
            Ok(plan) => {
                let v = json!({ "at": now_ms(), "changed": plan.changed.len(), "deleted": plan.deleted.len() });
                record(&state, LAST_PUSH_KEY, &v);
                record(&state, LAST_ERROR_KEY, &Value::Null);
                Ok(Json(v))
            }
            Err(e) => {
                if e.code != "not_connected" {
                    record_failure(&state, &e);
                }
                Err(e)
            }
        }
    })
    .await
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
        let held = state.cloud_lock.lock().await;
        let linked = link(&state, &held).await;
        let mut linked = match linked {
            Ok(l) => Some(l),
            Err(e) if body.mark_deleted => {
                if e.code != "not_connected" {
                    record_failure(&state, &e);
                }
                return Err(e);
            }
            Err(_) => None,
        };
        if let (true, Some(l)) = (body.mark_deleted, linked.as_mut()) {
            if let Err(e) = run_pass(&state, l, Vec::new(), None).await {
                record_failure(&state, &e);
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
        Ok(Json(json!({ "connected": false })))
    })
    .await
}
