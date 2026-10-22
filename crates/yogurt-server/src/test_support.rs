//! Test support: [`MockChunksLlm`] and [`run_with_mock_llm`], gated behind the `test-support` feature.

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

/// Lets `tests/llm_resolution.rs` reach the `pub(crate)` resolution chain.
pub use crate::llm_openai::resolve;

/// Each `stream` call replays `chunks` once, then emits the terminal frame.
/// `requests` records every streamed request so tests can assert on the prompt.
#[derive(Clone)]
pub struct MockChunksLlm {
    chunks: Vec<String>,
    pub requests: Arc<std::sync::Mutex<Vec<ChatRequest>>>,
}

impl MockChunksLlm {
    pub fn new(chunks: &[&str]) -> Self {
        Self {
            chunks: chunks.iter().map(|s| (*s).to_string()).collect(),
            requests: Default::default(),
        }
    }
}

#[async_trait]
impl LlmClient for MockChunksLlm {
    fn model_name(&self) -> String {
        "mock-chunks".into()
    }

    async fn complete(&self, _req: ChatRequest) -> Result<ChatResponse> {
        Ok(ChatResponse {
            content: self.chunks.join(""),
            model: "mock-chunks".into(),
        })
    }

    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        self.requests.lock().unwrap().push(req);
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

/// Owns the tempdir; dropping it cleans up storage and the session token.
pub struct TestServer {
    pub addr: std::net::SocketAddr,
    pub token: String,
    pub state: AppState,
    pub llm: MockChunksLlm,
    pub _tmp: tempfile::TempDir,
}

/// The caller creates meetings and aborts the returned task.
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

    let db = yogurt_db::Db::open_in_memory()?;
    let meeting_repo = Arc::new(yogurt_db::MeetingRepo::new(db.clone()));
    let label_repo = Arc::new(yogurt_db::LabelRepo::new(db.clone()));
    let (app_events_tx, _) = tokio::sync::broadcast::channel(64);
    let llm = MockChunksLlm::new(chunks);
    let state = AppState {
        mode: Mode::Release,
        storage,
        session,
        bind_port: addr.port(),
        meetings: meetings::Registry::new(),
        markdown_exporter,
        prompts,
        db,
        keys: Arc::new(yogurt_db::keys::MemoryKeyStore::default()),
        llm_override: Some(Arc::new(llm.clone())),
        meeting_repo,
        label_repo,
        app_events_tx,
        detect: Default::default(),
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
            llm,
            _tmp: tmp,
        },
        handle,
    ))
}

/// Seeds the meeting in the in-memory registry (so the WS handler can attach) and in
/// `MeetingRepo` (so the `chat_messages.meeting_id` FK resolves).
pub async fn seed_meeting(state: &AppState) -> Uuid {
    let m = state.meetings.create().await;
    let id_str = m.id.to_string();
    // enhance.rs hydrates from this row on re-enhance.
    let writer = state.storage.writer();
    let conn = writer.lock().expect("writer lock");
    conn.execute(
        r#"INSERT OR IGNORE INTO meetings (id, title, started_at, transcript_json)
              VALUES (?1, ?2, ?3, ?4)"#,
        rusqlite::params![id_str, "test meeting", 0i64, "[]"],
    )
    .expect("seed storage meetings row");
    drop(conn);
    state
        .meeting_repo
        .create(yogurt_db::NewMeeting {
            title: "test meeting".into(),
            started_at_unix_ms: Some(0),
            id: Some(id_str.clone()),
        })
        .expect("seed yogurt-db meetings row");
    m.id
}
