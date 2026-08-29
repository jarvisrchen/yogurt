//! Providers CRUD + built-in presets.
//!
//! `providers` rows describe an OpenAI-compatible LLM endpoint the user has
//! configured (base URL + model + active flag). API keys are NOT stored
//! here - they live in `~/.yogurt/keys.json` via the [`crate::keys`] module.
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
    /// Static list of popular model ids for this preset. Used by the
    /// Settings UI's MODEL `<datalist>` so a freshly-cloned provider has
    /// autocomplete suggestions before the user pastes a key. `default_model`
    /// is always included here so the initial state of the dropdown matches
    /// what gets saved. The user can still type any custom model - this is
    /// autocomplete, not a hard constraint.
    ///
    /// Lists go stale when a provider ships a new model; the Settings page
    /// exposes a `Refresh` button that replaces the datalist with the live
    /// `GET {base_url}/models` response (requires a stored key) and a
    /// `docs_url` link for when `/models` is wrong / missing / behind auth.
    pub models: &'static [&'static str],
    /// Public URL of the provider's "available models" page. Rendered by
    /// the Settings page as a small `See all models →` link next to the
    /// MODEL field - useful both as a Refresh fallback (some providers
    /// don't expose `/v1/models`) and as a discovery surface for preview /
    /// regional models the static list will never have.
    pub docs_url: &'static str,
}

/// Built-in presets. Order matters: it's the order chips appear in the UI.
/// Cloud majors first, then local runtimes, then the aggregator.
///
/// Every entry must speak the OpenAI `/chat/completions` shape, because
/// `OpenAiCompatClient` is the only client `yogurt-llm` ships - a preset is
/// purely a saved base URL + model, never a new adapter.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Minimax",
        base_url: "https://api.minimax.io/v1",
        default_model: "MiniMax-Text-01",
        models: &[
            "MiniMax-Text-01",
            "minimax-text-01-250515",
            "abab6.5s-chat",
            "abab6.5-chat",
        ],
        docs_url: "https://platform.MiniMax.io/docs/api-reference",
    },
    Preset {
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o-mini",
        models: &[
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4.1",
            "gpt-4.1-mini",
            "o4-mini",
            "o3",
        ],
        docs_url: "https://platform.openai.com/docs/models",
    },
    // Google exposes Gemini through an OpenAI-compatible shim; the native
    // `generativeLanguage` REST shape is NOT what we speak. The trailing
    // slash is load-bearing upstream but harmless here - `OpenAiCompatClient`
    // trims it before appending `/chat/completions`.
    Preset {
        name: "Google Gemini",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        default_model: "gemini-2.5-flash",
        models: &[
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.0-flash",
            "gemini-2.0-flash-lite",
            "gemini-1.5-pro",
            "gemini-1.5-flash",
        ],
        docs_url: "https://ai.google.dev/gemini/docs/models",
    },
    Preset {
        name: "DeepSeek",
        base_url: "https://api.deepseek.com/v1",
        default_model: "deepseek-chat",
        models: &["deepseek-chat", "deepseek-reasoner"],
        docs_url: "https://api-docs.deepseek.com/quick_start/pricing",
    },
    Preset {
        name: "Ollama (local)",
        base_url: "http://localhost:11434/v1",
        default_model: "llama3.2",
        // Ollama models depend entirely on what the user has pulled
        // locally, so these are hints, not a guarantee - a few common
        // `ollama pull` targets to seed the datalist before the user has
        // pulled anything. Ollama needs no key, so Refresh (live
        // `GET /v1/models`) is one click away and always wins once
        // there's a real local list to show.
        models: &["llama3.2", "llama3.1", "mistral", "qwen2.5", "gemma2"],
        docs_url: "https://ollama.com/library",
    },
    Preset {
        name: "LM Studio (local)",
        base_url: "http://localhost:1234/v1",
        default_model: "",
        // Empty, unlike Ollama: LM Studio has no well-known model names to
        // hint at, since whatever's loaded is purely local (a GGUF the
        // user downloaded). Refresh is the only way this datalist ever
        // gets populated.
        models: &[],
        docs_url: "https://lmstudio.ai/models",
    },
    Preset {
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        default_model: "anthropic/claude-3.5-sonnet",
        models: &[
            "anthropic/claude-3.5-sonnet",
            "anthropic/claude-3.5-haiku",
            "openai/gpt-4o-mini",
            "google/gemini-2.5-flash",
            "meta-llama/llama-3.1-70b-instruct",
        ],
        docs_url: "https://openrouter.ai/models",
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
/// key-file entry (see `keys::ApiKeyStore::delete`).
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
