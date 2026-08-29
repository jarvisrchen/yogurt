//! Shared per-request `AppState`.
//!
//! Phase 5 (Plan 05-02) extends Phase 4's `AppState` with two new fields:
//! - [`db`](AppState::db) — the new [`yogurt_db::Db`] (providers + settings
//!   tables in the same `~/.yogurt/db.sqlite` file as Phase 0 `storage`).
//! - [`keys`](AppState::keys) — an `Arc<dyn ApiKeyStore>` over
//!   `~/.yogurt/keys.json` (`FileKeyStore` in production, `MemoryKeyStore`
//!   in tests).

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use yogurt_db::keys::{ApiKeyStore, FileKeyStore, MemoryKeyStore};
use yogurt_db::{Db, LabelRepo, Meeting, MeetingPatch, MeetingRepo};
use yogurt_llm::LlmClient;

use crate::markdown_exporter::{MarkdownExporter, Meeting as ExpMeeting};
use crate::meetings;
use crate::session::SessionToken;
use crate::storage::Storage;
use crate::Mode;

/// Shared per-request app state.
///
/// Phase 4 fields (storage, session, meetings, markdown_exporter, prompts)
/// are preserved verbatim. Phase 5 adds `db` and `keys`.
#[derive(Clone)]
pub struct AppState {
    pub mode: Mode,
    pub storage: Arc<Storage>,
    pub session: Arc<SessionToken>,
    pub bind_port: u16,
    /// Phase 3: in-memory meeting registry.
    pub meetings: Arc<meetings::Registry>,
    /// Phase 4 (Plan 04-03): single-writer markdown file emitter.
    pub markdown_exporter: Arc<MarkdownExporter>,
    /// Phase 4 (Plan 04-03): bundled prompt templates.
    pub prompts: Arc<yogurt_prompts::Prompts>,
    /// Phase 5 (Plan 05-02): SQLite-backed providers + settings tables.
    pub db: Db,
    /// Phase 5 (Plan 05-02): API-key storage abstraction. `FileKeyStore`
    /// in production, `MemoryKeyStore` in tests.
    pub keys: Arc<dyn ApiKeyStore>,
    /// Test-only LLM override. `None` in production — enhance AND chat
    /// resolve the real client per-request via `llm_openai::resolve`
    /// (env vars → active provider row + stored key → MockLlm when nothing
    /// is configured). Tests inject `Some(mock)` to keep streaming tests
    /// deterministic. Never set this in a production constructor: a fixed
    /// client here silently ignores the user's configured provider (the
    /// original Phase 6 "chat always answers empty" bug).
    pub llm_override: Option<Arc<dyn LlmClient>>,
    /// Phase 7 (Plan 07-01): SQLite-backed Library directory.
    ///
    /// Coexists with the Phase-3 in-memory streaming registry on
    /// `Self::meetings` — `meetings` owns the live audio/transcript
    /// broadcasts, while `meeting_repo` owns the persisted directory
    /// (title, timestamps, starred flag, persisted notes/transcript/
    /// enriched bodies). REST handlers serving `/api/meetings*` go
    /// through `meeting_repo`; the WebSocket and recording pipeline
    /// still uses `meetings`.
    pub meeting_repo: Arc<MeetingRepo>,
    /// Feature: Granola-style meeting labels — SQLite-backed label
    /// directory (workspace-level named tags with a color).
    pub label_repo: Arc<LabelRepo>,
    /// Phase 8 (Plan 08-03): app-wide event broadcaster.  Carries JSON
    /// frames that aren't tied to a specific meeting — currently
    /// `stt_model_download_*` from `api::stt_models`.  The `/ws`
    /// handler subscribes once per upgrade and fans frames to its
    /// browser client.  Capacity 64 absorbs the ~2 Hz progress tick.
    pub app_events_tx: crate::ws::AppEventTx,
}

impl AppState {
    /// Phase 7 (Plan 07-01) helper: apply a `MeetingPatch` to the SQLite
    /// directory AND re-emit the canonical markdown file in
    /// `~/.yogurt/notes/`. Both layers stay in lockstep so the Phase 4
    /// "every notes/enriched mutation funnels through MarkdownExporter"
    /// invariant (STORE-04) survives the move from in-memory state to
    /// the new repo.
    ///
    /// The markdown write is best-effort here — failures bubble up via
    /// the returned `Result` so REST handlers can map them to 500. The
    /// SQLite write happens FIRST so a fail-during-write doesn't leave
    /// a stale file on disk pointing at outdated content.
    pub fn patch_and_export(&self, id: &str, p: MeetingPatch) -> anyhow::Result<Meeting> {
        let m = self.meeting_repo.patch(id, p)?;
        self.markdown_exporter.write(&ExpMeeting {
            id: &m.id,
            title: &m.title,
            started_at_unix_ms: m.started_at,
            ended_at_unix_ms: m.ended_at,
            // Prefer enriched body over raw notes when both exist —
            // matches Phase 4 `enhance` handler's emit choice.
            body_md: m.enriched_md.as_deref().unwrap_or(&m.notes_md),
        })?;
        Ok(m)
    }
}

/// Configuration for production constructors. Mirrors the field set of
/// `crate::RunConfig` so the top-level `run_with_config` can hand it through
/// without reshuffling.
pub struct ProductionConfig {
    pub mode: Mode,
    pub bind_port: u16,
    pub storage: Arc<Storage>,
    pub session: Arc<SessionToken>,
    pub notes_dir: PathBuf,
    /// Phase 5 collateral fix (SET-12): optional override for the
    /// `yogurt-db` SQLite path. Defaults to `~/.yogurt/db.sqlite` via
    /// `Db::open_default()`. Tests pass a tempdir-scoped path so parallel
    /// suites do not collide on the real user DB's WAL lock — matching how
    /// `RunConfig::db_path` already isolates the Phase 0 storage handle.
    pub app_db_path: Option<PathBuf>,
}

impl AppState {
    /// Build the Phase 5 `AppState` for production:
    /// - `Db::open_default()` opens `~/.yogurt/db.sqlite` (shared with
    ///   Phase 0 storage; tables are disjoint).
    /// - `FileKeyStore` over `~/.yogurt/keys.json`.
    pub fn production(cfg: ProductionConfig) -> Result<Self> {
        let exporter = Arc::new(MarkdownExporter::new(cfg.notes_dir)?);
        let prompt_mode = match cfg.mode {
            Mode::Dev => yogurt_prompts::Mode::Dev,
            Mode::Release => yogurt_prompts::Mode::Release,
        };
        let prompts = Arc::new(yogurt_prompts::Prompts::load(prompt_mode)?);
        // SET-12: honor optional db-path override so test suites can
        // tempdir-isolate the Phase 5 db just like Phase 0 storage.
        let db = match cfg.app_db_path {
            Some(p) => Db::open(&p)?,
            None => Db::open_default()?,
        };
        let meeting_repo = Arc::new(MeetingRepo::new(db.clone()));
        let label_repo = Arc::new(LabelRepo::new(db.clone()));
        let (app_events_tx, _) = tokio::sync::broadcast::channel(64);
        // Integration/CLI tests set `YOGURT_MEMORY_KEYSTORE` so they never
        // touch the user's real key file.
        let keys: Arc<dyn ApiKeyStore> =
            if std::env::var("YOGURT_MEMORY_KEYSTORE").is_ok_and(|v| !v.is_empty()) {
                tracing::info!("YOGURT_MEMORY_KEYSTORE set — using in-memory key store");
                Arc::new(MemoryKeyStore::default())
            } else {
                Arc::new(FileKeyStore::open_default()?)
            };
        Ok(Self {
            mode: cfg.mode,
            storage: cfg.storage,
            session: cfg.session,
            bind_port: cfg.bind_port,
            meetings: meetings::Registry::new(),
            markdown_exporter: exporter,
            prompts,
            db,
            keys,
            llm_override: None,
            // Phase 7 (Plan 07-01): the new SQLite-backed Library directory.
            meeting_repo,
            label_repo,
            // Phase 8 (Plan 08-03): app-wide event broadcaster — see field doc.
            app_events_tx,
        })
    }

    /// Test wiring: real Phase 0 storage at `storage_path`, in-memory
    /// yogurt-db, and a `MemoryKeyStore`. Intended for `tests/bootstrap.rs`
    /// and similar suites that need the new `db` + `keys` surface without
    /// touching the real key file.
    pub fn in_memory(
        mode: Mode,
        storage: Arc<Storage>,
        session: Arc<SessionToken>,
        bind_port: u16,
        notes_dir: PathBuf,
    ) -> Result<Self> {
        let exporter = Arc::new(MarkdownExporter::new(notes_dir)?);
        let prompt_mode = match mode {
            Mode::Dev => yogurt_prompts::Mode::Dev,
            Mode::Release => yogurt_prompts::Mode::Release,
        };
        let prompts = Arc::new(yogurt_prompts::Prompts::load(prompt_mode)?);
        let db = Db::open_in_memory()?;
        let meeting_repo = Arc::new(MeetingRepo::new(db.clone()));
        let label_repo = Arc::new(LabelRepo::new(db.clone()));
        let (app_events_tx, _) = tokio::sync::broadcast::channel(64);
        Ok(Self {
            mode,
            storage,
            session,
            bind_port,
            meetings: meetings::Registry::new(),
            markdown_exporter: exporter,
            prompts,
            db,
            keys: Arc::new(MemoryKeyStore::default()),
            llm_override: None,
            meeting_repo,
            label_repo,
            // Phase 8 (Plan 08-03): app-wide event broadcaster — see field doc.
            app_events_tx,
        })
    }
}
