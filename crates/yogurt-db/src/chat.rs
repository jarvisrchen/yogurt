//! Phase 6 (Plan 06-01) — `chat_messages` CRUD on the shared `Db` handle.
//!
//! The in-meeting chat UX persists one row per user turn and one row per
//! assistant turn. The assistant row is created **empty** at POST time and
//! mutated by `update_chat_message_content` once the LLM stream terminates
//! — that way GET history reconstructs the conversation faithfully across
//! reloads (CHAT-04 persistence requirement).
//!
//! ## Schema (see `migrations/V002__chat_messages.sql`)
//!
//! - `id` ULID (lexicographically sortable; doubles as a default ordering
//!   tiebreaker when two rows land in the same millisecond)
//! - `meeting_id` FK → `meetings(id)` ON DELETE CASCADE
//! - `role` TEXT CHECK IN ('user','assistant')
//! - `content` TEXT (may be empty for an in-flight assistant placeholder)
//! - `created_at` INTEGER unix millis
//!
//! ## Ordering invariant
//!
//! `list_chat_messages` MUST return rows by `(created_at ASC, id ASC)` so
//! same-millisecond inserts deterministically replay in ULID order. The
//! frontend relies on this for streaming-bubble alignment.

use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ulid::Ulid;

/// Persisted role for a chat message. The DB CHECK constraint enforces the
/// same two-value invariant — keep this enum in lockstep with the schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    /// Lowercase string representation matching the DB CHECK constraint
    /// (`'user'` | `'assistant'`).
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

impl FromStr for Role {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "user" => Ok(Role::User),
            "assistant" => Ok(Role::Assistant),
            other => Err(anyhow::anyhow!("unknown chat role: {other:?}")),
        }
    }
}

/// One row in the `chat_messages` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub meeting_id: String,
    pub role: Role,
    pub content: String,
    /// Unix milliseconds.
    pub created_at: i64,
}

impl ChatMessage {
    /// Mint a fresh row with a new ULID id and `now()` timestamp.
    pub fn new(meeting_id: impl Into<String>, role: Role, content: impl Into<String>) -> Self {
        Self {
            id: Ulid::new().to_string(),
            meeting_id: meeting_id.into(),
            role,
            content: content.into(),
            created_at: now_ms(),
        }
    }
}

fn now_ms() -> i64 {
    // Saturating cast — `SystemTime::now()` should always exceed the epoch
    // on any sane host; the saturate keeps the call infallible while still
    // producing a sortable monotonic-ish value.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl crate::Db {
    /// Insert a new chat_messages row. Caller-supplied `id` is honored
    /// (typically minted via `ChatMessage::new`).
    pub fn insert_chat_message(&self, msg: &ChatMessage) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            r#"INSERT INTO chat_messages (id, meeting_id, role, content, created_at)
                  VALUES (?1, ?2, ?3, ?4, ?5)"#,
            params![
                msg.id,
                msg.meeting_id,
                msg.role.as_str(),
                msg.content,
                msg.created_at,
            ],
        )
        .with_context(|| format!("insert chat_message id={}", msg.id))?;
        Ok(())
    }

    /// List all chat_messages rows for a meeting in `(created_at, id)`
    /// ASC order. Returns an empty vec if the meeting has no chat history.
    pub fn list_chat_messages(&self, meeting_id: &str) -> Result<Vec<ChatMessage>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            r#"SELECT id, meeting_id, role, content, created_at
                 FROM chat_messages
                WHERE meeting_id = ?1
                ORDER BY created_at ASC, id ASC"#,
        )?;
        let rows = stmt.query_map(params![meeting_id], row_to_message)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Look up one row by id. `Ok(None)` for missing rows.
    pub fn get_chat_message(&self, id: &str) -> Result<Option<ChatMessage>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            r#"SELECT id, meeting_id, role, content, created_at
                 FROM chat_messages WHERE id = ?1"#,
        )?;
        let mut rows = stmt.query_map(params![id], row_to_message)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Replace the `content` field on an existing row. Used by the chat
    /// streaming task to persist the accumulated assistant text once the
    /// LLM stream terminates.
    pub fn update_chat_message_content(&self, id: &str, content: &str) -> Result<()> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE chat_messages SET content = ?1 WHERE id = ?2",
            params![content, id],
        )?;
        if n == 0 {
            anyhow::bail!("update_chat_message_content: no row with id {id}");
        }
        Ok(())
    }
}

fn row_to_message(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChatMessage> {
    let role_str: String = r.get(2)?;
    let role = Role::from_str(&role_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into())
    })?;
    Ok(ChatMessage {
        id: r.get(0)?,
        meeting_id: r.get(1)?,
        role,
        content: r.get(3)?,
        created_at: r.get(4)?,
    })
}
