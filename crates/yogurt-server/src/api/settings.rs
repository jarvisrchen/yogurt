//! `/api/settings*` REST surface — Phase 5 Plan 05-03.
//!
//! Mounts the provider CRUD + activate + key-set + general-settings + presets
//! endpoints. **Load-bearing security invariant:** API responses NEVER
//! include the raw API key. Only the canonical mask (`••••XXXX`, last 4
//! chars) is exposed via [`ProviderView::api_key_masked`]. The regression
//! test that enforces this lives at
//! `crates/yogurt-server/tests/settings_api.rs::api_responses_never_include_the_raw_api_key`
//! — never weaken it.
//!
//! ## Endpoint map
//!
//! | Method | Path                                       | Handler            |
//! |--------|--------------------------------------------|--------------------|
//! | GET    | `/api/settings`                            | `get_settings`     |
//! | PATCH  | `/api/settings`                            | `patch_settings`   |
//! | GET    | `/api/settings/providers`                  | `list_providers`   |
//! | POST   | `/api/settings/providers`                  | `create_provider`  |
//! | PATCH  | `/api/settings/providers/{id}`             | `update_provider`  |
//! | DELETE | `/api/settings/providers/{id}`             | `delete_provider`  |
//! | POST   | `/api/settings/providers/{id}/activate`    | `activate_provider`|
//! | POST   | `/api/settings/providers/{id}/key`         | `set_provider_key` |
//! | GET    | `/api/settings/presets`                    | `list_presets`     |
//!
//! ## axum 0.8 path syntax
//!
//! This crate is on axum 0.8 (matches the rest of `routes.rs`), so path
//! params use `{id}` not `:id`. The plan's superpowers source was written
//! against 0.7 — the route shapes still match conceptually.

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
        .route("/api/settings/stt/key", post(set_stt_key))
        .route("/api/settings/presets", get(list_presets))
}

// ─── Provider serialization (NO API KEY EVER) ────────────────────────────────

/// Wire-format view of a provider row. **Intentionally has no `api_key`
/// field.** The Keychain-resident secret is exposed only via
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

/// Convert a DB row + its pre-fetched masked key into the wire view.
/// The masked form comes from [`masked_views`] so the (potentially
/// blocking) Keychain reads stay off the tokio reactor (SET-10).
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

/// Fetch the masked key for each provider row on a blocking thread.
/// SET-10: `ApiKeyStore` reads can stall for seconds on a cold Keychain
/// (user prompt on first access) — never run them on the reactor.
async fn masked_views(
    state: &AppState,
    rows: Vec<providers::Provider>,
) -> Result<Vec<ProviderView>, Error> {
    let keys = state.keys.clone();
    // Bounded at 5s: a wedged Keychain (unanswered macOS access prompt
    // after a binary rebuild) must degrade to "no mask shown" — not hang
    // the whole Settings page on its loading skeleton.
    let fallback: Vec<providers::Provider> = rows.clone();
    match tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || {
            rows.into_iter()
                .map(|p| {
                    let masked = keys.masked(&p.id).ok().flatten();
                    to_view(p, masked)
                })
                .collect()
        }),
    )
    .await
    {
        Ok(joined) => joined.map_err(|e| Error::Internal(format!("keychain task join: {e}"))),
        Err(_) => {
            tracing::warn!(
                "keychain did not respond within 5s while masking provider keys — \
                 serving settings without key masks"
            );
            Ok(fallback.into_iter().map(|p| to_view(p, None)).collect())
        }
    }
}

#[derive(Serialize)]
struct PresetView {
    name: &'static str,
    base_url: &'static str,
    default_model: &'static str,
}

#[derive(Serialize)]
struct SettingsView {
    general: settings::General,
    providers: Vec<ProviderView>,
    presets: Vec<PresetView>,
    /// `Some("••••XXXX")` when a Deepgram STT key is stored (Keychain entry
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
        })
        .collect();
    let keys = s.keys.clone();
    // Same 5s bound as `masked_views` — see the wedged-Keychain note there.
    // Run both Keychain lookups concurrently so a wedged Keychain costs
    // one 5s window, not two in sequence.
    let deepgram_fut = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::task::spawn_blocking(move || keys.masked(crate::meetings::DEEPGRAM_KEY_ID)),
    );
    let (providers, deepgram_res) =
        tokio::join!(masked_views(&s, providers::list(&s.db)?), deepgram_fut);
    let providers = providers?;
    let deepgram_key_masked = deepgram_res
        .ok()
        .and_then(|j| j.ok())
        .and_then(|r| r.ok())
        .flatten();
    Ok(Json(SettingsView {
        general,
        providers,
        presets,
        deepgram_key_masked,
    }))
}

async fn patch_settings(
    State(s): State<AppState>,
    Json(patch): Json<settings::GeneralPatch>,
) -> Result<Json<settings::General>, Error> {
    Ok(Json(settings::save_general_patch(&s.db, patch)?))
}

async fn list_providers(State(s): State<AppState>) -> Result<Json<Vec<ProviderView>>, Error> {
    Ok(Json(masked_views(&s, providers::list(&s.db)?).await?))
}

async fn list_presets() -> Json<Vec<PresetView>> {
    Json(
        providers::PRESETS
            .iter()
            .map(|p| PresetView {
                name: p.name,
                base_url: p.base_url,
                default_model: p.default_model,
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
    let view = masked_views(&s, vec![p]).await?.remove(0);
    Ok(Json(view))
}

/// Body for `PATCH /api/settings/providers/{id}`. All three fields must be
/// supplied (full replace semantics) — partial updates are a future
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
    let view = masked_views(&s, vec![p]).await?.remove(0);
    Ok(Json(view))
}

async fn delete_provider(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, Error> {
    // Clean up the matching Keychain entry FIRST. This is best-effort —
    // any error is swallowed since the DB row deletion is the canonical
    // "remove provider" intent. A stale Keychain entry without a DB row
    // is harmless (the warm-up loop ignores it).
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
    let view = masked_views(&s, vec![p]).await?.remove(0);
    Ok(Json(view))
}

/// Body for `POST /api/settings/providers/{id}/key`. The `api_key` field is
/// the only place a raw secret ever appears in this module — and it is
/// inbound only. It is never echoed back, persisted in SQLite, or logged.
#[derive(Deserialize)]
struct SetKeyBody {
    api_key: String,
}

/// `POST /api/settings/stt/key` — store the Deepgram STT key. The STT key
/// has no `providers` table row; the Keychain entry under
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
        .map_err(|e| Error::Internal(format!("keychain set: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_provider_key(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetKeyBody>,
) -> Result<StatusCode, Error> {
    // Verify the provider exists before touching the Keychain. Without
    // this, a typo'd ID would leak an orphan Keychain entry on every
    // failed attempt.
    if providers::list(&s.db)?.iter().all(|p| p.id != id) {
        return Err(Error::NotFound);
    }
    s.keys
        .set(&id, &body.api_key)
        .map_err(|e| Error::Internal(format!("keychain set: {e}")))?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── Error mapping ───────────────────────────────────────────────────────────

#[derive(Debug)]
enum Error {
    NotFound,
    Internal(String),
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
        }
    }
}
