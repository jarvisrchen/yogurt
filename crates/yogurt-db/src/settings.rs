//! Settings KV store.
//!
//! `settings(key, value)` rows are the canonical persistence target for
//! non-secret configuration (SET-09). Secrets live in the key file, NEVER
//! here. `General` is a typed projection over the well-known keys.

use crate::Db;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Raw KV read. Returns `Ok(None)` when the key is absent.
pub fn get(db: &Db, key: &str) -> Result<Option<String>> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT value FROM settings WHERE key=?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
    })
    .map_err(Into::into)
}

/// Raw KV upsert. Idempotent — repeated `set` calls with the same value are
/// a no-op at the application layer.
pub fn set(db: &Db, key: &str, value: &str) -> Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )
    })?;
    Ok(())
}

/// Typed projection of the well-known `general.*` + `audio.*` keys.
///
/// `first_run_completed` is read/written here (rather than via a dedicated
/// onboarding module) so the Phase 7 Welcome flow can flip it in a single
/// `PATCH /api/settings` round-trip alongside any general-section state.
/// The migration seeds it to `"false"` (V003__meetings.sql).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    pub port: u16,
    pub open_browser_on_start: bool,
    pub audio_input_device: String,
    /// Phase 7 onboarding: `true` once the user has completed the `/welcome`
    /// flow at least once. `useFirstRunRedirect` reads this to decide whether
    /// to land the SPA on `/welcome` or `/`.
    pub first_run_completed: bool,
    /// Phase 8 (Plan 08-03): which STT engine `meetings/start.rs` should
    /// construct. `"cloud"` → `DeepgramStt`; `"local"` → `WhisperLocal`.
    /// Seeded to `"cloud"` by V005.
    pub stt_provider: String,
    /// Phase 8 (Plan 08-03): which whisper.cpp model to load when
    /// `stt_provider == "local"`.  Must match a `name` in
    /// `yogurt_stt::models::REGISTRY` (`tiny.en` / `small.en` /
    /// `medium.en` / `large-v3`).  Seeded to `"small.en"` by V005 per
    /// D-02 baseline.
    pub stt_model: String,
    /// MTG-11: watch for meeting app windows and offer to start
    /// recording. Detection never starts a recording on its own — it only
    /// surfaces the prompt (and stops a recording once the detected
    /// window closes). Defaults to `true`; there is no seed row, so the
    /// projection's fallback is the default.
    pub meeting_detection: bool,
}

/// Load the `General` struct from the KV table, falling back to defaults
/// for missing or malformed values.
pub fn load_general(db: &Db) -> Result<General> {
    Ok(General {
        port: get(db, "general.port")?
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(7878),
        open_browser_on_start: get(db, "general.open_browser_on_start")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(true),
        audio_input_device: get(db, "audio.input_device")?.unwrap_or_default(),
        first_run_completed: get(db, "first_run_completed")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(false),
        // Phase 8 (Plan 08-03): defaults match the V005 seed rows.  The
        // unwrap_or fallbacks make the projection robust to legacy
        // user DBs where the V005 migration hasn't run yet (which
        // shouldn't happen, but defense-in-depth).
        stt_provider: get(db, "stt.provider")?.unwrap_or_else(|| "cloud".to_string()),
        stt_model: get(db, "stt.model")?.unwrap_or_else(|| "small.en".to_string()),
        meeting_detection: get(db, "general.meeting_detection")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(true),
    })
}

/// All-optional patch type — `None` fields are left untouched.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GeneralPatch {
    pub port: Option<u16>,
    pub open_browser_on_start: Option<bool>,
    pub audio_input_device: Option<String>,
    /// Phase 7 — the Welcome route flips this to `true` when the user
    /// clicks "Take me to my meetings →".
    pub first_run_completed: Option<bool>,
    /// Phase 8 (Plan 08-03): `"cloud"` (Deepgram) or `"local"`
    /// (WhisperLocal).  Settings page flips this when the user clicks
    /// "Use Local" on the LocalSTTCard.
    pub stt_provider: Option<String>,
    /// Phase 8 (Plan 08-03): one of the names in
    /// `yogurt_stt::models::REGISTRY`.  Settings page flips this when
    /// the user clicks a downloaded model pill.
    pub stt_model: Option<String>,
    /// MTG-11 — Settings → General flips this to silence the
    /// meeting-detected prompt.
    pub meeting_detection: Option<bool>,
}

/// Apply `patch` to the KV table and return the resulting `General`. The
/// load-after-write avoids a class of bugs where the caller assumes the
/// patch landed but a concurrent writer overwrote the row.
pub fn save_general_patch(db: &Db, patch: GeneralPatch) -> Result<General> {
    if let Some(p) = patch.port {
        set(db, "general.port", &p.to_string())?;
    }
    if let Some(o) = patch.open_browser_on_start {
        set(
            db,
            "general.open_browser_on_start",
            if o { "true" } else { "false" },
        )?;
    }
    if let Some(d) = patch.audio_input_device {
        set(db, "audio.input_device", &d)?;
    }
    if let Some(f) = patch.first_run_completed {
        set(db, "first_run_completed", if f { "true" } else { "false" })?;
    }
    if let Some(p) = patch.stt_provider {
        set(db, "stt.provider", &p)?;
    }
    if let Some(m) = patch.stt_model {
        set(db, "stt.model", &m)?;
    }
    if let Some(d) = patch.meeting_detection {
        set(
            db,
            "general.meeting_detection",
            if d { "true" } else { "false" },
        )?;
    }
    load_general(db)
}
