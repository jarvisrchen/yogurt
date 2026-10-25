//! End-to-end `POST /api/meetings/{id}/enhance` against a real server with tempdir SQLite and notes dir.
//! `MockLlm` is the LLM because the `YOGURT_LLM_*` vars are unset. It emits one AI bullet per transcript segment from the first 8 words.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
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
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
        keys_path: None,
    };
    let handle = tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });
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
    // Clear LLM env vars so MockLlm is the resolved client.
    // SAFETY: this test owns the env, and `from_env()` only checks for presence.
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }

    let (addr, token, _handle, tmp, db_path) = spawn_server().await;
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
    let meeting_id = created
        .get("id")
        .and_then(|v| v.as_str())
        .expect("meeting id")
        .to_string();

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
    assert_eq!(
        response.get("llm_model").and_then(|v| v.as_str()),
        Some("mock"),
        "response names the model that produced enriched_md"
    );

    // ammonia normalizes the boolean attribute to `data-ai-grey=""` on the way out.
    assert!(
        enriched_md.contains("- pricing"),
        "user notes preserved verbatim. got: {enriched_md}",
    );
    assert!(
        enriched_md.contains(r#"data-ai-grey="" data-ts="120""#),
        "AI bullet tagged with transcript timestamp. got: {enriched_md}",
    );
    // The mock, like a real model, emits its own `<span data-ai-grey>` scaffolding.
    // Render must strip it, not escape and double-wrap it. Escaped `&lt;span data-ai-grey` in the body means the corruption is back.
    assert!(
        !enriched_md.contains("&lt;span"),
        "model-emitted span markup must be stripped, not escaped. got: {enriched_md}",
    );
    assert!(
        enriched_md.contains("↳ 02:00"),
        "deep-link suffix formatted MM:SS. got: {enriched_md}",
    );

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
    // LLM provenance lives on the Library row (yogurt-db).
    let app_conn = rusqlite::Connection::open(tmp.path().join("yogurt-app.sqlite"))
        .expect("reopen app sqlite");
    let llm_model_db: Option<String> = app_conn
        .query_row(
            "SELECT llm_model FROM meetings WHERE id = ?",
            [&meeting_id],
            |row| row.get(0),
        )
        .expect("library row should exist after enhance");
    assert_eq!(
        llm_model_db.as_deref(),
        Some("mock"),
        "enhance stamps the model that produced the summary",
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

/// Regression: with an empty request `transcript_json`, `enhance` must fall back to the stored row's transcript rather than produce a notes-only doc.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_falls_back_to_stored_transcript_when_request_body_is_empty() {
    // SAFETY: clear LLM env so MockLlm is the resolved path.
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

    // Seed the transcript on the row, as the server-side persistence task would.
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

    // One AI bullet per stored segment can only appear if enhance read the stored transcript.
    assert!(
        enriched_md.contains(r#"data-ai-grey="" data-ts="90""#),
        "enhance must fall back to the stored transcript row. got: {enriched_md}"
    );
    assert!(
        enriched_md.contains("We should ship the stored"),
        "AI bullet must summarize the stored transcript text. got: {enriched_md}"
    );
}

/// A trivial transcript with no typed notes must skip the LLM/merge/persist pipeline and report `too_short: true`.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_skips_llm_for_a_too_short_meeting() {
    // The too-short path returns before the LLM is resolved. Clear env anyway so a leaked var cannot mask a removed early return.
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

    // The pipeline never ran and the stub row from POST /api/meetings is gone: an accidental start/stop must not leave an "Untitled meeting".
    let app_db_path = tmp.path().join("yogurt-app.sqlite");
    let conn = rusqlite::Connection::open(&app_db_path).expect("reopen yogurt-db");
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM meetings WHERE id = ?",
            [&meeting_id],
            |row| row.get(0),
        )
        .expect("count query");
    assert_eq!(rows, 0, "too-short meeting row must be deleted");
}

/// Real user notes must still enhance when the transcript is trivial. The too-short skip applies only when there is nothing to work with.
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

    // The auth middleware rejects before the handler, so no meeting is needed.
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

/// After enhance, `GET /api/meetings/{id}` returns the persisted columns for direct-link hydration. 404 on unknown id, 403 without token.
#[tokio::test(flavor = "multi_thread")]
async fn it_gets_a_meeting_after_enhance() {
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

    let unknown_id = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    let resp_404 = client
        .get(format!("http://{addr}/api/meetings/{unknown_id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp_404.status(), 404, "unknown meeting must be 404");

    let resp_403 = client
        .get(format!("http://{addr}/api/meetings/{meeting_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_403.status(), 403, "missing token must be 403");
}

/// Malicious notes and transcript must be neutralized before `enriched_md` leaves the server, since markdown-it renders with `html: true`.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_strips_xss_payloads_from_notes_and_transcript() {
    // SAFETY: clear LLM env so MockLlm is the resolved path.
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

    // Escaped forms like `&lt;script&gt;` are safe text. Assert no raw `<script>`, `<img`, `<iframe` survives.
    // Attribute substrings inside escaped tag bodies are inert.
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
    // Only <span> with data-* attrs is allowlisted: no live <a>, <link>, <object>, <embed>, <form>, <input>, <style>.
    for forbidden in [
        "<a ", "<a>", "<link", "<object", "<embed", "<form", "<input", "<style",
    ] {
        assert!(
            !enriched.contains(forbidden),
            "{forbidden} must not survive sanitization. got: {enriched}"
        );
    }

    // The wire-format spans must survive so the editor can render the notes.
    assert!(
        enriched.contains("data-ai-grey"),
        "data-ai-grey spans must survive sanitization. got: {enriched}"
    );
}

/// Answers every request with the same text.
#[derive(Clone)]
struct FixedLlm(String);

#[async_trait::async_trait]
impl LlmClient for FixedLlm {
    fn model_name(&self) -> String {
        "fixed".into()
    }

    async fn complete(&self, _req: ChatRequest) -> anyhow::Result<yogurt_llm::ChatResponse> {
        Ok(yogurt_llm::ChatResponse {
            content: self.0.clone(),
            model: "fixed".into(),
        })
    }

    async fn stream(
        &self,
        _req: ChatRequest,
    ) -> anyhow::Result<
        futures_util::stream::BoxStream<'static, anyhow::Result<yogurt_llm::ChatChunk>>,
    > {
        let chunks = vec![
            Ok(yogurt_llm::ChatChunk {
                delta: self.0.clone(),
                done: false,
            }),
            Ok(yogurt_llm::ChatChunk {
                delta: String::new(),
                done: true,
            }),
        ];
        Ok(futures_util::stream::iter(chunks).boxed())
    }
}

async fn serve(state: AppState) -> tokio::task::JoinHandle<()> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], state.bind_port));
    let app = yogurt_server::__test_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    })
}

async fn ephemeral_port() -> u16 {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    port
}

/// A model that answers with nothing must surface as an error, not the user's notes persisted as the enhanced document.
/// `merge_notes(notes, "")` is exactly the notes, so only the handler can notice.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_fails_loudly_when_the_model_returns_nothing() {
    let port = ephemeral_port().await;
    let (state, token, tmp) = build_test_state(port, Arc::new(FixedLlm("  \n\n".into())));
    let _server = serve(state.clone()).await;
    let meeting_id = state.meetings.create().await.id;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "- pricing\n- 247k\n",
            "transcript_json": r#"[{"ts_ms":1000,"channel":"them","text":"base salary is two forty seven"}]"#,
            "title": "Empty reply",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        502,
        "an empty model reply is a failed enhance"
    );
    let body = resp.text().await.unwrap();
    assert!(body.contains("empty reply"), "got: {body}");

    let conn = rusqlite::Connection::open(tmp.path().join("yogurt-test.db")).unwrap();
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM meetings WHERE id = ?1",
            [meeting_id.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        rows, 0,
        "no enriched row must be written for an empty reply"
    );
    assert!(
        std::fs::read_dir(tmp.path().join("notes"))
            .map(|d| d.count() == 0)
            .unwrap_or(true),
        "no markdown file must be written for an empty reply"
    );
}

/// Regression: the model folded the one-word note `247k` into `Base salary: 247k`.
/// The merge must render it as ink inside the bullet exactly once, not paint the bullet grey and append an orphan `247k`.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_weaves_a_one_word_note_into_the_ai_bullet_without_duplicating_it() {
    let port = ephemeral_port().await;
    let model_output =
        "## Offer\n\n- Base salary: 247k\n- 464k RSU with quarterly vesting at 6.25% per quarter\n";
    let (state, token, _tmp) = build_test_state(port, Arc::new(FixedLlm(model_output.into())));
    let _server = serve(state.clone()).await;
    let meeting_id = state.meetings.create().await.id;

    let resp = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/api/meetings/{meeting_id}/enhance"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "247k\n\n464k RSU\n",
            "transcript_json": r#"[{"ts_ms":40000,"channel":"them","text":"a base salary of 247,000 and an equity grant of 464,000"}]"#,
            "title": "Offer",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let enriched = body["enriched_md"].as_str().unwrap();

    assert_eq!(
        enriched.matches("247k").count(),
        1,
        "the note appears once, inside the bullet - no orphan copy. got:\n{enriched}"
    );
    assert!(
        enriched.contains("</span>247k"),
        "the user's word is outside the grey span. got:\n{enriched}"
    );
    assert_eq!(enriched.matches("464k RSU").count(), 1, "got:\n{enriched}");
    assert!(
        !enriched.trim_end().ends_with("247k"),
        "no trailing orphan paragraph. got:\n{enriched}"
    );
}

fn build_mock_test_state(bind_port: u16) -> (AppState, String, tempfile::TempDir) {
    build_test_state(
        bind_port,
        Arc::new(yogurt_server::__test_only_llm_mock::MockLlm),
    )
}

fn build_test_state(
    bind_port: u16,
    llm: Arc<dyn LlmClient>,
) -> (AppState, String, tempfile::TempDir) {
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
        llm_override: Some(llm),
        meeting_repo,
        label_repo,
        app_events_tx: tokio::sync::broadcast::channel(64).0,
        detect: Default::default(),
    };
    (state, token_str, tmp)
}

/// The enhance handler streams LLM output over the meeting WS: `sending`, at least one `streaming` frame whose non-empty `text` has length `chars`, then `done`.
/// The last `streaming` text must equal the raw MockLlm completion, computed independently via `Prompts::render_enhance` + `MockLlm::complete`.
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

    // Reconstruct the raw MockLlm output with the same building blocks the handler uses, without re-implementing the mock's formatting.
    let user_prompt = prompts
        .render_enhance(&EnhanceCtx {
            notes: notes_md,
            transcript: transcript_json,
            template: None,
        })
        .expect("render enhance prompt");
    let mock_raw = yogurt_server::__test_only_llm_mock::MockLlm
        .complete(ChatRequest {
            messages: vec![ChatMessage::user(user_prompt)],
            stream: false,
        })
        .await
        .expect("mock complete")
        .content;
    // The streamed preview is the document after the `template:` marker line.
    let expected_raw = yogurt_prompts::split_template_line(&mock_raw).1.to_string();

    // Connect the WS before posting: the handler broadcasts every frame inside the POST's response future, so a late subscriber misses them all.
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

    // By the time POST returned every frame is broadcast; drain the channel.
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
