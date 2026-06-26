//! Providers CRUD + built-in presets.
//!
//! `providers` rows describe an OpenAI-compatible LLM endpoint the user has
//! configured (base URL + model + active flag). API keys are NOT stored
//! here — they live in the macOS Keychain via the [`crate::keychain`] module.
//!
//! The "single active LLM provider" invariant is enforced by a partial
//! unique index in `V001__initial.sql`:
//! `CREATE UNIQUE INDEX ... ON providers(kind) WHERE is_active = 1`.

use crate::Db;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Built-in provider presets surfaced as dashed-border chips in the Model
/// section's "CLONE A PRESET" row (PRD §5.6).
pub struct Preset {
    pub name: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
}

/// The five v1 presets. Order matters: it's the order chips appear in the UI.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Minimax",
        base_url: "https://api.minimax.io/v1",
        default_model: "MiniMax-Text-01",
    },
    Preset {
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
    },
    Preset {
        name: "Ollama (local)",
        base_url: "http://localhost:11434/v1",
        default_model: "llama3.2",
    },
    Preset {
        name: "LM Studio (local)",
        base_url: "http://localhost:1234/v1",
        default_model: "",
    },
    Preset {
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        default_model: "anthropic/claude-3.5-sonnet",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub is_active: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewProvider {
    pub name: String,
    pub base_url: String,
    pub model: String,
}

/// Insert a new provider (kind='llm', is_active=0) and return its ULID.
pub fn insert(db: &Db, p: NewProvider) -> Result<String> {
    let id = Ulid::new().to_string();
    let now = unix_millis();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO providers (id, name, base_url, model, kind, is_active, created_at) \
             VALUES (?1, ?2, ?3, ?4, 'llm', 0, ?5)",
            params![id, p.name, p.base_url, p.model, now],
        )
    })?;
    Ok(id)
}

/// List all LLM providers ordered by creation time (oldest first).
pub fn list(db: &Db) -> Result<Vec<Provider>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, name, base_url, model, is_active, created_at \
             FROM providers WHERE kind='llm' ORDER BY created_at ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Provider {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    base_url: r.get(2)?,
                    model: r.get(3)?,
                    is_active: r.get::<_, i64>(4)? != 0,
                    created_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok::<_, rusqlite::Error>(rows)
    })
    .map_err(Into::into)
}

/// List all LLM provider names. Cheaper than `list` when only the names matter
/// (e.g. the bootstrap idempotence check).
pub fn list_names(db: &Db) -> Result<Vec<String>> {
    db.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT name FROM providers WHERE kind='llm' ORDER BY created_at ASC")?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok::<_, rusqlite::Error>(rows)
    })
    .map_err(Into::into)
}

/// Return the active LLM provider, or `None` if none.
pub fn active(db: &Db) -> Result<Option<Provider>> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT id, name, base_url, model, is_active, created_at \
             FROM providers WHERE kind='llm' AND is_active=1",
            [],
            |r| {
                Ok(Provider {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    base_url: r.get(2)?,
                    model: r.get(3)?,
                    is_active: true,
                    created_at: r.get(5)?,
                })
            },
        )
        .optional()
    })
    .map_err(Into::into)
}

/// Atomically deactivate all LLM providers and activate `id`.
///
/// Wrapped in `BEGIN IMMEDIATE` so a partial failure rolls back (the partial
/// unique index would otherwise reject the second UPDATE mid-transaction
/// and leave the table in a no-active state).
pub fn set_active(db: &Db, id: &str) -> Result<()> {
    db.with_conn(|conn| -> rusqlite::Result<()> {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let res: rusqlite::Result<()> = (|| {
            conn.execute("UPDATE providers SET is_active=0 WHERE kind='llm'", [])?;
            let updated = conn.execute(
                "UPDATE providers SET is_active=1 WHERE id=?1 AND kind='llm'",
                params![id],
            )?;
            if updated == 0 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            Ok(())
        })();
        match res {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    })?;
    Ok(())
}

/// Update a provider's mutable fields. Does NOT touch `is_active`.
pub fn update(db: &Db, id: &str, name: &str, base_url: &str, model: &str) -> Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE providers SET name=?2, base_url=?3, model=?4 WHERE id=?1",
            params![id, name, base_url, model],
        )
    })?;
    Ok(())
}

/// Delete a provider row. Caller is responsible for deleting the matching
/// Keychain entry (see `keychain::ApiKeyStore::delete`).
pub fn delete(db: &Db, id: &str) -> Result<()> {
    db.with_conn(|conn| conn.execute("DELETE FROM providers WHERE id=?1", params![id]))?;
    Ok(())
}

fn unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
