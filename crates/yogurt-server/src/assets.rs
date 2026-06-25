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
pub async fn serve_embedded(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let candidate = if path.is_empty() { "index.html" } else { path };

    match WebDist::get(candidate) {
        Some(file) => {
            let mime = mime_guess::from_path(candidate).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(file.data.into_owned()))
                .unwrap()
        }
        None => match WebDist::get("index.html") {
            Some(idx) => Response::builder()
                .header(header::CONTENT_TYPE, "text/html")
                .body(Body::from(idx.data.into_owned()))
                .unwrap(),
            None => (StatusCode::NOT_FOUND, "asset not found").into_response(),
        },
    }
}
