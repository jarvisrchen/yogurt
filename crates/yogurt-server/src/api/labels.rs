//! Granola-style meeting labels — REST surface.
//!
//! | Method | Path               | Purpose                              |
//! |--------|--------------------|---------------------------------------|
//! | GET    | `/api/labels`      | List all labels with meeting counts.  |
//! | POST   | `/api/labels`      | Find-or-create a label by name.       |
//! | PATCH  | `/api/labels/{id}` | Rename / recolor a label.             |
//! | DELETE | `/api/labels/{id}` | Remove a label (cascades on meetings).|
//!
//! **Auth:** every route is mounted behind `routes::require_session_token`
//! by `routes::router`, same as `api::meetings`. Handlers here don't
//! repeat that check.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use serde::Deserialize;

use crate::api::ApiError;
use crate::AppState;
use yogurt_db::{Label, LabelWithCount};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/labels", get(list).post(create))
        .route("/api/labels/{id}", patch(update).delete(delete_one))
}

#[derive(Debug, Deserialize)]
pub struct CreateBody {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct UpdateBody {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<LabelWithCount>>, ApiError> {
    let repo = s.label_repo.clone();
    let xs = tokio::task::spawn_blocking(move || repo.list_with_counts())
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    Ok(Json(xs))
}

/// `POST /api/labels` — find-or-create semantics. 201 when a new label
/// row was inserted, 200 when an existing (case-insensitive name match)
/// label was returned unchanged.
async fn create(
    State(s): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<Label>), ApiError> {
    let repo = s.label_repo.clone();
    let (label, created) =
        tokio::task::spawn_blocking(move || repo.find_or_create(&body.name, body.color.as_deref()))
            .await
            .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
            .map_err(ApiError::from)?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(label)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<Label>, ApiError> {
    let repo = s.label_repo.clone();
    let id_for_blocking = id.clone();
    let result = tokio::task::spawn_blocking(move || {
        repo.update(
            &id_for_blocking,
            body.name.as_deref(),
            body.color.as_deref(),
        )
    })
    .await
    .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?;
    match result {
        Ok(label) => Ok(Json(label)),
        // Unlike a meeting PATCH's `label_ids` (where "label not found"
        // means a bad reference against an existing meeting), here the id
        // in the URL path itself is the label — unknown means 404.
        Err(e) if e.to_string().contains("label not found") => Err(ApiError::NotFound),
        Err(e) => Err(ApiError::from(e)),
    }
}

async fn delete_one(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let repo = s.label_repo.clone();
    let removed = tokio::task::spawn_blocking(move || repo.delete(&id))
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
