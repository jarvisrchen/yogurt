//! Integration tests for the `/api/settings*` REST surface.
//!
//! `api_responses_never_include_the_raw_api_key` is the security regression for key handling. Never weaken it: any change that lets a raw key escape in an API response must trip its substring assertion.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

use yogurt_server::state::AppState;
use yogurt_server::{session, storage, Mode};

/// Matches only when no `authorization` header is present (wiremock has no built-in absent-header matcher).
struct NoAuthHeader;

impl wiremock::Match for NoAuthHeader {
    fn matches(&self, request: &wiremock::Request) -> bool {
        request.headers.get("authorization").is_none()
    }
}

/// Spin up a fresh in-memory `AppState` and axum server on an ephemeral port. Returns the state, session token, and base URL.
/// The listener is bound before the serve task spawns, so connections queue in the accept backlog and no readiness sleep is needed.
async fn boot() -> (AppState, String, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Leak the tempdir so it lives as long as the spawned server. The OS reclaims it on exit.
    let path = tmp.keep();
    let storage = Arc::new(storage::Storage::init_at(&path.join("db.sqlite")).expect("storage"));
    let session = Arc::new(session::load_or_create(&path.join("session-token")).expect("session"));
    let token = session.as_str().to_string();
    let notes_dir: PathBuf = path.join("notes");

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let state = AppState::in_memory(Mode::Release, storage, session, addr.port(), notes_dir)
        .expect("state");

    let app = yogurt_server::__test_router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (state, token, format!("http://{addr}"))
}

#[tokio::test]
async fn it_lists_seeded_settings_with_no_providers() {
    let (_state, token, base) = boot().await;
    let v: Value = reqwest::Client::new()
        .get(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(v["general"]["port"], 7878);
    assert_eq!(v["providers"].as_array().unwrap().len(), 0);
    assert!(v["presets"].as_array().unwrap().len() >= 5);
}

#[tokio::test]
async fn it_creates_a_provider_and_round_trips_via_get() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({ "name": "Minimax", "base_url": "https://x/v1", "model": "M" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    let listed: Value = client
        .get(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let arr = listed.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["id"], id);
    // No key set: `api_key_masked` must be JSON null, never an empty string, which would imply "key set but empty".
    assert_eq!(arr[0]["api_key_masked"], Value::Null);
}

#[tokio::test]
async fn api_responses_never_include_the_raw_api_key() {
    // Security regression, never weaken: the raw API key must never escape in any API response, only the mask (`••••XXXX`).
    // An `api_key` field on `ProviderView` would fail the `!s.contains(...)` assertion below.
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({ "name": "P", "base_url": "https://x/v1", "model": "m" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = client
        .post(format!("{base}/api/settings/providers/{id}/key"))
        .bearer_auth(&token)
        .json(&json!({ "api_key": "sk-supersecret-XYZA" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let listed: Value = client
        .get(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let s = serde_json::to_string(&listed).unwrap();
    assert!(!s.contains("sk-supersecret-XYZA"), "raw key leaked in: {s}");
    assert!(s.contains("••••XYZA"), "masked key should be present: {s}");

    // The Deepgram key travels a different path (`deepgram_key_masked` on `GET /api/settings`, no `providers` row), so it needs its own proof.
    let resp = client
        .post(format!("{base}/api/settings/stt/key"))
        .bearer_auth(&token)
        .json(&json!({ "api_key": "dg-supersecret-WXYZ" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let general: Value = client
        .get(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let s = serde_json::to_string(&general).unwrap();
    assert!(
        !s.contains("dg-supersecret-WXYZ"),
        "raw STT key leaked in: {s}"
    );
    assert!(
        s.contains("••••WXYZ"),
        "masked STT key should be present: {s}"
    );
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    // Regression: `/api/settings*` writes must require the session token, or an unauthenticated cross-origin POST could repoint the active LLM provider at an attacker `base_url`.
    // A tokenless GET and a tokenless provider-create must both 403.
    let (_state, _token, base) = boot().await;
    let client = reqwest::Client::new();

    let get_status = client
        .get(format!("{base}/api/settings"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(get_status, 403, "tokenless GET /api/settings must 403");

    let post_status = client
        .post(format!("{base}/api/settings/providers"))
        .json(&json!({ "name": "evil", "base_url": "http://attacker/v1", "model": "x" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        post_status, 403,
        "tokenless POST /api/settings/providers must 403"
    );
}

// STT local-model validation: `patch_settings` must reject any PATCH whose effective settings (current row + patch) leave `stt_provider == "local"` on an undownloaded model.
// `"ghost.en"` is not in `yogurt_stt::models::REGISTRY`, so the check stays hermetic on machines with real models downloaded.

#[tokio::test]
async fn patch_to_local_with_undownloaded_model_is_rejected_and_settings_unchanged() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();

    let res = client
        .patch(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .json(&json!({ "stt_provider": "local", "stt_model": "ghost.en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
    let body: Value = res.json().await.unwrap();
    let msg = body["error"].as_str().expect("error field");
    assert!(msg.contains("ghost.en"), "got: {msg}");
    assert!(msg.contains("not downloaded"), "got: {msg}");

    // A rejected PATCH must not write anything.
    let after: Value = client
        .get(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["general"]["stt_provider"], "cloud");
    assert_eq!(after["general"]["stt_model"], "small.en");
}

#[tokio::test]
async fn patch_provider_alone_to_local_is_rejected_when_stored_model_is_undownloaded() {
    // Provider flip alone: the effective model comes from the current row.
    // Seed a name guaranteed not on disk rather than rely on the `small.en` default, which a dev machine may have downloaded.
    let (state, token, base) = boot().await;
    yogurt_db::settings::save_general_patch(
        &state.db,
        yogurt_db::settings::GeneralPatch {
            stt_model: Some("ghost.en".to_string()),
            ..Default::default()
        },
    )
    .expect("seed undownloaded model");

    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .json(&json!({ "stt_provider": "local" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
}

#[tokio::test]
async fn patch_model_alone_is_rejected_when_provider_is_already_local() {
    // Model flip alone while local is active: seed the row via the db layer to bypass the validation under test.
    // The effective provider must come from the stored row.
    let (state, token, base) = boot().await;
    yogurt_db::settings::save_general_patch(
        &state.db,
        yogurt_db::settings::GeneralPatch {
            stt_provider: Some("local".to_string()),
            stt_model: Some("tiny.en".to_string()),
            ..Default::default()
        },
    )
    .expect("seed local settings");

    let client = reqwest::Client::new();
    let res = client
        .patch(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .json(&json!({ "stt_model": "ghost.en" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);

    let after: Value = client
        .get(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after["general"]["stt_model"], "tiny.en");
}

#[tokio::test]
async fn patch_to_cloud_is_always_ok_regardless_of_model_state() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();

    let status = client
        .patch(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .json(&json!({ "stt_provider": "cloud", "stt_model": "ghost.en" }))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status, 200);
}

async fn make_provider(base: &str, token: &str, provider_base_url: &str) -> String {
    let v: Value = reqwest::Client::new()
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(token)
        .json(&json!({
            "name": "Under test",
            "base_url": provider_base_url,
            "model": "gpt-4o-mini",
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v["id"].as_str().expect("provider id").to_string()
}

async fn post_test(base: &str, token: &str, id: &str, body: Value) -> (u16, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/settings/providers/{id}/test"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = resp.status().as_u16();
    // A 404 has a plain-text body, so fall back to Null rather than panicking.
    let v: Value = resp.json().await.unwrap_or(Value::Null);
    (status, v)
}

#[tokio::test]
async fn test_provider_reports_ok_for_a_working_draft_key() {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer sk-draft-not-yet-saved",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "model": "gpt-4o-mini",
            "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
        })))
        .expect(1)
        .mount(&upstream)
        .await;

    let (_state, token, base) = boot().await;
    let id = make_provider(&base, &token, &upstream.uri()).await;

    let (status, v) = post_test(
        &base,
        &token,
        &id,
        json!({ "api_key": "sk-draft-not-yet-saved" }),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(v["ok"], true, "expected a passing test, got {v}");
    assert_eq!(v["model"], "gpt-4o-mini");

    // Testing a draft key must not persist it.
    let settings: Value = reqwest::Client::new()
        .get(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let provider = settings["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id.as_str())
        .expect("provider present");
    assert!(
        provider["api_key_masked"].is_null(),
        "testing a draft key must not store it, got {provider}"
    );
}

/// SECURITY: providers quote the rejected key in error bodies. That text must never reach the client verbatim.
#[tokio::test]
async fn test_provider_reports_failure_without_echoing_the_key() {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/chat/completions"))
        .respond_with(wiremock::ResponseTemplate::new(401).set_body_json(json!({
            "error": { "message": "Incorrect API key provided: sk-bad-key-12345" }
        })))
        .mount(&upstream)
        .await;

    let (_state, token, base) = boot().await;
    let id = make_provider(&base, &token, &upstream.uri()).await;

    let (status, v) = post_test(&base, &token, &id, json!({ "api_key": "sk-bad-key-12345" })).await;
    assert_eq!(status, 200, "a rejected key is still a completed test");
    assert_eq!(v["ok"], false);

    let body = v.to_string();
    assert!(
        !body.contains("sk-bad-key-12345"),
        "raw key leaked into the test result: {body}"
    );
    assert!(
        body.contains("401"),
        "error should keep the provider's status: {body}"
    );
}

#[tokio::test]
async fn test_provider_says_so_when_no_key_is_stored() {
    let (_state, token, base) = boot().await;
    let id = make_provider(&base, &token, "https://example.invalid/v1").await;

    let (status, v) = post_test(&base, &token, &id, json!({})).await;
    assert_eq!(status, 200);
    assert_eq!(v["ok"], false);
    assert!(
        v["error"].as_str().unwrap().contains("No key stored"),
        "got {v}"
    );
}

#[tokio::test]
async fn test_provider_404s_for_an_unknown_id() {
    let (_state, token, base) = boot().await;
    let (status, _) = post_test(
        &base,
        &token,
        "01HZZZZZZZZZZZZZZZZZZZZZZZ",
        json!({ "api_key": "sk-x" }),
    )
    .await;
    assert_eq!(status, 404);
}

/// Regression: the model `Refresh` button must work with a draft API key, so the user can see what a provider offers before saving the key.
#[tokio::test]
async fn list_models_uses_draft_key_when_no_stored_key() {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/models"))
        .and(wiremock::matchers::header(
            "authorization",
            "Bearer sk-draft-models",
        ))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [
                { "id": "gemini-2.5-pro",   "object": "model" },
                { "id": "gemini-2.5-flash", "object": "model" },
                { "id": "gemini-2.0-flash", "object": "model" }
            ]
        })))
        .expect(1)
        .mount(&upstream)
        .await;

    let (_state, token, base) = boot().await;
    let id = make_provider(&base, &token, &upstream.uri()).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/api/settings/providers/{id}/models"))
        .bearer_auth(&token)
        .json(&json!({ "api_key": "sk-draft-models" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let models: Vec<String> = resp.json().await.unwrap();
    assert_eq!(
        models,
        vec![
            "gemini-2.5-pro".to_string(),
            "gemini-2.5-flash".to_string(),
            "gemini-2.0-flash".to_string()
        ]
    );

    // Probing with a draft must not store the key.
    let settings: Value = reqwest::Client::new()
        .get(format!("{base}/api/settings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let provider = settings["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id.as_str())
        .expect("provider present");
    assert!(
        provider["api_key_masked"].is_null(),
        "listing models with a draft key must not store it, got {provider}"
    );
}

/// No key, draft or stored, is not an error: local runtimes (Ollama, LM Studio) need none, so the probe proceeds keyless.
/// It must reach upstream with no `authorization` header.
#[tokio::test]
async fn list_models_without_any_key_probes_with_no_auth_header() {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/models"))
        .and(NoAuthHeader)
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
            "object": "list",
            "data": [{ "id": "llama3.2", "object": "model" }]
        })))
        .expect(1)
        .mount(&upstream)
        .await;

    let (_state, token, base) = boot().await;
    let id = make_provider(&base, &token, &upstream.uri()).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/api/settings/providers/{id}/models"))
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let models: Vec<String> = resp.json().await.unwrap();
    assert_eq!(models, vec!["llama3.2".to_string()]);
}

// `POST /api/settings/stt/test` probes the real `api.deepgram.com`, with no local redirect for the 2xx/401 paths.
// This covers the no-key-stored path and the route wiring.
#[tokio::test]
async fn test_stt_key_says_so_when_no_key_is_stored() {
    let (_state, token, base) = boot().await;
    let resp = reqwest::Client::new()
        .post(format!("{base}/api/settings/stt/test"))
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let v: Value = resp.json().await.unwrap();
    assert_eq!(v["ok"], false);
    assert_eq!(
        v["error"],
        "No Deepgram key stored yet - paste one above, then test."
    );
}

/// SECURITY: an upstream 401 must surface as our 502 with a JSON `{"error": ...}` body that never echoes the draft key, even if the provider quotes it.
#[tokio::test]
async fn list_models_upstream_401_surfaces_as_502_without_the_key() {
    let upstream = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/models"))
        .respond_with(wiremock::ResponseTemplate::new(401).set_body_json(json!({
            "error": { "message": "Incorrect API key provided: sk-draft-bad-99999" }
        })))
        .mount(&upstream)
        .await;

    let (_state, token, base) = boot().await;
    let id = make_provider(&base, &token, &upstream.uri()).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/api/settings/providers/{id}/models"))
        .bearer_auth(&token)
        .json(&json!({ "api_key": "sk-draft-bad-99999" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 502);
    let body: Value = resp.json().await.unwrap();
    let msg = body["error"].as_str().expect("error field");
    assert!(
        !msg.contains("sk-draft-bad-99999"),
        "raw key leaked in 502 body: {msg}"
    );
}

#[tokio::test]
async fn creates_a_cli_provider_and_round_trips_its_adapter() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({
            "name": "Claude Code (local CLI)",
            "base_url": "",
            "model": "claude",
            "adapter": "cli"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(created["adapter"], "cli");
    assert_eq!(created["model"], "claude");
    // A `cli` provider never has a key: the row must round-trip with no masked value.
    assert_eq!(created["api_key_masked"], Value::Null);

    let listed: Value = client
        .get(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed[0]["adapter"], "cli");
}

/// The built-in CLI presets round-trip through `GET /api/settings` and `GET /api/settings/presets` with `adapter: "cli"` in both separately built `PresetView` mappings.
#[tokio::test]
async fn cli_presets_are_advertised_with_their_adapter() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();

    for path in ["/api/settings", "/api/settings/presets"] {
        let v: Value = client
            .get(format!("{base}{path}"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let presets = if path == "/api/settings" {
            v["presets"].as_array().unwrap().clone()
        } else {
            v.as_array().unwrap().clone()
        };
        let cli_presets: Vec<&Value> = presets.iter().filter(|p| p["adapter"] == "cli").collect();
        assert_eq!(cli_presets.len(), 3, "{path}: expected 3 cli presets");
        for p in cli_presets {
            assert_eq!(p["base_url"], "");
        }
    }
}

#[tokio::test]
async fn rejects_a_cli_provider_with_an_unrecognized_program() {
    let (_state, token, base) = boot().await;
    let res = reqwest::Client::new()
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({
            "name": "Bogus",
            "base_url": "",
            "model": "not-a-real-cli-program",
            "adapter": "cli"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
    let body: Value = res.json().await.unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("not-a-real-cli-program"));
}

#[tokio::test]
async fn rejects_an_unrecognized_adapter_value() {
    let (_state, token, base) = boot().await;
    let res = reqwest::Client::new()
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({
            "name": "Bogus",
            "base_url": "https://x/v1",
            "model": "m",
            "adapter": "ssh"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
}

/// `claude` has no model-list mode, so its four aliases ship as static preset suggestions.
/// Refresh returns 422, never 502, so the UI can tell it from a genuine list-models failure.
#[tokio::test]
async fn claude_cli_provider_has_no_model_catalog_to_refresh() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({
            "name": "Claude Code (local CLI)",
            "base_url": "",
            "model": "claude",
            "adapter": "cli"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let res = client
        .post(format!("{base}/api/settings/providers/{id}/models"))
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 422);
}

/// `POST .../test` on a `cli` provider never asks for a key. It either finds the program on `$PATH` and reports a real completion, or reports `ok: false` naming the program.
/// Both are asserted so the test holds whether or not `claude` is installed.
#[tokio::test]
async fn tests_a_cli_provider_without_ever_asking_for_a_key() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({
            "name": "Claude Code (local CLI)",
            "base_url": "",
            "model": "claude",
            "adapter": "cli"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let res: Value = client
        .post(format!("{base}/api/settings/providers/{id}/test"))
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    match res["ok"].as_bool().unwrap() {
        true => assert_eq!(res["model"], "cli:claude"),
        false => assert!(res["error"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("claude")),
    }
}

/// `POST .../test` on a `cli` provider with a `cli_model` override echoes it in the verdict's `model` field, so the UI names the model actually exercised.
#[tokio::test]
async fn tests_a_cli_provider_echoing_the_model_override_it_was_tested_with() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();
    let created: Value = client
        .post(format!("{base}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&json!({
            "name": "Claude Code (local CLI)",
            "base_url": "",
            "model": "claude",
            "adapter": "cli",
            "cli_model": "haiku"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let res: Value = client
        .post(format!("{base}/api/settings/providers/{id}/test"))
        .bearer_auth(&token)
        .json(&json!({}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    match res["ok"].as_bool().unwrap() {
        true => assert_eq!(res["model"], "cli:claude:haiku"),
        false => assert!(res["error"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("claude")),
    }
}
