//! Integration tests for the `/api/settings*` REST surface — Phase 5 Plan 05-03.
//!
//! The third test (`api_responses_never_include_the_raw_api_key`) is the
//! load-bearing security regression for the entire phase. **Never weaken
//! it.** Any change that lets a raw key escape the server in an API
//! response will trip the substring assertion below.

use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;

use yogurt_server::state::AppState;
use yogurt_server::{session, storage, Mode};

/// Matches only when no `authorization` header is present at all. Used to
/// assert the keyless `/models` probe (local runtimes need no key)
/// reaches upstream with no auth header - wiremock has no built-in
/// "header absent" matcher.
struct NoAuthHeader;

impl wiremock::Match for NoAuthHeader {
    fn matches(&self, request: &wiremock::Request) -> bool {
        request.headers.get("authorization").is_none()
    }
}

/// Spin up a fresh in-memory `AppState` and start an axum server on an
/// ephemeral kernel-assigned port. Returns the state handle (mostly
/// dropped), the session token, and the `http://127.0.0.1:{port}` base
/// URL to hit.
///
/// The listener is bound *before* the serve task is spawned, so incoming
/// connections queue in the accept backlog — no readiness sleep needed.
async fn boot() -> (AppState, String, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Keep the tempdir alive for the duration of the spawned server by
    // leaking it intentionally — this is a short-lived integration test
    // and the OS will reclaim on process exit.
    let path = tmp.keep();
    let storage = Arc::new(storage::Storage::init_at(&path.join("db.sqlite")).expect("storage"));
    let session = Arc::new(session::load_or_create(&path.join("session-token")).expect("session"));
    // The `/api/settings*` routes require the session token (hardened
    // 2026-08-13). Read it out so each request below can present it.
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
    // No key has been set yet — api_key_masked must be JSON null, NEVER
    // an empty string (an empty string would imply "key set but empty",
    // which would round-trip a key-not-present state as a different shape
    // to the frontend).
    assert_eq!(arr[0]["api_key_masked"], Value::Null);
}

#[tokio::test]
async fn api_responses_never_include_the_raw_api_key() {
    // ─── Load-bearing security regression. NEVER WEAKEN. ───────────────
    // This test is the canonical proof that the raw API key never escapes
    // the server in any API response — only the canonical mask
    // (`••••XXXX`) is exposed. Any future refactor that adds an `api_key`
    // field to `ProviderView` (or otherwise echoes the raw secret) will
    // fail the `!s.contains("sk-supersecret-XYZA")` assertion below.
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

    // The Deepgram STT key travels a different path to a different field
    // (`deepgram_key_masked` on `GET /api/settings`, no `providers` row),
    // so it needs its own proof. Same rule: mask out, raw never.
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
    // Hardening regression (2026-08-13): `/api/settings*` writes were once
    // reachable with no token — an unauthenticated cross-origin POST could
    // repoint the active LLM provider at an attacker `base_url`. Lock it:
    // a tokenless GET and a tokenless provider-create must both 403.
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

// ─── STT local-model validation (server-side lie guard) ─────────────────────
//
// Settings previously let `stt_provider = "local"` persist against a model
// that was never downloaded — the UI then showed "local/medium.en" with
// nothing in `~/.yogurt/models`. Recording "worked" only because an
// already-running cloud session ignores mid-recording settings changes
// (settings apply at the *next* start). `patch_settings` now rejects any
// PATCH whose EFFECTIVE settings (current row + patch overlaid) would
// leave `stt_provider == "local"` pointed at an undownloaded model.
//
// `"ghost.en"` mirrors `meetings.rs::rejects_local_when_model_missing` —
// a name that is not in `yogurt_stt::models::REGISTRY` at all, so the
// check is hermetic: it can never accidentally pass because some engineer's
// machine happens to have a real model already downloaded.

#[tokio::test]
async fn patch_to_local_with_undownloaded_model_is_rejected_and_settings_unchanged() {
    let (_state, token, base) = boot().await;
    let client = reqwest::Client::new();

    // Both fields in one PATCH: provider flips to local AND the model is
    // bogus/undownloaded at the same time.
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

    // Rejected PATCH must not have written anything — GET still shows the
    // V005-seeded defaults.
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
    // Provider flip alone: the PATCH doesn't mention `stt_model` at all, so
    // the effective model comes from the CURRENT row. Seed it to a name
    // that's guaranteed not to be on disk (rather than relying on the
    // V005 default `"small.en"`, which a dev machine that's actually used
    // local STT before may well have downloaded already — that ambient
    // state would make this test flaky).
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
    // Model flip alone while local is already active: seed the row
    // directly via the db layer (bypassing the HTTP validation this test
    // exercises) so the PATCH under test carries only `stt_model`. The
    // effective provider ("local") must come from the stored row, not the
    // patch body.
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
    // Unchanged from the seed — the rejected PATCH did not overwrite it.
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

// ─── POST /api/settings/providers/{id}/test ──────────────────────────────────

/// Create a provider pointed at `base_url` and return its id.
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

    // The draft key must NOT have been persisted as a side effect of
    // testing it — testing is not saving.
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

/// SECURITY: providers routinely quote the rejected key back inside their
/// error body. That text must never reach the client verbatim — it would
/// smuggle a raw key into an API response.
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

    // No draft key and nothing in the key store.
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

/// REGRESSION: the MODEL `Refresh` button on the Settings page must
/// work with a draft API key (the user hasn't saved the key yet, but
/// they want to see what models the provider offers so they can pick
/// one before clicking `Save key`). The whole point: when the saved
/// `model` is the only thing wrong with the provider (e.g. Google's
/// frequent deprecations), the user needs to discover what's available
/// before they can choose a replacement.
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

    // Side-effect-free: probing with a draft must not store the key.
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

/// No key at all - neither draft nor stored - must NOT be treated as an
/// error: local runtimes (Ollama, LM Studio) need no key at all, so the
/// probe proceeds keyless. The request must reach the upstream with no
/// `authorization` header, and a provider that's happy to answer without
/// one (as a local runtime would) returns its model list normally.
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

// ─── STT key test ────────────────────────────────────────────────────────────
//
// `POST /api/settings/stt/test` probes the real `api.deepgram.com`, so
// unlike `test_provider` (which points at a `wiremock` server via the
// provider's own `base_url`) there is no way to redirect the 2xx/401 paths
// to a local mock without adding a mocking dependency this file doesn't
// already use, or making the probe URL configurable (out of scope - see
// the task contract). This test covers what's fully local: the
// no-key-stored path and the route wiring.
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

/// SECURITY: an upstream 401 must surface as our 502 (the provider is at
/// fault, not us), with a JSON `{"error": ...}` body that never echoes the
/// draft key back - even in the error text a misconfigured provider might
/// quote it into.
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

// ── LLM-4: explicit CLI provider selection ──────────────────────────────

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
    // A `cli` provider never has a key - the row must round-trip with no
    // masked value even though nothing was ever posted to `/key`.
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

/// The two built-in CLI presets round-trip through `GET /api/settings` and
/// `GET /api/settings/presets` with `adapter: "cli"` - the exact bug that
/// briefly existed when `get_settings` and `list_presets` each built their
/// own `PresetView` mapping and only one of them got the field added.
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
        assert_eq!(cli_presets.len(), 2, "{path}: expected 2 cli presets");
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

#[tokio::test]
async fn cli_provider_has_no_model_catalog_to_refresh() {
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

/// `POST .../test` on a `cli` provider never asks for a key - it either
/// finds the named program on `$PATH` and reports a real completion, or
/// reports `ok: false` naming the program. Both are asserted so the test
/// is meaningful whether or not `claude` happens to be installed on the
/// machine running it (mine has it; CI does not).
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

/// `POST .../test` on a `cli` provider with a `cli_model` override echoes
/// it back in the verdict's `model` field, so the Settings UI's "answered
/// as …" text names the specific model that was actually exercised - not
/// just the CLI program - which is the whole point of letting the user
/// pin one in the first place.
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
