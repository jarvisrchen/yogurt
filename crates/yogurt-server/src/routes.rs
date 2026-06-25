use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
}

async fn index() -> &'static str {
    "hello yogurt — phase 0 scaffold (web UI coming in task 0.5)"
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}
