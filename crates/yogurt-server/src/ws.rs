//! WebSocket endpoint with Origin allowlist + session-token auth (D-20 / D-21).
//!
//! Phase 0 scope: lock down the upgrade path. The actual WS payload protocol
//! lands in Phase 3 — for now we just echo any messages we receive so the
//! transport surface can be smoke-tested.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::collections::HashSet;

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct WsParams {
    /// Optional token query param: `/ws?token=<token>`.
    pub token: Option<String>,
}

/// Axum handler for `GET /ws`. Performs the Origin + token check before
/// upgrading the connection.
pub async fn ws_handler(
    State(state): State<AppState>,
    Query(params): Query<WsParams>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Origin allowlist (D-20). Build from the actual bound port so tests on
    // ephemeral ports still pass.
    let allowed = allowed_origins(state.bind_port);
    let origin = headers
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !allowed.contains(origin) {
        tracing::warn!(%origin, "ws: rejected — origin not in allowlist");
        return (StatusCode::FORBIDDEN, "forbidden: bad origin").into_response();
    }

    // Token: prefer `?token=<token>` query param; fall back to the
    // `Sec-WebSocket-Protocol: yogurt.<token>` subprotocol header (D-21).
    let candidate = params.token.clone().or_else(|| {
        headers
            .get("sec-websocket-protocol")
            .and_then(|h| h.to_str().ok())
            .and_then(|proto| {
                proto
                    .split(',')
                    .map(str::trim)
                    .find_map(|p| p.strip_prefix("yogurt.").map(str::to_string))
            })
    });

    let Some(candidate) = candidate else {
        tracing::warn!("ws: rejected — no session token presented");
        return (StatusCode::FORBIDDEN, "forbidden: missing token").into_response();
    };

    let stored = state.session.as_str();
    let token = crate::session::SessionToken(stored.to_string());
    if !token.validate(&candidate) {
        tracing::warn!("ws: rejected — session token mismatch");
        return (StatusCode::FORBIDDEN, "forbidden: bad token").into_response();
    }

    ws.on_upgrade(handle_socket)
}

/// Phase 0 stub: echo text messages back; close on binary/ping anomalies.
async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        let Ok(msg) = msg else {
            return;
        };
        match msg {
            Message::Text(t) => {
                if socket.send(Message::Text(t)).await.is_err() {
                    return;
                }
            }
            Message::Ping(p) => {
                if socket.send(Message::Pong(p)).await.is_err() {
                    return;
                }
            }
            Message::Close(_) => return,
            // Binary / Pong: Phase 0 has nothing to do with these.
            _ => {}
        }
    }
}

fn allowed_origins(port: u16) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert(format!("http://localhost:{port}"));
    set.insert(format!("http://127.0.0.1:{port}"));
    set
}
