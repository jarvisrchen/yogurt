//! Shared per-request `AppState`.

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

#[derive(Clone)]
pub struct AppState {
    pub mode: Mode,
    pub storage: Arc<Storage>,
    pub session: Arc<SessionToken>,
    pub bind_port: u16,
    /// In-memory registry owning the live audio and transcript broadcasts.
    pub meetings: Arc<meetings::Registry>,
    pub markdown_exporter: Arc<MarkdownExporter>,
    pub prompts: Arc<yogurt_prompts::Prompts>,
    pub db: Db,
    /// `FileKeyStore` in production, `MemoryKeyStore` in tests.
    pub keys: Arc<dyn ApiKeyStore>,
    /// Shown in Settings, so a `$YOGURT_DATA_DIR` instance names its own files.
    pub data_dir: PathBuf,
    pub keys_path: PathBuf,
    /// Test-only. A fixed client set in production would ignore the user's configured provider,
    /// so enhance and chat resolve per request via `llm_openai::resolve`.
    pub llm_override: Option<Arc<dyn LlmClient>>,
    /// Persisted directory behind `/api/meetings*`. The WebSocket and recording pipeline use
    /// `meetings` instead.
    pub meeting_repo: Arc<MeetingRepo>,
    pub label_repo: Arc<LabelRepo>,
    /// JSON frames not tied to a meeting, such as `stt_model_download_*`. Each `/ws` upgrade
    /// subscribes once. Capacity 64 absorbs the ~2 Hz progress tick.
    pub app_events_tx: crate::ws::AppEventTx,
    /// Written by the `crate::detect` watcher, read by `GET /api/meetings/detected`.
    pub detect: crate::detect::SharedDetectState,
    /// Serializes everything that uses the Yogurt Cloud refresh token: connect, push, disconnect.
    /// Supabase rotates the token on every use, so two overlapping refreshes would kill the connection.
    pub cloud_lock: Arc<tokio::sync::Mutex<()>>,
    /// The Granola import job and its last result.
    pub import: Arc<crate::granola_import::ImportJob>,
    /// The one background Yogurt Cloud push and its progress.
    pub cloud_push: Arc<crate::cloud_push::PushJob>,
}

impl AppState {
    /// The one stop path for manual and auto stops. The first stop stamps `ended_at`.
    pub async fn stop_meeting(&self, id: &uuid::Uuid) -> anyhow::Result<()> {
        self.meetings.stop(id).await?;
        self.cloud_push
            .request(chrono::Utc::now().timestamp_millis());
        let repo = self.meeting_repo.clone();
        let id_str = id.to_string();
        let stamped = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            if let Some(m) = repo.get(&id_str)? {
                if m.ended_at.is_none() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0);
                    repo.patch(
                        &id_str,
                        MeetingPatch {
                            ended_at: Some(Some(now)),
                            ..Default::default()
                        },
                    )?;
                }
            }
            Ok(())
        })
        .await;
        if !matches!(stamped, Ok(Ok(()))) {
            tracing::warn!(meeting = %id, ?stamped, "failed to stamp ended_at");
        }
        Ok(())
    }

    /// Patches SQLite, then re-emits the markdown file. SQLite goes first so a failed write
    /// never leaves a file ahead of the database. Export errors bubble up for a 500.
    pub fn patch_and_export(&self, id: &str, p: MeetingPatch) -> anyhow::Result<Meeting> {
        let m = self.meeting_repo.patch(id, p)?;
        self.markdown_exporter.write(&ExpMeeting {
            id: &m.id,
            title: &m.title,
            started_at_unix_ms: m.started_at,
            ended_at_unix_ms: m.ended_at,
            // Same choice the enhance handler makes.
            body_md: m.enriched_md.as_deref().unwrap_or(&m.notes_md),
        })?;
        Ok(m)
    }
}

pub struct ProductionConfig {
    pub mode: Mode,
    pub bind_port: u16,
    pub storage: Arc<Storage>,
    pub session: Arc<SessionToken>,
    pub notes_dir: PathBuf,
    pub data_dir: PathBuf,
    /// Defaults to `~/.yogurt/db.sqlite`. Tests pass a tempdir so parallel suites do not
    /// collide on the real DB's WAL lock.
    pub app_db_path: Option<PathBuf>,
    /// Defaults to `~/.yogurt/keys.json`.
    pub keys_path: Option<PathBuf>,
}

impl AppState {
    /// Opens `~/.yogurt/db.sqlite` (shared with `storage`, disjoint tables) and a `FileKeyStore`
    /// over `~/.yogurt/keys.json`, unless the config overrides either path.
    pub fn production(cfg: ProductionConfig) -> Result<Self> {
        let exporter = Arc::new(MarkdownExporter::new(cfg.notes_dir)?);
        let prompt_mode = match cfg.mode {
            Mode::Dev => yogurt_prompts::Mode::Dev,
            Mode::Release => yogurt_prompts::Mode::Release,
        };
        let prompts = Arc::new(yogurt_prompts::Prompts::load(prompt_mode)?);
        let db = match cfg.app_db_path {
            Some(p) => Db::open(&p)?,
            None => Db::open_default()?,
        };
        let meeting_repo = Arc::new(MeetingRepo::new(db.clone()));
        let label_repo = Arc::new(LabelRepo::new(db.clone()));
        let (app_events_tx, _) = tokio::sync::broadcast::channel(64);
        let keys_path = match cfg.keys_path {
            Some(p) => p,
            None => yogurt_db::paths::keys_path()?,
        };
        // Integration/CLI tests set `YOGURT_MEMORY_KEYSTORE` so they never
        // touch the user's real key file.
        let keys: Arc<dyn ApiKeyStore> =
            if std::env::var("YOGURT_MEMORY_KEYSTORE").is_ok_and(|v| !v.is_empty()) {
                tracing::info!("YOGURT_MEMORY_KEYSTORE set — using in-memory key store");
                Arc::new(MemoryKeyStore::default())
            } else {
                Arc::new(FileKeyStore::open(keys_path.clone())?)
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
            data_dir: cfg.data_dir,
            keys_path,
            llm_override: None,
            meeting_repo,
            label_repo,
            app_events_tx,
            detect: Default::default(),
            cloud_lock: Default::default(),
            import: Default::default(),
            cloud_push: Default::default(),
        })
    }

    /// Test wiring: real `storage`, in-memory `Db`, and a `MemoryKeyStore` that never touches
    /// the real key file.
    pub fn in_memory(
        mode: Mode,
        storage: Arc<Storage>,
        session: Arc<SessionToken>,
        bind_port: u16,
        notes_dir: PathBuf,
    ) -> Result<Self> {
        let data_dir = notes_dir.parent().unwrap_or(&notes_dir).to_path_buf();
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
            keys_path: data_dir.join("keys.json"),
            data_dir,
            llm_override: None,
            meeting_repo,
            label_repo,
            app_events_tx,
            detect: Default::default(),
            cloud_lock: Default::default(),
            import: Default::default(),
            cloud_push: Default::default(),
        })
    }
}
