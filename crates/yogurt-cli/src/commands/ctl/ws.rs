//! `yogurt ctl ws` (CLI-6 / D1 second slice) -- subscribes to the server's
//! WebSocket and prints one JSON frame per line, until Ctrl-C or `--count`.
//!
//! `tokio-tungstenite` + `futures-util` are already compiled into the
//! `yogurt` binary through `yogurt-server` (Deepgram STT speaks WS too) --
//! adding them to `yogurt-cli`'s own `[dependencies]` links no new crate
//! into the build, it just makes an already-resolved one callable from
//! this file. Both are pinned at the workspace's existing versions
//! (`tokio-tungstenite` 0.24, `futures-util` 0.3), so this is the
//! "smallest one already in Cargo.lock" the ticket asks for when a new
//! dependency can't be avoided -- here it isn't even new.
//!
//! [`connect`] is also used by `models.rs`'s `download --wait` to follow
//! `stt_model_download_*` progress on the same app-wide `/ws` socket.

use futures_util::StreamExt;
use serde_json::Value;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use super::client::{Client, CtlError};
use super::meeting::{client_for_ref, MeetingRef};

pub type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// Open a WS connection to `path` (`/ws` or `/ws/meetings/:id`) on `c`'s
/// instance, presenting the same `Origin: http://127.0.0.1:<port>` header
/// a real browser tab would send -- `enforce_ws_auth` on the server side
/// checks it before the token.
pub(super) async fn connect(c: &Client, path: &str) -> Result<WsStream, CtlError> {
    let url = c.ws_url(path);
    // Never interpolate `url` itself into a message -- it carries the
    // session token in `?token=...` (the WS auth contract; see
    // `crate::ws`'s doc comment on the server side), and every `CtlError`
    // message reaches stdout. Reuses the server's own redaction helper
    // rather than a second copy of the same string-scrubbing logic.
    let redacted = yogurt_server::ws::redact_token_in_uri(&url);
    let mut req = url.clone().into_client_request().map_err(|e| {
        CtlError::local(
            format!("bad websocket url {redacted}: {e}"),
            "check `yogurt ctl status`",
        )
    })?;
    req.headers_mut().insert(
        "Origin",
        HeaderValue::from_str(&format!("http://127.0.0.1:{}", c.port))
            .expect("a port number always formats to a valid header value"),
    );
    let (stream, _resp) = tokio_tungstenite::connect_async(req).await.map_err(|e| {
        CtlError::local(
            format!("could not connect to {redacted}: {e}"),
            "check `yogurt ctl status`",
        )
    })?;
    Ok(stream)
}

pub async fn run(
    meeting: Option<String>,
    types: Vec<String>,
    count: Option<usize>,
    port_flag: Option<u16>,
    _json_out: bool,
) -> Result<(), CtlError> {
    // `ws` frames are already JSON text off the wire; there is no
    // meaningful "compact text" projection across the heterogeneous frame
    // shapes (transcript / enhance_progress / chat_chunk / stt model
    // download events), so this ignores --json and always prints the raw
    // frame -- one JSON object per line either way.
    let (c, path) = match &meeting {
        Some(m) => {
            let r = MeetingRef::parse(m);
            let (c, id) = client_for_ref(port_flag, &r).await?;
            (c, format!("/ws/meetings/{id}"))
        }
        None => (Client::discover(port_flag).await?, "/ws".to_string()),
    };

    let mut stream = connect(&c, &path).await?;
    let mut printed = 0usize;
    while let Some(msg) = stream.next().await {
        let msg = msg.map_err(|e| {
            CtlError::local(format!("websocket error: {e}"), "check `yogurt ctl status`")
        })?;
        let Message::Text(text) = msg else { continue };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if !types.is_empty() {
            let frame_type = value.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if !types.iter().any(|want| want == frame_type) {
                continue;
            }
        }
        println!("{text}");
        printed += 1;
        if count.is_some_and(|n| printed >= n) {
            break;
        }
    }
    Ok(())
}
