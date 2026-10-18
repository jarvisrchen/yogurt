use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};

const VITE_BASE_DEFAULT: &str = "http://127.0.0.1:5173";

/// Overridable via `YOGURT_VITE_BASE` so tests can target a dead port even when a dev server
/// is running on :5173.
fn vite_base() -> String {
    std::env::var("YOGURT_VITE_BASE").unwrap_or_else(|_| VITE_BASE_DEFAULT.to_string())
}

/// Larger request bodies get a 413 instead of being buffered.
const MAX_PROXY_BODY: usize = 16 * 1024 * 1024;

/// Axum fallback in `Mode::Dev` that forwards requests to the Vite dev server.
///
/// WebSocket upgrades get a 426 because reqwest cannot perform the handshake. The server's
/// own `/ws` route is registered first, so only unknown WS paths such as `/__vite_hmr` hit this.
pub async fn proxy_to_vite(method: Method, uri: Uri, headers: HeaderMap, body: Body) -> Response {
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
    let target = format!("{}{path_and_query}", vite_base());

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
                    "yogurt dev proxy: cannot reach vite at {}\n\nrun: pnpm --dir web dev",
                    vite_base()
                ),
            )
                .into_response()
        }
    }
}

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

// `HeaderName::as_str()` is already lowercase.
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
