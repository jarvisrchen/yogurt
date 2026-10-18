//! The assistant row is created empty at POST time and filled by `update_chat_message_content` once the LLM stream terminates,
//! so GET history reconstructs the conversation across reloads.
//!
//! `list_chat_messages` must return rows by `(created_at ASC, id ASC)` so same-millisecond inserts replay in ULID order.
//! The frontend relies on this for streaming-bubble alignment.

use anyhow::{Context, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ulid::Ulid;

/// The DB CHECK constraint enforces the same two values; keep this enum in lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

impl Role {
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
    // Saturating cast keeps this infallible on a pre-epoch clock.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl crate::Db {
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

    /// Rows in `(created_at, id)` ASC order.
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
