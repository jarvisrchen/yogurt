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
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::collections::HashSet;
use uuid::Uuid;

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
    if let Err(resp) = enforce_ws_auth(&state, &headers, &params, "ws") {
        return resp;
    }
    ws.on_upgrade(handle_socket)
}

/// Shared Origin-allowlist + session-token check for all WS upgrade handlers.
///
/// Returns `Err(403 response)` on any auth failure; `Ok(())` if the request
/// may proceed to `ws.on_upgrade(...)`. The `endpoint` label only appears in
/// tracing warnings — never include token material here (BL-02).
///
/// `axum::response::Response` is large (~144 bytes) so clippy warns about
/// the Result<(), Response> shape. We deliberately accept the size: the
/// auth path runs once per upgrade (not in a hot loop), and the alternative
/// (boxing the response) obscures the call site without a real perf win.
#[allow(clippy::result_large_err)]
pub(crate) fn enforce_ws_auth(
    state: &AppState,
    headers: &HeaderMap,
    params: &WsParams,
    endpoint: &'static str,
) -> Result<(), Response> {
    let allowed = allowed_origins(state.bind_port);
    let origin = headers
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !allowed.contains(origin) {
        tracing::warn!(%endpoint, %origin, "ws: rejected — origin not in allowlist");
        return Err((StatusCode::FORBIDDEN, "forbidden: bad origin").into_response());
    }

    let Some(candidate) = params.token.as_deref() else {
        tracing::warn!(%endpoint, "ws: rejected — no session token presented");
        return Err((StatusCode::FORBIDDEN, "forbidden: missing token").into_response());
    };

    if !state.session.validate(candidate) {
        // Do NOT log the candidate or the stored token. The mismatch fact is
        // enough; the actual values are credentials.
        tracing::warn!(%endpoint, "ws: rejected — session token mismatch");
        return Err((StatusCode::FORBIDDEN, "forbidden: bad token").into_response());
    }

    Ok(())
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

// ─────────────────────────────────────────────────────────────────────────────
// Phase 3: per-meeting transcript WebSocket
// ─────────────────────────────────────────────────────────────────────────────
//
// `/ws/meetings/:id` — upgrades the HTTP request, looks up the meeting in
// the registry, subscribes to its transcript broadcast, then loops:
// for each `TranscriptEvent`, serialize as JSON and push as `Message::Text`.
//
// S→C JSON frames per PRD §10 + CONTEXT D-10:
//   { "type": "transcript", "payload": { ts_ms, channel, text, is_final } }
// C→S frames in v1: none yet (Phase 4 adds `notes_edit`, Phase 6 adds
// `chat_send`); inbound frames are read-and-discarded so the WS stays
// bidirectionally healthy.
//
// If the meeting id is unknown, close with code 4404 + reason
// `"meeting not found"` (CONTEXT D-09).

/// Axum handler for `GET /ws/meetings/:id`. Enforces the same Origin
/// allowlist + session-token check as the Phase 0 `/ws` endpoint
/// (BL-01: this is the most privacy-sensitive surface in the product —
/// transcript frames carry every word the user speaks, so the same auth
/// gate that protects `/ws` must also gate this route).
pub async fn ws_meeting_handler(
    State(state): State<AppState>,
    Query(params): Query<WsParams>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(resp) = enforce_ws_auth(&state, &headers, &params, "ws/meetings") {
        return resp;
    }
    ws.on_upgrade(move |socket| handle_meeting_socket(socket, id, state))
}

async fn handle_meeting_socket(mut socket: WebSocket, id: Uuid, state: AppState) {
    let mut rx = match state.meetings.subscribe(&id).await {
        Some(r) => r,
        None => {
            let _ = socket
                .send(Message::Close(Some(CloseFrame {
                    code: 4404,
                    reason: "meeting not found".into(),
                })))
                .await;
            return;
        }
    };

    loop {
        tokio::select! {
            // Server → Client: transcript events.
            ev = rx.recv() => match ev {
                Ok(ev) => {
                    let frame = match serde_json::to_string(&serde_json::json!({
                        "type": "transcript",
                        "payload": ev,
                    })) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(?e, "ws/meetings: serialize failed");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(frame.into())).await.is_err() {
                        tracing::info!(meeting=%id, "ws/meetings: client disconnected");
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(meeting=%id, n, "ws/meetings: client lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!(meeting=%id, "ws/meetings: transcript stream ended");
                    return;
                }
            },
            // Client → Server: drained so the WS stays healthy.
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None => {
                    tracing::info!(meeting=%id, "ws/meetings: client closed");
                    return;
                }
                Some(Ok(_)) => {
                    // Ignore for now — Phase 4 (notes_edit) and Phase 6 (chat_send)
                    // will route here.
                }
                Some(Err(e)) => {
                    tracing::warn!(meeting=%id, ?e, "ws/meetings: client recv error");
                    return;
                }
            },
        }
    }
}
