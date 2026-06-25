use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct WebDist;

/// Axum fallback handler that serves the embedded SPA.
///
/// On unknown paths, falls back to `index.html` so client-side routes
/// resolve via the SPA router (Plan 02 introduces this; later plans wire
/// the actual routes).
///
/// MD-01: defense-in-depth against path traversal. `rust-embed::get` does
/// exact-string matching against the embedded table, so `../etc/passwd`
/// returns None today — but a future swap to `debug-embed` (disk-backed)
/// would make this a real bug. We reject any path containing `..` or `.`
/// segments up-front to make the property structural rather than incidental.
pub async fn serve_embedded(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // MD-01: reject traversal segments and any absolute-style path.
    if path
        .split('/')
        .any(|seg| seg == ".." || seg == "." || seg.starts_with('\0'))
    {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let candidate = if path.is_empty() { "index.html" } else { path };

    // MD-02: `.expect("...")` documents the panic conditions explicitly
    // -- mime_guess always returns ASCII MIME strings, and Body::from
    // accepts arbitrary bytes, so the builder cannot fail here.
    match WebDist::get(candidate) {
        Some(file) => {
            let mime = mime_guess::from_path(candidate).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(file.data.into_owned()))
                .expect("response builder: static MIME from mime_guess is always valid")
        }
        None => match WebDist::get("index.html") {
            Some(idx) => Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(idx.data.into_owned()))
                .expect("response builder: text/html is always valid"),
            None => (StatusCode::NOT_FOUND, "asset not found").into_response(),
        },
    }
}
