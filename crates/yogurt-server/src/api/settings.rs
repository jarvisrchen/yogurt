//! `/api/settings*` REST surface - Phase 5 Plan 05-03.
//!
//! Mounts the provider CRUD + activate + key-set + general-settings + presets
//! endpoints. **Load-bearing security invariant:** API responses NEVER
//! include the raw API key. Only the canonical mask (`••••XXXX`, last 4
//! chars) is exposed via [`ProviderView::api_key_masked`]. The regression
//! test that enforces this lives at
//! `crates/yogurt-server/tests/settings_api.rs::api_responses_never_include_the_raw_api_key`
//! - never weaken it.
//!
//! ## Endpoint map
//!
//! | Method | Path                                       | Handler               |
//! |--------|--------------------------------------------|-----------------------|
//! | GET    | `/api/settings`                            | `get_settings`        |
//! | PATCH  | `/api/settings`                            | `patch_settings`      |
//! | GET    | `/api/settings/providers`                  | `list_providers`      |
//! | POST   | `/api/settings/providers`                  | `create_provider`     |
//! | PATCH  | `/api/settings/providers/{id}`             | `update_provider`     |
//! | DELETE | `/api/settings/providers/{id}`             | `delete_provider`     |
//! | POST   | `/api/settings/providers/{id}/activate`    | `activate_provider`   |
//! | POST   | `/api/settings/providers/{id}/key`         | `set_provider_key`    |
//! | POST   | `/api/settings/providers/{id}/test`        | `test_provider`       |
//! | POST   | `/api/settings/providers/{id}/models`      | `list_provider_models`|
//! | GET    | `/api/settings/presets`                    | `list_presets`        |
//!
//! ## axum 0.8 path syntax
//!
//! This crate is on axum 0.8 (matches the rest of `routes.rs`), so path
//! params use `{id}` not `:id`. The plan's superpowers source was written
//! against 0.7 - the route shapes still match conceptually.

use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use yogurt_db::{providers, settings};

/// Build the `Router<AppState>` containing every `/api/settings*` route.
///
/// Caller is responsible for `.with_state(state)` and for merging into the
/// top-level router. See `crate::routes::router` for the call site.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/settings", get(get_settings).patch(patch_settings))
        .route(
            "/api/settings/providers",
            get(list_providers).post(create_provider),
        )
        .route(
            "/api/settings/providers/{id}",
            axum::routing::patch(update_provider).delete(delete_provider),
        )
        .route(
            "/api/settings/providers/{id}/activate",
            post(activate_provider),
        )
        .route("/api/settings/providers/{id}/key", post(set_provider_key))
        .route("/api/settings/providers/{id}/test", post(test_provider))
        .route(
            "/api/settings/providers/{id}/models",
            post(list_provider_models),
        )
        .route("/api/settings/stt/key", post(set_stt_key))
        .route("/api/settings/presets", get(list_presets))
}

// ─── Provider serialization (NO API KEY EVER) ────────────────────────────────

/// Wire-format view of a provider row. **Intentionally has no `api_key`
/// field.** The stored secret is exposed only via
/// [`Self::api_key_masked`] (`"••••XXXX"` or `null`). The load-bearing
/// regression test
/// `api_responses_never_include_the_raw_api_key` enforces that no handler
/// ever leaks the raw key.
#[derive(Serialize)]
struct ProviderView {
    id: String,
    name: String,
    base_url: String,
    model: String,
    is_active: bool,
    created_at: i64,
    /// `Some("••••XXXX")` when a key is stored, `None` otherwise. Never the raw key.
    api_key_masked: Option<String>,
}

/// Convert a DB row + its masked key into the wire view.
fn to_view(p: providers::Provider, masked: Option<String>) -> ProviderView {
    ProviderView {
        id: p.id,
        name: p.name,
        base_url: p.base_url,
        model: p.model,
        is_active: p.is_active,
        created_at: p.created_at,
        api_key_masked: masked,
    }
}

fn masked_views(state: &AppState, rows: Vec<providers::Provider>) -> Vec<ProviderView> {
    rows.into_iter()
        .map(|p| {
            let masked = state.keys.masked(&p.id).ok().flatten();
            to_view(p, masked)
        })
        .collect()
}

#[derive(Serialize)]
struct PresetView {
    name: &'static str,
    base_url: &'static str,
    default_model: &'static str,
    models: &'static [&'static str],
    docs_url: &'static str,
}

#[derive(Serialize)]
struct SettingsView {
    general: settings::General,
    providers: Vec<ProviderView>,
    presets: Vec<PresetView>,
    /// `Some("••••XXXX")` when a Deepgram STT key is stored (key-file entry
    /// `stt-deepgram`), `None` otherwise. Never the raw key.
    deepgram_key_masked: Option<String>,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn get_settings(State(s): State<AppState>) -> Result<Json<SettingsView>, Error> {
    let general = settings::load_general(&s.db)?;
    let presets = providers::PRESETS
        .iter()
        .map(|p| PresetView {
            name: p.name,
            base_url: p.base_url,
            default_model: p.default_model,
            models: p.models,
            docs_url: p.docs_url,
        })
        .collect();
    let providers = masked_views(&s, providers::list(&s.db)?);
    let deepgram_key_masked = s
        .keys
        .masked(crate::meetings::DEEPGRAM_KEY_ID)
        .ok()
        .flatten();
    Ok(Json(SettingsView {
        general,
        providers,
        presets,
        deepgram_key_masked,
    }))
}

/// Reject a PATCH that would leave `stt_provider == "local"` pointed at a
/// model that isn't actually on disk. Settings only take effect at the
/// *next* recording start (`Registry::start` reads them fresh via
/// `select_stt`), so persisting an undownloaded model here is a lie the
/// UI would show immediately, not just a future footgun - exactly the
/// bug this handler exists to prevent.
///
/// Checks the EFFECTIVE settings (current row with `patch` overlaid), so
/// it catches all three PATCH shapes: provider flips to local while the
/// stored model is undownloaded, model flips while local is already
/// active, or both fields arrive in the same PATCH.
async fn validate_stt_patch(
    db: &yogurt_db::Db,
    patch: &settings::GeneralPatch,
) -> Result<(), Error> {
    let current = settings::load_general(db)?;
    let effective_provider = patch
        .stt_provider
        .as_deref()
        .unwrap_or(&current.stt_provider);
    if effective_provider != "local" {
        return Ok(());
    }
    let effective_model = patch
        .stt_model
        .clone()
        .unwrap_or_else(|| current.stt_model.clone());

    // `is_downloaded` can fall back to hashing a multi-GB file on a
    // legacy/stale marker (see yogurt_stt::models docs) - keep it off
    // the tokio reactor, same as `select_stt` does at meeting start.
    let downloaded = tokio::task::spawn_blocking({
        let model = effective_model.clone();
        move || {
            yogurt_stt::models::lookup(&model)
                .map(yogurt_stt::models::is_downloaded)
                .unwrap_or(false)
        }
    })
    .await
    .map_err(|e| Error::Internal(format!("join is_downloaded check: {e}")))?;

    if downloaded {
        Ok(())
    } else {
        Err(Error::Unprocessable(format!(
            "local model {effective_model} is not downloaded - download it in \
             Settings > Transcription first"
        )))
    }
}

async fn patch_settings(
    State(s): State<AppState>,
    Json(patch): Json<settings::GeneralPatch>,
) -> Result<Json<settings::General>, Error> {
    validate_stt_patch(&s.db, &patch).await?;
    Ok(Json(settings::save_general_patch(&s.db, patch)?))
}

async fn list_providers(State(s): State<AppState>) -> Result<Json<Vec<ProviderView>>, Error> {
    Ok(Json(masked_views(&s, providers::list(&s.db)?)))
}

async fn list_presets() -> Json<Vec<PresetView>> {
    Json(
        providers::PRESETS
            .iter()
            .map(|p| PresetView {
                name: p.name,
                base_url: p.base_url,
                default_model: p.default_model,
                models: p.models,
                docs_url: p.docs_url,
            })
            .collect(),
    )
}

async fn create_provider(
    State(s): State<AppState>,
    Json(body): Json<providers::NewProvider>,
) -> Result<Json<ProviderView>, Error> {
    let id = providers::insert(&s.db, body)?;
    let p = providers::list(&s.db)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| Error::Internal("inserted provider missing".into()))?;
    let view = masked_views(&s, vec![p]).remove(0);
    Ok(Json(view))
}

/// Body for `PATCH /api/settings/providers/{id}`. All three fields must be
/// supplied (full replace semantics) - partial updates are a future
/// enhancement once the UI grows per-field "Save" buttons.
#[derive(Deserialize)]
struct UpdateProviderBody {
    name: String,
    base_url: String,
    model: String,
}

async fn update_provider(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateProviderBody>,
) -> Result<Json<ProviderView>, Error> {
    providers::update(&s.db, &id, &body.name, &body.base_url, &body.model)?;
    let p = providers::list(&s.db)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or(Error::NotFound)?;
    let view = masked_views(&s, vec![p]).remove(0);
    Ok(Json(view))
}

async fn delete_provider(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    // Clean up the stored key FIRST. This is best-effort - any error is
    // swallowed since the DB row deletion is the canonical "remove
    // provider" intent. A stale key entry without a DB row is harmless.
    let _ = s.keys.delete(&id);
    providers::delete(&s.db, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn activate_provider(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ProviderView>, Error> {
    providers::set_active(&s.db, &id)?;
    let p = providers::active(&s.db)?
        .ok_or_else(|| Error::Internal("active provider missing after set_active".into()))?;
    let view = masked_views(&s, vec![p]).remove(0);
    Ok(Json(view))
}

/// Body for `POST /api/settings/providers/{id}/key`. The `api_key` field is
/// the only place a raw secret ever appears in this module - and it is
/// inbound only. It is never echoed back, persisted in SQLite, or logged.
#[derive(Deserialize)]
struct SetKeyBody {
    api_key: String,
}

/// `POST /api/settings/stt/key` - store the Deepgram STT key. The STT key
/// has no `providers` table row; the key-file entry under
/// [`crate::meetings::DEEPGRAM_KEY_ID`] IS the configuration. Cloud
/// recording reads it at meeting start.
async fn set_stt_key(
    State(s): State<AppState>,
    Json(body): Json<SetKeyBody>,
) -> Result<StatusCode, Error> {
    let key = body.api_key.trim();
    if key.is_empty() {
        return Err(Error::Internal("api_key must not be empty".into()));
    }
    s.keys
        .set(crate::meetings::DEEPGRAM_KEY_ID, key)
        .map_err(|e| Error::Internal(format!("key store set: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_provider_key(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetKeyBody>,
) -> Result<StatusCode, Error> {
    // Verify the provider exists before touching the key store. Without
    // this, a typo'd ID would leak an orphan key entry on every
    // failed attempt.
    if providers::list(&s.db)?.iter().all(|p| p.id != id) {
        return Err(Error::NotFound);
    }
    s.keys
        .set(&id, &body.api_key)
        .map_err(|e| Error::Internal(format!("key store set: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Connection test ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TestProviderBody {
    /// Draft key to test WITHOUT storing it. `None`/empty means "test the
    /// key already in the key store". This is the whole point of the
    /// endpoint: the user can verify a pasted key before committing it.
    #[serde(default)]
    api_key: Option<String>,
}

/// Outcome of a live round-trip against the provider.
///
/// A rejected key is a SUCCESSFUL request - it answers the user's question.
/// So the happy and unhappy paths both return 200 with `ok` discriminating;
/// non-200 is reserved for "the test itself could not be run" (unknown
/// provider id).
#[derive(Serialize)]
struct TestProviderResult {
    ok: bool,
    /// Model name the provider echoed back, which is not always the one we
    /// asked for (aliases, `-latest` tags, OpenRouter routing).
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Longest error text handed back to the UI. Provider error bodies can be
/// multi-KB JSON blobs; the actionable part is always at the front.
const MAX_TEST_ERROR_LEN: usize = 400;

/// `POST /api/settings/providers/{id}/test` - one real chat completion
/// against this provider, reporting whether the key works.
///
/// The draft key travels in the request body and is used to build a
/// throwaway client. It is NEVER written to the key file, never logged,
/// and never echoed back: `OpenAiCompatClient` scrubs it out of the
/// provider's own error text, because providers routinely quote the
/// offending key back at you ("Incorrect API key provided: sk-abc123").
/// That would otherwise smuggle a raw key into an API response and
/// violate the invariant `api_responses_never_include_the_raw_api_key`
/// enforces.
async fn test_provider(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TestProviderBody>,
) -> Result<Json<TestProviderResult>, Error> {
    let (provider, key) = provider_and_key(&s, &id, body.api_key)?;
    let key = match key {
        Some(k) => k,
        None => {
            return Ok(Json(TestProviderResult {
                ok: false,
                model: None,
                error: Some(
                    "No key stored for this provider yet - paste one above, \
                     then test."
                        .into(),
                ),
            }))
        }
    };

    // ponytail: a plain one-turn completion rather than a `/models` probe.
    // It exercises the exact path enhance and chat use, so it also catches a
    // wrong model name and a base URL that answers but is not OpenAI-shaped
    // - which `/models` would both miss. No `max_tokens` cap: some reasoning
    // models reject a tiny budget outright, and a false failure here is
    // worse than a handful of tokens.
    // `complete` lives on the LlmClient trait, not the concrete type.
    use yogurt_llm::LlmClient as _;
    let client = crate::llm_openai::OpenAiCompatClient::new(
        provider.base_url.clone(),
        key,
        provider.model.clone(),
    );
    let req = yogurt_llm::ChatRequest {
        messages: vec![yogurt_llm::ChatMessage::user("Reply with: ok")],
        stream: false,
    };

    Ok(Json(match client.complete(req).await {
        Ok(resp) => TestProviderResult {
            ok: true,
            model: Some(resp.model),
            error: None,
        },
        Err(e) => TestProviderResult {
            ok: false,
            model: None,
            error: Some(truncate(&format!("{e:#}"), MAX_TEST_ERROR_LEN)),
        },
    }))
}

/// Optional `POST /api/settings/providers/{id}/models` body. When present,
/// the draft `api_key` is used in place of the stored key so
/// the user can discover what models are available *before* committing
/// the key - useful when the saved `model` is the only thing wrong with
/// the provider (e.g. Google's frequent deprecations).
#[derive(Deserialize)]
struct ListModelsBody {
    #[serde(default)]
    api_key: Option<String>,
}

/// `POST /api/settings/providers/{id}/models` - probe the provider's
/// `/v1/models` endpoint and return its model list.
///
/// The Settings UI's `Refresh` button hits this. The handler prefers a
/// draft `api_key` from the request body over the stored key
/// so the button is useful on a freshly-cloned provider (the whole point:
/// when the saved `model` is deprecated, the user needs to see what is
/// actually available before they can pick a replacement). The draft
/// never reaches the key file - it's used once for the probe and
/// discarded.
///
/// No key at all (neither draft nor stored) is not an error here: local
/// runtimes (Ollama, LM Studio) need no key, so the probe proceeds with an
/// empty one. A provider that does need a key simply 401s, which surfaces
/// to the UI as a normal inline error.
///
/// A missing-or-wrong key surfaces as a normal upstream HTTP error, mapped
/// to `Error::BadGateway` since it's the upstream rejecting us, not our own
/// bug. `OpenAiCompatClient` scrubs the raw secret out of the message so a
/// misconfigured provider can't smuggle it back through the response.
async fn list_provider_models(
    State(s): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<ListModelsBody>>,
) -> Result<Json<Vec<String>>, Error> {
    let draft = body.and_then(|b| b.api_key.clone());
    let (provider, key) = provider_and_key(&s, &id, draft)?;

    let client = crate::llm_openai::OpenAiCompatClient::new(
        provider.base_url.clone(),
        key.unwrap_or_default(),
        provider.model.clone(),
    );
    match client.list_models().await {
        Ok(models) => Ok(Json(models)),
        Err(e) => Err(Error::BadGateway(truncate(
            &format!("{e:#}"),
            MAX_TEST_ERROR_LEN,
        ))),
    }
}

/// Resolve the provider row for `id` plus the API key a live probe should
/// use: a trimmed non-empty `draft` wins, otherwise whatever is already in
/// the key store. `None` means neither is available - callers decide what
/// that means (`test_provider` reports a friendly "no key stored" result;
/// `list_provider_models` proceeds keyless, since local runtimes need no
/// key at all).
fn provider_and_key(
    s: &AppState,
    id: &str,
    draft: Option<String>,
) -> Result<(providers::Provider, Option<String>), Error> {
    let provider = providers::list(&s.db)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or(Error::NotFound)?;

    let draft = draft
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    let key = match draft {
        Some(k) => Some(k),
        None => read_stored_key(s, id)?,
    };
    Ok((provider, key))
}

fn read_stored_key(s: &AppState, id: &str) -> Result<Option<String>, Error> {
    s.keys
        .get(id)
        .map_err(|e| Error::Internal(format!("key store read: {e}")))
}

/// Truncate on a char boundary, appending an ellipsis when cut.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}…")
}

// ─── Error mapping ───────────────────────────────────────────────────────────

#[derive(Debug)]
enum Error {
    NotFound,
    Internal(String),
    /// 422 - the patch is well-formed JSON but describes an invalid
    /// configuration (e.g. local STT pointed at an undownloaded model).
    Unprocessable(String),
    /// 502 - a live probe against the provider's own endpoint failed
    /// (upstream rejected the key, refused the connection, or timed out).
    /// This is the *provider's* fault, not ours, so it's distinct from
    /// `Internal`.
    BadGateway(String),
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(format!("{e:#}"))
    }
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(format!("{e}"))
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Error::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
            Error::Internal(s) => (StatusCode::INTERNAL_SERVER_ERROR, s).into_response(),
            Error::Unprocessable(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "error": msg })),
            )
                .into_response(),
            Error::BadGateway(msg) => (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": msg })),
            )
                .into_response(),
        }
    }
}
