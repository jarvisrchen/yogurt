//! Settings KV store.
//!
//! `settings(key, value)` rows are the canonical persistence target for
//! non-secret configuration (SET-09). Secrets live in the Keychain, NEVER
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct General {
    pub port: u16,
    pub open_browser_on_start: bool,
    pub audio_input_device: String,
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
    })
}

/// All-optional patch type — `None` fields are left untouched.
#[derive(Debug, Clone, Deserialize)]
pub struct GeneralPatch {
    pub port: Option<u16>,
    pub open_browser_on_start: Option<bool>,
    pub audio_input_device: Option<String>,
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
    load_general(db)
}
