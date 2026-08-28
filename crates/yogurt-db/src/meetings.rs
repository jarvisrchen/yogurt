//! Phase 7 (Plan 07-01) — `meetings` repo backing the Library route.
//!
//! Replaces the Phase-3 in-memory `HashMap<MeetingId, Meeting>` registry
//! with a SQLite-backed CRUD layer that survives restarts. The streaming
//! surface (audio + transcript broadcasts, capture-thread join handles)
//! still lives in-memory on `yogurt-server::meetings::Meeting`; THIS
//! module owns the **directory** of meetings (id, title, timestamps,
//! starred flag, persisted notes/transcript/enriched bodies).
//!
//! ## Wire contract
//!
//! `created_at` / `updated_at` travel as ISO 8601 UTC strings via the
//! `chrono::DateTime<Utc>` serde adapter. The DB column stores unix
//! milliseconds so SQL comparisons (newest-first ordering, starred index)
//! stay cheap; conversion happens in [`row_to_meeting`].
//!
//! `enriched_md: Option<Option<String>>` on `MeetingPatch` is the
//! load-bearing tri-state: `None` means "leave alone", `Some(None)` means
//! "explicitly clear" (e.g. user retracted the enhancement), `Some(Some(s))`
//! means "set to s". The `Option<Option<T>>` pattern is the conventional
//! serde way to round-trip a JSON `null` distinctly from a missing field
//! when paired with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
//!
//! ## Concurrency
//!
//! Repo methods are synchronous because `Db::with_conn` holds a
//! `MutexGuard<Connection>`. Callers that need them async-friendly should
//! wrap the call in `tokio::task::spawn_blocking` — the same pattern the
//! enhance handler already uses for the storage writer. The Library REST
//! handlers in `crates/yogurt-server/src/api/meetings.rs` do exactly that.

use crate::Db;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// One row in the `meetings` table, projected onto the Library wire shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    /// Unix ms — meeting recording-start clock (distinct from `created_at`
    /// which is the SQLite row's birthday).
    pub started_at: i64,
    /// Unix ms; `None` while the meeting is still live.
    pub ended_at: Option<i64>,
    pub notes_md: String,
    pub enriched_md: Option<String>,
    pub transcript_json: String,
    pub starred: bool,
    /// STT engine provenance, e.g. "local · small.en" or "cloud · nova-3".
    /// `None` for meetings recorded before this column existed.
    pub stt_engine: Option<String>,
    /// Row creation timestamp (ISO 8601 over the wire).
    pub created_at: DateTime<Utc>,
    /// Last mutation timestamp (ISO 8601 over the wire). Updated on every
    /// `patch` call so the Library can re-sort on edit.
    pub updated_at: DateTime<Utc>,
}

/// Constructor payload for [`MeetingRepo::create`]. Only the fields a fresh
/// meeting genuinely needs — the rest are filled in by the repo (defaults +
/// auto-minted id + timestamps).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewMeeting {
    pub title: String,
    /// Optional started-at override. Defaults to "now" when absent —
    /// matches the Phase-3 in-memory registry where `Meeting::new` minted
    /// `created_at_ms = SystemTime::now()`.
    #[serde(default)]
    pub started_at_unix_ms: Option<i64>,
    /// Optional caller-supplied id. Defaults to a freshly minted ULID
    /// when absent. The Phase-7 Library REST handler passes
    /// `Uuid::now_v7().to_string()` here so the existing UUID-typed
    /// streaming routes (`/api/meetings/:id/start` etc.) keep working
    /// against the same id — without this escape hatch the directory
    /// (ULID strings) and the streaming registry (UUID v7) would have
    /// mutually-incompatible identifier spaces.
    #[serde(default)]
    pub id: Option<String>,
}

/// Patch payload for [`MeetingRepo::patch`]. Every field is `Option`
/// because a PATCH only carries the values the caller wants to change.
///
/// The `enriched_md` field is `Option<Option<String>>` on purpose — see
/// the module-level docs for the tri-state semantics.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MeetingPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Updates the recording-start clock. Use for back-dated imports or
    /// for the Phase 4 enhance handler which sets `started_at` to the
    /// known recording start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_md: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_json: Option<String>,
    /// Tri-state: `None` (leave alone) / `Some(None)` (clear) /
    /// `Some(Some(s))` (set). See module docs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enriched_md: Option<Option<String>>,
    /// Tri-state mirror of `enriched_md` for the meeting end time.
    /// `Some(Some(ts))` marks the meeting as ended; `Some(None)` un-ends it
    /// (rare — used by re-enhance flows). `None` leaves it alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<Option<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starred: Option<bool>,
    /// Plain `Option<String>` semantics like `notes_md` (not tri-state):
    /// `None` leaves the column alone, `Some(s)` overwrites it. There is
    /// no "clear" use case for provenance, so no tri-state is needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_engine: Option<String>,
}

/// CRUD facade over the `meetings` table.
///
/// Holds a `Db` handle, not a `Connection`. The Db handle's interior
/// `Arc<Mutex<Connection>>` is the locking unit — every method locks for
/// the duration of one statement (or a small transaction for `patch`).
#[derive(Clone)]
pub struct MeetingRepo {
    db: Db,
}

impl MeetingRepo {
    /// Construct a repo bound to `db`. Cheap — no IO.
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Insert a new meeting row. Auto-mints a 26-char ULID id and stamps
    /// `created_at = updated_at = now`. Rejects an empty / whitespace-only
    /// `title` per the LIB-08 inline-rename rule (callers should default
    /// to "Untitled meeting" before calling this).
    pub fn create(&self, m: NewMeeting) -> Result<Meeting> {
        let title = m.title.trim();
        if title.is_empty() {
            bail!("title must not be empty or whitespace");
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let id = m.id.unwrap_or_else(|| Ulid::new().to_string());
        let started_at = m.started_at_unix_ms.unwrap_or(now_ms);
        let title_owned = title.to_string();
        self.db.with_conn(|conn| {
            conn.execute(
                r#"INSERT INTO meetings
                       (id, title, started_at, ended_at, notes_md,
                        enriched_md, transcript_json, starred,
                        created_at, updated_at)
                   VALUES (?1, ?2, ?3, NULL, '', NULL, '[]', 0, ?4, ?4)"#,
                params![id, title_owned, started_at, now_ms],
            )
            .context("insert meeting")
        })?;
        // Reload — cheaper than constructing the wire shape inline; also
        // double-checks the row landed.
        self.get(&id)?
            .ok_or_else(|| anyhow::anyhow!("create: row vanished after insert"))
    }

    /// Look up one row by id. `Ok(None)` for missing rows (not an error —
    /// the REST handler maps to 404).
    pub fn get(&self, id: &str) -> Result<Option<Meeting>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare_cached(SELECT_ALL_WHERE_ID)?;
            stmt.query_row(params![id], row_to_meeting)
                .optional()
                .map_err(Into::into)
        })
    }

    /// List all meetings newest-first by `started_at`. The Library sidebar
    /// uses this as its primary feed; date-bucketing happens client-side
    /// in `DateGroup`.
    pub fn list(&self) -> Result<Vec<Meeting>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare_cached(SELECT_ALL_ORDER_BY_STARTED_DESC)?;
            let rows = stmt.query_map([], row_to_meeting)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// Apply `patch` to the row identified by `id`. Bumps `updated_at`
    /// to "now" iff at least one field changed. Returns the post-patch
    /// row. Errors:
    ///   - empty title (LIB-08 — caller should default before patching)
    ///   - no such id
    pub fn patch(&self, id: &str, patch: MeetingPatch) -> Result<Meeting> {
        if let Some(t) = patch.title.as_deref() {
            if t.trim().is_empty() {
                bail!("title must not be empty or whitespace");
            }
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let id_owned = id.to_string();
        // The patch builds a dynamic UPDATE because SQLite has no
        // `COALESCE-on-missing-bind` story. We keep it inline (not a
        // query builder crate) — the field set is small and stable.
        self.db.with_conn(|conn| -> Result<()> {
            let mut sets: Vec<&str> = Vec::new();
            let mut args: Vec<rusqlite::types::Value> = Vec::new();
            if let Some(t) = patch.title.as_ref() {
                sets.push("title = ?");
                args.push(t.trim().to_string().into());
            }
            if let Some(s) = patch.started_at {
                sets.push("started_at = ?");
                args.push(s.into());
            }
            if let Some(n) = patch.notes_md.as_ref() {
                sets.push("notes_md = ?");
                args.push(n.clone().into());
            }
            if let Some(t) = patch.transcript_json.as_ref() {
                sets.push("transcript_json = ?");
                args.push(t.clone().into());
            }
            // Tri-state enriched_md: outer None ⇒ no SET; outer Some
            // unconditionally writes (inner None becomes SQL NULL).
            if let Some(inner) = patch.enriched_md.as_ref() {
                sets.push("enriched_md = ?");
                match inner {
                    Some(s) => args.push(s.clone().into()),
                    None => args.push(rusqlite::types::Value::Null),
                }
            }
            if let Some(inner) = patch.ended_at.as_ref() {
                sets.push("ended_at = ?");
                match inner {
                    Some(v) => args.push((*v).into()),
                    None => args.push(rusqlite::types::Value::Null),
                }
            }
            if let Some(b) = patch.starred {
                sets.push("starred = ?");
                args.push((if b { 1i64 } else { 0i64 }).into());
            }
            if let Some(e) = patch.stt_engine.as_ref() {
                sets.push("stt_engine = ?");
                args.push(e.clone().into());
            }
            if sets.is_empty() {
                // No fields supplied — still touch updated_at? PRD §10
                // says "PATCH with empty body is a no-op". Match that.
                return Ok(());
            }
            sets.push("updated_at = ?");
            args.push(now_ms.into());
            let sql = format!("UPDATE meetings SET {} WHERE id = ?", sets.join(", "));
            args.push(id_owned.clone().into());
            let n = conn
                .execute(&sql, rusqlite::params_from_iter(args.iter()))
                .with_context(|| format!("update meeting id={id_owned}"))?;
            if n == 0 {
                bail!("meeting not found: {id_owned}");
            }

            // Phase 7 Plan 07-02: keep the FTS5 index's `transcript_text`
            // column in sync. The V004 AFTER UPDATE trigger has already
            // re-indexed `title` and `notes_md` from the new row, but the
            // trigger writes '' for `transcript_text` (it has no JSON
            // parser). This explicit UPDATE on `meetings_fts` fills the
            // flattened transcript so search can match transcript words.
            //
            // We always run this UPDATE — even when only `title` /
            // `notes_md` changed — because the trigger zeroed transcript_text
            // and we need to restore the prior flattened transcript from
            // the current `transcript_json`. The cost is one extra UPDATE
            // per patch, which is amortized into the meeting-edit hot path.
            let mut stmt = conn.prepare_cached(
                "SELECT title, notes_md, transcript_json FROM meetings WHERE id = ?1",
            )?;
            let (title, notes_md, transcript_json): (String, String, String) = stmt
                .query_row(params![id_owned], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .with_context(|| format!("re-read meeting for FTS sync id={id_owned}"))?;
            let transcript_text = extract_transcript_text(&transcript_json);
            conn.execute(
                "UPDATE meetings_fts \
                 SET title = ?1, notes_md = ?2, transcript_text = ?3 \
                 WHERE rowid = (SELECT rowid FROM meetings WHERE id = ?4)",
                params![title, notes_md, transcript_text, id_owned],
            )
            .with_context(|| format!("update meetings_fts id={id_owned}"))?;
            Ok(())
        })?;
        self.get(id)?
            .ok_or_else(|| anyhow::anyhow!("patch: row vanished after update"))
    }

    /// FTS5 keyword search across title + notes_md + transcript_text.
    ///
    /// `query`: free-text user input. Empty or whitespace-only input
    /// short-circuits to `self.list()` (capped at `limit`) — the Library
    /// UI relies on this to render the chronological feed when the
    /// search pill is cleared.
    ///
    /// Quoting strategy: the user query is wrapped as
    /// `"<sanitized>"*` so the FTS5 parser treats it as a single
    /// prefix-matched phrase. Embedded `"` characters are escaped by
    /// doubling them (the FTS5 string-quoting convention). This avoids
    /// the FTS5 query-syntax operators (`OR`, `NEAR`, column filters)
    /// leaking into the user input — typing `notes:foo` should search
    /// for the literal string "notes:foo", not a column-restricted
    /// query.
    ///
    /// Ranking: `bm25(meetings_fts)` puts the most-relevant row first;
    /// ties break by `started_at DESC` so the newest of two equally-
    /// scoring meetings wins.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Meeting>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            // Truncate the chronological list to `limit` for parity with
            // the matched-query branch — the REST handler caps `limit`
            // at 200, so this never returns more than the caller asked.
            let mut all = self.list()?;
            all.truncate(limit);
            return Ok(all);
        }
        // Escape `"` by doubling (FTS5 quoting), then wrap in quotes and
        // suffix `*` for prefix matching. Empty after escaping is
        // impossible because we trimmed above.
        let escaped = trimmed.replace('"', "\"\"");
        let match_expr = format!("\"{escaped}\"*");
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT m.id, m.title, m.started_at, m.ended_at, m.notes_md, \
                        m.enriched_md, m.transcript_json, m.starred, \
                        m.created_at, m.updated_at, m.stt_engine \
                 FROM meetings m \
                 JOIN meetings_fts f ON f.rowid = m.rowid \
                 WHERE meetings_fts MATCH ?1 \
                 ORDER BY bm25(meetings_fts), m.started_at DESC \
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![match_expr, limit as i64], row_to_meeting)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// Delete the meeting row. Returns `Ok(true)` if a row was removed,
    /// `Ok(false)` if the id was unknown. Cascades to `chat_messages` via
    /// the V003b FK rebuild.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let id_owned = id.to_string();
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM meetings WHERE id = ?1", params![id_owned])?;
            Ok(n > 0)
        })
    }
}

// ─── Internal SQL ──────────────────────────────────────────────────────────

const SELECT_ALL_WHERE_ID: &str = "SELECT id, title, started_at, ended_at, \
    notes_md, enriched_md, transcript_json, starred, created_at, updated_at, \
    stt_engine FROM meetings WHERE id = ?1";

const SELECT_ALL_ORDER_BY_STARTED_DESC: &str = "SELECT id, title, started_at, \
    ended_at, notes_md, enriched_md, transcript_json, starred, created_at, \
    updated_at, stt_engine FROM meetings ORDER BY started_at DESC, id DESC";

/// Project one SQLite row onto the `Meeting` wire shape. Used by `get`,
/// `list`, and (indirectly) `create` / `patch` via `get`.
///
/// Coerces SQL NULL on `notes_md` / `transcript_json` to `""` / `"[]"`
/// because legacy Phase 0 rows may have NULLs there even though the
/// Phase 7 schema declares NOT NULL DEFAULT.
fn row_to_meeting(r: &rusqlite::Row<'_>) -> rusqlite::Result<Meeting> {
    let id: String = r.get(0)?;
    let title: Option<String> = r.get(1)?;
    let started_at: i64 = r.get(2)?;
    let ended_at: Option<i64> = r.get(3)?;
    let notes_md: Option<String> = r.get(4)?;
    let enriched_md: Option<String> = r.get(5)?;
    let transcript_json: Option<String> = r.get(6)?;
    let starred_i: i64 = r.get(7)?;
    let created_at_ms: i64 = r.get(8)?;
    let updated_at_ms: i64 = r.get(9)?;
    let stt_engine: Option<String> = r.get(10)?;
    Ok(Meeting {
        id,
        title: title.unwrap_or_default(),
        started_at,
        ended_at,
        notes_md: notes_md.unwrap_or_default(),
        enriched_md,
        transcript_json: transcript_json.unwrap_or_else(|| "[]".into()),
        starred: starred_i != 0,
        stt_engine,
        created_at: ms_to_dt(created_at_ms),
        updated_at: ms_to_dt(updated_at_ms),
    })
}

/// Phase 7 Plan 07-02: flatten the `transcript_json` blob into a single
/// searchable string the FTS5 index can tokenize.
///
/// `transcript_json` is the wire shape produced by the Phase-3 transcript
/// pipeline — an array of `{ "text": "...", ... }` (or `{ "transcript": "..." }`
/// for older Deepgram payloads) objects. We concatenate every `.text` or
/// `.transcript` string field separated by " ".
///
/// Invariants:
/// - Returns `""` for empty / malformed / non-array JSON. Callers (FTS sync
///   in `MeetingRepo::patch`) treat the empty string as "no transcript text"
///   — that's correct: an unparseable transcript shouldn't match anything.
/// - Does NOT walk arbitrarily-nested objects; only `arr[].text` or
///   `arr[].transcript` at depth 1. Deeper Deepgram fields like `words[]`
///   are skipped intentionally — the segment-level `text` already
///   concatenates all the word strings.
fn extract_transcript_text(transcript_json: &str) -> String {
    let trimmed = transcript_json.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return String::new();
    };
    let Some(arr) = value.as_array() else {
        return String::new();
    };
    let mut parts: Vec<&str> = Vec::with_capacity(arr.len());
    for entry in arr {
        // Prefer `.text` (current Phase-3 shape); fall back to `.transcript`
        // for older payloads that may still be sitting in user DBs.
        if let Some(t) = entry.get("text").and_then(|v| v.as_str()) {
            if !t.is_empty() {
                parts.push(t);
            }
        } else if let Some(t) = entry.get("transcript").and_then(|v| v.as_str()) {
            if !t.is_empty() {
                parts.push(t);
            }
        }
    }
    parts.join(" ")
}

fn ms_to_dt(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_else(|| {
        // Saturating fallback for the unreasonably-far-future case —
        // never expected in practice, but keeps the projection infallible.
        Utc.timestamp_opt(0, 0).single().expect("epoch")
    })
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{ChatMessage, Role};

    fn fresh_repo() -> MeetingRepo {
        let db = Db::open_in_memory().expect("open in-memory db");
        MeetingRepo::new(db)
    }

    #[test]
    fn it_creates_with_ulid_id_and_stamps() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "Standup".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .expect("create");
        // ULID strings are exactly 26 chars (Crockford base32).
        assert_eq!(m.id.len(), 26, "id should be a 26-char ULID");
        assert_eq!(m.title, "Standup");
        assert_eq!(m.notes_md, "");
        assert_eq!(m.transcript_json, "[]");
        assert!(!m.starred);
        assert!(m.created_at <= chrono::Utc::now());
        assert_eq!(m.created_at, m.updated_at);
        assert_eq!(m.stt_engine, None, "new meetings have no stamp yet");
    }

    #[test]
    fn stt_engine_patch_persists_and_reads_back() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "Standup".into(),
                ..Default::default()
            })
            .unwrap();
        let patched = repo
            .patch(
                &m.id,
                MeetingPatch {
                    stt_engine: Some("local \u{b7} small.en".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(patched.stt_engine.as_deref(), Some("local \u{b7} small.en"));
        let reloaded = repo.get(&m.id).unwrap().unwrap();
        assert_eq!(
            reloaded.stt_engine.as_deref(),
            Some("local \u{b7} small.en")
        );
    }

    #[test]
    fn it_rejects_empty_title() {
        let repo = fresh_repo();
        let err = repo
            .create(NewMeeting {
                title: "   ".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn it_lists_newest_first() {
        let repo = fresh_repo();
        let a = repo
            .create(NewMeeting {
                title: "Older".into(),
                started_at_unix_ms: Some(1_000_000),
                ..Default::default()
            })
            .unwrap();
        let b = repo
            .create(NewMeeting {
                title: "Newer".into(),
                started_at_unix_ms: Some(2_000_000),
                ..Default::default()
            })
            .unwrap();
        let xs = repo.list().unwrap();
        assert_eq!(xs.len(), 2);
        assert_eq!(xs[0].id, b.id, "newer first");
        assert_eq!(xs[1].id, a.id);
    }

    #[test]
    fn patch_updates_updated_at_and_touches_only_provided_fields() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "T1".into(),
                started_at_unix_ms: Some(1_000),
                ..Default::default()
            })
            .unwrap();
        let before = m.updated_at;
        // Sleep a tick so the updated_at delta is observable.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let m2 = repo
            .patch(
                &m.id,
                MeetingPatch {
                    title: Some("T2".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(m2.title, "T2");
        assert_eq!(m2.notes_md, ""); // untouched
        assert!(m2.updated_at > before, "updated_at must advance");
        assert_eq!(m2.created_at, m.created_at, "created_at unchanged");
    }

    #[test]
    fn enriched_md_tri_state() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "T".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .unwrap();
        // Set
        let m1 = repo
            .patch(
                &m.id,
                MeetingPatch {
                    enriched_md: Some(Some("**bold**".into())),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(m1.enriched_md.as_deref(), Some("**bold**"));
        // Leave alone — different patch, no enriched_md key
        let m2 = repo
            .patch(
                &m.id,
                MeetingPatch {
                    notes_md: Some("hi".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(m2.enriched_md.as_deref(), Some("**bold**"));
        // Explicit clear
        let m3 = repo
            .patch(
                &m.id,
                MeetingPatch {
                    enriched_md: Some(None),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(m3.enriched_md.is_none(), "explicit clear should be NULL");
    }

    #[test]
    fn missing_id_returns_none_false_err() {
        let repo = fresh_repo();
        assert!(repo.get("nonesuch").unwrap().is_none());
        assert!(!repo.delete("nonesuch").unwrap());
        let err = repo
            .patch(
                "nonesuch",
                MeetingPatch {
                    title: Some("x".into()),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn delete_returns_true_when_row_removed() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "T".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .unwrap();
        assert!(repo.delete(&m.id).unwrap());
        assert!(repo.get(&m.id).unwrap().is_none());
    }

    // ─── Phase 7 Plan 07-02: FTS5 search ───────────────────────────────

    #[test]
    fn it_indexes_a_meeting_on_create_and_finds_by_title() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "Standup roadmap".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .unwrap();
        let hits = repo.search("standup", 50).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, m.id);
    }

    #[test]
    fn it_finds_by_notes_md_after_patch() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "T".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .unwrap();
        repo.patch(
            &m.id,
            MeetingPatch {
                notes_md: Some("- discuss palette".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let hits = repo.search("palette", 50).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, m.id);
    }

    #[test]
    fn it_finds_by_transcript_text_after_patch() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "T".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .unwrap();
        repo.patch(
            &m.id,
            MeetingPatch {
                transcript_json: Some(r#"[{"text":"shipping the homebrew formula"}]"#.into()),
                ..Default::default()
            },
        )
        .unwrap();
        let hits = repo.search("homebrew", 50).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, m.id);
    }

    #[test]
    fn it_removes_from_index_on_delete() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "Findable".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .unwrap();
        assert!(repo.delete(&m.id).unwrap());
        let hits = repo.search("findable", 50).unwrap();
        assert!(hits.is_empty(), "delete must drop the FTS index row");
    }

    #[test]
    fn it_returns_full_list_on_empty_query() {
        let repo = fresh_repo();
        repo.create(NewMeeting {
            title: "A".into(),
            started_at_unix_ms: Some(1_000),
            ..Default::default()
        })
        .unwrap();
        repo.create(NewMeeting {
            title: "B".into(),
            started_at_unix_ms: Some(2_000),
            ..Default::default()
        })
        .unwrap();
        let hits = repo.search("", 50).unwrap();
        let list = repo.list().unwrap();
        assert_eq!(hits.len(), list.len());
        assert_eq!(
            hits.iter().map(|m| &m.id).collect::<Vec<_>>(),
            list.iter().map(|m| &m.id).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn it_ranks_by_bm25() {
        let repo = fresh_repo();
        // Both meetings contain "palette"; meeting B contains it more
        // times in notes_md, so bm25 should rank it ahead of A.
        let a = repo
            .create(NewMeeting {
                title: "A".into(),
                started_at_unix_ms: Some(1_000),
                ..Default::default()
            })
            .unwrap();
        let b = repo
            .create(NewMeeting {
                title: "B".into(),
                started_at_unix_ms: Some(2_000),
                ..Default::default()
            })
            .unwrap();
        repo.patch(
            &a.id,
            MeetingPatch {
                notes_md: Some("- palette discussion".into()),
                ..Default::default()
            },
        )
        .unwrap();
        repo.patch(
            &b.id,
            MeetingPatch {
                notes_md: Some("- palette refresh\n- palette tokens".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let hits = repo.search("palette", 50).unwrap();
        assert_eq!(hits.len(), 2, "both meetings should match");
        assert_eq!(
            hits[0].id, b.id,
            "bm25 should rank the more-frequent match first",
        );
    }

    #[test]
    fn it_cascade_deletes_chat_messages() {
        let db = Db::open_in_memory().expect("open in-memory db");
        let repo = MeetingRepo::new(db.clone());
        let m = repo
            .create(NewMeeting {
                title: "Standup".into(),
                started_at_unix_ms: None,
                ..Default::default()
            })
            .unwrap();
        // Seed two chat rows on that meeting.
        let msg1 = ChatMessage::new(&m.id, Role::User, "ping");
        let msg2 = ChatMessage::new(&m.id, Role::Assistant, "pong");
        db.insert_chat_message(&msg1).unwrap();
        db.insert_chat_message(&msg2).unwrap();
        assert_eq!(db.list_chat_messages(&m.id).unwrap().len(), 2);
        // Cascade.
        assert!(repo.delete(&m.id).unwrap());
        assert_eq!(
            db.list_chat_messages(&m.id).unwrap().len(),
            0,
            "chat_messages must cascade-delete with the parent meeting"
        );
    }
}
