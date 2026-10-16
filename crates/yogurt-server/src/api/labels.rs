use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch, put},
    Json, Router,
};
use serde::Deserialize;

use crate::api::ApiError;
use crate::AppState;
use yogurt_db::{Label, LabelWithCount};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/labels", get(list).post(create))
        .route("/api/labels/order", put(reorder))
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

#[derive(Debug, Deserialize)]
pub struct ReorderBody {
    /// Every label id in display order.
    pub ids: Vec<String>,
}

async fn list(State(s): State<AppState>) -> Result<Json<Vec<LabelWithCount>>, ApiError> {
    let repo = s.label_repo.clone();
    let xs = tokio::task::spawn_blocking(move || repo.list_with_counts())
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    Ok(Json(xs))
}

/// 201 when a label was inserted, 200 when a case-insensitive name match was returned.
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
        // The path id is the label itself, so unknown is 404 (unlike a meeting PATCH's `label_ids`).
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

/// An unknown id in `ids` is a 400, same as an unknown `label_ids` entry on a meeting PATCH.
async fn reorder(
    State(s): State<AppState>,
    Json(body): Json<ReorderBody>,
) -> Result<StatusCode, ApiError> {
    let repo = s.label_repo.clone();
    tokio::task::spawn_blocking(move || repo.reorder(&body.ids))
        .await
        .map_err(|e| ApiError::Internal(anyhow::Error::new(e)))?
        .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
