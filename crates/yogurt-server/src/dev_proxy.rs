use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};

const VITE_BASE: &str = "http://127.0.0.1:5173";

/// Cap on request bodies forwarded to Vite (HI-03). Dev requests are tiny
/// (HMR pings, asset requests, JSON RPC). 16 MiB is generous; anything larger
/// is rejected with 413 Payload Too Large rather than buffered, eliminating
/// the trivial OOM DoS that `usize::MAX` allowed.
const MAX_PROXY_BODY: usize = 16 * 1024 * 1024;

/// Axum fallback handler used in `Mode::Dev`. Forwards the entire request to
/// the Vite dev server on :5173. Hop-by-hop headers are stripped on both legs.
///
/// On upstream failure (Vite not running), returns 502 with actionable copy
/// telling the user to run `pnpm --dir web dev`.
///
/// HI-04: WebSocket upgrade requests are NOT proxied — reqwest cannot perform
/// the upgrade handshake. They are rejected with 426 Upgrade Required and a
/// message pointing the user at `http://localhost:5173` (Vite directly) for
/// HMR. The yogurt server's own `/ws` route is registered before this
/// fallback, so this only affects unknown WS paths like `/__vite_hmr`.
pub async fn proxy_to_vite(method: Method, uri: Uri, headers: HeaderMap, body: Body) -> Response {
    // HI-04: detect WS upgrade attempts and refuse cleanly.
    if is_websocket_upgrade(&headers) {
        tracing::warn!(
            path = uri.path(),
            "vite proxy: refusing websocket upgrade -- use http://localhost:5173 for HMR"
        );
        return (
            StatusCode::UPGRADE_REQUIRED,
            [(header::CONTENT_TYPE, "text/plain")],
            "yogurt dev proxy does not forward websocket upgrades.\n\
             Open http://localhost:5173 directly for Vite HMR, or use\n\
             http://localhost:7878 only for non-HMR pages.\n",
        )
            .into_response();
    }

    let path_and_query = uri.path_and_query().map(|x| x.as_str()).unwrap_or("/");
    let target = format!("{VITE_BASE}{path_and_query}");

    // HI-03: cap request body at MAX_PROXY_BODY. Reject larger bodies with 413
    // rather than allocating arbitrary memory.
    let body_bytes = match axum::body::to_bytes(body, MAX_PROXY_BODY).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                ?e,
                cap = MAX_PROXY_BODY,
                "vite proxy: request body exceeded cap or read failed"
            );
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("vite proxy: request body exceeded {MAX_PROXY_BODY} bytes"),
            )
                .into_response();
        }
    };

    let client = reqwest::Client::new();
    let mut req = client.request(method, &target).body(body_bytes.to_vec());

    for (name, value) in headers.iter() {
        if is_hop_by_hop(name) {
            continue;
        }
        if let Ok(v) = value.to_str() {
            req = req.header(name.as_str(), v);
        }
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let mut builder = Response::builder().status(status);
            for (name, value) in resp.headers() {
                if is_hop_by_hop(name) {
                    continue;
                }
                builder = builder.header(name.as_str(), value.as_bytes());
            }
            // MD-03: surface upstream body-read failures explicitly rather
            // than silently returning the original status with an empty body.
            let bytes = match resp.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(?e, target = %target, "vite proxy: upstream body read failed");
                    return (
                        StatusCode::BAD_GATEWAY,
                        format!("vite proxy: upstream body error: {e}"),
                    )
                        .into_response();
                }
            };
            // MD-02: builder.body cannot fail with our well-formed headers,
            // but use .expect for self-documenting intent.
            builder
                .body(Body::from(bytes))
                .expect("response builder accepts well-formed proxy headers")
        }
        Err(e) => {
            tracing::warn!(
                target = %target,
                ?e,
                "vite proxy: upstream error — is `pnpm --dir web dev` running?"
            );
            (
                StatusCode::BAD_GATEWAY,
                [(header::CONTENT_TYPE, "text/plain")],
                format!(
                    "yogurt dev proxy: cannot reach vite at {VITE_BASE}\n\nrun: pnpm --dir web dev"
                ),
            )
                .into_response()
        }
    }
}

/// True if the request looks like an HTTP→WS upgrade attempt.
fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
    let upgrade_is_ws = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    let connection_has_upgrade = headers
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .any(|t| t.eq_ignore_ascii_case("upgrade"))
        })
        .unwrap_or(false);
    upgrade_is_ws && connection_has_upgrade
}

// MD-04: `HeaderName::as_str()` is already guaranteed lowercase by the http
// crate; no `to_ascii_lowercase()` allocation needed per header per request.
fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            | "host"
    )
}
