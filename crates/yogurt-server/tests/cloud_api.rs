//! Yogurt Cloud REST surface (`/api/cloud*`) without a cloud: the not-connected states and input validation.

use std::time::Duration;

use yogurt_server::{run_with_config, Mode, RunConfig};

async fn spawn_server() -> (
    std::net::SocketAddr,
    String,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
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
        db_path: Some(db_path),
        session_token_path: Some(token_path),
        notes_dir: Some(notes_dir),
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
        keys_path: None,
    };
    let handle = tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });
    for _ in 0..400 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return (addr, token, handle, tmp);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable within 8 seconds");
}

async fn call(
    addr: std::net::SocketAddr,
    token: &str,
    path: &str,
    body: serde_json::Value,
) -> (u16, serde_json::Value) {
    let resp = reqwest::Client::new()
        .post(format!("http://{addr}{path}"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    (resp.status().as_u16(), resp.json().await.unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn not_connected_states_and_input_errors_are_typed() {
    let (addr, token, handle, _tmp) = spawn_server().await;

    let status: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/api/cloud"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status, serde_json::json!({ "connected": false }));

    let (code, body) = call(addr, &token, "/api/cloud/push", serde_json::json!({})).await;
    assert_eq!((code, body["code"].as_str()), (409, Some("not_connected")));

    let (code, body) = call(
        addr,
        &token,
        "/api/cloud/sign-in",
        serde_json::json!({"password": "x"}),
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (409, Some("not_connected")));

    let (code, body) = call(
        addr,
        &token,
        "/api/cloud/check",
        serde_json::json!({"address": "http://example.com"}),
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (400, Some("bad_address")));
    let (code, body) = call(
        addr,
        &token,
        "/api/cloud/check",
        serde_json::json!({"address": "http://127.0.0.1:9"}),
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (502, Some("bad_config")));

    let connect = |address: &str, name: &str| serde_json::json!({ "address": address, "email": "a@b.c", "password": "pw", "machine_name": name });
    let (code, body) = call(
        addr,
        &token,
        "/api/cloud/connect",
        connect("http://example.com", "Mac"),
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (400, Some("bad_address")));

    let (code, body) = call(
        addr,
        &token,
        "/api/cloud/connect",
        connect("example.com", "  "),
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (400, Some("bad_request")));

    // Nothing listens here, so the config fetch fails before any password is sent.
    let (code, body) = call(
        addr,
        &token,
        "/api/cloud/connect",
        connect("http://127.0.0.1:9", "Mac"),
    )
    .await;
    assert_eq!((code, body["code"].as_str()), (502, Some("bad_config")));
    assert!(!body.to_string().contains("pw"));

    // Disconnecting when nothing is connected is a no-op.
    let (code, body) = call(addr, &token, "/api/cloud/disconnect", serde_json::json!({})).await;
    assert_eq!(
        (code, body),
        (200, serde_json::json!({ "connected": false }))
    );

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn every_cloud_route_rejects_a_request_without_the_session_token() {
    let (addr, _token, handle, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();
    for (method, path) in [
        ("GET", "/api/cloud"),
        ("POST", "/api/cloud/check"),
        ("POST", "/api/cloud/connect"),
        ("POST", "/api/cloud/sign-in"),
        ("POST", "/api/cloud/push"),
        ("POST", "/api/cloud/disconnect"),
    ] {
        let req = client.request(method.parse().unwrap(), format!("http://{addr}{path}"));
        for req in [req.try_clone().unwrap(), req.bearer_auth("not-the-token")] {
            let resp = req.json(&serde_json::json!({})).send().await.unwrap();
            assert_eq!(resp.status().as_u16(), 403, "{method} {path}");
        }
    }
    handle.abort();
}
