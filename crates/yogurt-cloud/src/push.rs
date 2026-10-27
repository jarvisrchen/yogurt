//! One push pass (design Flow 4): list what the cloud holds for this device, send the meetings whose content changed in small paced batches, and mark the ones that disappeared.

use crate::{Error, Project, Result, Session};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::watch;

/// How a pass paces itself. A batch closes at `rows` meetings or once its bodies reach `bytes`, so it can overshoot by one meeting.
/// A failed request waits `retries[n]` before attempt n + 2, or the server's `Retry-After` when that is longer.
#[derive(Clone, Debug)]
pub struct Pacing {
    pub rows: usize,
    pub bytes: usize,
    pub pause: Duration,
    pub retries: Vec<Duration>,
}

impl Default for Pacing {
    /// Supabase gives the `authenticated` role an 8 second statement timeout, and a free instance has a shared CPU and disk budget.
    /// Small batches keep each insert far below both, and the pause leaves the instance idle between requests.
    fn default() -> Self {
        Self {
            rows: 25,
            bytes: 256 << 10,
            pause: Duration::from_secs(1),
            retries: [5, 15, 45, 120].map(Duration::from_secs).to_vec(),
        }
    }
}

/// The longest a `Retry-After` can make a retry wait.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(300);

/// Reported to the caller as a pass moves. `retry_in` is set while the pass waits to retry a failed request.
/// `batches` is how many requests the whole pass needs, known once the plan is made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub sent: usize,
    pub total: usize,
    pub batches: usize,
    pub retry_in: Option<Duration>,
}

pub struct LocalMeeting {
    pub id: String,
    /// The hash stored after the last successful push, `None` if never pushed.
    pub cloud_hash: Option<String>,
    pub hash: String,
    /// Bytes of the serialized bundle, used to size batches before any bundle is rebuilt.
    pub size: usize,
}

impl LocalMeeting {
    /// Keeps the hash and size, not the body, so a pass holds one batch of bundles instead of the whole library.
    pub fn new(id: String, cloud_hash: Option<String>, body: &Value) -> Self {
        let bytes = body_bytes(body);
        Self {
            id,
            cloud_hash,
            hash: digest(&bytes),
            size: bytes.len(),
        }
    }
}

fn body_bytes(body: &Value) -> Vec<u8> {
    serde_json::to_vec(body).expect("a Value serializes")
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `serde_json` orders object keys, so the same content always hashes the same.
pub fn hash_body(body: &Value) -> String {
    digest(&body_bytes(body))
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub changed: Vec<String>,
    pub deleted: Vec<String>,
}

/// `skip` is the meeting being recorded: it is neither pushed nor, if the cloud already has it, marked deleted.
/// A meeting the cloud no longer lists counts as changed even when its hash matches, so a purged device heals on the next pass.
pub fn plan(local: &[LocalMeeting], cloud_ids: &[String], skip: Option<&str>) -> Plan {
    let in_cloud: HashSet<&str> = cloud_ids.iter().map(String::as_str).collect();
    let here: HashSet<&str> = local.iter().map(|m| m.id.as_str()).collect();
    let mut changed: Vec<String> = local
        .iter()
        .filter(|m| Some(m.id.as_str()) != skip)
        .filter(|m| m.cloud_hash.as_deref() != Some(&m.hash) || !in_cloud.contains(m.id.as_str()))
        .map(|m| m.id.clone())
        .collect();
    let mut deleted: Vec<String> = cloud_ids
        .iter()
        .filter(|id| !here.contains(id.as_str()) && Some(id.as_str()) != skip)
        .cloned()
        .collect();
    changed.sort();
    deleted.sort();
    Plan { changed, deleted }
}

/// Splits a pass's request sizes into consecutive batches, closing one at `rows` entries or once it holds `bytes`.
fn batches(sizes: &[usize], pacing: &Pacing) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let (mut start, mut bytes) = (0, 0);
    for (i, n) in sizes.iter().enumerate() {
        if i > start && (i - start >= pacing.rows || bytes >= pacing.bytes) {
            out.push(start..i);
            start = i;
            bytes = 0;
        }
        bytes += n;
    }
    if start < sizes.len() {
        out.push(start..sizes.len());
    }
    out
}

/// How long to wait before attempt `attempt + 2`, or `None` when the error is final or the retries are spent.
/// A 429, a 5xx, a statement timeout (`57014`), and a network failure are worth another try. A 401, an RLS denial, or a bad request is not.
fn delay(pacing: &Pacing, attempt: usize, e: &Error) -> Option<Duration> {
    let (transient, retry_after) = match e {
        Error::Network(_) => (true, None),
        Error::Http {
            status,
            code,
            retry_after,
            ..
        } => (
            *status == 429 || *status >= 500 || code == "57014",
            *retry_after,
        ),
        _ => (false, None),
    };
    let scheduled = *pacing.retries.get(attempt)?;
    transient.then(|| {
        retry_after
            .map_or(scheduled, |s| scheduled.max(Duration::from_secs(s)))
            .min(MAX_RETRY_WAIT)
    })
}

/// Sleeps, or returns `Cancelled` as soon as the caller asks the pass to stop.
/// A dropped sender means nobody can ask, so the sleep runs to its end.
async fn wait(d: Duration, cancel: &mut watch::Receiver<bool>) -> Result<()> {
    let stop = async {
        if cancel.wait_for(|stop| *stop).await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    tokio::select! {
        () = tokio::time::sleep(d) => Ok(()),
        () = stop => Err(Error::Cancelled),
    }
}

/// Renews the access token when it is within 5 minutes of expiring, so a long pass never trips a 401.
async fn fresh(project: &Project, session: &mut Session, hooks: &mut impl Hooks) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    if session.expires_at - now < 300 {
        *session = project.refresh(&session.refresh_token).await?;
        hooks.refreshed(session).map_err(Error::Local)?;
    }
    Ok(())
}

/// What a pass asks of its caller. Every `Err` string ends the pass as [`Error::Local`].
pub trait Hooks {
    /// Builds one meeting's document when its batch is about to go out, so memory holds one batch at a time. `None` means the meeting is gone and is skipped.
    fn bundle(&mut self, id: &str) -> std::result::Result<Option<Value>, String>;
    /// Runs with every renewed session, so the caller can persist the rotated refresh token before it is used.
    fn refreshed(&mut self, session: &Session) -> std::result::Result<(), String>;
    /// Runs after each batch commits with the `(id, hash)` of its changed meetings, so a later failure or a stop leaves earlier batches recorded.
    fn stored(&mut self, hashes: &[(String, String)]) -> std::result::Result<(), String>;
    /// Runs when the plan is made, after each batch, and when a failed request starts waiting to be retried.
    fn progress(&mut self, progress: &Progress);
}

/// One pass over `local`. `skip` is the meeting being recorded.
pub struct Pass<'a> {
    pub project: &'a Project,
    pub device_id: &'a str,
    pub local: &'a [LocalMeeting],
    pub skip: Option<&'a str>,
    pub pacing: &'a Pacing,
}

impl Pass<'_> {
    /// `session` is updated in place when it is renewed.
    /// A committed batch is followed by a search catch-up, so search fills in while a backlog uploads.
    /// `cancel` stops the pass at the next batch boundary or retry wait with [`Error::Cancelled`].
    /// A pass that finds nothing to do starts no push, so the history stays free of empty pushes.
    /// A failing batch's error names its first and last meeting id.
    pub async fn run(
        &self,
        session: &mut Session,
        cancel: &mut watch::Receiver<bool>,
        hooks: &mut impl Hooks,
    ) -> Result<Plan> {
        let Self {
            project,
            device_id,
            local,
            skip,
            pacing,
        } = *self;
        let args = json!({ "device_id": device_id });
        let mut attempt = 0;
        let cloud: Vec<String> = loop {
            fresh(project, session, hooks).await?;
            match project
                .rpc_all(session, "yogurt_device_meetings", &args, "")
                .await
            {
                Ok(v) => break v,
                Err(e) => {
                    let Some(d) = delay(pacing, attempt, &e) else {
                        return Err(e);
                    };
                    wait(d, cancel).await?;
                    attempt += 1;
                }
            }
        }
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
        let plan = plan(local, &cloud, skip);
        if plan.changed.is_empty() && plan.deleted.is_empty() {
            return Ok(plan);
        }
        fresh(project, session, hooks).await?;
        let push_id = project
            .rpc(session, "yogurt_start_push", &args)
            .await?
            .as_i64()
            .ok_or_else(|| Error::Network("start_push returned no push id".into()))?;

        let size: HashMap<&str, usize> = local.iter().map(|m| (m.id.as_str(), m.size)).collect();
        let ids: Vec<&String> = plan.changed.iter().chain(&plan.deleted).collect();
        let sizes: Vec<usize> = plan
            .changed
            .iter()
            .map(|id| size.get(id.as_str()).copied().unwrap_or(0))
            .chain(plan.deleted.iter().map(|_| 0))
            .collect();
        let groups = batches(&sizes, pacing);
        let total = ids.len();
        let mut sent = 0;
        let report = |sent, retry_in| Progress {
            sent,
            total,
            batches: groups.len(),
            retry_in,
        };
        hooks.progress(&report(0, None));

        let changed: HashSet<&String> = plan.changed.iter().collect();
        for (n, group) in groups.iter().enumerate() {
            if *cancel.borrow() {
                return Err(Error::Cancelled);
            }
            let mut rows = Vec::new();
            let mut hashes = Vec::new();
            for id in &ids[group.clone()] {
                let body = if changed.contains(id) {
                    let Some(body) = hooks.bundle(id).map_err(Error::Local)? else {
                        continue;
                    };
                    hashes.push((id.to_string(), hash_body(&body)));
                    body
                } else {
                    Value::Null
                };
                rows.push(json!({ "meeting_id": id, "push_id": push_id, "device_id": device_id, "body": body }));
            }
            if rows.is_empty() {
                sent += group.len();
                continue;
            }
            let span = (ids[group.start].as_str(), ids[group.end - 1].as_str());
            let rows = Value::Array(rows);
            let mut attempt = 0;
            loop {
                fresh(project, session, hooks).await?;
                match project.insert(session, "yogurt_versions", &rows).await {
                    Ok(()) => break,
                    // A retry after a lost response finds the first attempt's rows already committed.
                    Err(Error::Http {
                        status: 409, code, ..
                    }) if attempt > 0 && code == "23505" => break,
                    Err(e) => {
                        let Some(d) = delay(pacing, attempt, &e) else {
                            return Err(match e {
                                Error::Http {
                                    status,
                                    code,
                                    message,
                                    retry_after,
                                } => Error::Http {
                                    status,
                                    code,
                                    message: format!(
                                        "{message} (batch of meetings {} to {})",
                                        span.0, span.1
                                    ),
                                    retry_after,
                                },
                                other => other,
                            });
                        };
                        hooks.progress(&report(sent, Some(d)));
                        wait(d, cancel).await?;
                        attempt += 1;
                    }
                }
            }
            hooks.stored(&hashes).map_err(Error::Local)?;
            sent += group.len();
            catch_up_quietly(project, session).await;
            hooks.progress(&report(sent, None));
            if n + 1 < groups.len() {
                wait(pacing.pause, cancel).await?;
            }
        }
        Ok(plan)
    }
}

/// Calls per pass, each about 2 seconds of server time, so one pass catches up a large push and a longer backlog continues on the next pass.
const CATCH_UP_CALLS: usize = 10;

/// Search work never runs inside a push, so a pass ends by bringing the cloud's search index up to the latest versions.
/// Returns how many meetings were still behind after the last call.
pub async fn catch_up_search(project: &Project, session: &Session) -> Result<i64> {
    let mut left = 0;
    for _ in 0..CATCH_UP_CALLS {
        left = project
            .rpc(
                session,
                "yogurt_search_catch_up",
                &json!({ "max_ms": 2000 }),
            )
            .await?
            .as_i64()
            .ok_or_else(|| Error::Network("search catch-up returned no count".into()))?;
        if left == 0 {
            break;
        }
    }
    Ok(left)
}

/// A lagging index only delays search, so a failure is logged and never fails the push.
pub async fn catch_up_quietly(project: &Project, session: &Session) {
    match catch_up_search(project, session).await {
        Ok(0) => {}
        Ok(left) => tracing::info!(left, "search index still catching up"),
        Err(e) => tracing::warn!(code = e.code(), error = %e, "search index catch-up failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, stored: Option<&str>, body: Value) -> LocalMeeting {
        LocalMeeting::new(id.into(), stored.map(str::to_string), &body)
    }
    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plan_sends_new_and_edited_and_marks_vanished() {
        let same = m("a", None, json!({"n": 1}));
        let same = m("a", Some(&same.hash.clone()), json!({"n": 1}));
        let edited = m("b", Some("stale"), json!({"n": 2}));
        let fresh = m("c", None, json!({"n": 3}));
        let p = plan(&[same, edited, fresh], &ids(&["a", "b", "gone"]), None);
        assert_eq!(p.changed, ids(&["b", "c"]));
        assert_eq!(p.deleted, ids(&["gone"]));
    }

    #[test]
    fn plan_with_nothing_to_do_is_empty() {
        let a = m("a", None, json!(1));
        let a = m("a", Some(&a.hash.clone()), json!(1));
        assert_eq!(plan(&[a], &ids(&["a"]), None), Plan::default());
    }

    #[test]
    fn plan_resends_a_meeting_the_cloud_lost() {
        let a = m("a", None, json!(1));
        let a = m("a", Some(&a.hash.clone()), json!(1));
        assert_eq!(plan(&[a], &[], None).changed, ids(&["a"]));
    }

    #[test]
    fn plan_skips_the_recording_meeting_both_ways() {
        let p = plan(&[m("rec", None, json!(1))], &[], Some("rec"));
        assert_eq!(p, Plan::default());
        let p = plan(&[], &ids(&["rec"]), Some("rec"));
        assert_eq!(p, Plan::default());
    }

    #[test]
    fn plan_with_no_local_meetings_deletes_everything() {
        assert_eq!(plan(&[], &ids(&["b", "a"]), None).deleted, ids(&["a", "b"]));
    }

    #[test]
    fn hash_follows_content_only() {
        assert_eq!(
            hash_body(&json!({"b": 1, "a": 2})),
            hash_body(&json!({"a": 2, "b": 1}))
        );
        assert_ne!(hash_body(&json!({"a": 1})), hash_body(&json!({"a": 2})));
        assert_eq!(hash_body(&json!(null)).len(), 64);
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    fn sizes(pacing: &Pacing, sizes: &[usize]) -> Vec<usize> {
        batches(sizes, pacing).iter().map(|r| r.len()).collect()
    }

    #[test]
    fn batches_close_on_row_and_byte_limits() {
        let pacing = Pacing {
            rows: 100,
            bytes: 1 << 20,
            ..Pacing::default()
        };
        assert_eq!(sizes(&pacing, &[1; 250]), [100, 100, 50]);
        assert_eq!(sizes(&pacing, &[1 << 19; 5]), [2, 2, 1]);
        assert!(batches(&[], &pacing).is_empty());
    }

    #[test]
    fn the_default_pacing_keeps_a_30_kb_backlog_in_small_requests() {
        let p = Pacing::default();
        let b = batches(&[30_000; 1400], &p);
        assert!(b.iter().all(|r| r.len() <= 9), "{:?}", b[0]);
        assert!((150..=190).contains(&b.len()), "{}", b.len());
        assert_eq!(sizes(&p, &[100; 60]), [25, 25, 10]);
    }

    #[test]
    fn retries_follow_the_schedule_and_honor_retry_after() {
        let p = Pacing::default();
        let http = |status: u16, code: &str, retry_after| Error::Http {
            status,
            code: code.into(),
            message: String::new(),
            retry_after,
        };
        let secs = |e: &Error, n| delay(&p, n, e).map(|d| d.as_secs());
        let net = Error::Network("down".into());
        assert_eq!(secs(&net, 0), Some(5));
        assert_eq!(secs(&net, 3), Some(120));
        assert_eq!(secs(&net, 4), None);
        assert_eq!(secs(&http(500, "", None), 1), Some(15));
        assert_eq!(secs(&http(429, "", None), 2), Some(45));
        assert_eq!(secs(&http(400, "57014", None), 0), Some(5));
        assert_eq!(secs(&http(429, "", Some(60)), 0), Some(60));
        assert_eq!(secs(&http(429, "", Some(2)), 1), Some(15));
        assert_eq!(secs(&http(503, "", Some(99_999)), 0), Some(300));
        for fatal in [
            http(401, "", None),
            http(403, "42501", None),
            http(400, "", None),
            http(409, "23505", None),
        ] {
            assert_eq!(secs(&fatal, 0), None, "{fatal}");
        }
        assert_eq!(secs(&Error::SessionExpired, 0), None);
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const INSERT: &str = "/rest/v1/yogurt_versions";

    #[derive(Default)]
    struct Rec {
        stored: Vec<Vec<(String, String)>>,
        progress: Vec<Progress>,
        gone: Vec<String>,
    }

    impl Hooks for Rec {
        fn bundle(&mut self, id: &str) -> std::result::Result<Option<Value>, String> {
            Ok((!self.gone.iter().any(|g| g == id))
                .then(|| json!({ "id": id, "pad": "x".repeat(50) })))
        }
        fn refreshed(&mut self, _: &Session) -> std::result::Result<(), String> {
            Ok(())
        }
        fn stored(&mut self, hashes: &[(String, String)]) -> std::result::Result<(), String> {
            self.stored.push(hashes.to_vec());
            Ok(())
        }
        fn progress(&mut self, p: &Progress) {
            self.progress.push(p.clone());
        }
    }

    fn fast(rows: usize) -> Pacing {
        Pacing {
            rows,
            bytes: usize::MAX,
            pause: Duration::from_millis(40),
            retries: vec![Duration::from_millis(10); 3],
        }
    }

    async fn cloud(listed: &[&str]) -> (MockServer, Project, Session) {
        let server = MockServer::start().await;
        let ok = |p: &str, body: Value| {
            Mock::given(method("POST"))
                .and(path(p.to_string()))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
        };
        // rpc_all pages until an empty page, so only the first page lists anything.
        Mock::given(method("POST"))
            .and(path("/rest/v1/rpc/yogurt_device_meetings"))
            .and(query_param("offset", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!(listed)))
            .mount(&server)
            .await;
        ok("/rest/v1/rpc/yogurt_device_meetings", json!([]))
            .mount(&server)
            .await;
        ok("/rest/v1/rpc/yogurt_start_push", json!(9))
            .mount(&server)
            .await;
        ok("/rest/v1/rpc/yogurt_search_catch_up", json!(0))
            .mount(&server)
            .await;
        let project = Project::new(&server.uri(), "key");
        let session = Session {
            access_token: "a".into(),
            refresh_token: "r".into(),
            user_id: "u".into(),
            expires_at: i64::MAX,
        };
        (server, project, session)
    }

    fn insert(status: u16) -> Mock {
        Mock::given(method("POST"))
            .and(path(INSERT))
            .respond_with(ResponseTemplate::new(status))
    }

    fn meetings(n: usize, stored: &[&str]) -> Vec<LocalMeeting> {
        (0..n)
            .map(|i| {
                let id = format!("m{i}");
                let body = json!({ "id": id, "pad": "x".repeat(50) });
                let hash = stored.contains(&id.as_str()).then(|| hash_body(&body));
                LocalMeeting::new(id, hash, &body)
            })
            .collect()
    }

    async fn go(
        server: &MockServer,
        project: &Project,
        session: &mut Session,
        local: &[LocalMeeting],
        pacing: &Pacing,
        rec: &mut Rec,
    ) -> Result<Plan> {
        let _ = server;
        let (_tx, mut rx) = watch::channel(false);
        Pass {
            project,
            device_id: "d",
            local,
            skip: None,
            pacing,
        }
        .run(session, &mut rx, rec)
        .await
    }

    async fn hits(server: &MockServer, p: &str) -> usize {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == p)
            .count()
    }

    #[tokio::test]
    async fn sends_paced_chunks_stores_each_and_catches_search_up() {
        let (server, project, mut session) = cloud(&[]).await;
        insert(201).mount(&server).await;
        let mut rec = Rec::default();
        let started = std::time::Instant::now();
        let plan = go(
            &server,
            &project,
            &mut session,
            &meetings(5, &[]),
            &fast(2),
            &mut rec,
        )
        .await
        .unwrap();
        assert_eq!(plan.changed.len(), 5);
        assert_eq!(hits(&server, INSERT).await, 3);
        assert_eq!(
            hits(&server, "/rest/v1/rpc/yogurt_search_catch_up").await,
            3
        );
        assert_eq!(
            rec.stored.iter().map(Vec::len).collect::<Vec<_>>(),
            [2, 2, 1]
        );
        let sent: Vec<_> = rec
            .progress
            .iter()
            .map(|p| (p.sent, p.total, p.batches))
            .collect();
        assert_eq!(sent, [(0, 5, 3), (2, 5, 3), (4, 5, 3), (5, 5, 3)]);
        assert!(
            started.elapsed() >= Duration::from_millis(80),
            "a pause between chunks, none after the last"
        );
    }

    #[tokio::test]
    async fn a_transient_failure_retries_with_retry_after_and_reports_the_wait() {
        let (server, project, mut session) = cloud(&[]).await;
        Mock::given(method("POST"))
            .and(path(INSERT))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "1"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        insert(500).up_to_n_times(1).mount(&server).await;
        insert(201).mount(&server).await;
        let mut rec = Rec::default();
        let started = std::time::Instant::now();
        go(
            &server,
            &project,
            &mut session,
            &meetings(2, &[]),
            &fast(10),
            &mut rec,
        )
        .await
        .unwrap();
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "waited out Retry-After"
        );
        assert_eq!(hits(&server, INSERT).await, 3);
        let waits: Vec<_> = rec.progress.iter().filter_map(|p| p.retry_in).collect();
        assert_eq!(waits, [Duration::from_secs(1), Duration::from_millis(10)]);
        assert_eq!(rec.stored.len(), 1, "one batch, recorded once");
        assert_eq!(rec.progress.last().unwrap().retry_in, None);
    }

    #[tokio::test]
    async fn gives_up_after_the_last_retry_and_fatal_errors_do_not_retry() {
        let (server, project, mut session) = cloud(&[]).await;
        insert(503).mount(&server).await;
        let mut rec = Rec::default();
        let err = go(
            &server,
            &project,
            &mut session,
            &meetings(1, &[]),
            &fast(10),
            &mut rec,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::Http { status: 503, .. }), "{err}");
        assert_eq!(
            hits(&server, INSERT).await,
            4,
            "the first try and three retries"
        );
        assert!(rec.stored.is_empty());

        let (server, project, mut session) = cloud(&[]).await;
        insert(403).mount(&server).await;
        let err = go(
            &server,
            &project,
            &mut session,
            &meetings(1, &[]),
            &fast(10),
            &mut Rec::default(),
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("batch of meetings m0 to m0"),
            "{err}"
        );
        assert_eq!(hits(&server, INSERT).await, 1);
    }

    #[tokio::test]
    async fn a_retry_that_finds_its_rows_already_committed_counts_as_sent() {
        let (server, project, mut session) = cloud(&[]).await;
        insert(504).up_to_n_times(1).mount(&server).await;
        Mock::given(method("POST"))
            .and(path(INSERT))
            .respond_with(
                ResponseTemplate::new(409)
                    .set_body_json(json!({ "code": "23505", "message": "duplicate key" })),
            )
            .mount(&server)
            .await;
        let mut rec = Rec::default();
        go(
            &server,
            &project,
            &mut session,
            &meetings(2, &[]),
            &fast(10),
            &mut rec,
        )
        .await
        .unwrap();
        assert_eq!(rec.stored.len(), 1);

        let (server, project, mut session) = cloud(&[]).await;
        Mock::given(method("POST"))
            .and(path(INSERT))
            .respond_with(ResponseTemplate::new(409).set_body_json(json!({ "code": "23505" })))
            .mount(&server)
            .await;
        let first = go(
            &server,
            &project,
            &mut session,
            &meetings(1, &[]),
            &fast(10),
            &mut Rec::default(),
        )
        .await;
        assert!(
            first.is_err(),
            "a duplicate on the first attempt is a real conflict"
        );
    }

    #[tokio::test]
    async fn a_failure_midway_keeps_earlier_batches_and_the_next_pass_resumes() {
        let (server, project, mut session) = cloud(&[]).await;
        insert(201).up_to_n_times(2).mount(&server).await;
        insert(400).mount(&server).await;
        let mut rec = Rec::default();
        let err = go(
            &server,
            &project,
            &mut session,
            &meetings(6, &[]),
            &fast(2),
            &mut rec,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, Error::Http { status: 400, .. }));
        let done: Vec<String> = rec
            .stored
            .iter()
            .flatten()
            .map(|(id, _)| id.clone())
            .collect();
        assert_eq!(done, ["m0", "m1", "m2", "m3"]);

        let (server, project, mut session) = cloud(&["m0", "m1", "m2", "m3"]).await;
        insert(201).mount(&server).await;
        let local = meetings(6, &["m0", "m1", "m2", "m3"]);
        let mut rec = Rec::default();
        let plan = go(&server, &project, &mut session, &local, &fast(2), &mut rec)
            .await
            .unwrap();
        assert_eq!(plan.changed, ["m4", "m5"]);
        assert_eq!(
            hits(&server, INSERT).await,
            1,
            "committed batches are not sent again"
        );
    }

    #[tokio::test]
    async fn a_stop_ends_the_pass_at_the_next_boundary_or_mid_wait() {
        let (server, project, mut session) = cloud(&[]).await;
        insert(201).mount(&server).await;
        let local = meetings(4, &[]);
        let pacing = Pacing {
            pause: Duration::from_secs(30),
            ..fast(2)
        };
        let (tx, mut rx) = watch::channel(false);
        let mut rec = Rec::default();
        let pass = Pass {
            project: &project,
            device_id: "d",
            local: &local,
            skip: None,
            pacing: &pacing,
        };
        let stop = async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            tx.send(true).unwrap();
        };
        let (res, ()) = tokio::join!(pass.run(&mut session, &mut rx, &mut rec), stop);
        assert!(matches!(res, Err(Error::Cancelled)));
        assert_eq!(
            rec.stored.len(),
            1,
            "stopped during the pause after the first batch"
        );
    }

    #[tokio::test]
    async fn a_meeting_deleted_mid_pass_is_skipped_and_deletions_go_as_null_bodies() {
        let (server, project, mut session) = cloud(&["old"]).await;
        insert(201).mount(&server).await;
        let mut rec = Rec {
            gone: vec!["m1".into()],
            ..Rec::default()
        };
        let plan = go(
            &server,
            &project,
            &mut session,
            &meetings(2, &[]),
            &fast(10),
            &mut rec,
        )
        .await
        .unwrap();
        assert_eq!(plan.deleted, ["old"]);
        let body: Value = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .find(|r| r.url.path() == INSERT)
            .unwrap()
            .body_json()
            .unwrap();
        let ids: Vec<_> = body
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["meeting_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["m0", "old"]);
        assert_eq!(body[1]["body"], Value::Null);
        assert_eq!(rec.stored[0].len(), 1);
    }
}
