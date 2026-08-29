//! Regression (2026-08-29, meeting 01a04c36): the post view showed
//! "userâ\u{80}\u{99}s" even though `fix_model_mojibake` repairs that form.
//! The fixer was fine; `yogurt_notes::ast::strip_markers` re-mangled the
//! clean "\u{2019}" afterwards by copying bytes with `byte as char`, which
//! only triggers when a bullet carries an inline wire-format span.
//!
//! Drives the real `/enhance` handler against a fake OpenAI-compatible
//! server with a canned completion, then reads `enriched_md` back out of
//! SQLite so the assertion covers the whole write path. Its own test
//! binary because it sets the `YOGURT_LLM_*` env vars, which would race
//! with `enhance_endpoint.rs` (that file removes them).

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use yogurt_server::{run_with_config, Mode, RunConfig};

/// Minimal OpenAI-compatible chat endpoint: ignores the request, returns
/// `content` as the single choice. Serves one request per accepted
/// connection, forever.
async fn spawn_fake_llm(content: String) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(x) => x,
                Err(_) => return,
            };
            let content = content.clone();
            tokio::spawn(async move {
                // Read headers + body (small; one read is enough after the
                // headers arrive, but loop until we see the JSON close).
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                loop {
                    let n = match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&tmp[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    if let Some(hdr_end) = text.find("\r\n\r\n") {
                        let len = text
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length: "))
                            .or_else(|| {
                                text.lines()
                                    .find_map(|l| l.strip_prefix("Content-Length: "))
                            })
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if buf.len() >= hdr_end + 4 + len {
                            break;
                        }
                    }
                }
                let body = serde_json::json!({
                    "id": "fake",
                    "model": "fake-model",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": content },
                        "finish_reason": "stop"
                    }]
                })
                .to_string();
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    addr
}

async fn spawn_server() -> (
    std::net::SocketAddr,
    String,
    tempfile::TempDir,
    std::path::PathBuf,
) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let token = yogurt_server::session::load_or_create(&token_path)
        .expect("seed session token")
        .as_str()
        .to_string();
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    std::env::set_var("YOGURT_MEMORY_KEYSTORE", "1");
    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path.clone()),
        session_token_path: Some(token_path),
        notes_dir: Some(tmp.path().join("notes")),
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
    };
    tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });
    for _ in 0..100 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return (addr, token, tmp, db_path);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable");
}

/// Run one enhance against a fake LLM returning `llm_content`; return the
/// `enriched_md` persisted in SQLite (not the HTTP response) so the
/// assertion covers the whole write path.
async fn enhance_with(llm_content: &str) -> String {
    let llm = spawn_fake_llm(llm_content.to_string()).await;
    // SAFETY: this test binary owns these vars; nothing else in the
    // process reads them concurrently.
    unsafe {
        std::env::set_var("YOGURT_LLM_BASE_URL", format!("http://{llm}/v1"));
        std::env::set_var("YOGURT_LLM_API_KEY", "test");
        std::env::set_var("YOGURT_LLM_MODEL", "fake-model");
    }
    let (addr, token, _tmp, db_path) = spawn_server().await;
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
    let id = created["id"].as_str().unwrap().to_string();
    let body = serde_json::json!({
        "notes_md": "",
        "transcript_json": "[{\"ts_ms\":4000,\"channel\":\"mic\",\"text\":\"today is my last day at disney\"}]",
        "title": "Last day",
        "started_at_unix_ms": 1_700_000_000_000_i64,
    });
    let resp = client
        .post(format!("http://{addr}/api/meetings/{id}/enhance"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "enhance: {}",
        resp.text().await.unwrap_or_default()
    );
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.query_row(
        "SELECT enriched_md FROM meetings WHERE id = ?1",
        [&id],
        |r| r.get::<_, String>(0),
    )
    .unwrap()
}

/// One test, two cases in sequence: `enhance_with` sets process-wide
/// `YOGURT_LLM_*` env vars, so parallel test fns would race each other's
/// fake LLM and assert against the wrong canned output.
#[tokio::test(flavor = "multi_thread")]
async fn model_mojibake_is_repaired_before_persist() {
    // Latin-1 form, exactly the chars stored for meeting 01a04c36:
    // U+00E2 U+0080 U+0099.
    let stored = enhance_with(
        "## Final Day\n\n- Today is the user\u{e2}\u{80}\u{99}s last day <span data-ai-grey data-ts=\"4\">User submitted their two weeks\u{e2}\u{80}\u{99} notice</span>\n",
    )
    .await;
    assert!(
        !stored.contains('\u{e2}'),
        "mojibake survived into enriched_md: {stored:?}"
    );
    assert!(
        stored.contains("user\u{2019}s"),
        "expected curly apostrophe: {stored:?}"
    );

    // Clean UTF-8 must pass through untouched.
    let stored = enhance_with("## Final Day\n\n- Today is the user\u{2019}s last day\n").await;
    assert!(
        stored.contains("user\u{2019}s"),
        "clean apostrophe mangled: {stored:?}"
    );
}
