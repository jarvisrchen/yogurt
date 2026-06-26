//! Phase 6 (Plan 06-01) test-support module.
//!
//! Provides:
//! - [`MockChunksLlm`] — a deterministic `LlmClient` that yields a
//!   pre-canned sequence of `&'static str` chunks, then a terminal
//!   `done = true`. Used by `tests/chat_streaming.rs` and any future
//!   test that needs to assert against exact token boundaries.
//! - [`run_with_mock_llm`] — boot a full axum server (per-test isolated
//!   tempdir for storage + session token) wired with a `MockChunksLlm`.
//!   Returns the bound `SocketAddr`, the session token, the AppState
//!   handle (so the test can pre-create the meeting), and a tempdir
//!   guard.
//!
//! The whole module is feature-gated under the `test-support` cargo
//! feature so it only compiles into integration tests; release builds
//! never see this code.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::{self, BoxStream, StreamExt};
use uuid::Uuid;
use yogurt_llm::{ChatChunk, ChatRequest, ChatResponse, LlmClient};

use crate::meetings;
use crate::session::{load_or_create, SessionToken};
use crate::storage::Storage;
use crate::{AppState, Mode};

/// Pre-canned-chunks mock LLM. Each `stream` call replays `chunks` once,
/// then emits the terminal `done = true` frame.
#[derive(Clone)]
pub struct MockChunksLlm {
    chunks: Vec<String>,
}

impl MockChunksLlm {
    pub fn new(chunks: &[&str]) -> Self {
        Self {
            chunks: chunks.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

#[async_trait]
impl LlmClient for MockChunksLlm {
    async fn complete(&self, _req: ChatRequest) -> Result<ChatResponse> {
        Ok(ChatResponse {
            content: self.chunks.join(""),
            model: "mock-chunks".into(),
        })
    }

    async fn stream(&self, _req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        let mut frames: Vec<Result<ChatChunk>> = self
            .chunks
            .iter()
            .map(|s| {
                Ok(ChatChunk {
                    delta: s.clone(),
                    done: false,
                })
            })
            .collect();
        frames.push(Ok(ChatChunk {
            delta: String::new(),
            done: true,
        }));
        Ok(stream::iter(frames).boxed())
    }
}

/// Handle returned by [`run_with_mock_llm`]. Owns the tempdir; drop to
/// clean up the per-test storage + session-token files.
pub struct TestServer {
    pub addr: std::net::SocketAddr,
    pub token: String,
    pub state: AppState,
    pub _tmp: tempfile::TempDir,
}

/// Boot a server wired with [`MockChunksLlm`] and a fresh tempdir-scoped
/// storage handle. The caller is responsible for creating meetings,
/// seeding rows, and aborting the returned task.
pub async fn run_with_mock_llm(
    chunks: &[&str],
) -> Result<(TestServer, tokio::task::JoinHandle<()>)> {
    let probe = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = probe.local_addr()?;
    drop(probe);

    let tmp = tempfile::tempdir()?;
    let db_path = tmp.path().join("yogurt-test.db");
    let token_path = tmp.path().join("session-token");
    let storage = Arc::new(Storage::init_at(&db_path)?);
    let session_token = load_or_create(&token_path)?;
    let token_str = session_token.as_str().to_string();
    let session: Arc<SessionToken> = Arc::new(session_token);
    let (markdown_exporter, prompts) = crate::__test_only_aux_state(tmp.path().join("notes"))?;

    let state = AppState {
        mode: Mode::Release,
        storage,
        session,
        bind_port: addr.port(),
        meetings: meetings::Registry::new(),
        markdown_exporter,
        prompts,
        db: yogurt_db::Db::open_in_memory()?,
        keys: Arc::new(yogurt_db::keychain::MemoryKeyStore::default()),
        llm: Arc::new(MockChunksLlm::new(chunks)),
    };

    let app = crate::__test_router(state.clone());
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let server_state = state.clone();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    Ok((
        TestServer {
            addr,
            token: token_str,
            state: server_state,
            _tmp: tmp,
        },
        handle,
    ))
}

/// Seed a meeting row in BOTH the in-memory registry (so the WS handler
/// can attach) AND the Phase 0 storage meetings table (so the transcript
/// read in `spawn_stream` doesn't trip on a missing row). Returns the
/// minted meeting `Uuid`.
pub async fn seed_meeting(state: &AppState) -> Uuid {
    // Phase 6 chat uses the per-meeting events_tx for chunk delivery and
    // the storage meetings row for transcript context. Both must exist
    // for the chat-streaming integration tests to exercise the full
    // happy-path code without hitting NotFound branches.
    let m = state.meetings.create().await;
    let id_str = m.id.to_string();
    // We can't go through the read-only pool; reach the writer instead.
    let writer = state.storage.writer();
    let conn = writer.lock().expect("writer lock");
    conn.execute(
        r#"INSERT OR IGNORE INTO meetings (id, title, started_at, transcript_json)
              VALUES (?1, ?2, ?3, ?4)"#,
        rusqlite::params![id_str, "test meeting", 0i64, "[]"],
    )
    .expect("seed meetings row");
    // Mirror the row into yogurt-db's in-memory db so the FK on
    // chat_messages.meeting_id has a valid parent. The in-memory db
    // doesn't share the file with `storage` (it lives in :memory:), so
    // we create the meetings table inline if needed.
    let db_conn = state.db.conn();
    db_conn
        .execute_batch(
            r#"CREATE TABLE IF NOT EXISTS meetings (
                id TEXT PRIMARY KEY,
                title TEXT,
                started_at INTEGER NOT NULL,
                ended_at INTEGER,
                notes_md TEXT,
                enriched_md TEXT,
                transcript_json TEXT
            );"#,
        )
        .expect("create meetings table in yogurt-db");
    db_conn
        .execute(
            "INSERT OR IGNORE INTO meetings (id, title, started_at) VALUES (?, ?, ?)",
            rusqlite::params![id_str, "test meeting", 0i64],
        )
        .expect("seed meetings row in yogurt-db");
    m.id
}
