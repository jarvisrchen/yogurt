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

use std::time::Duration;

use yogurt_server::{run_with_config, Mode, RunConfig};

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

    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path.clone()),
        session_token_path: Some(token_path),
        notes_dir: Some(notes_dir.clone()),
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

    let (addr, token, _handle, _tmp, db_path) = spawn_server().await;
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

    // 3) Wire-format assertions on the response body.
    assert!(
        enriched_md.contains("- pricing"),
        "user notes preserved verbatim. got: {enriched_md}",
    );
    assert!(
        enriched_md.contains(r#"data-ai-grey data-ts="120""#),
        "AI bullet tagged with transcript timestamp. got: {enriched_md}",
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
        file_body.contains(r#"data-ai-grey data-ts="120""#),
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
    assert_eq!(title_db, "Sales sync", "SQLite title persisted");
    let doc: serde_json::Value =
        serde_json::from_str(&enriched_doc_json_db).expect("enriched_doc_json parses");
    assert!(
        doc.get("blocks").and_then(|b| b.as_array()).is_some(),
        "enriched_doc_json shape: {{ blocks: [...] }}. got: {enriched_doc_json_db}",
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
