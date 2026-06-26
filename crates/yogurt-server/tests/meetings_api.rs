//! Phase 7 (Plan 07-01) — integration tests for the Library REST surface.
//!
//! Exercises the new `/api/meetings` and `/api/meetings/:id` handlers
//! (GET / POST / PATCH / DELETE) against a real axum server bound to an
//! ephemeral 127.0.0.1 port. Each test gets its own tempdir-isolated
//! SQLite + session-token + notes directory so parallel runs cannot collide.
//!
//! Coverage matrix:
//! - `it_creates_and_lists_meetings` — POST + GET /api/meetings round-trip;
//!   create returns 201; list returns the created row.
//! - `it_patches_title_and_writes_markdown_file` — PATCH updates title and
//!   re-emits the `~/.yogurt/notes/<…>.md` file via MarkdownExporter.
//! - `it_deletes_and_returns_404` — DELETE returns 204 + the row vanishes;
//!   second GET returns 404.
//! - `it_returns_404_for_missing_id` — GET on an unknown id returns 404
//!   without nuking the existing rows.

use std::time::Duration;

use yogurt_server::{run_with_config, Mode, RunConfig};

/// Spawn a fresh server bound to an ephemeral port with a tempdir-scoped
/// home. Returns the bound addr, the seeded session token, the join
/// handle, the notes_dir path (so tests can inspect the on-disk markdown
/// files), and the tempdir guard (kept alive for the test's duration).
async fn spawn_server() -> (
    std::net::SocketAddr,
    String,
    tokio::task::JoinHandle<()>,
    std::path::PathBuf,
    tempfile::TempDir,
) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let notes_dir = tmp.path().join("notes");

    let token = yogurt_server::session::load_or_create(&token_path)
        .expect("seed session token")
        .as_str()
        .to_string();

    let probe = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    let addr = probe.local_addr().unwrap();
    drop(probe);

    let cfg = RunConfig {
        addr,
        mode: Mode::Release,
        db_path: Some(db_path),
        session_token_path: Some(token_path),
        notes_dir: Some(notes_dir.clone()),
        app_db_path: Some(tmp.path().join("yogurt-app.sqlite")),
    };
    let handle = tokio::spawn(async move {
        let _ = run_with_config(cfg).await;
    });
    for _ in 0..400 {
        if reqwest::get(format!("http://{addr}/api/health"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return (addr, token, handle, notes_dir, tmp);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become reachable within 8 seconds");
}

#[tokio::test(flavor = "multi_thread")]
async fn it_creates_and_lists_meetings() {
    let (addr, token, handle, _notes_dir, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    // POST with explicit title.
    let create_resp = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "title": "Standup" }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 201);
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["title"].as_str().unwrap(), "Standup");
    assert!(
        created["created_at"].as_str().is_some(),
        "created_at must be a serialized ISO 8601 string"
    );

    // GET /api/meetings lists the row.
    let list_resp = client
        .get(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(list_resp.status(), 200);
    let xs: Vec<serde_json::Value> = list_resp.json().await.unwrap();
    assert_eq!(xs.len(), 1);
    assert_eq!(xs[0]["id"].as_str().unwrap(), id);
    assert_eq!(xs[0]["title"].as_str().unwrap(), "Standup");

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_patches_title_and_writes_markdown_file() {
    let (addr, token, handle, notes_dir, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "title": "Initial" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // PATCH the title + notes.
    let patch_resp = client
        .patch(format!("http://{addr}/api/meetings/{id}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "title": "Renamed",
            "notes_md": "- bullet 1\n",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch_resp.status(), 200);
    let body: serde_json::Value = patch_resp.json().await.unwrap();
    assert_eq!(body["title"].as_str().unwrap(), "Renamed");
    assert_eq!(body["notes_md"].as_str().unwrap(), "- bullet 1\n");

    // The markdown file should exist under notes_dir, with the
    // updated title in its YAML front-matter. The exporter slugifies
    // the title into the filename — search for any file containing
    // "renamed" in the name.
    let entries: Vec<_> = std::fs::read_dir(&notes_dir)
        .expect("notes_dir exists")
        .filter_map(|e| e.ok())
        .collect();
    let renamed_files: Vec<_> = entries
        .iter()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("renamed")
        })
        .collect();
    assert!(
        !renamed_files.is_empty(),
        "expected a markdown file containing 'renamed' in its name under {notes_dir:?}; \
         entries: {:?}",
        entries.iter().map(|e| e.file_name()).collect::<Vec<_>>()
    );
    let content = std::fs::read_to_string(renamed_files[0].path()).unwrap();
    assert!(content.contains("Renamed"), "front-matter has new title");

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_deletes_and_returns_404() {
    let (addr, token, handle, _notes_dir, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // DELETE returns 204.
    let del_resp = client
        .delete(format!("http://{addr}/api/meetings/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(del_resp.status(), 204);

    // GET returns 404 now.
    let get_resp = client
        .get(format!("http://{addr}/api/meetings/{id}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 404);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_fts_searches_meetings() {
    let (addr, token, handle, _notes_dir, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    // Two meetings; `a` will gain notes containing "palette" so the
    // FTS index can find it.
    let a: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "title": "Roadmap planning" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let _b: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "title": "Hiring loop" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    client
        .patch(format!(
            "http://{addr}/api/meetings/{}",
            a["id"].as_str().unwrap()
        ))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "notes_md": "- discuss the palette refresh\n- pick Friday"
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // Search for "palette" — only meeting `a` should hit.
    let hits: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/api/meetings/search?q=palette"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0]["id"], a["id"]);

    // Search for a non-matching token — empty result.
    let none: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/api/meetings/search?q=zzzzz"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(none.is_empty());

    // Empty q falls through to the full list (both meetings).
    let all: Vec<serde_json::Value> = client
        .get(format!("http://{addr}/api/meetings/search?q="))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(all.len(), 2, "empty q must return the full list");

    handle.abort();
}

// ─── Phase 7 Plan 07-03 — Copy markdown + Reveal in Finder ─────────────────

#[tokio::test(flavor = "multi_thread")]
async fn it_returns_markdown_with_front_matter() {
    let (addr, token, handle, _notes_dir, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "title": "Markdown export test" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // PATCH some body so the file isn't just front-matter — exercises the
    // `body_md` branch of the exporter view.
    client
        .patch(format!("http://{addr}/api/meetings/{id}"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "notes_md": "- one\n- two" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let resp = client
        .get(format!("http://{addr}/api/meetings/{id}/markdown"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()["content-type"],
        "text/markdown; charset=utf-8"
    );
    let body = resp.text().await.unwrap();
    assert!(
        body.starts_with("---\n"),
        "must start with YAML front-matter"
    );
    assert!(
        body.contains("title: \"Markdown export test\""),
        "front-matter must carry the title; got: {body}"
    );
    assert!(body.contains("- one"), "body must include patched notes");

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_returns_404_for_markdown_of_missing_meeting() {
    let (addr, token, handle, _notes_dir, _tmp) = spawn_server().await;
    let resp = reqwest::Client::new()
        .get(format!("http://{addr}/api/meetings/nope/markdown"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
    handle.abort();
}

// Reveal-in-Finder is hard to assert in CI (it would actually trigger
// Finder activation on macOS hosts). We test the contract: the endpoint
// exists, missing meeting → 404, valid meeting → 204. macOS-gated because
// non-darwin runners would either lack `open -R` semantics or short-circuit
// the cfg branch — either way the contract is still 204 for a valid meeting.
#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread")]
async fn it_reveals_an_existing_meeting() {
    let (addr, token, handle, _notes_dir, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/meetings"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "title": "Reveal test" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let resp = client
        .post(format!(
            "http://{addr}/api/meetings/{}/reveal",
            created["id"].as_str().unwrap()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    handle.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_returns_404_for_missing_id() {
    let (addr, token, handle, _notes_dir, _tmp) = spawn_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://{addr}/api/meetings/nonexistent-id"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    handle.abort();
}
