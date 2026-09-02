//! `yogurt ctl meeting *` (CLI-4 / D1).
//!
//! Mutations (`new`, `start`, `stop`, `enhance`) always need a live server.
//! Reads (`list`, `show`, `summary`, `transcript`) fall back to the local
//! SQLite directory when discovery finds no server, printing `source: db`
//! (text: stderr; JSON: a field) so the reader knows the answer may be
//! stale relative to a server that's merely on a different port.

use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde::Deserialize;
use serde_json::json;

use super::client::{self, Client, CtlError};

#[derive(Subcommand, Debug)]
pub enum MeetingCmd {
    /// List meetings, newest first.
    List {
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Create a meeting (Library row); `--start` also begins recording.
    New {
        #[arg(long)]
        title: Option<String>,
        /// Also begin recording. Refused (exit 2) together with
        /// `--transcript-file`/`--from-script` -- a fixture meeting was
        /// never recorded, so there's nothing to record into.
        #[arg(long, conflicts_with_all = ["transcript_file", "from_script"])]
        start: bool,
        /// Seed a finished fixture meeting from a JSON file of transcript
        /// segments (`[{"ts_ms":0,"channel":"me","text":"..."}, ...]`,
        /// `channel` is `me`/`them` -- see docs/DEBUGGING-TRANSCRIPTS.md
        /// for the exact shape the server stores). Implies the meeting is
        /// created already ended.
        #[arg(long, conflicts_with = "from_script")]
        transcript_file: Option<PathBuf>,
        /// Seed a finished fixture meeting by converting a
        /// scripts/eval-style conversation (`A:`/`B:` lines, `PAUSE
        /// <seconds>`, `#` comments -- see scripts/eval/conversation.txt)
        /// into `me`/`them` segments with synthetic timestamps: 4 seconds
        /// per spoken line, plus any `PAUSE` seconds, so the eval ground
        /// truth doubles as a fixture without needing to actually speak
        /// and record it. Implies the meeting is created already ended.
        #[arg(long)]
        from_script: Option<PathBuf>,
    },
    /// Begin recording an existing meeting. No-op if it's already recording.
    Start { meeting: String },
    /// Stop the active meeting, or a specific one. No-op if nothing is recording.
    Stop { meeting: Option<String> },
    /// Print the full meeting row.
    Show { meeting: String },
    /// Print the enhanced (or raw) notes, tags stripped.
    Summary { meeting: String },
    /// Print the transcript, one line per segment.
    Transcript {
        #[arg(long)]
        follow: bool,
        meeting: String,
    },
    /// Run augmented-notes generation and forward progress to stderr.
    Enhance { meeting: String },
}

pub async fn run(cmd: MeetingCmd, port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    match cmd {
        MeetingCmd::List { limit } => list(port, json_out, limit).await,
        MeetingCmd::New {
            title,
            start,
            transcript_file,
            from_script,
        } => new(port, json_out, title, start, transcript_file, from_script).await,
        MeetingCmd::Start { meeting } => start(port, json_out, &meeting).await,
        MeetingCmd::Stop { meeting } => stop(port, json_out, meeting).await,
        MeetingCmd::Show { meeting } => show(port, json_out, &meeting).await,
        MeetingCmd::Summary { meeting } => summary(port, json_out, &meeting).await,
        MeetingCmd::Transcript { follow, meeting } => {
            transcript(port, json_out, &meeting, follow).await
        }
        MeetingCmd::Enhance { meeting } => enhance(port, json_out, &meeting).await,
    }
}

// ─── <id|url|last> ──────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
enum MeetingRef {
    Id(String),
    Url { port: u16, id: String },
    Last,
}

impl MeetingRef {
    fn parse(s: &str) -> Self {
        if s == "last" {
            return MeetingRef::Last;
        }
        if let Ok(u) = reqwest::Url::parse(s) {
            if let Some(port) = u.port() {
                if let Some(mut segs) = u.path_segments() {
                    if segs.next() == Some("meeting") {
                        if let Some(id) = segs.next().filter(|s| !s.is_empty()) {
                            return MeetingRef::Url {
                                port,
                                id: id.to_string(),
                            };
                        }
                    }
                }
            }
        }
        MeetingRef::Id(s.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct ActiveRef {
    id: String,
}

/// Resolve a ref to a live client + id. Mutation path -- never falls back
/// to the DB, since there's nothing to mutate there.
async fn client_for_ref(
    port_flag: Option<u16>,
    r: &MeetingRef,
) -> Result<(Client, String), CtlError> {
    match r {
        MeetingRef::Url { port, id } => Ok((Client::at_port(*port)?, id.clone())),
        MeetingRef::Id(id) => Ok((Client::discover(port_flag).await?, id.clone())),
        MeetingRef::Last => {
            let c = Client::discover(port_flag).await?;
            let list: Vec<yogurt_db::Meeting> = c.get("/api/meetings").await?;
            let id = list.into_iter().next().map(|m| m.id).ok_or_else(|| {
                CtlError::local("no meetings found", "run `yogurt ctl meeting new`")
            })?;
            Ok((c, id))
        }
    }
}

enum Resolved {
    Server(Client, String),
    Db(yogurt_db::Db, String),
}

/// Read path: same as [`client_for_ref`], but a [`CtlError::NoServer`]
/// falls back to the local database instead of propagating.
async fn resolve_read(port_flag: Option<u16>, r: &MeetingRef) -> Result<Resolved, CtlError> {
    match client_for_ref(port_flag, r).await {
        Ok((c, id)) => Ok(Resolved::Server(c, id)),
        Err(CtlError::NoServer(_)) => {
            let db = open_db_readonly()?;
            let id = match r {
                MeetingRef::Url { id, .. } | MeetingRef::Id(id) => id.clone(),
                MeetingRef::Last => {
                    let repo = yogurt_db::MeetingRepo::new(db.clone());
                    let mut xs = repo.list().map_err(db_err)?;
                    if xs.is_empty() {
                        return Err(CtlError::local(
                            "no meetings found",
                            "run `yogurt ctl meeting new` (needs a running server)",
                        ));
                    }
                    xs.remove(0).id
                }
            };
            Ok(Resolved::Db(db, id))
        }
        Err(e) => Err(e),
    }
}

fn open_db_readonly() -> Result<yogurt_db::Db, CtlError> {
    // CLI-7: honor $YOGURT_DATA_DIR (no --data-dir flag on ctl itself) so
    // this read-through-DB fallback agrees with wherever `yogurt start
    // --data-dir` / $YOGURT_DATA_DIR put the database -- otherwise `ctl
    // meeting list` with no server running would read the wrong one.
    let dir = match crate::data_dir::resolve(None) {
        Ok(Some(dir)) => dir,
        Ok(None) => client::yogurt_home()?,
        Err(e) => return Err(CtlError::local(format!("{e:#}"), "check $YOGURT_DATA_DIR")),
    };
    let path = dir.join("db.sqlite");
    if !path.exists() {
        return Err(CtlError::local(
            "no local database found",
            "run `yogurt start` at least once",
        ));
    }
    yogurt_db::Db::open(&path).map_err(|e| {
        CtlError::local(
            format!("could not open local database: {e:#}"),
            "run `yogurt doctor`",
        )
    })
}

fn db_err(e: anyhow::Error) -> CtlError {
    CtlError::local(format!("database error: {e:#}"), "run `yogurt doctor`")
}

fn get_or_not_found(
    repo: &yogurt_db::MeetingRepo,
    id: &str,
) -> Result<yogurt_db::Meeting, CtlError> {
    repo.get(id).map_err(db_err)?.ok_or_else(|| {
        CtlError::local(
            format!("no such meeting: {id}"),
            "run `yogurt ctl meeting list`",
        )
    })
}

// ─── list ───────────────────────────────────────────────────────────────

async fn list(
    port_flag: Option<u16>,
    json_out: bool,
    limit: Option<usize>,
) -> Result<(), CtlError> {
    let (mut meetings, source) = match Client::discover(port_flag).await {
        Ok(c) => (
            c.get::<Vec<yogurt_db::Meeting>>("/api/meetings").await?,
            "server",
        ),
        Err(CtlError::NoServer(_)) => {
            let db = open_db_readonly()?;
            let repo = yogurt_db::MeetingRepo::new(db);
            (repo.list().map_err(db_err)?, "db")
        }
        Err(e) => return Err(e),
    };
    let total = meetings.len();
    if let Some(n) = limit {
        meetings.truncate(n);
    }
    if json_out {
        println!(
            "{}",
            json!({ "meetings": meetings, "total": total, "source": source })
        );
    } else {
        if source == "db" {
            eprintln!("source: db");
        }
        println!("{total} meeting(s)");
        for m in &meetings {
            let state = if m.ended_at.is_some() {
                "ended"
            } else {
                "live"
            };
            println!("{}  {state:<5}  {}", m.id, m.title);
        }
    }
    Ok(())
}

// ─── new ────────────────────────────────────────────────────────────────

async fn new(
    port_flag: Option<u16>,
    json_out: bool,
    title: Option<String>,
    start: bool,
    transcript_file: Option<PathBuf>,
    from_script: Option<PathBuf>,
) -> Result<(), CtlError> {
    // CLI-5: `--transcript-file` is forwarded to the server byte-for-shape
    // untouched (read as a bare `serde_json::Value`, not parsed into our
    // own segment type) so a malformed file fails on the server's own
    // validation and the caller sees the server's message, and so a
    // well-formed file round-trips exactly. `--from-script` has no
    // existing JSON to forward, so it's built into segments here.
    let transcript_json = match (&transcript_file, &from_script) {
        (Some(p), _) => Some(read_json_file(p)?),
        (None, Some(p)) => Some(
            serde_json::to_value(segments_from_script(p)?)
                .expect("Vec<FixtureSegment> always serializes"),
        ),
        (None, None) => None,
    };
    let ended = transcript_json.is_some();

    let c = Client::discover(port_flag).await?;
    let mut body = json!({ "title": title });
    if let Some(tj) = transcript_json {
        body["transcript_json"] = tj;
    }
    if ended {
        body["ended"] = json!(true);
    }
    let m: yogurt_db::Meeting = c.post("/api/meetings", &body).await?;
    let started = if start {
        Some(start_meeting(&c, &m.id).await?)
    } else {
        None
    };
    if json_out {
        println!(
            "{}",
            json!({ "id": m.id, "title": m.title, "started": started })
        );
    } else {
        println!("created {} ({})", m.id, m.title);
        match started {
            Some(true) => println!("started {}", m.id),
            Some(false) => println!("already started {}", m.id),
            None => {}
        }
    }
    Ok(())
}

/// One `--transcript-file` / `--from-script` segment. Mirrors the
/// `{ts_ms, channel, text}` shape `docs/DEBUGGING-TRANSCRIPTS.md`
/// documents and the server validates on `POST /api/meetings`.
#[derive(Debug, serde::Serialize)]
struct FixtureSegment {
    ts_ms: i64,
    channel: &'static str,
    text: String,
}

/// Read `--transcript-file` as raw JSON, no shape validation -- the
/// server is the one place that owns `transcript_json`'s shape (see
/// AGENTS.md), so a malformed file is rejected there and this command
/// surfaces the server's own message rather than a second, possibly
/// divergent, local check.
fn read_json_file(path: &Path) -> Result<serde_json::Value, CtlError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CtlError::local(
            format!("could not read --transcript-file {}: {e}", path.display()),
            "check the path exists",
        )
    })?;
    serde_json::from_str(&content).map_err(|e| {
        CtlError::local(
            format!(
                "--transcript-file {} is not valid JSON: {e}",
                path.display()
            ),
            "check the file contains a JSON array of segments (see docs/DEBUGGING-TRANSCRIPTS.md)",
        )
    })
}

/// Convert an eval-style script (`A:`/`B:` lines, `PAUSE <seconds>`, `#`
/// comments -- see `scripts/eval/conversation.txt`) into `me`/`them`
/// segments. Timestamps are synthetic: 4 seconds per spoken line (long
/// enough to separate turns without parsing word counts to estimate a
/// speaking rate) plus any `PAUSE` seconds, so a longer silence in the
/// script still shows up as a gap between segments.
fn segments_from_script(path: &Path) -> Result<Vec<FixtureSegment>, CtlError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CtlError::local(
            format!("could not read --from-script {}: {e}", path.display()),
            "check the path exists",
        )
    })?;
    let bad_line = |line: &str| {
        CtlError::local(
            format!("--from-script {}: unrecognized line: {line}", path.display()),
            "expected `A: <line>`, `B: <line>`, `PAUSE <seconds>`, or a `#` comment (see scripts/eval/conversation.txt)",
        )
    };

    let mut ts_ms: i64 = 0;
    let mut segments = Vec::new();
    for line in content.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(secs) = line.strip_prefix("PAUSE ") {
            let secs: f64 = secs.trim().parse().map_err(|_| bad_line(line))?;
            ts_ms += (secs * 1000.0).round() as i64;
            continue;
        }
        let (channel, text) = if let Some(t) = line.strip_prefix("A: ") {
            ("me", t)
        } else if let Some(t) = line.strip_prefix("B: ") {
            ("them", t)
        } else {
            return Err(bad_line(line));
        };
        segments.push(FixtureSegment {
            ts_ms,
            channel,
            text: text.to_string(),
        });
        ts_ms += 4000;
    }
    Ok(segments)
}

/// Returns `true` if this call actually started recording, `false` if it
/// was already active (idempotent no-op per CLI-4's spec).
async fn start_meeting(c: &Client, id: &str) -> Result<bool, CtlError> {
    match c
        .post_empty::<serde_json::Value>(&format!("/api/meetings/{id}/start"))
        .await
    {
        Ok(_) => Ok(true),
        Err(CtlError::Server(msg)) if msg.contains("already started") => Ok(false),
        Err(e) => Err(e),
    }
}

// ─── start ──────────────────────────────────────────────────────────────

async fn start(port_flag: Option<u16>, json_out: bool, meeting: &str) -> Result<(), CtlError> {
    let r = MeetingRef::parse(meeting);
    let (c, id) = client_for_ref(port_flag, &r).await?;
    let started_now = start_meeting(&c, &id).await?;
    if json_out {
        println!(
            "{}",
            json!({ "id": id, "status": if started_now { "started" } else { "already started" } })
        );
    } else if started_now {
        println!("started {id}");
    } else {
        println!("already started {id}");
    }
    Ok(())
}

// ─── stop ───────────────────────────────────────────────────────────────

async fn stop(
    port_flag: Option<u16>,
    json_out: bool,
    meeting: Option<String>,
) -> Result<(), CtlError> {
    let (c, id) = match meeting {
        Some(m) => client_for_ref(port_flag, &MeetingRef::parse(&m)).await?,
        None => {
            let c = Client::discover(port_flag).await?;
            let active: Option<ActiveRef> = c.get("/api/meetings/active").await?;
            match active {
                Some(a) => (c, a.id),
                None => {
                    if json_out {
                        println!("{}", json!({ "status": "no active meeting" }));
                    } else {
                        println!("no active meeting");
                    }
                    return Ok(());
                }
            }
        }
    };
    let _: serde_json::Value = c.post_empty(&format!("/api/meetings/{id}/stop")).await?;
    if json_out {
        println!("{}", json!({ "id": id, "status": "stopped" }));
    } else {
        println!("stopped {id}");
    }
    Ok(())
}

// ─── show ───────────────────────────────────────────────────────────────

async fn show(port_flag: Option<u16>, json_out: bool, meeting: &str) -> Result<(), CtlError> {
    let r = MeetingRef::parse(meeting);
    let (m, source) = match resolve_read(port_flag, &r).await? {
        Resolved::Server(c, id) => (
            c.get::<yogurt_db::Meeting>(&format!("/api/meetings/{id}"))
                .await?,
            "server",
        ),
        Resolved::Db(db, id) => {
            let repo = yogurt_db::MeetingRepo::new(db);
            (get_or_not_found(&repo, &id)?, "db")
        }
    };
    let segment_count = parse_segments(&m.transcript_json).len();
    if json_out {
        println!(
            "{}",
            json!({ "meeting": m, "source": source, "segments": segment_count })
        );
    } else {
        if source == "db" {
            eprintln!("source: db");
        }
        println!("id: {}", m.id);
        println!("title: {}", m.title);
        println!("started_at: {}", m.started_at);
        println!("ended: {}", m.ended_at.is_some());
        println!(
            "stt_engine: {}",
            m.stt_engine.as_deref().unwrap_or("unknown")
        );
        println!("enhanced: {}", m.enriched_md.is_some());
        println!("segments: {segment_count}");
    }
    Ok(())
}

// ─── summary ────────────────────────────────────────────────────────────

async fn summary(port_flag: Option<u16>, json_out: bool, meeting: &str) -> Result<(), CtlError> {
    let r = MeetingRef::parse(meeting);
    let (body, source) = match resolve_read(port_flag, &r).await? {
        Resolved::Server(c, id) => (
            c.get_text(&format!("/api/meetings/{id}/markdown")).await?,
            "server",
        ),
        Resolved::Db(db, id) => {
            let repo = yogurt_db::MeetingRepo::new(db);
            let m = get_or_not_found(&repo, &id)?;
            (m.enriched_md.unwrap_or(m.notes_md), "db")
        }
    };
    let text = strip_tags(&body);
    if json_out {
        println!("{}", json!({ "summary": text, "source": source }));
    } else {
        if source == "db" {
            eprintln!("source: db");
        }
        println!("{text}");
    }
    Ok(())
}

/// Strip `<span data-ai-grey ...>` / `<span data-transcript-link ...>`
/// wrappers the enhance renderer embeds (see `docs/AI-INTEGRATION.md`'s
/// `sed -E 's/<[^>]+>//g'` recipe, which this replaces).
fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

// ─── transcript ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Segment {
    ts_ms: i64,
    channel: String,
    text: String,
}

fn parse_segments(transcript_json: &str) -> Vec<Segment> {
    serde_json::from_str(transcript_json).unwrap_or_default()
}

fn print_segment(json_out: bool, s: &Segment) {
    if json_out {
        println!(
            "{}",
            json!({ "ts_ms": s.ts_ms, "channel": s.channel, "text": s.text })
        );
    } else {
        println!("{} {}: {}", s.ts_ms / 1000, s.channel, s.text);
    }
}

async fn transcript(
    port_flag: Option<u16>,
    json_out: bool,
    meeting: &str,
    follow: bool,
) -> Result<(), CtlError> {
    let r = MeetingRef::parse(meeting);
    match resolve_read(port_flag, &r).await? {
        Resolved::Db(db, id) => {
            if follow {
                return Err(CtlError::local(
                    "--follow needs a running server",
                    "run `yogurt start`",
                ));
            }
            let repo = yogurt_db::MeetingRepo::new(db);
            let m = get_or_not_found(&repo, &id)?;
            eprintln!("source: db");
            for s in parse_segments(&m.transcript_json) {
                print_segment(json_out, &s);
            }
        }
        Resolved::Server(c, id) if follow => follow_transcript(&c, &id, json_out).await?,
        Resolved::Server(c, id) => {
            let m: yogurt_db::Meeting = c.get(&format!("/api/meetings/{id}")).await?;
            for s in parse_segments(&m.transcript_json) {
                print_segment(json_out, &s);
            }
        }
    }
    Ok(())
}

/// Polls rather than opening a WebSocket -- `tokio-tungstenite` is not a
/// `yogurt-cli` dependency (CLI-4's spec caps new dependencies at
/// `reqwest`), so `--follow` re-fetches the meeting row on an interval and
/// prints whatever segments are new. Stops once the meeting has an
/// `ended_at` and a poll produced nothing new, or on Ctrl-C.
async fn follow_transcript(c: &Client, id: &str, json_out: bool) -> Result<(), CtlError> {
    let mut printed = 0usize;
    loop {
        let m: yogurt_db::Meeting = c.get(&format!("/api/meetings/{id}")).await?;
        let segs = parse_segments(&m.transcript_json);
        let is_new = segs.len() > printed;
        for s in &segs[printed.min(segs.len())..] {
            print_segment(json_out, s);
        }
        printed = segs.len();
        if m.ended_at.is_some() && !is_new {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    }
    Ok(())
}

// ─── enhance ────────────────────────────────────────────────────────────

/// Blocks for the whole LLM generation. CLI-4's spec asks for the
/// server's `enhance_progress` WS frames forwarded to stderr as
/// `phase: ...` lines; that needs a WS client this binary doesn't carry
/// (see `follow_transcript` above for the same constraint), so this
/// prints `phase: sending` up front and a `phase: waiting` heartbeat
/// every 5s instead of going silent -- the documented deviation, so an
/// agent driving this doesn't read a long pause as hung and retry.
async fn enhance(port_flag: Option<u16>, json_out: bool, meeting: &str) -> Result<(), CtlError> {
    let r = MeetingRef::parse(meeting);
    let (c, id) = client_for_ref(port_flag, &r).await?;
    let m: yogurt_db::Meeting = c.get(&format!("/api/meetings/{id}")).await?;
    let body = json!({
        "notes_md": m.notes_md,
        "transcript_json": m.transcript_json,
        "title": m.title,
        "started_at_unix_ms": m.started_at,
        "ended_at_unix_ms": m.ended_at,
    });

    eprintln!("phase: sending");
    let ticker = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            eprintln!("phase: waiting");
        }
    });
    let result: Result<serde_json::Value, CtlError> =
        c.post(&format!("/api/meetings/{id}/enhance"), &body).await;
    ticker.abort();
    let resp = result?;
    eprintln!("phase: done");

    let too_short = resp
        .get("too_short")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if json_out {
        println!("{resp}");
    } else if too_short {
        println!("too_short: meeting had no notes and a trivial transcript, nothing to enhance");
    } else {
        println!("enhanced {id}");
        if let Some(f) = resp.get("notes_file").and_then(|v| v.as_str()) {
            println!("notes file: {f}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::MeetingRef;

    #[test]
    fn plain_id_is_id() {
        assert_eq!(
            MeetingRef::parse("01a060ee-1fa5-7fc0-952a-1f67a5f38172"),
            MeetingRef::Id("01a060ee-1fa5-7fc0-952a-1f67a5f38172".to_string())
        );
    }

    #[test]
    fn last_is_last() {
        assert_eq!(MeetingRef::parse("last"), MeetingRef::Last);
    }

    #[test]
    fn meeting_url_carries_port_and_id() {
        assert_eq!(
            MeetingRef::parse("http://127.0.0.1:7879/meeting/abc123"),
            MeetingRef::Url {
                port: 7879,
                id: "abc123".to_string(),
            }
        );
    }

    #[test]
    fn post_suffixed_meeting_url_carries_port_and_id() {
        assert_eq!(
            MeetingRef::parse("http://127.0.0.1:7879/meeting/abc123/post"),
            MeetingRef::Url {
                port: 7879,
                id: "abc123".to_string(),
            }
        );
    }

    #[test]
    fn a_non_meeting_url_falls_back_to_a_bare_id() {
        // Not a parseable URL, not "last" -- treated as a literal id, same
        // as any other free-text string a caller might pass.
        assert_eq!(
            MeetingRef::parse("not a url or id"),
            MeetingRef::Id("not a url or id".to_string())
        );
    }
}
