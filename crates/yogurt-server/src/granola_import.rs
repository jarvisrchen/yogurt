//! Granola import: the key, a preview, and the background job that copies notes into the library.
//! The job is the only code in yogurt that talks to Granola. It sends the API key and read requests, nothing else.
//! The key lives in `keys.json` under [`ACCOUNT`] and appears in no response, frame, log line, or error.
//! Progress goes out as `import_*` frames on `/ws`. `GET /api/import/granola` carries the same state for a client that reconnected.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use yogurt_granola::{map_note, Client, Config, Error as GranolaError, SOURCE};

use crate::markdown_exporter::Meeting as ExpMeeting;
use crate::sanitize::sanitize_enriched_md;
use crate::ws::{self, WsEvent};
use crate::AppState;

use crate::bootstrap::GRANOLA_ACCOUNT as ACCOUNT;
const PROGRESS_EVERY: Duration = Duration::from_secs(1);
const MAX_FAILED_TITLES: usize = 50;
/// Notes in a row that exhaust their retries before the job calls it an outage and stops.
const OUTAGE_AFTER: usize = 5;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/import/granola",
            get(status).post(start).delete(cancel),
        )
        .route("/api/import/granola/key", put(set_key).delete(delete_key))
        .route("/api/import/granola/preview", post(preview))
}

#[derive(Clone, Default)]
struct Counts {
    total: usize,
    imported: usize,
    already_imported: usize,
    empty: usize,
    failed: usize,
}

impl Counts {
    fn skipped(&self) -> usize {
        self.already_imported + self.empty
    }
    fn json(&self) -> Value {
        json!({
            "done": self.imported + self.skipped() + self.failed,
            "total": self.total,
            "imported": self.imported,
            "skipped": self.skipped(),
            "already_imported": self.already_imported,
            "empty": self.empty,
            "failed": self.failed,
        })
    }
}

struct Running {
    cancel: watch::Sender<bool>,
    counts: Counts,
}

#[derive(Default)]
struct Inner {
    running: Option<Running>,
    last: Option<Value>,
}

/// One import at a time. The in-memory `last` result is what a reconnecting client reads.
pub struct ImportJob {
    cfg: Config,
    inner: Mutex<Inner>,
}

impl ImportJob {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            inner: Mutex::default(),
        }
    }

    fn begin(&self) -> Option<watch::Receiver<bool>> {
        let mut g = self.inner.lock().unwrap();
        if g.running.is_some() {
            return None;
        }
        let (cancel, rx) = watch::channel(false);
        g.running = Some(Running {
            cancel,
            counts: Counts::default(),
        });
        Some(rx)
    }

    fn update(&self, f: impl FnOnce(&mut Counts)) -> Counts {
        let mut g = self.inner.lock().unwrap();
        let r = g.running.as_mut().expect("update outside a running import");
        f(&mut r.counts);
        r.counts.clone()
    }

    fn finish(&self, outcome: &str, error: Option<String>, failed_titles: Vec<String>) -> Counts {
        let mut g = self.inner.lock().unwrap();
        let counts = g.running.take().map(|r| r.counts).unwrap_or_default();
        let mut last = counts.json();
        last["outcome"] = outcome.into();
        last["error"] = json!(error);
        last["failed_titles"] = json!(failed_titles);
        last["finished_at"] = json!(crate::routes::now_unix_ms());
        g.last = Some(last);
        counts
    }
}

impl Default for ImportJob {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

#[derive(Debug)]
enum Error {
    BadRequest(String),
    Conflict(String),
    BadGateway(String),
    Internal(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (code, msg) = match self {
            Error::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Error::Conflict(m) => (StatusCode::CONFLICT, m),
            Error::BadGateway(m) => (StatusCode::BAD_GATEWAY, m),
            Error::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (code, Json(json!({ "error": msg }))).into_response()
    }
}

fn saved_key(s: &AppState) -> Result<String, Error> {
    s.keys
        .get(ACCOUNT)
        .map_err(|e| Error::Internal(format!("read key store: {e}")))?
        .ok_or_else(|| Error::BadRequest("Save a Granola API key first.".into()))
}

fn granola_error(e: GranolaError) -> Error {
    match e {
        GranolaError::Unauthorized => Error::BadRequest(e.to_string()),
        e => Error::BadGateway(e.to_string()),
    }
}

async fn status(State(s): State<AppState>) -> Result<Json<Value>, Error> {
    let key = s
        .keys
        .masked(ACCOUNT)
        .map_err(|e| Error::Internal(format!("read key store: {e}")))?;
    let g = s.import.inner.lock().unwrap();
    Ok(Json(json!({
        "key": key,
        "running": g.running.is_some(),
        "progress": g.running.as_ref().map(|r| r.counts.json()),
        "last": g.last,
    })))
}

#[derive(Deserialize)]
struct KeyBody {
    key: String,
}

/// Checks the key with one request before storing it, so a typo never reaches `keys.json`.
async fn set_key(
    State(s): State<AppState>,
    Json(body): Json<KeyBody>,
) -> Result<Json<Value>, Error> {
    let key = body.key.trim().to_string();
    if key.is_empty() {
        return Err(Error::BadRequest("The API key is empty.".into()));
    }
    Client::new(&key, s.import.cfg.clone())
        .check()
        .await
        .map_err(granola_error)?;
    s.keys
        .set(ACCOUNT, &key)
        .map_err(|e| Error::Internal(format!("save key: {e}")))?;
    let masked = s.keys.masked(ACCOUNT).ok().flatten();
    Ok(Json(json!({ "key": masked })))
}

async fn delete_key(State(s): State<AppState>) -> Result<StatusCode, Error> {
    s.keys
        .delete(ACCOUNT)
        .map_err(|e| Error::Internal(format!("forget key: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

/// The account's own email is not in the API, so the owner of most notes stands in for the user.
fn most_common_owner(notes: &[yogurt_granola::NoteSummary]) -> Option<&str> {
    let mut by_owner: HashMap<&str, usize> = HashMap::new();
    for n in notes {
        *by_owner.entry(n.owner.email.as_str()).or_default() += 1;
    }
    by_owner.into_iter().max_by_key(|(_, c)| *c).map(|(e, _)| e)
}

async fn imported_ids(s: &AppState) -> Result<std::collections::HashSet<String>, Error> {
    let repo = s.meeting_repo.clone();
    tokio::task::spawn_blocking(move || repo.imported_source_ids(SOURCE))
        .await
        .map_err(|e| Error::Internal(e.to_string()))?
        .map_err(|e| Error::Internal(format!("{e:#}")))
}

/// Lists every note and counts what a run would do. Fetches no note bodies.
async fn preview(State(s): State<AppState>) -> Result<Json<Value>, Error> {
    if s.import.inner.lock().unwrap().running.is_some() {
        return Err(Error::Conflict("An import is already running.".into()));
    }
    let client = Client::new(&saved_key(&s)?, s.import.cfg.clone());
    let notes = client.list_notes().await.map_err(granola_error)?;
    let existing = imported_ids(&s).await?;
    let me = most_common_owner(&notes);
    let already = notes.iter().filter(|n| existing.contains(&n.id)).count();
    let others = notes
        .iter()
        .filter(|n| Some(n.owner.email.as_str()) != me)
        .count();
    Ok(Json(json!({
        "total": notes.len(),
        "already_imported": already,
        "to_import": notes.len() - already,
        "captured_by_others": others,
    })))
}

async fn start(State(s): State<AppState>) -> Result<StatusCode, Error> {
    let key = saved_key(&s)?;
    let cancel = s
        .import
        .begin()
        .ok_or_else(|| Error::Conflict("An import is already running.".into()))?;
    tokio::spawn(run(s, key, cancel));
    Ok(StatusCode::ACCEPTED)
}

/// The job sees the flag between notes. Meetings already inserted stay, and the next run resumes after them.
async fn cancel(State(s): State<AppState>) -> Result<StatusCode, Error> {
    match &s.import.inner.lock().unwrap().running {
        Some(r) => {
            r.cancel.send_replace(true);
            Ok(StatusCode::ACCEPTED)
        }
        None => Err(Error::Conflict("No import is running.".into())),
    }
}

enum End {
    Complete,
    Cancelled,
    Failed(String),
}

async fn run(s: AppState, key: String, cancel: watch::Receiver<bool>) {
    // A separate task, so a panic cannot leave the job marked running.
    let inner = {
        let s = s.clone();
        tokio::spawn(async move {
            let mut failed_titles = Vec::new();
            let end = import_all(&s, &key, cancel, &mut failed_titles).await;
            (end, failed_titles)
        })
    };
    let (end, failed_titles) = match inner.await {
        Ok(done) => done,
        Err(e) => {
            tracing::error!(error = %e, "granola import task crashed");
            (
                End::Failed("The import crashed. Start it again to resume.".into()),
                Vec::new(),
            )
        }
    };
    let (outcome, error) = match &end {
        End::Complete => ("complete", None),
        End::Cancelled => ("cancelled", None),
        End::Failed(e) => ("failed", Some(e.clone())),
    };
    let c = s.import.finish(outcome, error, failed_titles.clone());
    let ev = match end {
        End::Complete => WsEvent::ImportComplete {
            imported: c.imported,
            skipped: c.skipped(),
            failed: c.failed,
            failed_titles,
        },
        End::Cancelled => WsEvent::ImportCancelled {
            imported: c.imported,
            skipped: c.skipped(),
            failed: c.failed,
        },
        End::Failed(error) => WsEvent::ImportError { error },
    };
    ws::send_event(&s.app_events_tx, ev);
}

fn progress_event(c: &Counts) -> WsEvent {
    WsEvent::ImportProgress {
        done: c.imported + c.skipped() + c.failed,
        total: c.total,
        imported: c.imported,
        skipped: c.skipped(),
        failed: c.failed,
    }
}

async fn import_all(
    s: &AppState,
    key: &str,
    mut cancel: watch::Receiver<bool>,
    failed_titles: &mut Vec<String>,
) -> End {
    let client = Client::new(key, s.import.cfg.clone());
    let notes = match cancellable(&mut cancel, client.list_notes()).await {
        None => return End::Cancelled,
        Some(Ok(n)) => n,
        Some(Err(e)) => return End::Failed(e.to_string()),
    };
    let existing = match imported_ids(s).await {
        Ok(e) => e,
        Err(_) => return End::Failed("could not read the library".into()),
    };
    let c = s.import.update(|c| {
        c.total = notes.len();
        c.already_imported = notes.iter().filter(|n| existing.contains(&n.id)).count();
    });
    ws::send_event(&s.app_events_tx, progress_event(&c));

    let mut last_emit = Instant::now();
    let mut exhausted_in_a_row = 0;
    for n in notes.iter().filter(|n| !existing.contains(&n.id)) {
        if *cancel.borrow() {
            return End::Cancelled;
        }
        let step = import_note(s, &client, &n.id, &mut cancel).await;
        if matches!(step, Err(Stop::Exhausted(_))) {
            exhausted_in_a_row += 1;
        } else {
            exhausted_in_a_row = 0;
        }
        let c = match step {
            Err(Stop::Cancelled) => return End::Cancelled,
            Err(Stop::Exhausted(e)) if exhausted_in_a_row >= OUTAGE_AFTER => {
                return End::Failed(e);
            }
            Ok(Step::Imported) => s.import.update(|c| c.imported += 1),
            Ok(Step::Existing) => s.import.update(|c| c.already_imported += 1),
            Ok(Step::Empty) => s.import.update(|c| c.empty += 1),
            Err(Stop::Fatal(e)) => return End::Failed(e),
            Err(Stop::Note(e) | Stop::Exhausted(e)) => {
                tracing::warn!(note = %n.id, error = %e, "granola note failed to import");
                if failed_titles.len() < MAX_FAILED_TITLES {
                    failed_titles.push(n.title.clone().unwrap_or_else(|| n.id.clone()));
                }
                s.import.update(|c| c.failed += 1)
            }
        };
        if last_emit.elapsed() >= PROGRESS_EVERY {
            last_emit = Instant::now();
            ws::send_event(&s.app_events_tx, progress_event(&c));
        }
    }
    End::Complete
}

enum Step {
    Imported,
    Existing,
    Empty,
}

enum Stop {
    /// The whole job cannot continue.
    Fatal(String),
    /// Only this note fails.
    Note(String),
    /// This note kept failing through every retry. Several in a row mean Granola is down.
    Exhausted(String),
    Cancelled,
}

/// Runs `fut` unless cancel is requested first, which drops it and so aborts an in-flight request or retry sleep.
async fn cancellable<T>(
    cancel: &mut watch::Receiver<bool>,
    fut: impl std::future::Future<Output = T>,
) -> Option<T> {
    tokio::select! {
        r = fut => Some(r),
        _ = cancel.wait_for(|c| *c) => None,
    }
}

async fn import_note(
    s: &AppState,
    client: &Client,
    id: &str,
    cancel: &mut watch::Receiver<bool>,
) -> Result<Step, Stop> {
    #[cfg(test)]
    if id == "not_panic" {
        panic!("forced panic");
    }
    let note = cancellable(cancel, client.get_note(id))
        .await
        .ok_or(Stop::Cancelled)?
        .map_err(|e| match e {
            GranolaError::Unauthorized => Stop::Fatal(e.to_string()),
            GranolaError::Exhausted(_) => Stop::Exhausted(e.to_string()),
            e => Stop::Note(e.to_string()),
        })?;
    let mapped = map_note(&note, sanitize_enriched_md, |ms| {
        format!("Meeting {}", yogurt_db::meetings::short_date(ms))
    })
    .map_err(Stop::Note)?;
    let Some(meeting) = mapped else {
        return Ok(Step::Empty);
    };
    let repo = s.meeting_repo.clone();
    let exporter = s.markdown_exporter.clone();
    let inserted = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let Some(m) = repo.import(meeting)? else {
            return Ok(false);
        };
        // The file is re-emitted on demand, so a failed write does not undo the import.
        if let Err(e) = exporter.write(&ExpMeeting {
            id: &m.id,
            title: &m.title,
            started_at_unix_ms: m.started_at,
            ended_at_unix_ms: m.ended_at,
            body_md: m.enriched_md.as_deref().unwrap_or(&m.notes_md),
        }) {
            tracing::warn!(meeting = %m.id, error = %e, "markdown export failed after import");
        }
        Ok(true)
    })
    .await
    .map_err(|e| Stop::Note(e.to_string()))?
    .map_err(|e| Stop::Note(format!("{e:#}")))?;
    Ok(if inserted {
        Step::Imported
    } else {
        Step::Existing
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const KEY: &str = "grn_secret_abcdwxyz";

    struct Harness {
        addr: std::net::SocketAddr,
        token: String,
        state: AppState,
        mock: MockServer,
        http: reqwest::Client,
        _tmp: tempfile::TempDir,
    }

    impl Harness {
        async fn new() -> Self {
            let mock = MockServer::start().await;
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
            state.import = Arc::new(ImportJob::new(Config {
                base_url: mock.uri(),
                min_interval: Duration::ZERO,
                backoff: Duration::from_millis(1),
                request_timeout: Duration::from_secs(5),
            }));
            let token = state.session.as_str().to_string();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let app = crate::routes::router(state.clone());
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
            Self {
                addr,
                token,
                state,
                mock,
                http: reqwest::Client::new(),
                _tmp: tmp,
            }
        }

        async fn call(&self, m: reqwest::Method, p: &str, body: Option<Value>) -> (u16, Value) {
            let mut req = self
                .http
                .request(m, format!("http://{}{p}", self.addr))
                .bearer_auth(&self.token);
            if let Some(b) = body {
                req = req.json(&b);
            }
            let resp = req.send().await.unwrap();
            let code = resp.status().as_u16();
            (code, resp.json().await.unwrap_or(Value::Null))
        }

        async fn save_key(&self) {
            Mock::given(method("GET"))
                .and(path("/notes"))
                .and(query_param("page_size", "1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({ "notes": [], "hasMore": false, "cursor": null })),
                )
                .mount(&self.mock)
                .await;
            let (code, _) = self
                .call(
                    reqwest::Method::PUT,
                    "/api/import/granola/key",
                    Some(json!({ "key": KEY })),
                )
                .await;
            assert_eq!(code, 200);
        }

        async fn mock_list(&self, notes: &[(&str, &str, &str)]) {
            let items: Vec<Value> = notes
                .iter()
                .map(|(id, title, owner)| {
                    json!({ "id": id, "title": title, "owner": { "name": null, "email": owner } })
                })
                .collect();
            Mock::given(method("GET"))
                .and(path("/notes"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({ "notes": items, "hasMore": false, "cursor": null })),
                )
                .mount(&self.mock)
                .await;
        }

        async fn mock_note(&self, id: &str, body: Value, delay_ms: u64) {
            Mock::given(method("GET"))
                .and(path(format!("/notes/{id}")))
                .and(query_param("include", "transcript"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(body)
                        .set_delay(Duration::from_millis(delay_ms)),
                )
                .mount(&self.mock)
                .await;
        }

        async fn wait_idle(&self) -> Value {
            for _ in 0..400 {
                let (_, st) = self
                    .call(reqwest::Method::GET, "/api/import/granola", None)
                    .await;
                if st["running"] == false {
                    return st;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            panic!("import did not finish");
        }
    }

    fn note(id: &str, title: &str, owner: &str, transcript: Value) -> Value {
        json!({
            "id": id,
            "title": title,
            "owner": { "name": "Pat", "email": owner },
            "created_at": "2026-01-02T09:00:00Z",
            "updated_at": "2026-01-02T10:00:00Z",
            "web_url": "https://notes.example/d/1",
            "folder_membership": [{ "name": "Clients" }],
            "summary_markdown": "### Decisions\n\n- ship it <script>alert(1)</script>",
            "private_notes_markdown": null,
            "transcript": transcript,
        })
    }

    fn lines() -> Value {
        json!([{
            "speaker": { "source": "microphone", "attribution": "me" },
            "text": "synthetic words",
            "start_time": "2026-01-02T09:00:05Z",
            "end_time": "2026-01-02T09:00:08Z",
        }])
    }

    #[tokio::test]
    async fn the_key_is_checked_stored_and_never_returned() {
        let h = Harness::new().await;
        Mock::given(method("GET"))
            .and(path("/notes"))
            .respond_with(ResponseTemplate::new(401))
            .up_to_n_times(1)
            .mount(&h.mock)
            .await;
        let (code, body) = h
            .call(
                reqwest::Method::PUT,
                "/api/import/granola/key",
                Some(json!({ "key": "nope" })),
            )
            .await;
        assert_eq!(code, 400);
        assert!(body["error"].as_str().unwrap().contains("rejected"));
        let (_, st) = h
            .call(reqwest::Method::GET, "/api/import/granola", None)
            .await;
        assert_eq!(st["key"], Value::Null, "a rejected key is not stored");

        h.mock_list(&[]).await;
        let (code, put) = h
            .call(
                reqwest::Method::PUT,
                "/api/import/granola/key",
                Some(json!({ "key": KEY })),
            )
            .await;
        assert_eq!(code, 200);
        let (_, st) = h
            .call(reqwest::Method::GET, "/api/import/granola", None)
            .await;
        assert_eq!(st["key"], "\u{2022}\u{2022}\u{2022}\u{2022}wxyz");
        for body in [put.to_string(), st.to_string()] {
            assert!(!body.contains(KEY), "the key leaked: {body}");
        }
        assert_eq!(h.state.keys.get(ACCOUNT).unwrap().as_deref(), Some(KEY));

        let (code, _) = h
            .call(reqwest::Method::DELETE, "/api/import/granola/key", None)
            .await;
        assert_eq!(code, 204);
        assert_eq!(h.state.keys.get(ACCOUNT).unwrap(), None);
        let (code, _) = h
            .call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        assert_eq!(code, 400, "no key, no import");
    }

    #[tokio::test]
    async fn imports_skips_dedupes_and_streams_frames() {
        let h = Harness::new().await;
        h.save_key().await;
        h.mock_list(&[
            ("not_a", "Alpha", "me@example.com"),
            ("not_b", "Bravo", "me@example.com"),
            ("not_c", "Empty", "me@example.com"),
            ("not_d", "Shared", "other@example.com"),
        ])
        .await;
        h.mock_note(
            "not_a",
            note("not_a", "Alpha", "me@example.com", lines()),
            0,
        )
        .await;
        h.mock_note(
            "not_b",
            note("not_b", "Bravo", "me@example.com", lines()),
            0,
        )
        .await;
        h.mock_note(
            "not_c",
            note("not_c", "Empty", "me@example.com", json!([])),
            0,
        )
        .await;
        h.mock_note(
            "not_d",
            note("not_d", "Shared", "other@example.com", lines()),
            0,
        )
        .await;

        let (_, pre) = h
            .call(reqwest::Method::POST, "/api/import/granola/preview", None)
            .await;
        assert_eq!(pre["total"], 4);
        assert_eq!(pre["already_imported"], 0);
        assert_eq!(pre["captured_by_others"], 1);

        let mut rx = h.state.app_events_tx.subscribe();
        let (code, _) = h
            .call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        assert_eq!(code, 202);
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["outcome"], "complete");
        assert_eq!(st["last"]["imported"], 3);
        assert_eq!(st["last"]["empty"], 1);
        assert_eq!(st["last"]["failed"], 0);
        assert!(!st.to_string().contains(KEY));

        let mut types = Vec::new();
        while let Ok(f) = rx.try_recv() {
            assert!(!f.to_string().contains(KEY));
            types.push(f["type"].as_str().unwrap().to_string());
        }
        assert_eq!(types.first().map(String::as_str), Some("import_progress"));
        assert_eq!(types.last().map(String::as_str), Some("import_complete"));

        let page = h
            .state
            .meeting_repo
            .list_page(&Default::default(), 100, 0)
            .unwrap();
        assert_eq!(page.total, 3);
        let shared = page.meetings.iter().find(|m| m.title == "Shared").unwrap();
        assert_eq!(shared.source.as_deref(), Some("granola"));
        let full = h.state.meeting_repo.get(&shared.id).unwrap().unwrap();
        assert_eq!(full.source_id.as_deref(), Some("not_d"));
        assert_eq!(
            full.source_json.as_ref().unwrap()["owner"]["email"],
            "other@example.com"
        );
        assert_eq!(full.labels[0].name, "Clients");
        let alpha = page.meetings.iter().find(|m| m.title == "Alpha").unwrap();
        for (id, is_user) in [(&shared.id, false), (&alpha.id, true)] {
            let (_, got) = h
                .call(reqwest::Method::GET, &format!("/api/meetings/{id}"), None)
                .await;
            assert_eq!(got["source_json"]["owner"]["is_user"], is_user);
        }
        assert!(
            !full.enriched_md.unwrap().contains("<script"),
            "summary is sanitized"
        );
        assert!(full.ended_at.unwrap() > full.started_at);

        let (_, pre) = h
            .call(reqwest::Method::POST, "/api/import/granola/preview", None)
            .await;
        assert_eq!(pre["already_imported"], 3);
        let (code, _) = h
            .call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        assert_eq!(code, 202);
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["imported"], 0, "a re-run imports nothing");
        assert_eq!(st["last"]["already_imported"], 3);
        assert_eq!(
            h.state
                .meeting_repo
                .list_page(&Default::default(), 100, 0)
                .unwrap()
                .total,
            3
        );
    }

    #[tokio::test]
    async fn one_bad_note_fails_alone_and_a_rejected_key_stops_the_job() {
        let h = Harness::new().await;
        h.save_key().await;
        h.mock_list(&[
            ("not_a", "Alpha", "me@example.com"),
            ("not_b", "Bravo", "me@example.com"),
        ])
        .await;
        Mock::given(method("GET"))
            .and(path("/notes/not_a"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&h.mock)
            .await;
        h.mock_note(
            "not_b",
            note("not_b", "Bravo", "me@example.com", lines()),
            0,
        )
        .await;
        h.call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["imported"], 1);
        assert_eq!(st["last"]["failed"], 1);
        assert_eq!(st["last"]["failed_titles"], json!(["Alpha"]));

        h.mock.reset().await;
        h.mock_list(&[("not_z", "Zed", "me@example.com")]).await;
        Mock::given(method("GET"))
            .and(path("/notes/not_z"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&h.mock)
            .await;
        let mut rx = h.state.app_events_tx.subscribe();
        h.call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["outcome"], "failed");
        assert_eq!(st["last"]["error"], "Granola rejected the API key");
        let mut last = Value::Null;
        while let Ok(f) = rx.try_recv() {
            last = f;
        }
        assert_eq!(last["type"], "import_error");
    }

    #[tokio::test]
    async fn a_note_that_always_fails_does_not_block_the_rest_but_an_outage_stops_the_job() {
        let h = Harness::new().await;
        h.save_key().await;
        h.mock_list(&[
            ("not_a", "Poison", "me@example.com"),
            ("not_b", "Bravo", "me@example.com"),
            ("not_c", "Charlie", "me@example.com"),
        ])
        .await;
        Mock::given(method("GET"))
            .and(path("/notes/not_a"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&h.mock)
            .await;
        h.mock_note(
            "not_b",
            note("not_b", "Bravo", "me@example.com", lines()),
            0,
        )
        .await;
        h.mock_note(
            "not_c",
            note("not_c", "Charlie", "me@example.com", lines()),
            0,
        )
        .await;
        h.call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["outcome"], "complete");
        assert_eq!(st["last"]["imported"], 2);
        assert_eq!(st["last"]["failed"], 1);
        assert_eq!(st["last"]["failed_titles"], json!(["Poison"]));

        h.mock.reset().await;
        let ids = ["not_1", "not_2", "not_3", "not_4", "not_5", "not_6"];
        h.mock_list(&ids.map(|i| (i, "Down", "me@example.com")))
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&h.mock)
            .await;
        h.call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["outcome"], "failed");
        assert_eq!(
            st["last"]["failed"], 4,
            "the fifth consecutive failure ends the job"
        );
    }

    #[tokio::test]
    async fn cancel_interrupts_a_request_that_is_in_flight() {
        let h = Harness::new().await;
        h.save_key().await;
        h.mock_list(&[("not_a", "Stuck", "me@example.com")]).await;
        h.mock_note(
            "not_a",
            note("not_a", "Stuck", "me@example.com", lines()),
            30_000,
        )
        .await;
        h.call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        h.call(reqwest::Method::DELETE, "/api/import/granola", None)
            .await;
        let t = Instant::now();
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["outcome"], "cancelled");
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn a_panic_clears_running_and_reports_an_error() {
        let h = Harness::new().await;
        h.save_key().await;
        h.mock_list(&[("not_panic", "Boom", "me@example.com")])
            .await;
        let mut rx = h.state.app_events_tx.subscribe();
        let (code, _) = h
            .call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        assert_eq!(code, 202);
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["outcome"], "failed");
        let mut last = Value::Null;
        while let Ok(f) = rx.try_recv() {
            last = f;
        }
        assert_eq!(last["type"], "import_error");
        let (code, _) = h
            .call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        assert_eq!(code, 202, "a new import can start");
        h.wait_idle().await;
    }

    #[tokio::test]
    async fn a_second_start_is_409_and_cancel_stops_between_notes() {
        let h = Harness::new().await;
        h.save_key().await;
        let ids = ["not_a", "not_b", "not_c", "not_d"];
        h.mock_list(&ids.map(|i| (i, "Slow", "me@example.com")))
            .await;
        for id in ids {
            h.mock_note(id, note(id, "Slow", "me@example.com", lines()), 150)
                .await;
        }
        let (code, _) = h
            .call(reqwest::Method::DELETE, "/api/import/granola", None)
            .await;
        assert_eq!(code, 409, "nothing to cancel");

        let mut rx = h.state.app_events_tx.subscribe();
        let (code, _) = h
            .call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        assert_eq!(code, 202);
        let (code, _) = h
            .call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        assert_eq!(code, 409);
        let (code, _) = h
            .call(reqwest::Method::DELETE, "/api/import/granola", None)
            .await;
        assert_eq!(code, 202);
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["outcome"], "cancelled");
        assert!(st["last"]["imported"].as_u64().unwrap() < 4);
        let mut last = Value::Null;
        while let Ok(f) = rx.try_recv() {
            last = f;
        }
        assert_eq!(last["type"], "import_cancelled");

        h.call(reqwest::Method::POST, "/api/import/granola", None)
            .await;
        let st = h.wait_idle().await;
        assert_eq!(st["last"]["outcome"], "complete");
        assert_eq!(
            h.state
                .meeting_repo
                .list_page(&Default::default(), 100, 0)
                .unwrap()
                .total,
            4
        );
    }
}
