//! `settings(key, value)` holds non-secret configuration. Secrets live in the key file, never here.

use crate::Db;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

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
/// `first_run_completed` lives here so the Welcome flow can flip it in one `PATCH /api/settings` alongside other general state. The migration seeds it to `"false"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    pub port: u16,
    pub open_browser_on_start: bool,
    pub audio_input_device: String,
    /// `true` once the user has completed `/welcome`. `useFirstRunRedirect` reads this to land the SPA on `/welcome` or `/`.
    pub first_run_completed: bool,
    /// Which STT engine `meetings/start.rs` constructs: `"cloud"` (`DeepgramStt`) or `"local"` (`WhisperLocal`). Seeded to `"cloud"` by V005.
    pub stt_provider: String,
    /// Whisper.cpp model to load when `stt_provider == "local"`. Must match a `name` in `yogurt_stt::models::REGISTRY`. Seeded to `"small.en"` by V005.
    pub stt_model: String,
    /// Watch for meeting app windows and offer to start recording. Detection never starts a recording on its own;
    /// it only surfaces the prompt (and stops a recording once the detected window closes). No seed row; absent means `true`.
    pub meeting_detection: bool,
    /// Raise the installed app when a meeting is detected. No seed row;
    /// absent means `true`.
    pub meeting_detection_focus: bool,
    /// Stop the recording when the meeting window closes or both channels
    /// stay silent. No seed row; absent means `true`.
    pub meeting_auto_stop: bool,
    /// Send the latest transcript to the LLM provider every few minutes while
    /// recording and show the bullets it returns. No seed row; absent means `true`.
    pub live_recap: bool,
    /// Whether the mic echo feature is exposed at all. Off by default; the
    /// in-meeting echo controls and auto-start only exist when this is on.
    pub audio_echo_feature: bool,
    /// Output device the mic echo plays to. `""` = system default.
    pub audio_echo_output_device: String,
    /// Persisted state of the in-meeting Echo button.
    pub audio_echo_enabled: bool,
    /// Echo output buffer size in frames, one of 128/256/512/1024/2048.
    pub audio_echo_buffer: u32,
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
        // Fallbacks keep the projection working on DBs where V005 has not run.
        stt_provider: get(db, "stt.provider")?.unwrap_or_else(|| "cloud".to_string()),
        stt_model: get(db, "stt.model")?.unwrap_or_else(|| "small.en".to_string()),
        meeting_detection: get(db, "general.meeting_detection")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(true),
        meeting_detection_focus: get(db, "general.meeting_detection_focus")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(true),
        meeting_auto_stop: get(db, "general.meeting_auto_stop")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(true),
        live_recap: get(db, "general.live_recap")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(true),
        audio_echo_feature: get(db, "audio.echo_feature")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(false),
        audio_echo_output_device: get(db, "audio.echo_output_device")?.unwrap_or_default(),
        audio_echo_enabled: get(db, "audio.echo_enabled")?
            .as_deref()
            .map(|s| s == "true")
            .unwrap_or(false),
        audio_echo_buffer: get(db, "audio.echo_buffer")?
            .as_deref()
            .and_then(|s| s.parse().ok())
            .unwrap_or(512),
    })
}

/// `None` fields are left untouched.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GeneralPatch {
    pub port: Option<u16>,
    pub open_browser_on_start: Option<bool>,
    pub audio_input_device: Option<String>,
    pub first_run_completed: Option<bool>,
    /// `"cloud"` (Deepgram) or `"local"` (WhisperLocal).
    pub stt_provider: Option<String>,
    /// One of the names in `yogurt_stt::models::REGISTRY`.
    pub stt_model: Option<String>,
    /// Silences the meeting-detected prompt.
    pub meeting_detection: Option<bool>,
    pub meeting_detection_focus: Option<bool>,
    pub meeting_auto_stop: Option<bool>,
    pub live_recap: Option<bool>,
    pub audio_echo_feature: Option<bool>,
    pub audio_echo_output_device: Option<String>,
    pub audio_echo_enabled: Option<bool>,
    pub audio_echo_buffer: Option<u32>,
}

/// Apply `patch` and return the resulting `General`. Loading after the write avoids assuming the patch landed when a concurrent writer overwrote the row.
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
    if let Some(d) = patch.meeting_detection_focus {
        set(
            db,
            "general.meeting_detection_focus",
            if d { "true" } else { "false" },
        )?;
    }
    if let Some(d) = patch.meeting_auto_stop {
        set(
            db,
            "general.meeting_auto_stop",
            if d { "true" } else { "false" },
        )?;
    }
    if let Some(r) = patch.live_recap {
        set(db, "general.live_recap", if r { "true" } else { "false" })?;
    }
    if let Some(f) = patch.audio_echo_feature {
        set(db, "audio.echo_feature", if f { "true" } else { "false" })?;
    }
    if let Some(d) = patch.audio_echo_output_device {
        set(db, "audio.echo_output_device", &d)?;
    }
    if let Some(e) = patch.audio_echo_enabled {
        set(db, "audio.echo_enabled", if e { "true" } else { "false" })?;
    }
    if let Some(b) = patch.audio_echo_buffer {
        set(db, "audio.echo_buffer", &b.to_string())?;
    }
    load_general(db)
}
