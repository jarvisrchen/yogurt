//! The Cloud webapp enhances a phone meeting through yogurt-wasm, and must store exactly what the Mac's `/enhance` route stores.
//! Each case in `crates/yogurt-wasm/tests/fixture.json` runs through the real route against a fake OpenAI-compatible server that streams the canned reply in small chunks, and through yogurt-wasm's exports.
//! The prompt the route sent, its `enriched_md`, and its template must match yogurt-wasm's, and both must match the fixture, which `cloud/src/lib/wasm.test.ts` checks the built WASM against.
//! Its own test binary because it sets `YOGURT_LLM_*` env vars.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use yogurt_server::{run_with_config, Mode, RunConfig};

const FIXTURE: &str = include_str!("../../yogurt-wasm/tests/fixture.json");

/// Streams `reply` as SSE deltas of a few characters each and records each request body.
async fn spawn_fake_llm(
    reply: Arc<Mutex<String>>,
    seen: Arc<Mutex<Vec<Value>>>,
) -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            let reply = reply.lock().unwrap().clone();
            let seen = seen.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let body = loop {
                    let n = match sock.read(&mut tmp).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    buf.extend_from_slice(&tmp[..n]);
                    let text = String::from_utf8_lossy(&buf);
                    let Some(end) = text.find("\r\n\r\n") else {
                        continue;
                    };
                    let len = text[..end]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(str::to_string)
                        })
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + len {
                        break buf[end + 4..end + 4 + len].to_vec();
                    }
                };
                seen.lock()
                    .unwrap()
                    .push(serde_json::from_slice(&body).unwrap());
                let chars: Vec<char> = reply.chars().collect();
                let mut sse = String::new();
                for piece in chars.chunks(5) {
                    let delta: String = piece.iter().collect();
                    let chunk = serde_json::json!({"choices": [{"delta": {"content": delta}, "finish_reason": null}]});
                    sse.push_str(&format!("data: {chunk}\n\n"));
                }
                sse.push_str("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n");
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",
                    sse.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            });
        }
    });
    addr
}

async fn spawn_server() -> (std::net::SocketAddr, String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let token_path = tmp.path().join("session-token");
    let token = yogurt_server::session::load_or_create(&token_path)
        .unwrap()
        .as_str()
        .to_string();
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    std::env::set_var("YOGURT_MEMORY_KEYSTORE", "1");
    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(tmp.path().join("yogurt-test.db")),
        session_token_path: Some(token_path),
        notes_dir: Some(tmp.path().join("notes")),
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
        keys_path: None,
    };
    tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });
    for _ in 0..100 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .is_ok_and(|r| r.status().is_success())
        {
            return (addr, token, tmp);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_wasm_enhance_stores_what_the_mac_stores() {
    let reply = Arc::new(Mutex::new(String::new()));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let llm = spawn_fake_llm(reply.clone(), seen.clone()).await;
    // SAFETY: this test binary owns these vars and sets them before the server starts.
    unsafe {
        std::env::set_var("YOGURT_LLM_BASE_URL", format!("http://{llm}/v1"));
        std::env::set_var("YOGURT_LLM_API_KEY", "test");
        std::env::set_var("YOGURT_LLM_MODEL", "fake-model");
    }
    let (addr, token, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    let cases: Vec<Value> = serde_json::from_str(FIXTURE).unwrap();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let notes = case["notes_md"].as_str().unwrap();
        let transcript = case["transcript_json"].as_str().unwrap();
        let template = case["template"].as_str().map(str::to_string);
        let llm_output = case["llm_output"].as_str().unwrap();
        *reply.lock().unwrap() = llm_output.to_string();
        seen.lock().unwrap().clear();

        let created: Value = client
            .post(format!("http://{addr}/api/meetings"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let id = created["id"].as_str().unwrap();
        let resp = client
            .post(format!("http://{addr}/api/meetings/{id}/enhance"))
            .bearer_auth(&token)
            .json(&serde_json::json!({"notes_md": notes, "transcript_json": transcript, "template": template}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            200,
            "{name}: {}",
            resp.text().await.unwrap_or_default()
        );
        let mac: Value = resp.json().await.unwrap();

        let sent = seen.lock().unwrap()[0]["messages"].clone();
        let rendered: Value = serde_json::from_str(
            &yogurt_wasm::render_enhance(notes, transcript, template.clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(rendered, sent, "{name}: prompt");

        let wasm = yogurt_wasm::finish_enhance(notes, transcript, llm_output, template).unwrap();
        assert_eq!(
            wasm.enriched_md,
            mac["enriched_md"].as_str().unwrap(),
            "{name}: enriched_md"
        );
        assert_eq!(
            wasm.template,
            mac["template"].as_str().unwrap(),
            "{name}: template"
        );
        assert_eq!(
            wasm.enriched_md,
            case["enriched_md"].as_str().unwrap(),
            "{name}: fixture enriched_md"
        );
        assert_eq!(
            wasm.template,
            case["expected_template"].as_str().unwrap(),
            "{name}: fixture template"
        );
    }
}
