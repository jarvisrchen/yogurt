//! Integration tests for the Granola-style meeting labels REST surface
//! (`/api/labels*` + `label_ids` on `PATCH /api/meetings/:id`).
//!
//! Mirrors `tests/meetings_api.rs`'s `spawn_server` helper — a real axum
//! server bound to an ephemeral 127.0.0.1 port, tempdir-isolated SQLite +
//! session token + notes directory per test.

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

#[tokio::test(flavor = "multi_thread")]
async fn it_finds_or_creates_and_lists_with_counts() {
    let (addr, token, handle, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    let create_resp = client
        .post(format!("http://{addr}/api/labels"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "name": "Sales" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 201);
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let lid = created["id"].as_str().unwrap().to_string();

    // Same name, different case -> 200, same id.
    let dup_resp = client
        .post(format!("http://{addr}/api/labels"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "name": "sales" }))
        .send()
        .await
        .unwrap();
    assert_eq!(dup_resp.status(), 200);
    let dup: serde_json::Value = dup_resp.json().await.unwrap();
    assert_eq!(dup["id"].as_str().unwrap(), lid);

    let list_resp = client
        .get(format!("http://{addr}/api/labels"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let xs: Vec<serde_json::Value> = list_resp.json().await.unwrap();
    assert_eq!(xs.len(), 1);
    assert_eq!(xs[0]["meeting_count"].as_i64().unwrap(), 0);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_applies_labels_to_a_meeting_via_patch() {
    let (addr, token, handle, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    let lid: String = client
        .post(format!("http://{addr}/api/labels"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "name": "Sales" }))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mid: String = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "title": "Standup" }))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let patch_resp = client
        .patch(format!("http://{addr}/api/meetings/{mid}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "label_ids": [lid] }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 200);
    let patched: serde_json::Value = patch_resp.json().await.unwrap();
    assert_eq!(patched["labels"][0]["name"].as_str().unwrap(), "Sales");

    let counts: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/api/labels"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(counts[0]["meeting_count"].as_i64().unwrap(), 1);

    let list_resp: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(list_resp[0]["labels"][0]["id"].as_str().unwrap(), lid);

    // Rename + recolor via PATCH /api/labels/:id — meeting reflects it.
    let rename_resp = client
        .patch(format!("http://{addr}/api/labels/{lid}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "name": "Customers", "color": "straw" }))
        .send()
        .await
        .unwrap();
    assert_eq!(rename_resp.status(), 200);

    let reloaded: serde_json::Value = client
        .get(format!("http://{addr}/api/meetings/{mid}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reloaded["labels"][0]["name"].as_str().unwrap(), "Customers");
    assert_eq!(reloaded["labels"][0]["color"].as_str().unwrap(), "straw");

    // Unknown label id on a meeting PATCH -> 400.
    let bad_patch = client
        .patch(format!("http://{addr}/api/meetings/{mid}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "label_ids": ["nonesuch"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_patch.status(), 400);

    // Delete the label -> 204; meeting now has empty labels; delete again -> 404.
    let del_resp = client
        .delete(format!("http://{addr}/api/labels/{lid}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(del_resp.status(), 204);

    let after_delete: serde_json::Value = client
        .get(format!("http://{addr}/api/meetings/{mid}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(after_delete["labels"].as_array().unwrap().is_empty());

    let del_again = client
        .delete(format!("http://{addr}/api/labels/{lid}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(del_again.status(), 404);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_requires_a_bearer_token() {
    let (addr, _token, handle, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/api/labels"))
        .send()
        .await
        .unwrap();
    assert!(!resp.status().is_success());
    handle.abort();
}
