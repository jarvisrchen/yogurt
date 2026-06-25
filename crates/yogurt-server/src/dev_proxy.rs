use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};

const VITE_BASE: &str = "http://127.0.0.1:5173";

/// Axum fallback handler used in `Mode::Dev`. Forwards the entire request to
/// the Vite dev server on :5173. Hop-by-hop headers are stripped on both legs.
///
/// On upstream failure (Vite not running), returns 502 with actionable copy
/// telling the user to run `pnpm --dir web dev`.
pub async fn proxy_to_vite(
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let path_and_query = uri.path_and_query().map(|x| x.as_str()).unwrap_or("/");
    let target = format!("{VITE_BASE}{path_and_query}");

    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(?e, "vite proxy: failed to buffer request body");
            return (StatusCode::BAD_GATEWAY, "vite proxy: body read failed")
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
            let bytes = resp.bytes().await.unwrap_or_default();
            builder.body(Body::from(bytes)).unwrap()
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

fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
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
