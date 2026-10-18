//! `models::download_to` against an in-test axum mock that respects `Range:` headers.
//!
//! Covers a full download with SHA256 verify and progress callbacks, resume from a partial file via `Range: bytes=102400-` and a 206 response,
//! and a bad SHA producing an error mentioning "sha256" or "hash" with the file deleted.
//!
//! The mock is in-process rather than Wiremock to control `Content-Range` formatting, a frequent footgun.

#![cfg(feature = "local-stt")]

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, Response, StatusCode},
    routing::get,
    Router,
};
use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use yogurt_stt::models::{self, DownloadProgress};
use yogurt_stt::sha256;

const TOTAL: usize = 600_000;

fn payload() -> Vec<u8> {
    (0..TOTAL as u32).map(|i| (i % 251) as u8).collect()
}

#[derive(Clone)]
struct AppState {
    body: Arc<Vec<u8>>,
}

async fn serve_file(State(state): State<AppState>, headers: HeaderMap) -> Response<Body> {
    let total = state.body.len() as u64;

    // Only open-ended `START-` ranges, which is all `download_to` sends on resume.
    let range_start: Option<u64> = headers
        .get(header::RANGE)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("bytes="))
        .and_then(|s| s.strip_suffix('-'))
        .and_then(|s| s.parse::<u64>().ok());

    if let Some(start) = range_start {
        let end = total - 1;
        let slice = &state.body[start as usize..];
        return Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::CONTENT_LENGTH, slice.len())
            .header(header::ACCEPT_RANGES, "bytes")
            .header(
                header::CONTENT_RANGE,
                format!("bytes {}-{}/{}", start, end, total),
            )
            .body(Body::from(slice.to_vec()))
            .unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_LENGTH, total)
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::from(state.body.as_ref().clone()))
        .unwrap()
}

/// Spin up the mock on `127.0.0.1:0` and return its base URL.
async fn spawn_server() -> String {
    let state = AppState {
        body: Arc::new(payload()),
    };
    let app = Router::new()
        .route("/file.bin", get(serve_file))
        .with_state(state);

    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}/file.bin", addr)
}

#[tokio::test]
async fn it_downloads_a_full_file_and_verifies_sha256() {
    let url = spawn_server().await;
    let body = payload();
    let expected_sha = sha256::hash_bytes(&body);

    let tmp = tempfile::NamedTempFile::new().unwrap();
    let dest = tmp.path().to_path_buf();
    // Drop the handle so `download_to` recreates the empty tempfile.
    drop(tmp);
    let _ = std::fs::remove_file(&dest);

    let progress = Arc::new(std::sync::Mutex::new(Vec::<DownloadProgress>::new()));
    let progress_clone = Arc::clone(&progress);

    models::download_to(&url, &dest, &expected_sha, move |p| {
        progress_clone.lock().unwrap().push(p);
    })
    .await
    .expect("download should succeed");

    let actual = sha256::hash_file(&dest).unwrap();
    assert_eq!(actual, expected_sha, "downloaded file must match SHA256");

    let ticks = progress.lock().unwrap();
    assert!(
        !ticks.is_empty(),
        "progress callback must fire at least once"
    );
    let last = ticks.last().unwrap();
    assert_eq!(
        last.bytes_downloaded, last.total_bytes,
        "final tick should report 100% complete"
    );
    assert_eq!(
        last.total_bytes, TOTAL as u64,
        "total_bytes should equal payload size"
    );

    let _ = std::fs::remove_file(&dest);
}

#[tokio::test]
async fn it_resumes_from_a_partial_file() {
    let url = spawn_server().await;
    let body = payload();
    let expected_sha = sha256::hash_bytes(&body);

    let tmp_dir = tempfile::tempdir().unwrap();
    let dest = tmp_dir.path().join("partial.bin");

    {
        let mut f = std::fs::File::create(&dest).unwrap();
        f.write_all(&body[..100_000]).unwrap();
        f.sync_all().unwrap();
    }
    assert_eq!(std::fs::metadata(&dest).unwrap().len(), 100_000);

    models::download_to(&url, &dest, &expected_sha, |_| {})
        .await
        .expect("resume download should succeed");

    let final_hash = sha256::hash_file(&dest).unwrap();
    assert_eq!(
        final_hash, expected_sha,
        "resumed file should hash to the full payload"
    );
    assert_eq!(
        std::fs::metadata(&dest).unwrap().len(),
        TOTAL as u64,
        "resumed file should be full payload length"
    );
}

#[tokio::test]
async fn it_rejects_a_bad_sha_and_removes_the_file() {
    let url = spawn_server().await;

    let tmp_dir = tempfile::tempdir().unwrap();
    let dest = tmp_dir.path().join("badhash.bin");

    // 64 hex chars that are definitely wrong.
    let bogus_sha = "deadbeef".repeat(8);

    let err = models::download_to(&url, &dest, &bogus_sha, |_| {})
        .await
        .expect_err("must error on hash mismatch");
    let msg = format!("{}", err).to_ascii_lowercase();
    assert!(
        msg.contains("sha256") || msg.contains("hash") || msg.contains("mismatch"),
        "error message should mention sha256/hash/mismatch; got: {}",
        msg
    );
    assert!(
        !dest.exists(),
        "file must be deleted on hash mismatch; still exists at {:?}",
        dest
    );
}
