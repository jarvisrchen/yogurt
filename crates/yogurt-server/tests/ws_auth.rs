//! `/ws` auth against the real server on an ephemeral loopback port with tempdir overrides, driven by `tokio-tungstenite`.

use std::net::TcpListener as StdTcpListener;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use yogurt_server::{Mode, RunConfig};

struct TestServer {
    port: u16,
    _tempdir: TempDir,
    token: String,
    join: tokio::task::JoinHandle<anyhow::Result<()>>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.join.abort();
    }
}

async fn spawn_test_server() -> TestServer {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("db.sqlite");
    let token_path: PathBuf = tmp.path().join("session-token");

    let token = yogurt_server::session::load_or_create(&token_path)
        .expect("seed token")
        .as_str()
        .to_string();

    // Bind a std listener to get a free port, drop it, and have the server bind it. The race window is tiny on loopback.
    let port = {
        let l = StdTcpListener::bind("127.0.0.1:0").expect("ephemeral bind");
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    };
    let addr = format!("127.0.0.1:{port}").parse().unwrap();

    std::env::set_var("YOGURT_MEMORY_KEYSTORE", "1");
    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path),
        session_token_path: Some(token_path),
        notes_dir: Some(tmp.path().join("notes")),
        // Parallel tests deadlock on the real user DB's WAL lock.
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
    };
    let join = tokio::spawn(yogurt_server::run_with_config(cfg));

    for _ in 0..50 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    TestServer {
        port,
        _tempdir: tmp,
        token,
        join,
    }
}

#[tokio::test]
async fn it_rejects_ws_with_bad_origin() {
    let server = spawn_test_server().await;
    let url = format!("ws://127.0.0.1:{}/ws?token={}", server.port, server.token);

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_static("http://evil.example.com"),
    );

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("expected 403 rejection");
    let msg = format!("{err}");
    assert!(
        msg.contains("403") || msg.to_lowercase().contains("forbidden"),
        "expected 403 in error; got: {msg}"
    );
}

#[tokio::test]
async fn it_rejects_ws_without_token() {
    let server = spawn_test_server().await;
    let url = format!("ws://127.0.0.1:{}/ws", server.port);

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://localhost:{}", server.port)).unwrap(),
    );

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("expected 403 rejection");
    let msg = format!("{err}");
    assert!(
        msg.contains("403") || msg.to_lowercase().contains("forbidden"),
        "expected 403 in error; got: {msg}"
    );
}

#[tokio::test]
async fn it_rejects_ws_with_subprotocol_only_no_query_token() {
    // Only `?token=` is honored: the correct token in the `Sec-WebSocket-Protocol` header must still fail auth.
    let server = spawn_test_server().await;
    let url = format!("ws://127.0.0.1:{}/ws", server.port);

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://localhost:{}", server.port)).unwrap(),
    );
    req.headers_mut().insert(
        "sec-websocket-protocol",
        HeaderValue::from_str(&format!("yogurt.{}", server.token)).unwrap(),
    );

    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("subprotocol path must not authenticate");
    let msg = format!("{err}");
    assert!(
        msg.contains("403") || msg.to_lowercase().contains("forbidden"),
        "expected 403 even with subprotocol token; got: {msg}"
    );
}

#[tokio::test]
async fn it_rejects_ws_with_header_injection_attempt_in_query() {
    // A token containing CR/LF must not smuggle a downstream HTTP header.
    let server = spawn_test_server().await;
    // %0D%0A is CRLF, percent-encoded because tungstenite refuses a raw newline in a URL.
    let url = format!(
        "ws://127.0.0.1:{}/ws?token={}%0D%0AX-Injected:%20yes",
        server.port, server.token
    );

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://localhost:{}", server.port)).unwrap(),
    );

    // Must reject with no 101 upgrade (the decoded value is not the real token).
    let err = tokio_tungstenite::connect_async(req)
        .await
        .expect_err("header-injection token must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("403") || msg.to_lowercase().contains("forbidden"),
        "expected 403 for header-injection token; got: {msg}"
    );
}

#[test]
fn it_redacts_token_query_in_uri_logs() {
    use yogurt_server::ws::redact_token_in_uri;
    let safe = redact_token_in_uri("/ws?token=abcDEF123-_&other=keep");
    assert_eq!(safe, "/ws?token=<REDACTED>&other=keep");

    let lone = redact_token_in_uri("/ws?token=abcDEF123");
    assert_eq!(lone, "/ws?token=<REDACTED>");

    let none = redact_token_in_uri("/ws");
    assert_eq!(none, "/ws");

    let multi = redact_token_in_uri("/ws?a=1&token=secret&b=2");
    assert_eq!(multi, "/ws?a=1&token=<REDACTED>&b=2");
}

#[tokio::test]
async fn it_accepts_ws_with_correct_origin_and_token() {
    let server = spawn_test_server().await;
    let url = format!("ws://127.0.0.1:{}/ws?token={}", server.port, server.token);

    let mut req = url.into_client_request().expect("build req");
    req.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://localhost:{}", server.port)).unwrap(),
    );

    let (mut socket, response) = tokio_tungstenite::connect_async(req)
        .await
        .expect("ws upgrade should succeed");
    assert_eq!(
        response.status().as_u16(),
        101,
        "expected 101 Switching Protocols"
    );

    use futures_util::{SinkExt, StreamExt};
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            "ping".to_string(),
        ))
        .await
        .expect("send");
    let echoed = socket.next().await.expect("recv").expect("ok");
    match echoed {
        tokio_tungstenite::tungstenite::Message::Text(t) => assert_eq!(t, "ping"),
        other => panic!("expected text echo; got {other:?}"),
    }
}
