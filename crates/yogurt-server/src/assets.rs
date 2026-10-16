use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct WebDist;

/// Serves the embedded SPA, falling back to `index.html` so client-side routes resolve.
///
/// Traversal segments are rejected up front. `rust-embed::get` matches exact strings today,
/// but a switch to disk-backed `debug-embed` would make `..` a real hole.
pub async fn serve_embedded(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if path
        .split('/')
        .any(|seg| seg == ".." || seg == "." || seg.starts_with('\0'))
    {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let candidate = if path.is_empty() { "index.html" } else { path };

    match WebDist::get(candidate) {
        Some(file) => Response::builder()
            .header(header::CONTENT_TYPE, file.metadata.mimetype())
            .body(Body::from(file.data.into_owned()))
            .expect("response builder: embedded MIME is always valid"),
        None => match WebDist::get("index.html") {
            Some(idx) => Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(idx.data.into_owned()))
                .expect("response builder: text/html is always valid"),
            None => (StatusCode::NOT_FOUND, "asset not found").into_response(),
        },
    }
}
