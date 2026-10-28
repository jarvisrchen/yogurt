//! WebSocket endpoints behind an Origin allowlist and session-token auth.
//!
//! The browser `WebSocket` API cannot set headers, so `?token=<token>` is the sole auth path.
//! Tokens are redacted from tracing logs (see `redact_token_in_uri`).
//! They may show in devtools, which is acceptable for a localhost-only single-user app.

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

/// Typed frames sharing the `{"type": "<snake_case>", ...}` convention of the ad-hoc
/// `serde_json::Value` frames (e.g. `enhance_progress`) on the per-meeting `events_tx`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// Terminates with a `done: true` chunk (possibly with an empty `delta`).
    ChatChunk {
        message_id: String,
        delta: String,
        #[serde(default)]
        done: bool,
    },
    /// Sent on the app-wide `AppState::app_events_tx` so Settings sees it with no meeting open.
    SttModelDownloadProgress {
        model: String,
        bytes_downloaded: u64,
        total_bytes: u64,
        bytes_per_sec: u64,
        eta_seconds: Option<u64>,
    },
    /// SHA256 verified and the file persisted.
    SttModelDownloadComplete {
        model: String,
    },
    SttModelDownloadError {
        model: String,
        error: String,
    },
    SttModelDownloadCancelled {
        model: String,
    },
    /// Granola import frames, also on the app-wide stream. Progress is sent at most once a second.
    ImportProgress {
        done: usize,
        total: usize,
        imported: usize,
        skipped: usize,
        failed: usize,
    },
    ImportComplete {
        imported: usize,
        skipped: usize,
        failed: usize,
        failed_titles: Vec<String>,
    },
    ImportError {
        error: String,
    },
    ImportCancelled {
        imported: usize,
        skipped: usize,
        failed: usize,
    },
    /// Yogurt Cloud push frames for a pass of more than one batch. `retry_in` is the seconds left before a failed request is retried.
    CloudPushProgress {
        sent: usize,
        total: usize,
        retry_in: Option<u64>,
        /// Seconds left in the pass at its pace so far, `None` until a batch lands.
        left_secs: Option<u64>,
    },
    CloudPushComplete {
        pushed: usize,
        deleted: usize,
    },
    CloudPushError {
        error: String,
        code: String,
    },
    CloudPushCancelled,
    /// The whole recap on every change, so a reconnecting page needs no delta logic.
    LiveRecap(crate::recap::RecapState),
}

/// Meeting-independent sibling of the per-meeting `events_tx`, surfaced on the global `/ws`.
pub type AppEventTx = tokio::sync::broadcast::Sender<serde_json::Value>;

/// Send errors (no subscriber) are ignored.
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

pub fn send_event(tx: &AppEventTx, ev: WsEvent) {
    if let Ok(value) = serde_json::to_value(&ev) {
        let _ = tx.send(value);
    }
}

pub fn send_stt_model_download_complete(tx: &AppEventTx, model: &str) {
    let ev = WsEvent::SttModelDownloadComplete {
        model: model.to_string(),
    };
    if let Ok(value) = serde_json::to_value(&ev) {
        let _ = tx.send(value);
    }
}

pub fn send_stt_model_download_cancelled(tx: &AppEventTx, model: &str) {
    let ev = WsEvent::SttModelDownloadCancelled {
        model: model.to_string(),
    };
    if let Ok(value) = serde_json::to_value(&ev) {
        let _ = tx.send(value);
    }
}

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
    pub token: Option<String>,
}

// Manual Debug impl: never leak the token into tracing output.
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

pub async fn ws_handler(
    State(state): State<AppState>,
    Query(params): Query<WsParams>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(resp) = enforce_ws_auth(&state, &headers, &params, "ws") {
        return resp;
    }
    // Subscribe before the upgrade so the receiver exists before the first frame.
    let app_rx = state.app_events_tx.subscribe();
    ws.on_upgrade(move |socket| handle_socket(socket, app_rx))
}

/// Returns a 403 response on any auth failure.
/// The `endpoint` label only appears in tracing warnings, so never put token material in it.
///
/// `Response` is large, which trips `clippy::result_large_err`. The auth path runs once
/// per upgrade, so boxing it would only obscure the call site.
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
        // Never log the candidate or the stored token.
        tracing::warn!(%endpoint, "ws: rejected — session token mismatch");
        return Err((StatusCode::FORBIDDEN, "forbidden: bad token").into_response());
    }

    Ok(())
}

/// Replace any `token=<value>` query string in a URI with `token=<REDACTED>`.
pub fn redact_token_in_uri(uri: &str) -> String {
    let mut out = String::with_capacity(uri.len());
    let mut rest = uri;
    while let Some(idx) = rest.find("token=") {
        out.push_str(&rest[..idx]);
        out.push_str("token=<REDACTED>");
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

/// Echoes inbound text and forwards `AppState::app_events_tx` broadcasts as Text frames.
async fn handle_socket(
    mut socket: WebSocket,
    mut app_rx: tokio::sync::broadcast::Receiver<serde_json::Value>,
) {
    loop {
        tokio::select! {
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
                    tracing::debug!("ws: app_events stream ended");
                    return;
                }
            },
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
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Vite's dev port, for the origin allowlist below. Defaults to 5173 and
/// follows `YOGURT_VITE_PORT` / `YOGURT_VITE_BASE` so a second worktree,
/// which `just dev` puts on a different pair, is allowed too.
fn vite_dev_port() -> u16 {
    vite_port_from(
        std::env::var("YOGURT_VITE_PORT").ok(),
        std::env::var("YOGURT_VITE_BASE").ok(),
    )
}

/// The env-reading half of `vite_dev_port`, split out so it is testable
/// without mutating process-global env.
fn vite_port_from(port_var: Option<String>, base_var: Option<String>) -> u16 {
    const DEFAULT: u16 = 5173;
    if let Some(p) = port_var.and_then(|p| p.trim().parse().ok()) {
        return p;
    }
    base_var
        .and_then(|base| base.trim_end_matches('/').rsplit(':').next()?.parse().ok())
        .unwrap_or(DEFAULT)
}

fn allowed_origins(port: u16, mode: crate::Mode) -> HashSet<String> {
    let mut set = HashSet::new();
    set.insert(format!("http://localhost:{port}"));
    set.insert(format!("http://127.0.0.1:{port}"));
    // In dev the SPA may be served from Vite, whose proxy keeps the page's Origin header.
    if mode == crate::Mode::Dev {
        let vite = vite_dev_port();
        set.insert(format!("http://localhost:{vite}"));
        set.insert(format!("http://127.0.0.1:{vite}"));
    }
    set
}

/// Transcript frames carry every word the user speaks, so this route needs the same auth gate as `/ws`.
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
    let m = match state.meetings.get(&id).await {
        Some(m) => m,
        None => {
            // The registry is wiped on restart, but chat chunks and enhance_progress still
            // ride this socket for SQLite-only meetings. Hydrate when the row exists and
            // close only for unknown ids, else chat streams into the void.
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
    // The recap sends a frame only on change, so a client that connects late, or reconnects after
    // missing one, starts from the current state.
    if let Ok(frame) = serde_json::to_string(&WsEvent::LiveRecap(crate::recap::snapshot(&m))) {
        if socket.send(Message::Text(frame.into())).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
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
                        tracing::debug!(meeting=%id, "ws/meetings: client disconnected");
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(meeting=%id, n, "ws/meetings: client lagged");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!(meeting=%id, "ws/meetings: transcript stream ended");
                    return;
                }
            },
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
                        tracing::debug!(meeting=%id, "ws/meetings: client disconnected (events)");
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(meeting=%id, n, "ws/meetings: events lagged");
                    continue;
                }
                // Non-fatal: keep the WS alive for the transcript stream.
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::debug!(meeting=%id, "ws/meetings: events stream ended");
                }
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None => {
                    tracing::debug!(meeting=%id, "ws/meetings: client closed");
                    return;
                }
                Some(Ok(_)) => {}
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

    #[test]
    fn vite_port_follows_env_then_falls_back() {
        assert_eq!(vite_port_from(None, None), 5173);
        assert_eq!(
            vite_port_from(Some("5273".into()), Some("http://127.0.0.1:5999".into())),
            5273
        );
        assert_eq!(
            vite_port_from(None, Some("http://127.0.0.1:5273".into())),
            5273
        );
        assert_eq!(
            vite_port_from(None, Some("http://127.0.0.1:5273/".into())),
            5273
        );
        assert_eq!(vite_port_from(Some("nope".into()), None), 5173);
        assert_eq!(vite_port_from(None, Some("http://127.0.0.1".into())), 5173);
    }

    /// The browser's `useModelDownloadProgress` hook reads these exact names, so a rename breaks it silently.
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
    fn import_frames_serialize_with_tags() {
        let json = |ev: WsEvent| serde_json::to_value(&ev).unwrap();
        let p = json(WsEvent::ImportProgress {
            done: 5,
            total: 9,
            imported: 3,
            skipped: 1,
            failed: 1,
        });
        assert_eq!(p["type"], "import_progress");
        assert_eq!(
            (p["done"].as_u64(), p["total"].as_u64()),
            (Some(5), Some(9))
        );
        let c = json(WsEvent::ImportComplete {
            imported: 3,
            skipped: 1,
            failed: 1,
            failed_titles: vec!["Weekly".into()],
        });
        assert_eq!(c["type"], "import_complete");
        assert_eq!(c["failed_titles"][0], "Weekly");
        assert_eq!(
            json(WsEvent::ImportError { error: "x".into() })["type"],
            "import_error"
        );
        let x = json(WsEvent::ImportCancelled {
            imported: 1,
            skipped: 0,
            failed: 0,
        });
        assert_eq!(x["type"], "import_cancelled");
    }

    #[test]
    fn cloud_push_frames_serialize_with_tags() {
        let json = |ev: WsEvent| serde_json::to_value(&ev).unwrap();
        let p = json(WsEvent::CloudPushProgress {
            sent: 340,
            total: 1412,
            retry_in: Some(45),
            left_secs: Some(300),
        });
        assert_eq!(p["type"], "cloud_push_progress");
        assert_eq!(
            (
                p["sent"].as_u64(),
                p["total"].as_u64(),
                p["retry_in"].as_u64()
            ),
            (Some(340), Some(1412), Some(45))
        );
        let c = json(WsEvent::CloudPushComplete {
            pushed: 1412,
            deleted: 2,
        });
        assert_eq!(c["type"], "cloud_push_complete");
        let e = json(WsEvent::CloudPushError {
            error: "x".into(),
            code: "network".into(),
        });
        assert_eq!(
            (e["type"].as_str(), e["code"].as_str()),
            (Some("cloud_push_error"), Some("network"))
        );
        assert_eq!(
            json(WsEvent::CloudPushCancelled)["type"],
            "cloud_push_cancelled"
        );
    }

    #[test]
    fn dl_cancelled_serializes_with_tag() {
        let ev = WsEvent::SttModelDownloadCancelled {
            model: "small.en".into(),
        };
        let json = serde_json::to_value(&ev).unwrap();
        assert_eq!(json["type"], "stt_model_download_cancelled");
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

    /// The browser's `useChat` hook reads these exact names, so a rename breaks it silently.
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
