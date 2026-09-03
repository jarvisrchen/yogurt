//! LLM-9: note formats ("templates") on `POST /api/meetings/{id}/enhance`
//! and the `GET /api/templates` list behind the post-meeting picker.
//!
//! Runs against the deterministic `MockLlm`, which names `standup` on its
//! first line when the transcript mentions one and `general` otherwise -
//! enough to pin the whole path: the marker is parsed and stripped, the
//! id lands on the meeting row and in the response, a forced format beats
//! the model's pick, and an unknown id is a 400 before any LLM work.

use std::time::Duration;

use yogurt_server::{run_with_config, Mode, RunConfig};

async fn spawn_server() -> (std::net::SocketAddr, String, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let token_path = tmp.path().join("session-token");
    let token = yogurt_server::session::load_or_create(&token_path)
        .expect("seed session token")
        .as_str()
        .to_string();
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = probe.local_addr().unwrap();
    drop(probe);
    std::env::set_var("YOGURT_MEMORY_KEYSTORE", "1");
    // SAFETY: this test owns the env; MockLlm must be the resolved client.
    unsafe {
        std::env::remove_var("YOGURT_LLM_BASE_URL");
        std::env::remove_var("YOGURT_LLM_API_KEY");
        std::env::remove_var("YOGURT_LLM_MODEL");
    }
    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(tmp.path().join("yogurt-test.db")),
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
            return (addr, token, tmp);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable within 2 seconds");
}

async fn create_meeting(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    token: &str,
) -> String {
    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    created["id"].as_str().unwrap().to_string()
}

const STANDUP_TRANSCRIPT: &str = r#"[{"ts_ms":5000,"channel":"them","text":"Quick standup: I finished the billing migration yesterday and today I'm on the retry queue"}]"#;

async fn enhance(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    token: &str,
    id: &str,
    template: Option<&str>,
) -> reqwest::Response {
    client
        .post(format!("http://{addr}/api/meetings/{id}/enhance"))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "notes_md": "- billing\n",
            "transcript_json": STANDUP_TRANSCRIPT,
            "title": "Daily",
            "template": template,
        }))
        .send()
        .await
        .unwrap()
}

async fn stored_template(
    client: &reqwest::Client,
    addr: std::net::SocketAddr,
    token: &str,
    id: &str,
) -> Option<String> {
    let row: serde_json::Value = client
        .get(format!("http://{addr}/api/meetings/{id}"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    row["template"].as_str().map(str::to_string)
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_every_template_in_picker_order_behind_auth() {
    let (addr, token, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();
    let unauth = client
        .get(format!("http://{addr}/api/templates"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        unauth.status(),
        403,
        "same session gate as every /api route"
    );

    let list: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/api/templates"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let ids: Vec<&str> = list.iter().map(|t| t["id"].as_str().unwrap()).collect();
    assert_eq!(ids, yogurt_prompts::TEMPLATE_IDS);
    assert_eq!(list[1]["name"], "Standup");
    assert!(list[1]["when"].as_str().unwrap().contains("standup"));
    assert!(list[0].get("body").is_none(), "the outline is prompt-only");
}

#[tokio::test(flavor = "multi_thread")]
async fn auto_detect_stamps_the_models_pick_and_strips_the_marker() {
    let (addr, token, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();
    let id = create_meeting(&client, addr, &token).await;

    let resp = enhance(&client, addr, &token, &id, None).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["template"], "standup",
        "mock picks standup from the transcript"
    );
    let enriched = body["enriched_md"].as_str().unwrap();
    assert!(
        !enriched.contains("template:"),
        "marker never reaches the document: {enriched}"
    );
    assert!(
        enriched.contains("- billing"),
        "notes preserved: {enriched}"
    );
    assert_eq!(
        stored_template(&client, addr, &token, &id).await.as_deref(),
        Some("standup")
    );

    // "auto" spelled out is the same as leaving it off.
    let resp = enhance(&client, addr, &token, &id, Some("auto")).await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.json::<serde_json::Value>().await.unwrap()["template"],
        "standup"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_forced_template_wins_over_the_models_pick() {
    let (addr, token, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();
    let id = create_meeting(&client, addr, &token).await;

    let resp = enhance(&client, addr, &token, &id, Some("interview")).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["template"], "interview");
    assert_eq!(
        stored_template(&client, addr, &token, &id).await.as_deref(),
        Some("interview")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_template_is_a_400_before_any_llm_work() {
    let (addr, token, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();
    let id = create_meeting(&client, addr, &token).await;

    let resp = enhance(&client, addr, &token, &id, Some("retro")).await;
    assert_eq!(resp.status(), 400);
    let text = resp.text().await.unwrap();
    assert!(
        text.contains("retro") && text.contains("standup"),
        "names the bad id and the options: {text}"
    );
    assert_eq!(
        stored_template(&client, addr, &token, &id).await,
        None,
        "nothing stamped"
    );
}
