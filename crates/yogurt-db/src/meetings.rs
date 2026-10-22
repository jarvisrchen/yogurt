//! `meetings` repo: the persisted directory of meetings (id, title, timestamps, starred flag, notes/transcript/enriched bodies).
//! Streaming state (audio and transcript broadcasts, capture-thread handles) lives in memory on `yogurt-server::meetings::Meeting`.
//!
//! `created_at` / `updated_at` travel as ISO 8601 UTC strings over the wire, while the DB stores unix milliseconds for cheap SQL comparisons. [`row_to_meeting`] converts.
//!
//! `MeetingPatch::enriched_md` is `Option<Option<String>>`: `None` leaves it alone, `Some(None)` clears it, `Some(Some(s))` sets it.
//! Paired with `#[serde(default, skip_serializing_if = "Option::is_none")]`, it distinguishes a JSON `null` from a missing field.
//!
//! Repo methods are synchronous because `Db::with_conn` holds a `MutexGuard<Connection>`. Async callers wrap them in `tokio::task::spawn_blocking`.

use crate::labels::{self, Label};
use crate::Db;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ulid::Ulid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    /// Unix ms of the recording-start clock, distinct from `created_at` (the row's birthday).
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
    /// LLM that produced `enriched_md`, e.g. "MiniMax-Text-01" or "mock".
    /// `None` until the meeting has been enhanced at least once.
    pub llm_model: Option<String>,
    /// Enhance note format that shaped `enriched_md`, an id from
    /// `yogurt_prompts::TEMPLATE_IDS` such as "standup". `None` until the
    /// meeting has been enhanced with a template-aware build.
    pub template: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Updated on every `patch` so the Library can re-sort on edit.
    pub updated_at: DateTime<Utc>,
    /// Labels sorted by name, hydrated by a second query in `get` / `list` / `search` (see [`labels_for`]). `row_to_meeting` starts with an empty `Vec`.
    #[serde(default)]
    pub labels: Vec<Label>,
}

/// One [`MeetingRepo::retrieve`] result. `excerpt` is the transcript text around the matched terms, empty when nothing matched there.
#[derive(Debug, Clone)]
pub struct RetrievalHit {
    pub meeting: Meeting,
    pub excerpt: String,
}

/// Constructor payload for [`MeetingRepo::create`]; the repo fills in defaults, id, and timestamps.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewMeeting {
    pub title: String,
    /// Optional started-at override. Defaults to now.
    #[serde(default)]
    pub started_at_unix_ms: Option<i64>,
    /// Optional caller-supplied id, defaulting to a fresh ULID.
    /// The Library REST handler passes `Uuid::now_v7().to_string()` so the UUID-typed streaming routes (`/api/meetings/:id/start` etc.) use the same id.
    #[serde(default)]
    pub id: Option<String>,
}

/// Patch payload for [`MeetingRepo::patch`]. Every field is `Option` because a PATCH only carries the values to change.
/// `enriched_md` is tri-state, see the module docs.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MeetingPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Updates the recording-start clock, e.g. for back-dated imports or to record the known start.
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
    /// Tri-state mirror of `enriched_md` for the meeting end time. `Some(None)` un-ends the meeting (used by re-enhance flows).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<Option<i64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starred: Option<bool>,
    /// Plain `Option<String>` semantics like `notes_md` (not tri-state):
    /// `None` leaves the column alone, `Some(s)` overwrites it. There is
    /// no "clear" use case for provenance, so no tri-state is needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_engine: Option<String>,
    /// Same plain-`Option` semantics as `stt_engine`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_model: Option<String>,
    /// Same plain-`Option` semantics as `stt_engine`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Replace this meeting's label set with exactly these ids. `None`
    /// leaves labels alone; `Some(vec![])` clears them. Applied via
    /// `labels::set_for_meeting_conn` inside the same transaction as the
    /// rest of the patch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
}

/// CRUD facade over the `meetings` table. Holds a `Db` handle; each method locks for one statement (or a small transaction for `patch`).
#[derive(Clone)]
pub struct MeetingRepo {
    db: Db,
}

impl MeetingRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Auto-mints a 26-char ULID id and stamps `created_at = updated_at = now`.
    /// Rejects an empty or whitespace-only `title`; callers default to "Untitled meeting" first.
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
        // Reload to build the wire shape and confirm the row landed.
        self.get(&id)?
            .ok_or_else(|| anyhow::anyhow!("create: row vanished after insert"))
    }

    /// `Ok(None)` for a missing row; the REST handler maps it to 404.
    pub fn get(&self, id: &str) -> Result<Option<Meeting>> {
        self.db.with_conn(|conn| -> Result<Option<Meeting>> {
            let mut stmt = conn.prepare_cached(SELECT_ALL_WHERE_ID)?;
            let mut m = stmt.query_row(params![id], row_to_meeting).optional()?;
            if let Some(m) = m.as_mut() {
                let mut map = labels_for(conn, std::slice::from_ref(&m.id))?;
                m.labels = map.remove(&m.id).unwrap_or_default();
            }
            Ok(m)
        })
    }

    /// All meetings newest-first by `started_at`.
    pub fn list(&self) -> Result<Vec<Meeting>> {
        self.db.with_conn(|conn| -> Result<Vec<Meeting>> {
            let mut stmt = conn.prepare_cached(SELECT_ALL_ORDER_BY_STARTED_DESC)?;
            let rows = stmt.query_map([], row_to_meeting)?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            hydrate_labels(conn, &mut out)?;
            Ok(out)
        })
    }

    /// Bumps `updated_at` to now iff at least one field changed and returns the post-patch row. Errors on an empty title or an unknown id.
    pub fn patch(&self, id: &str, patch: MeetingPatch) -> Result<Meeting> {
        if let Some(t) = patch.title.as_deref() {
            if t.trim().is_empty() {
                bail!("title must not be empty or whitespace");
            }
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        let id_owned = id.to_string();
        // Dynamic UPDATE because SQLite cannot skip a missing bind with COALESCE. Kept inline since the field set is small and stable.
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
            // Tri-state enriched_md: outer None writes nothing; outer Some writes, with inner None becoming SQL NULL.
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
            if let Some(e) = patch.llm_model.as_ref() {
                sets.push("llm_model = ?");
                args.push(e.clone().into());
            }
            if let Some(e) = patch.template.as_ref() {
                sets.push("template = ?");
                args.push(e.clone().into());
            }
            if sets.is_empty() && patch.label_ids.is_none() {
                // A PATCH with an empty body is a no-op.
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

            if let Some(label_ids) = patch.label_ids.as_ref() {
                labels::set_for_meeting_conn(conn, &id_owned, label_ids)?;
            }
            Ok(())
        })?;
        self.get(id)?
            .ok_or_else(|| anyhow::anyhow!("patch: row vanished after update"))
    }

    /// FTS5 keyword search across title + notes_md + transcript_text.
    ///
    /// Empty or whitespace-only `query` short-circuits to `self.list()` (capped at `limit`); the Library UI relies on this when the search pill is cleared.
    ///
    /// The query is wrapped as `"<sanitized>"*`, a single prefix-matched phrase, with embedded `"` doubled (FTS5 quoting).
    /// That keeps FTS5 operators (`OR`, `NEAR`, column filters) out: typing `notes:foo` searches the literal string.
    ///
    /// Ranking: `bm25(meetings_fts)`, ties broken by `started_at DESC`.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Meeting>> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            // Truncate to `limit` for parity with the matched branch.
            let mut all = self.list()?;
            all.truncate(limit);
            return Ok(all);
        }
        // Double `"` (FTS5 quoting), wrap in quotes, and suffix `*` for prefix matching.
        let escaped = trimmed.replace('"', "\"\"");
        let match_expr = format!("\"{escaped}\"*");
        // A date-shaped query ("10/6/26") also returns every meeting that
        // started that local day, ahead of the text matches.
        let day = parse_short_date(trimmed).and_then(local_day_bounds_ms);
        self.db.with_conn(|conn| -> Result<Vec<Meeting>> {
            let mut out = Vec::new();
            if let Some((start, end)) = day {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, title, started_at, ended_at, notes_md, enriched_md, \
                            transcript_json, starred, created_at, updated_at, \
                            stt_engine, llm_model, template \
                     FROM meetings WHERE started_at >= ?1 AND started_at < ?2 \
                     ORDER BY started_at DESC LIMIT ?3",
                )?;
                for r in stmt.query_map(params![start, end, limit as i64], row_to_meeting)? {
                    out.push(r?);
                }
            }
            let mut stmt = conn.prepare_cached(
                "SELECT m.id, m.title, m.started_at, m.ended_at, m.notes_md, \
                        m.enriched_md, m.transcript_json, m.starred, \
                        m.created_at, m.updated_at, m.stt_engine, m.llm_model, m.template \
                 FROM meetings m \
                 JOIN meetings_fts f ON f.rowid = m.rowid \
                 WHERE meetings_fts MATCH ?1 \
                 ORDER BY bm25(meetings_fts), m.started_at DESC \
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![match_expr, limit as i64], row_to_meeting)?;
            for r in rows {
                let m = r?;
                if out.len() < limit && !out.iter().any(|o| o.id == m.id) {
                    out.push(m);
                }
            }
            hydrate_labels(conn, &mut out)?;
            Ok(out)
        })
    }

    /// Top `limit` meetings for cross-meeting chat, each with a transcript excerpt around the matched terms.
    /// Unlike [`Self::search`], `terms` are ORed, so bm25 ranks a meeting higher the more of them it hits.
    /// No terms means the newest meetings. `range` is `[start, end)` unix ms on `started_at`.
    pub fn retrieve(
        &self,
        terms: &[String],
        range: Option<(i64, i64)>,
        limit: usize,
    ) -> Result<Vec<RetrievalHit>> {
        let (start, end) = range.unwrap_or((i64::MIN, i64::MAX));
        let match_expr = terms
            .iter()
            .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        self.db.with_conn(|conn| -> Result<Vec<RetrievalHit>> {
            let mut out = Vec::new();
            if match_expr.is_empty() {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, title, started_at, ended_at, notes_md, enriched_md, \
                            transcript_json, starred, created_at, updated_at, \
                            stt_engine, llm_model, template \
                     FROM meetings WHERE started_at >= ?1 AND started_at < ?2 \
                     ORDER BY started_at DESC LIMIT ?3",
                )?;
                for r in stmt.query_map(params![start, end, limit as i64], row_to_meeting)? {
                    out.push(RetrievalHit {
                        meeting: r?,
                        excerpt: String::new(),
                    });
                }
            } else {
                let mut stmt = conn.prepare_cached(
                    "SELECT m.id, m.title, m.started_at, m.ended_at, m.notes_md, \
                            m.enriched_md, m.transcript_json, m.starred, \
                            m.created_at, m.updated_at, m.stt_engine, m.llm_model, m.template, \
                            snippet(meetings_fts, 3, '', '', ' … ', 64) \
                     FROM meetings m \
                     JOIN meetings_fts f ON f.rowid = m.rowid \
                     WHERE meetings_fts MATCH ?1 AND m.started_at >= ?2 AND m.started_at < ?3 \
                     ORDER BY bm25(meetings_fts), m.started_at DESC \
                     LIMIT ?4",
                )?;
                let rows = stmt.query_map(params![match_expr, start, end, limit as i64], |r| {
                    Ok(RetrievalHit {
                        meeting: row_to_meeting(r)?,
                        excerpt: r.get(13)?,
                    })
                })?;
                for r in rows {
                    out.push(r?);
                }
            }
            Ok(out)
        })
    }

    /// `Ok(true)` if a row was removed, `Ok(false)` for an unknown id. Cascades to `chat_messages` via the V003b FK rebuild.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let id_owned = id.to_string();
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM meetings WHERE id = ?1", params![id_owned])?;
            Ok(n > 0)
        })
    }
}

/// `M/D/YY` in local time ("10/6/26"), the date format the user types
/// into titles and search.
pub fn short_date(unix_ms: i64) -> String {
    let d = Local
        .timestamp_millis_opt(unix_ms)
        .single()
        .unwrap_or_else(Local::now);
    d.format("%-m/%-d/%y").to_string()
}

/// Parse `M/D/YY` or `M/D/YYYY`. Anything else is `None`.
fn parse_short_date(q: &str) -> Option<NaiveDate> {
    let mut it = q.split('/');
    let (m, d, y) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() {
        return None;
    }
    let y: i32 = match y.len() {
        2 => 2000 + y.parse::<i32>().ok()?,
        4 => y.parse().ok()?,
        _ => return None,
    };
    NaiveDate::from_ymd_opt(y, m.parse().ok()?, d.parse().ok()?)
}

/// `[start, end)` unix ms of the local calendar day `d`.
fn local_day_bounds_ms(d: NaiveDate) -> Option<(i64, i64)> {
    let ms = |d: NaiveDate| {
        Local
            .from_local_datetime(&d.and_hms_opt(0, 0, 0)?)
            .earliest()
            .map(|t| t.timestamp_millis())
    };
    Some((ms(d)?, ms(d.succ_opt()?)?))
}

const SELECT_ALL_WHERE_ID: &str = "SELECT id, title, started_at, ended_at, \
    notes_md, enriched_md, transcript_json, starred, created_at, updated_at, \
    stt_engine, llm_model, template FROM meetings WHERE id = ?1";

const SELECT_ALL_ORDER_BY_STARTED_DESC: &str = "SELECT id, title, started_at, \
    ended_at, notes_md, enriched_md, transcript_json, starred, created_at, \
    updated_at, stt_engine, llm_model, template FROM meetings ORDER BY started_at DESC, id DESC";

/// Coerces SQL NULL on `notes_md` / `transcript_json` to `""` / `"[]"` because legacy rows may have NULLs there.
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
    let llm_model: Option<String> = r.get(11)?;
    let template: Option<String> = r.get(12)?;
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
        llm_model,
        template,
        created_at: ms_to_dt(created_at_ms),
        updated_at: ms_to_dt(updated_at_ms),
        labels: Vec::new(),
    })
}

/// One query hydrating labels for every id in `meeting_ids`. Returns an empty map without querying for an empty slice.
fn labels_for(
    conn: &Connection,
    meeting_ids: &[String],
) -> rusqlite::Result<HashMap<String, Vec<Label>>> {
    let mut map: HashMap<String, Vec<Label>> = HashMap::new();
    if meeting_ids.is_empty() {
        return Ok(map);
    }
    let placeholders = meeting_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT ml.meeting_id, l.id, l.name, l.color, l.position, l.updated_at \
         FROM meeting_labels ml JOIN labels l ON l.id = ml.label_id \
         WHERE ml.meeting_id IN ({placeholders}) \
         ORDER BY l.name COLLATE NOCASE"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params_from_iter(meeting_ids.iter()),
        |r| -> rusqlite::Result<(String, Label)> {
            Ok((
                r.get(0)?,
                Label {
                    id: r.get(1)?,
                    name: r.get(2)?,
                    color: r.get(3)?,
                    position: r.get(4)?,
                    updated_at: r.get(5)?,
                },
            ))
        },
    )?;
    for r in rows {
        let (meeting_id, label) = r?;
        map.entry(meeting_id).or_default().push(label);
    }
    Ok(map)
}

fn hydrate_labels(conn: &Connection, out: &mut [Meeting]) -> rusqlite::Result<()> {
    let ids: Vec<String> = out.iter().map(|m| m.id.clone()).collect();
    let mut map = labels_for(conn, &ids)?;
    for m in out.iter_mut() {
        m.labels = map.remove(&m.id).unwrap_or_default();
    }
    Ok(())
}

fn ms_to_dt(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_else(|| {
        // Saturating fallback keeps the projection infallible.
        Utc.timestamp_opt(0, 0).single().expect("epoch")
    })
}

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
    fn llm_model_patch_persists_and_reads_back() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "Standup".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(m.llm_model, None, "unenhanced meetings have no stamp");
        repo.patch(
            &m.id,
            MeetingPatch {
                llm_model: Some("MiniMax-Text-01".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let reloaded = repo.get(&m.id).unwrap().unwrap();
        assert_eq!(reloaded.llm_model.as_deref(), Some("MiniMax-Text-01"));
        assert_eq!(reloaded.template, None, "template is stamped separately");
        repo.patch(
            &m.id,
            MeetingPatch {
                template: Some("standup".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            repo.get(&m.id).unwrap().unwrap().template.as_deref(),
            Some("standup")
        );
        assert_eq!(
            repo.list().unwrap()[0].template.as_deref(),
            Some("standup"),
            "list carries the stamp too"
        );
        // list() uses a different SELECT than get(); both must project the column.
        assert_eq!(
            repo.list().unwrap()[0].llm_model.as_deref(),
            Some("MiniMax-Text-01")
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
    fn transcript_stays_searchable_after_a_write_that_bypasses_patch() {
        let db = Db::open_in_memory().unwrap();
        let repo = MeetingRepo::new(db.clone());
        let m = repo
            .create(NewMeeting {
                title: "T".into(),
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
        // The enhance path writes `meetings` with its own UPSERT.
        db.with_conn(|c| {
            c.execute(
                "UPDATE meetings SET enriched_md = '## Pricing' WHERE id = ?1",
                params![m.id],
            )
        })
        .unwrap();
        assert_eq!(repo.search("homebrew", 50).unwrap().len(), 1);
        assert_eq!(repo.search("pricing", 50).unwrap().len(), 1);
    }

    #[test]
    fn retrieve_ors_terms_ranks_by_hits_and_honors_the_range() {
        let repo = fresh_repo();
        let mk = |title: &str, ms: i64, transcript: &str, enriched: Option<&str>| {
            let m = repo
                .create(NewMeeting {
                    title: title.into(),
                    started_at_unix_ms: Some(ms),
                    ..Default::default()
                })
                .unwrap();
            repo.patch(
                &m.id,
                MeetingPatch {
                    transcript_json: Some(serde_json::json!([{ "text": transcript }]).to_string()),
                    enriched_md: Some(enriched.map(str::to_string)),
                    ..Default::default()
                },
            )
            .unwrap();
            m.id
        };
        let both = mk(
            "Sync",
            2_000,
            "Steven wants the pricing at twenty dollars",
            None,
        );
        let one = mk(
            "Roadmap",
            3_000,
            "nothing relevant",
            Some("## Pricing\n- tiers"),
        );
        mk("Lunch", 4_000, "sandwiches", None);
        let early = mk("Old sync", 500, "Steven pricing pricing", None);

        let terms = ["steven".to_string(), "pricing".to_string()];
        let hits = repo.retrieve(&terms, Some((1_000, 10_000)), 8).unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.meeting.id.as_str()).collect();
        assert_eq!(ids, [both.as_str(), one.as_str()]);
        assert!(
            hits[0].excerpt.contains("pricing at twenty"),
            "{:?}",
            hits[0].excerpt
        );

        let all = repo.retrieve(&terms, None, 8).unwrap();
        assert!(all.iter().any(|h| h.meeting.id == early));

        let newest = repo.retrieve(&[], Some((1_000, 10_000)), 2).unwrap();
        assert_eq!(newest.len(), 2);
        assert_eq!(newest[0].meeting.title, "Lunch");
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
        let msg1 = ChatMessage::new(&m.id, Role::User, "ping");
        let msg2 = ChatMessage::new(&m.id, Role::Assistant, "pong");
        db.insert_chat_message(&msg1).unwrap();
        db.insert_chat_message(&msg2).unwrap();
        assert_eq!(db.list_chat_messages(&m.id).unwrap().len(), 2);
        assert!(repo.delete(&m.id).unwrap());
        assert_eq!(
            db.list_chat_messages(&m.id).unwrap().len(),
            0,
            "chat_messages must cascade-delete with the parent meeting"
        );
    }

    #[test]
    fn date_query_returns_that_local_day_then_text_matches() {
        let repo = fresh_repo();
        let at = |y, mo, d, h| {
            Local
                .with_ymd_and_hms(y, mo, d, h, 0, 0)
                .unwrap()
                .timestamp_millis()
        };
        let mk = |title: &str, ms| {
            repo.create(NewMeeting {
                title: title.into(),
                started_at_unix_ms: Some(ms),
                ..Default::default()
            })
            .unwrap()
        };
        let morning = mk("Standup", at(2026, 10, 6, 0));
        let night = mk("Retro", at(2026, 10, 6, 23));
        mk("Next day", at(2026, 10, 7, 0));
        let typed = mk("Planning 10/6/26", at(2026, 9, 1, 12));

        let ids = |q: &str| -> Vec<String> {
            repo.search(q, 50)
                .unwrap()
                .into_iter()
                .map(|m| m.id)
                .collect()
        };
        assert_eq!(
            ids("10/6/26"),
            vec![night.id.clone(), morning.id.clone(), typed.id]
        );
        assert_eq!(ids("10/6/2026")[..2], [night.id, morning.id]);
        assert_eq!(short_date(at(2026, 10, 6, 9)), "10/6/26");
        assert_eq!(parse_short_date("13/1/26"), None);
        assert_eq!(parse_short_date("10/6"), None);
    }
}
