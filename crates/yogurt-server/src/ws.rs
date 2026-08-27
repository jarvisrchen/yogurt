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
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::AppState;

/// Typed WS event surface introduced in Phase 6.
///
/// Phase 0/3 used ad-hoc `serde_json::Value` frames over the per-meeting
/// `events_tx` broadcast (see `meetings.rs`). Phase 6 introduces this
/// `WsEvent` enum for new typed surfaces — currently only `ChatChunk` —
/// while leaving the Phase 4 `enhance_progress` `serde_json::Value` path
/// untouched. Both surfaces share the same WebSocket and the same
/// `{"type": "<snake_case>", ...}` discriminator convention so the browser
/// sees one homogeneous stream.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// Per-token chat streaming chunk. Emitted by `api::chat::spawn_stream`
    /// for each chunk returned by `LlmClient::stream`. Terminates with a
    /// `done: true` chunk (possibly with `delta = ""`) so the browser can
    /// release the streaming-caret state.
    ChatChunk {
        message_id: String,
        delta: String,
        #[serde(default)]
        done: bool,
    },
    /// Phase 8 (Plan 08-03): live model download progress for a
    /// whisper.cpp model.  Fanned out on the app-wide
    /// `AppState::app_events_tx` broadcast so the Settings page can show
    /// a matcha progress bar without a meeting being open.  Emitted every
    /// ~500 ms during a download by `api::stt_models::download_task`.
    SttModelDownloadProgress {
        model: String,
        bytes_downloaded: u64,
        total_bytes: u64,
        bytes_per_sec: u64,
        eta_seconds: Option<u64>,
    },
    /// Phase 8 (Plan 08-03): a model download finished successfully —
    /// SHA256 verified, file persisted at `~/.yogurt/models/<filename>`.
    /// The download dialog auto-closes ~600 ms after seeing this event.
    SttModelDownloadComplete { model: String },
    /// Phase 8 (Plan 08-03): a model download failed (HTTP error,
    /// hash mismatch, IO error).  The download dialog stays open with
    /// the error message so the user can retry.
    SttModelDownloadError { model: String, error: String },
}

/// Phase 8 (Plan 08-03): app-wide event broadcaster.  The chat path
/// already uses per-meeting `events_tx` channels; this is the
/// **meeting-independent** sibling that surfaces things like model
/// downloads on the global `/ws` endpoint.
///
/// Capacity 64 — STT model downloads emit ~2 events/sec; the cushion
/// absorbs slow consumers without blocking the writer.
pub type AppEventTx = tokio::sync::broadcast::Sender<serde_json::Value>;

/// Phase 8 (Plan 08-03): broadcast a `SttModelDownloadProgress` frame on
/// the app-wide event channel.  Errors are swallowed — a quiet WS (no
/// subscriber) is fine; downloads keep working regardless.
pub fn send_stt_model_download_progress(
    tx: &AppEventTx,
    model: &str,
    bytes_downloaded: u64,
    total_bytes: u64,
    bytes_per_sec: u64,
    eta_seconds: Option<u64>,
) {
    let ev = WsEvent::SttModelDownloadProgress {
        model: model.to_string(),
        bytes_downloaded,
        total_bytes,
        bytes_per_sec,
        eta_seconds,
    };
    if let Ok(value) = serde_json::to_value(&ev) {
        let _ = tx.send(value);
    }
}

/// Phase 8 (Plan 08-03): broadcast a `SttModelDownloadComplete` frame.
pub fn send_stt_model_download_complete(tx: &AppEventTx, model: &str) {
    let ev = WsEvent::SttModelDownloadComplete {
        model: model.to_string(),
    };
    if let Ok(value) = serde_json::to_value(&ev) {
        let _ = tx.send(value);
    }
}

/// Phase 8 (Plan 08-03): broadcast a `SttModelDownloadError` frame.
pub fn send_stt_model_download_error(tx: &AppEventTx, model: &str, error: &str) {
    let ev = WsEvent::SttModelDownloadError {
        model: model.to_string(),
        error: error.to_string(),
    };
    if let Ok(value) = serde_json::to_value(&ev) {
        let _ = tx.send(value);
    }
}

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
    // Phase 8 (Plan 08-03): subscribe to the app-wide event broadcaster
    // so the Settings page can receive STT model download progress
    // without a meeting being open.  Pre-upgrade subscription is
    // important — the receiver must exist before the first frame.
    let app_rx = state.app_events_tx.subscribe();
    ws.on_upgrade(move |socket| handle_socket(socket, app_rx))
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
    let allowed = allowed_origins(state.bind_port, state.mode);
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

/// Phase 0 stub → Phase 8 (Plan 08-03) extension:
///
/// Inbound text messages are still echoed (smoke-test surface preserved),
/// but the handler now ALSO forwards `AppState::app_events_tx`
/// broadcasts as Text frames so the Settings page can listen for
/// STT model download progress on the single global `/ws` socket.
async fn handle_socket(
    mut socket: WebSocket,
    mut app_rx: tokio::sync::broadcast::Receiver<serde_json::Value>,
) {
    loop {
        tokio::select! {
            // Server → Client: app-wide events (STT model downloads, etc.).
            ev = app_rx.recv() => match ev {
                Ok(value) => {
                    let frame = match serde_json::to_string(&value) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(?e, "ws: app_events serialize failed");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(frame.into())).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(n, "ws: app_events lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    // App-events channel closed (shutdown). The WS itself
                    // can stay open as long as inbound traffic continues;
                    // just stop polling this branch.
                    tracing::debug!("ws: app_events stream ended");
                    return;
                }
            },
            // Client → Server: Phase 0 echo + control frames.
            msg = socket.recv() => match msg {
                None => return,
                Some(Err(_)) => return,
                Some(Ok(Message::Text(t))) => {
                    if socket.send(Message::Text(t)).await.is_err() {
                        return;
                    }
                }
                Some(Ok(Message::Ping(p))) => {
                    if socket.send(Message::Pong(p)).await.is_err() {
                        return;
                    }
                }
                Some(Ok(Message::Close(_))) => return,
                // Binary / Pong: nothing to do.
                Some(Ok(_)) => {}
            },
        }
    }
}

fn allowed_origins(port: u16, mode: crate::Mode) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert(format!("http://localhost:{port}"));
    set.insert(format!("http://127.0.0.1:{port}"));
    // Dev mode: the SPA may be served straight from Vite on :5173 (its
    // proxy forwards /ws to us but keeps the page's Origin header), so
    // that origin is legitimate. Release builds serve embedded assets
    // from our own port only - no extra origins there.
    if mode == crate::Mode::Dev {
        set.insert("http://localhost:5173".to_string());
        set.insert("http://127.0.0.1:5173".to_string());
    }
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
    // Two broadcast subscriptions per WS connection:
    //   - transcript events (Phase 3, owned by the STT pump);
    //   - JSON meeting events (Phase 4: `enhance_progress`).
    // Both are fanned into the same socket, just wrapped in different
    // top-level `type` discriminators on the JSON frame.
    let m = match state.meetings.get(&id).await {
        Some(m) => m,
        None => {
            // HI-9 (same rule as the enhance handler): the in-memory
            // registry is wiped on restart, but chat chunks and
            // enhance_progress still ride this socket for meetings that
            // only exist in SQLite (every post-meeting view). Hydrate a
            // transient Meeting when the row exists; hard-close only for
            // genuinely unknown ids. Without this, chat on a post-meeting
            // view after a server restart streamed into the void — the UI
            // spinner never resolved.
            let id_str = id.to_string();
            let repo = state.meeting_repo.clone();
            let exists = tokio::task::spawn_blocking(move || repo.get(&id_str))
                .await
                .ok()
                .and_then(|r| r.ok())
                .flatten()
                .is_some();
            if !exists {
                let _ = socket
                    .send(Message::Close(Some(CloseFrame {
                        code: 4404,
                        reason: "meeting not found".into(),
                    })))
                    .await;
                return;
            }
            state.meetings.hydrate(id).await
        }
    };
    let mut rx = m.transcript_tx.subscribe();
    let mut events_rx = m.events_tx.subscribe();

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
            // Server → Client: Phase 4 enhance_progress (and future meeting
            // events). The value is already a fully-formed JSON object
            // (`{type, phase, …}`) so we serialize it directly.
            ev = events_rx.recv() => match ev {
                Ok(value) => {
                    let frame = match serde_json::to_string(&value) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(?e, "ws/meetings: events serialize failed");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(frame.into())).await.is_err() {
                        tracing::info!(meeting=%id, "ws/meetings: client disconnected (events)");
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(meeting=%id, n, "ws/meetings: events lagged");
                    continue;
                }
                // Events channel closing is non-fatal — keep the WS alive
                // for the transcript stream.
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!(meeting=%id, "ws/meetings: events stream ended");
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Dev mode must accept the Vite origin (:5173) - the SPA can be
    /// served straight from Vite, whose proxy forwards /ws but keeps
    /// the page's Origin header. Release must NOT accept it.
    #[test]
    fn allowed_origins_includes_vite_only_in_dev() {
        let dev = allowed_origins(7878, crate::Mode::Dev);
        assert!(dev.contains("http://localhost:7878"));
        assert!(dev.contains("http://127.0.0.1:7878"));
        assert!(dev.contains("http://localhost:5173"));
        assert!(dev.contains("http://127.0.0.1:5173"));

        let release = allowed_origins(7878, crate::Mode::Release);
        assert!(release.contains("http://localhost:7878"));
        assert!(!release.contains("http://localhost:5173"));
        assert!(!release.contains("http://127.0.0.1:5173"));
    }

    /// Phase 8 (Plan 08-03) wire-shape gate: the browser-side
    /// `useModelDownloadProgress` hook switches on
    /// `ev.type === "stt_model_download_progress"` (and the two terminal
    /// variants) and reads `model`, `bytes_downloaded`, `total_bytes`,
    /// `bytes_per_sec`, `eta_seconds`. Any rename here breaks the
    /// Settings download dialog silently.
    #[test]
    fn dl_progress_serializes_with_tag() {
        let ev = WsEvent::SttModelDownloadProgress {
            model: "small.en".into(),
            bytes_downloaded: 1_000_000,
            total_bytes: 487_000_000,
            bytes_per_sec: 12_345_678,
            eta_seconds: Some(39),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "stt_model_download_progress");
        assert_eq!(json["model"], "small.en");
        assert_eq!(json["bytes_downloaded"], 1_000_000);
        assert_eq!(json["total_bytes"], 487_000_000);
        assert_eq!(json["bytes_per_sec"], 12_345_678);
        assert_eq!(json["eta_seconds"], 39);
    }

    #[test]
    fn dl_complete_serializes_with_tag() {
        let ev = WsEvent::SttModelDownloadComplete {
            model: "small.en".into(),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "stt_model_download_complete");
        assert_eq!(json["model"], "small.en");
    }

    #[test]
    fn dl_error_serializes_with_tag() {
        let ev = WsEvent::SttModelDownloadError {
            model: "small.en".into(),
            error: "hash mismatch".into(),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "stt_model_download_error");
        assert_eq!(json["model"], "small.en");
        assert_eq!(json["error"], "hash mismatch");
    }

    /// Phase 6 (Plan 06-01) wire-shape gate: the browser-side `useChat`
    /// hook switches on `ev.type === "chat_chunk"` and reads `message_id`,
    /// `delta`, `done`. Any rename here breaks the frontend silently.
    #[test]
    fn it_serializes_chat_chunk_with_expected_keys() {
        let ev = WsEvent::ChatChunk {
            message_id: "01HXMSG".into(),
            delta: "hello".into(),
            done: false,
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "chat_chunk");
        assert_eq!(json["message_id"], "01HXMSG");
        assert_eq!(json["delta"], "hello");
        assert_eq!(json["done"], false);
    }
}
