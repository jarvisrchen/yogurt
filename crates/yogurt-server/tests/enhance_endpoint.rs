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
