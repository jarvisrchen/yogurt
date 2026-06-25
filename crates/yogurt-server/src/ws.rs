//! WebSocket endpoint with Origin allowlist + session-token auth (D-20 / D-21).
//!
//! Phase 0 scope: lock down the upgrade path. The actual WS payload protocol
//! lands in Phase 3 — for now we just echo any messages we receive so the
//! transport surface can be smoke-tested.
//!
//! ## Auth contract (BL-02 resolution)
//!
//! The browser `WebSocket` API has no way to set arbitrary headers, so the
//! `Sec-WebSocket-Protocol`-based auth path described in earlier drafts cannot
//! be exercised by the production client. Rather than ship a half-broken
//! subprotocol-echo flow (which violates RFC 6455 if not echoed correctly),
//! Phase 0 declares **`?token=<token>` query param as the SOLE auth path**.
//!
//! Tradeoffs we accepted:
//!   - Tokens appear in URLs. We mitigate by REDACTING `?token=...` from
//!     tracing logs (see `redact_token_in_uri`).
//!   - Tokens may appear in browser devtools network panel — acceptable for
//!     a localhost-only single-user app (PRD §7 trust assumption).
//!
//! This shrinks attack surface (no header-injection ambiguity from multi-
//! valued `Sec-WebSocket-Protocol`) and avoids the RFC 6455 echo-back bug.

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

#[derive(Deserialize)]
pub struct WsParams {
    /// Session-token query param: `/ws?token=<token>`. Required.
    pub token: Option<String>,
}

// Manual Debug impl: never leak the token into tracing output (BL-02).
impl std::fmt::Debug for WsParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WsParams")
            .field(
                "token",
                &self.token.as_ref().map(|_| "<REDACTED>").unwrap_or("None"),
            )
            .finish()
    }
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

    // Token comes from the `?token=<token>` query param ONLY. The
    // `Sec-WebSocket-Protocol` path was removed in BL-02 — see module docs.
    let Some(candidate) = params.token.clone() else {
        tracing::warn!("ws: rejected — no session token presented");
        return (StatusCode::FORBIDDEN, "forbidden: missing token").into_response();
    };

    if !state.session.validate(&candidate) {
        // Do NOT log the candidate or the stored token. The mismatch fact is
        // enough; the actual values are credentials.
        tracing::warn!("ws: rejected — session token mismatch");
        return (StatusCode::FORBIDDEN, "forbidden: bad token").into_response();
    }

    ws.on_upgrade(handle_socket)
}

/// Replace any `token=<value>` query string in a URI with `token=<REDACTED>`.
/// Public helper so future request-logging middleware can also use it.
pub fn redact_token_in_uri(uri: &str) -> String {
    // Cheap string-level redaction; we don't need a full URL parser for this.
    let mut out = String::with_capacity(uri.len());
    let mut rest = uri;
    while let Some(idx) = rest.find("token=") {
        out.push_str(&rest[..idx]);
        out.push_str("token=<REDACTED>");
        // Skip past "token=" and any value chars until next '&' or end.
        let after = &rest[idx + "token=".len()..];
        match after.find('&') {
            Some(amp) => rest = &after[amp..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
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
