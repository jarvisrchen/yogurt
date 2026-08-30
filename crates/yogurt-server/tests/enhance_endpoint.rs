//! End-to-end integration test for `POST /api/meetings/{id}/enhance`.
//!
//! Spins up a real `yogurt-server` bound to 127.0.0.1:0 with a tempdir for
//! both the SQLite database and the per-meeting markdown notes directory,
//! creates a meeting, hits the enhance endpoint with hand-crafted notes +
//! transcript, and asserts:
//!
//! 1. Response `enriched_md` contains the user's raw bullet preserved
//!    verbatim (`- pricing`).
//! 2. Response `enriched_md` contains an `aiGrey`-tagged AI bullet with
//!    the transcript timestamp baked in (`data-ai-grey data-ts="120"`)
//!    and the `↳ 02:00` deep-link suffix formatted MM:SS.
//! 3. The per-meeting markdown file exists at the configured notes dir
//!    and its body matches the response.
//! 4. The SQLite `meetings.enriched_doc_json` column is non-null and
//!    parses as the serialized `MergedDoc` JSON.
//!
//! `MockLlm` is used by virtue of the test environment not having
//! `YOGURT_LLM_BASE_URL` / `YOGURT_LLM_API_KEY` / `YOGURT_LLM_MODEL` set —
//! the mock deterministically emits one AI bullet per transcript segment
//! using the first 8 words, so the assertions above are stable.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use yogurt_db::keys::ApiKeyStore;
use yogurt_llm::{ChatMessage, ChatRequest, LlmClient};
use yogurt_prompts::EnhanceCtx;
use yogurt_server::{run_with_config, AppState, Mode, RunConfig};

async fn spawn_server() -> (
    std::net::SocketAddr,
    String,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
    std::path::PathBuf,
) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let notes_dir = tmp.path().join("notes");

    // Pre-create the session token so the test client can authenticate.
    let token = yogurt_server::session::load_or_create(&token_path)
        .expect("seed session token")
        .as_str()
        .to_string();

    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let addr = probe.local_addr().unwrap();
    drop(probe);

    std::env::set_var("YOGURT_MEMORY_KEYSTORE", "1");
    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path.clone()),
        session_token_path: Some(token_path),
        notes_dir: Some(notes_dir.clone()),
        // Phase 5 (SET-12): tempdir-isolate the new yogurt-db.
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
    };
    let handle = tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });
    // Poll until server is reachable.
    for _ in 0..100 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return (addr, token, handle, tmp, db_path);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable within 2 seconds");
}

#[tokio::test(flavor = "multi_thread")]
async fn it_enhances_a_meeting_end_to_end() {
    // Belt-and-suspenders: MockLlm must be the LLM the handler picks. If
    // any of these vars leak in from the test runner's environment the
    // assertions would race against a real provider.
    //
    // SAFETY: this test owns the env, and `from_env()` only checks for
    // *presence* — remove_var is the precise primitive.
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let (addr, token, _handle, tmp, db_path) = spawn_server().await;
    let client = reqwest::Client::new();

    // 1) Create a fresh meeting via POST /api/meetings.
    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let meeting_id = created
        .get("id")
        .and_then(|v| v.as_str())
        .expect("meeting id")
        .to_string();

    // 2) Drive the enhance handler. MockLlm will preserve `- pricing` and
    // append one AI bullet (first 8 words of the transcript text) tagged
    // with `data-ai-grey data-ts="120"` + `↳ 02:00`.
    let body = serde_json::json!({
        "notes_md": "- pricing\n",
        "transcript_json": "[{\"ts_ms\":120000,\"channel\":\"mic\",\"text\":\"We debated the pricing model in detail today\"}]",
        "title": "Sales sync",
        "started_at_unix_ms": 1_700_000_000_000_i64,
    });
    let resp = client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "enhance must return 200");
    let response: serde_json::Value = resp.json().await.unwrap();
    let enriched_md = response
        .get("enriched_md")
        .and_then(|v| v.as_str())
        .expect("enriched_md in response")
        .to_string();
    let notes_file = response
        .get("notes_file")
        .and_then(|v| v.as_str())
        .expect("notes_file in response")
        .to_string();
    assert!(
        response
            .get("llm_model")
            .and_then(|v| v.as_str())
            .is_some_and(|m| m.starts_with("cloud \u{b7} ")),
        "response names the model that produced enriched_md, got {:?}",
        response.get("llm_model"),
    );

    // 3) Wire-format assertions on the response body. The AI bullet is
    // wrapped in a `data-ai-grey` span; ammonia normalizes the boolean
    // attribute to `data-ai-grey=""` on the way out.
    assert!(
        enriched_md.contains("- pricing"),
        "user notes preserved verbatim. got: {enriched_md}",
    );
    assert!(
        enriched_md.contains(r#"data-ai-grey="" data-ts="120""#),
        "AI bullet tagged with transcript timestamp. got: {enriched_md}",
    );
    // Regression (2026-08-13): the mock (like a real model) emits its own
    // `<span data-ai-grey>` scaffolding; render must STRIP it, not escape +
    // double-wrap it. If the corruption returns, the escaped literal markup
    // (`&lt;span data-ai-grey`) reappears in the body.
    assert!(
        !enriched_md.contains("&lt;span"),
        "model-emitted span markup must be stripped, not escaped. got: {enriched_md}",
    );
    assert!(
        enriched_md.contains("↳ 02:00"),
        "deep-link suffix formatted MM:SS. got: {enriched_md}",
    );

    // 4) Per-meeting markdown file exists and contains the enriched body.
    let file_body = std::fs::read_to_string(&notes_file)
        .unwrap_or_else(|e| panic!("read notes file {notes_file}: {e}"));
    assert!(
        file_body.contains("---\n"),
        "markdown file starts with YAML front-matter. got: {file_body}",
    );
    assert!(
        file_body.contains(r#"data-ai-grey="" data-ts="120""#),
        "markdown file contains wire-format spans. got: {file_body}",
    );
    assert!(
        file_body.contains("Sales sync"),
        "markdown file front-matter carries the title. got: {file_body}",
    );

    // 5) SQLite assertion — enriched_doc_json was persisted and parses as
    // a JSON object (the serialized MergedDoc has a top-level `blocks` key).
    let conn = rusqlite::Connection::open(&db_path).expect("reopen sqlite");
    let (enriched_md_db, enriched_doc_json_db, title_db): (String, String, String) = conn
        .query_row(
            "SELECT enriched_md, enriched_doc_json, title FROM meetings WHERE id = ?",
            [&meeting_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("meeting row should exist after enhance");
    assert_eq!(
        enriched_md_db, enriched_md,
        "SQLite enriched_md matches response",
    );
    // LLM provenance lives on the Library row (yogurt-db), not Phase 0 storage.
    let app_conn = rusqlite::Connection::open(tmp.path().join("yogurt-app.sqlite"))
        .expect("reopen app sqlite");
    let llm_model_db: Option<String> = app_conn
        .query_row(
            "SELECT llm_model FROM meetings WHERE id = ?",
            [&meeting_id],
            |row| row.get(0),
        )
        .expect("library row should exist after enhance");
    assert!(
        llm_model_db
            .as_deref()
            .is_some_and(|m| m.starts_with("cloud \u{b7} ")),
        "enhance stamps the model that produced the summary, got {llm_model_db:?}",
    );
    assert_eq!(title_db, "Sales sync", "SQLite title persisted");
    let doc: serde_json::Value =
        serde_json::from_str(&enriched_doc_json_db).expect("enriched_doc_json parses");
    assert!(
        doc.get("blocks").and_then(|b| b.as_array()).is_some(),
        "enriched_doc_json shape: {{ blocks: [...] }}. got: {enriched_doc_json_db}",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reenhance_keeps_raw_notes_separate_from_ai_output() {
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let (addr, token, _handle, _tmp, db_path) = spawn_server().await;
    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let meeting_id = created["id"].as_str().unwrap();
    let transcript = r#"[{"ts_ms":1000,"channel":"mic","text":"We agreed to ship the feature on Monday morning"}]"#;

    client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "- My original note",
            "transcript_json": transcript,
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "- My original note",
            "transcript_json": transcript,
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let conn = rusqlite::Connection::open(db_path).unwrap();
    let notes_md: String = conn
        .query_row(
            "SELECT notes_md FROM meetings WHERE id = ?1",
            [meeting_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(notes_md, "- My original note");
}

/// Regression: the server-side transcript-persistence task (meetings.rs) is
/// the source of truth for what was said. When the request body's
/// `transcript_json` is empty (`""`/`"[]"`) — as it always is once the
/// browser stops sending the transcript itself — `enhance` must fall back
/// to the stored row's `transcript_json` rather than silently producing a
/// notes-only doc.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_falls_back_to_stored_transcript_when_request_body_is_empty() {
    // SAFETY: mirrors the other enhance tests — clear LLM env so MockLlm
    // (deterministic) is the resolved path.
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let (addr, token, _handle, _tmp, _db_path) = spawn_server().await;
    let client = reqwest::Client::new();

    // 1) Create a meeting.
    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let meeting_id = created["id"].as_str().unwrap().to_string();

    // 2) Seed a REAL transcript directly on the row — simulating the
    // server-side persistence task in meetings.rs having already
    // accumulated finals while the meeting was recording.
    let stored_transcript = serde_json::json!([
        {
            "ts_ms": 90000,
            "channel": "mic",
            "text": "We should ship the stored transcript path first"
        }
    ])
    .to_string();
    client
        .patch(format!("http://{addr}/api/meetings/{meeting_id}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "transcript_json": stored_transcript }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // 3) POST enhance with an EMPTY transcript_json in the request body.
    let resp = client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "- kickoff\n",
            "transcript_json": "[]",
            "title": "Stored transcript fallback",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "enhance must succeed");
    let body: serde_json::Value = resp.json().await.unwrap();
    let enriched_md = body["enriched_md"].as_str().expect("enriched_md");

    // MockLlm emits one `data-ai-grey` bullet per transcript segment,
    // tagged with `ts_ms / 1000` and summarized from the segment text —
    // this can only appear if enhance actually read the STORED transcript
    // (the request body's transcript was empty).
    assert!(
        enriched_md.contains(r#"data-ai-grey="" data-ts="90""#),
        "enhance must fall back to the stored transcript row. got: {enriched_md}"
    );
    assert!(
        enriched_md.contains("We should ship the stored"),
        "AI bullet must summarize the stored transcript text. got: {enriched_md}"
    );
}

/// A meeting with no typed notes and a trivial transcript (well under
/// `TOO_SHORT_TRANSCRIPT_WORDS`) must skip the LLM/merge/persist pipeline
/// entirely and report `too_short: true` instead of enhancing a near-empty
/// document.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_skips_llm_for_a_too_short_meeting() {
    // Belt-and-suspenders like the other tests: the too-short path returns
    // before the LLM is ever resolved, but clear the env anyway so a stray
    // leaked var can't mask a regression that removes the early return.
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let (addr, token, _handle, tmp, _db_path) = spawn_server().await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let meeting_id = created["id"].as_str().unwrap().to_string();

    // No notes, and a transcript well under the 20-word threshold - an
    // accidental tap that caught a stray "hello?" before the user hung up.
    let resp = client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "",
            "transcript_json": "[{\"ts_ms\":500,\"channel\":\"mic\",\"text\":\"hello is anyone there\"}]",
            "title": "Accidental tap",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "too-short meetings still return 200");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["too_short"].as_bool(), Some(true), "got: {body}");
    assert_eq!(body["enriched_md"].as_str(), Some(""), "got: {body}");

    // The LLM/merge/persist pipeline never ran - the Library row
    // POST /api/meetings created (in yogurt-db, not the Phase-0 storage
    // enhance itself writes to) stays untouched, so enriched_md is still
    // NULL.
    let app_db_path = tmp.path().join("yogurt-app.sqlite");
    let conn = rusqlite::Connection::open(&app_db_path).expect("reopen yogurt-db");
    let enriched_md_db: Option<String> = conn
        .query_row(
            "SELECT enriched_md FROM meetings WHERE id = ?",
            [&meeting_id],
            |row| row.get(0),
        )
        .expect("meeting row should exist from POST /api/meetings");
    assert!(
        enriched_md_db.is_none(),
        "too-short meeting must not persist an enriched_md: {enriched_md_db:?}"
    );
}

/// Regression guard for the threshold's notes-gate: real user notes must
/// still enhance normally even when the transcript itself is trivial - the
/// too-short skip is about having NOTHING to work with, not a short
/// transcript alone.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_still_runs_when_notes_are_present_even_with_a_trivial_transcript() {
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let (addr, token, _handle, _tmp, _db_path) = spawn_server().await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let meeting_id = created["id"].as_str().unwrap().to_string();

    let resp = client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "- pricing\n",
            "transcript_json": "[]",
            "title": "Real notes, silent transcript",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["too_short"].as_bool(),
        Some(false),
        "notes alone are enough to enhance. got: {body}"
    );
    assert!(
        body["enriched_md"].as_str().unwrap().contains("- pricing"),
        "got: {body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_rejects_enhance_without_session_token() {
    let (addr, _token, _handle, _tmp, _db_path) = spawn_server().await;
    let client = reqwest::Client::new();

    // POST /api/meetings/<any-uuid>/enhance with NO bearer token → 403
    // (matches the auth contract of the other meeting routes — WR-06).
    // We don't need to create a meeting first — the auth middleware
    // rejects before the handler runs.
    let placeholder_id = "00000000-0000-0000-0000-000000000000";
    let resp = client
        .post(format!(
            "http://{addr}/api/meetings/{placeholder_id}/enhance"
        ))
        .json(&serde_json::json!({
            "notes_md": "x",
            "transcript_json": "[]",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "missing token must be 403");
}

/// Plan 04-04: persistence acceptance gate (NOTES-13 step 10).
///
/// After enhance writes the row, `GET /api/meetings/{id}` must return the
/// persisted columns so the post-meeting route can hydrate on direct-link
/// or refresh. 404 on unknown ids; 403 without token.
#[tokio::test(flavor = "multi_thread")]
async fn it_gets_a_meeting_after_enhance() {
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let (addr, token, _handle, _tmp, _db_path) = spawn_server().await;
    let client = reqwest::Client::new();

    // 1) Create a meeting + run enhance so the row exists.
    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let meeting_id = created["id"].as_str().expect("id").to_string();

    let _enhance_resp = client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "- launch checklist\n",
            "transcript_json": "[{\"ts_ms\":60000,\"channel\":\"mic\",\"text\":\"We need to finalize the launch checklist before Friday\"}]",
            "title": "Launch sync",
            "started_at_unix_ms": 1_700_000_000_000_i64,
            "ended_at_unix_ms": 1_700_000_300_000_i64,
        }))
        .send()
        .await
        .unwrap();

    // 2) GET the meeting — must return all persisted columns.
    let get_resp = client
        .get(format!("http://{addr}/api/meetings/{meeting_id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 200, "GET must return 200 after enhance");
    let body: serde_json::Value = get_resp.json().await.unwrap();
    assert_eq!(body["id"].as_str().unwrap(), meeting_id);
    assert_eq!(body["title"].as_str().unwrap(), "Launch sync");
    assert_eq!(
        body["notes_md"].as_str().unwrap(),
        "- launch checklist\n",
        "GET returns notes_md verbatim",
    );
    let enriched = body["enriched_md"].as_str().expect("enriched_md");
    assert!(
        enriched.contains("- launch checklist"),
        "GET enriched_md preserves user notes. got: {enriched}",
    );
    assert!(
        enriched.contains("data-ai-grey"),
        "GET enriched_md contains AI spans. got: {enriched}",
    );
    // Phase 7 (Plan 07-01): the new Library GET returns the
    // `yogurt_db::Meeting` wire shape. Field names changed from
    // `started_at_unix_ms` / `ended_at_unix_ms` to `started_at` / `ended_at`
    // (still i64 unix millis on the wire).
    assert_eq!(
        body["started_at"].as_i64().unwrap(),
        1_700_000_000_000,
        "GET preserves started_at",
    );
    assert_eq!(
        body["ended_at"].as_i64().unwrap(),
        1_700_000_300_000,
        "GET preserves ended_at",
    );

    // 3) Unknown meeting id → 404.
    let unknown_id = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    let resp_404 = client
        .get(format!("http://{addr}/api/meetings/{unknown_id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp_404.status(), 404, "unknown meeting must be 404");

    // 4) No token → 403 (WR-06).
    let resp_403 = client
        .get(format!("http://{addr}/api/meetings/{meeting_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_403.status(), 403, "missing token must be 403");
}

/// BL-2 regression: malicious notes + transcript MUST be neutralized before
/// the enriched_md leaves the server. Any of `<script>`, `<img onerror=...>`,
/// `<iframe>`, or `javascript:` URLs in notes/transcript would otherwise
/// reach the browser DOM via markdown-it's `html: true` parser.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_strips_xss_payloads_from_notes_and_transcript() {
    // SAFETY: mirrors `it_enhances_a_meeting_end_to_end` — clear LLM env so
    // MockLlm is the deterministic path.
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let (addr, token, _handle, _tmp, _db_path) = spawn_server().await;
    let client = reqwest::Client::new();

    let create: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let meeting_id = create["id"].as_str().unwrap().to_string();

    // Adversarial notes: each line carries a different XSS vector.
    let notes_md = "- pricing\n- <script>alert('xss-notes')</script>\n- <img src=x onerror=\"alert('xss-img')\">\n- [click](javascript:alert('xss-link'))\n";
    let transcript_json = serde_json::json!([
        {
            "ts_ms": 60000,
            "channel": "mic",
            "text": "<script>alert('xss-transcript')</script> debate"
        },
        {
            "ts_ms": 120000,
            "channel": "system",
            "text": "<iframe srcdoc=\"<script>alert(1)</script>\"></iframe> pricing"
        }
    ])
    .to_string();

    let body = serde_json::json!({
        "notes_md": notes_md,
        "transcript_json": transcript_json,
        "title": "xss-regression",
        "started_at_unix_ms": 1_700_000_000_000_i64,
    });

    let resp = client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "enhance should succeed");
    let json: serde_json::Value = resp.json().await.unwrap();
    let enriched = json["enriched_md"].as_str().unwrap();

    // The sanitizer + html-escape layer must neutralize every dangerous
    // construct. Escaped forms like `&lt;script&gt;` are SAFE — they render
    // as text, not as a tag. We therefore assert that NO raw `<script>`,
    // `<img`, `<iframe` survives — only escaped (`&lt;script&gt;`) or
    // stripped variants. Live attributes (` onerror=`, ` srcdoc=`) only
    // matter if attached to a live tag; since the tags are all stripped or
    // escaped, attribute substrings appearing inside escaped tag bodies
    // (like `&lt;iframe srcdoc="..."&gt;`) are inert text.
    assert!(
        !enriched.contains("<script"),
        "<script tags must be stripped from enriched_md. got: {enriched}"
    );
    assert!(
        !enriched.contains("<img"),
        "<img tags must be stripped from enriched_md. got: {enriched}"
    );
    assert!(
        !enriched.contains("<iframe"),
        "<iframe tags must be stripped from enriched_md. got: {enriched}"
    );
    // Allowlisted live tags are <span> with data-* attrs only. Confirm no
    // live <a>, <link>, <object>, <embed>, <form>, <input>, <style> made it
    // through.
    for forbidden in [
        "<a ", "<a>", "<link", "<object", "<embed", "<form", "<input", "<style",
    ] {
        assert!(
            !enriched.contains(forbidden),
            "{forbidden} must not survive sanitization. got: {enriched}"
        );
    }

    // The wire-format spans MUST survive (otherwise the editor can't render
    // the augmented notes).
    assert!(
        enriched.contains("data-ai-grey"),
        "data-ai-grey spans must survive sanitization. got: {enriched}"
    );
}

/// Build an `AppState` rooted in a tempdir, wired with `MockLlm` via
/// `llm_override` (mirrors `meeting_ws.rs::build_test_state`). Gives the
/// test a handle on the meeting registry (to poll `events_tx.receiver_count`
/// before POSTing) and on `prompts` (to independently reconstruct the exact
/// user prompt the handler renders, so the mock's raw output can be
/// predicted without duplicating the mock's own bullet-formatting logic).
fn build_mock_test_state(bind_port: u16) -> (AppState, String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let storage = Arc::new(yogurt_server::storage::Storage::init_at(&db_path).unwrap());
    let session_token = yogurt_server::session::load_or_create(&token_path).unwrap();
    let token_str = session_token.as_str().to_string();
    let session: Arc<yogurt_server::session::SessionToken> = Arc::new(session_token);
    let (markdown_exporter, prompts) =
        yogurt_server::__test_only_aux_state(tmp.path().join("notes")).expect("build aux state");
    let db = yogurt_db::Db::open_in_memory().unwrap();
    let meeting_repo = Arc::new(yogurt_db::MeetingRepo::new(db.clone()));
    let label_repo = Arc::new(yogurt_db::LabelRepo::new(db.clone()));
    let state = AppState {
        mode: Mode::Release,
        storage,
        session,
        bind_port,
        meetings: yogurt_server::meetings::Registry::new(),
        markdown_exporter,
        prompts,
        db,
        keys: Arc::new(yogurt_db::keys::MemoryKeyStore::default()),
        llm_override: Some(Arc::new(yogurt_server::__test_only_llm_mock::MockLlm)),
        meeting_repo,
        label_repo,
        app_events_tx: tokio::sync::broadcast::channel(64).0,
    };
    (state, token_str, tmp)
}

/// Task 1 of "enhance streaming" — the enhance handler now streams the
/// LLM output over the meeting WS instead of emitting one placeholder
/// `streaming` frame after the whole completion lands.
///
/// Asserts, against the deterministic `MockLlm`:
/// - the WS sees `sending`, then at least one `streaming` frame, then `done`;
/// - every `streaming` frame carries a non-empty `text` snapshot whose
///   length equals `chars`;
/// - the last `streaming` frame's `text` equals the raw MockLlm completion
///   for this exact request (computed independently below via the same
///   `Prompts::render_enhance` + `MockLlm::complete` calls the handler
///   itself makes, so this doesn't just restate the handler's own output);
/// - the response body / persistence assertions from the non-streaming test
///   above still hold (`enriched_md`, `too_short`, `llm_model`).
#[tokio::test(flavor = "multi_thread")]
async fn enhance_streams_progress_over_ws() {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    let (state, token, _tmp) = build_mock_test_state(addr.port());
    let prompts = state.prompts.clone();
    let app = yogurt_server::__test_router(state.clone());
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    let meeting = state.meetings.create().await;
    let meeting_id = meeting.id;

    let notes_md = "- pricing\n";
    let transcript_json = r#"[{"ts_ms":120000,"channel":"mic","text":"We debated the pricing model in detail today"}]"#;

    // Independently reconstruct the exact raw MockLlm output the handler
    // will produce for this request, by driving the same two building
    // blocks (`render_enhance` + `MockLlm::complete`) the handler itself
    // uses — without re-implementing the mock's bullet-formatting rules.
    let user_prompt = prompts
        .render_enhance(&EnhanceCtx {
            notes: notes_md,
            transcript: transcript_json,
        })
        .expect("render enhance prompt");
    let expected_raw = yogurt_server::__test_only_llm_mock::MockLlm
        .complete(ChatRequest {
            messages: vec![ChatMessage::user(user_prompt)],
            stream: false,
        })
        .await
        .expect("mock complete")
        .content;

    // Connect the WS BEFORE posting — the handler runs the whole enhance
    // pipeline (including the `done` frame) inside the POST's response
    // future, so a subscriber that attaches after the POST returns would
    // have missed every frame.
    let ws_url = format!(
        "ws://127.0.0.1:{}/ws/meetings/{}?token={}",
        addr.port(),
        meeting_id,
        token
    );
    let mut req = ws_url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{}", addr.port())).unwrap(),
    );
    let (ws_stream, _) = tokio_tungstenite::connect_async(req)
        .await
        .expect("ws connect");
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Deterministic subscribe signal instead of a fixed sleep.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while meeting.events_tx.receiver_count() == 0 {
        assert!(
            tokio::time::Instant::now() < deadline,
            "WS handler never subscribed to events_tx within 5s"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(format!(
            "http://127.0.0.1:{}/api/meetings/{meeting_id}/enhance",
            addr.port()
        ))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": notes_md,
            "transcript_json": transcript_json,
            "title": "Streaming test",
        }))
        .send()
        .await
        .expect("post enhance");
    assert_eq!(resp.status(), 200, "enhance must return 200");
    let body: serde_json::Value = resp.json().await.expect("json body");
    assert_eq!(body["too_short"].as_bool(), Some(false), "got: {body}");
    assert!(
        body["enriched_md"]
            .as_str()
            .unwrap_or_default()
            .contains("- pricing"),
        "got: {body}"
    );

    // Collect every `enhance_progress` frame — by the time POST returned,
    // the handler has already broadcast all of them (sending, streaming
    // x N, done); this just drains what's sitting in the channel.
    let mut phases: Vec<String> = Vec::new();
    let mut streaming_frames: Vec<(usize, String)> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let next = tokio::time::timeout(remaining, ws_read.next()).await;
        let frame = match next {
            Ok(Some(Ok(f))) => f,
            _ => break,
        };
        let text = match frame {
            Message::Text(t) => t,
            _ => continue,
        };
        let value: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if value["type"] != "enhance_progress" {
            continue;
        }
        let phase = value["phase"].as_str().unwrap_or("").to_string();
        if phase == "streaming" {
            let chars = value["chars"].as_u64().unwrap_or(0) as usize;
            let snapshot = value["text"].as_str().unwrap_or("").to_string();
            streaming_frames.push((chars, snapshot));
        }
        let done = phase == "done" || phase == "error";
        phases.push(phase);
        if done {
            break;
        }
    }
    let _ = ws_write.send(Message::Close(None)).await;

    assert_eq!(
        phases.first().map(String::as_str),
        Some("sending"),
        "first frame must be sending. got: {phases:?}"
    );
    assert_eq!(
        phases.last().map(String::as_str),
        Some("done"),
        "last frame must be done. got: {phases:?}"
    );
    assert!(
        !streaming_frames.is_empty(),
        "expected at least one streaming frame. got phases: {phases:?}"
    );

    for (chars, snapshot) in &streaming_frames {
        assert!(
            !snapshot.is_empty(),
            "streaming frame text must be non-empty"
        );
        assert_eq!(
            *chars,
            snapshot.len(),
            "chars must equal text.len(). text: {snapshot:?}"
        );
    }

    let (_, last_snapshot) = streaming_frames.last().expect("checked non-empty above");
    assert_eq!(
        last_snapshot, &expected_raw,
        "last streaming frame's text must equal the raw MockLlm completion"
    );

    server.abort();
}

// ─── Task 1 (pill-color Option E): LLM locality stamp ───────────────────────
//
// The pill color depends on whether the LLM provider is local (localhost /
// 127.0.0.1) or cloud (hosted). `enhance.rs` stamps that locality into the
// response's `llm_model` as `"local · <model>"` / `"cloud · <model>"` so the
// frontend can pick a class without re-parsing the URL itself.
//
// These tests drive the **production resolve path** end-to-end: NO
// `state.llm_override`, no `BaseUrlMock` shim — `enhance.rs::enhance`
// resolves the LLM via `llm_openai::resolve` exactly as it would in a real
// user run. The local case uses a wiremock upstream so the streaming
// `/chat/completions` POST actually succeeds; the cloud case asserts the
// same production `OpenAiCompatClient` exposes `base_url()` correctly when
// constructed with a hosted URL (no real HTTP roundtrip — a wiremock only
// binds to 127.0.0.1, which the cloud path can't use for an E2E).
//
// Regression target: `OpenAiCompatClient` must override the trait's
// `base_url()` default of `""` so `stamp_llm_engine` parses the real
// upstream URL. Without the override every configured provider (including
// an Ollama at `http://localhost:11434/v1`) would stamp `cloud · <model>`.

/// Boot a real axum server bound to 127.0.0.1:0 with a fresh tempdir for
/// SQLite + notes, and an active provider row at `base_url` with a key
/// seeded in the `MemoryKeyStore`. `llm_override` is `None` so
/// `enhance.rs::enhance` resolves through the production
/// `llm_openai::resolve` chain (`override -> env -> active provider +
/// stored key -> MockLlm`). The returned `AppState` lets the caller
/// re-run `resolve(&state)` directly when an actual HTTP roundtrip to
/// `base_url` would either be redundant (cloud case) or impossible
/// (wiremock can only bind 127.0.0.1, so the local-test E2E goes
/// through this helper too, with `base_url` pointed at the mock).
async fn boot_app_with_provider(
    base_url: &str,
    model: &str,
) -> (
    std::net::SocketAddr,
    String,
    tempfile::TempDir,
    AppState,
) {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let storage = Arc::new(yogurt_server::storage::Storage::init_at(&db_path).unwrap());
    let session_token = yogurt_server::session::load_or_create(&token_path).unwrap();
    let token_str = session_token.as_str().to_string();
    let session: Arc<yogurt_server::session::SessionToken> = Arc::new(session_token);
    let (markdown_exporter, prompts) =
        yogurt_server::__test_only_aux_state(tmp.path().join("notes")).expect("build aux state");
    let db = yogurt_db::Db::open_in_memory().unwrap();
    let meeting_repo = Arc::new(yogurt_db::MeetingRepo::new(db.clone()));
    let label_repo = Arc::new(yogurt_db::LabelRepo::new(db.clone()));
    let keys: Arc<yogurt_db::keys::MemoryKeyStore> =
        Arc::new(yogurt_db::keys::MemoryKeyStore::default());

    let provider_id = yogurt_db::providers::insert(
        &db,
        yogurt_db::providers::NewProvider {
            name: "Test".to_string(),
            base_url: base_url.to_string(),
            model: model.to_string(),
        },
    )
    .unwrap();
    yogurt_db::providers::set_active(&db, &provider_id).unwrap();
    keys.set(&provider_id, "sk-test").unwrap();

    let state = AppState {
        mode: Mode::Release,
        storage,
        session,
        bind_port: addr.port(),
        meetings: yogurt_server::meetings::Registry::new(),
        markdown_exporter,
        prompts,
        db,
        keys,
        // No llm_override — `resolve()` MUST walk the active-provider path.
        llm_override: None,
        meeting_repo,
        label_repo,
        app_events_tx: tokio::sync::broadcast::channel(64).0,
    };

    let app = yogurt_server::__test_router(state.clone());
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });

    (addr, token_str, tmp, state)
}

async fn enhance_and_get_llm_model(addr: &std::net::SocketAddr, token: &str) -> String {
    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let meeting_id = created["id"].as_str().unwrap().to_string();
    let resp = client
        .post(format!("http://{addr}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "notes_md": "- test\n",
            "transcript_json": "[]",
        }))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();
    assert_eq!(
        status, 200,
        "enhance must return 200, got {status} body={body_text}"
    );
    let body: serde_json::Value = serde_json::from_str(&body_text)
        .unwrap_or_else(|e| panic!("parse enhance body {body_text:?}: {e}"));
    body["llm_model"]
        .as_str()
        .expect("llm_model in response")
        .to_string()
}

/// Drive the FULL production resolve path with a wiremock upstream at
/// `127.0.0.1:<wiremock_port>`. The provider row's `base_url` is set to
/// the wiremock URL so `OpenAiCompatClient::stream` can actually complete
/// a streaming POST; `stamp_llm_engine` parses the `127.0.0.1` host as
/// `local`, so the response `llm_model` must start with `"local · "`. The
/// regression that this catches: `OpenAiCompatClient` missing the
/// `base_url()` override silently stamps every provider (including a real
/// Ollama at `localhost:11434`) as `"cloud · "` because the trait default
/// of `""` parses as "no host -> cloud".
#[tokio::test(flavor = "multi_thread")]
async fn it_stamps_llm_model_as_local_for_localhost_provider() {
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    // Wiremock stands in for the OpenAI-compatible upstream. A single SSE
    // chat-completion event carrying `"- ai\n"` then the terminal `[DONE]`
    // — enough for `streaming::stream` to yield one chunk so enhance can
    // persist `enriched_md` and finalize the response with `too_short=false`.
    let upstream = wiremock::MockServer::start().await;
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"- ai\\n\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .and(wiremock::matchers::header("authorization", "Bearer sk-test"))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .insert_header("cache-control", "no-cache")
                .set_body_string(body),
        )
        .mount(&upstream)
        .await;

    // Point the active-provider row at the wiremock. The URL the production
    // `OpenAiCompatClient` ends up connecting to is `127.0.0.1:<port>` —
    // what `stamp_llm_engine` reads. Real users see the same shape for
    // Ollama: `http://localhost:11434/v1`.
    let (addr, token, _tmp, _state) =
        boot_app_with_provider(&upstream.uri(), "mock-model").await;
    let llm_model = enhance_and_get_llm_model(&addr, &token).await;
    assert!(
        llm_model.starts_with("local \u{b7} "),
        "expected 'local \u{b7} <model>' stamp when the active provider's base_url \
         is 127.0.0.1, got {llm_model:?}",
    );
}

/// Drive the production resolve path with a HOSTED base_url. Since
/// wiremock can only bind `127.0.0.1` (which would parse as `local`),
/// the hosted case can't go through an E2E HTTP roundtrip in CI. The
/// contract that matters is `OpenAiCompatClient::base_url()` returning
/// the constructor argument — verified here by calling
/// `test_support::resolve(&state)` directly and asserting on the returned
/// `Arc<dyn LlmClient>`'s `base_url()`. The local-hostname -> `local`
/// branch is separately covered by the wiremock E2E above; parsing of
/// any non-`localhost` / non-`127.0.0.1` / non-`[::1]` URL as `cloud` is
/// a one-line match arm tested structurally by `stamp_llm_engine`'s
/// existing coverage in `crates/yogurt-llm/src/lib.rs`.
#[tokio::test(flavor = "multi_thread")]
async fn it_stamps_llm_model_as_cloud_for_hosted_provider() {
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let hosted_url = "https://api.minimax.io/v1";
    let (_addr, _token, _tmp, state) =
        boot_app_with_provider(hosted_url, "MiniMax-Text-01").await;

    let llm = yogurt_server::test_support::resolve(&state)
        .await
        .expect("resolve must succeed against an active provider + key");
    assert_eq!(
        llm.base_url(),
        hosted_url,
        "production OpenAiCompatClient via resolve() must expose its constructor base_url",
    );
    assert_eq!(
        llm.model_name(),
        "MiniMax-Text-01",
        "model_name passes through unchanged"
    );
    // Combined with `stamp_llm_engine`'s parse contract
    // (`host != "localhost" | "127.0.0.1" | "[::1]"` -> `cloud`), the
    // assert above guarantees the response would carry `"cloud \u{b7}
    // MiniMax-Text-01"` if enhance were driven end-to-end against this
    // same provider row.
    assert!(
        !matches!(
            llm.base_url().split("://").nth(1).and_then(|s| s.split('/').next()),
            Some("localhost") | Some("127.0.0.1") | Some("[::1]")
        ),
        "hosted URL must not parse as local for stamp_llm_engine: {}",
        llm.base_url(),
    );
}
