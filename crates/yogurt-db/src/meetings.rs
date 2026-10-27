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
    /// Where the meeting came from, such as "granola". `None` for a meeting recorded in yogurt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The source's own id for the meeting. Unique together with `source`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Display-only provenance: `{"owner":{"name","email"},"web_url","updated_at","imported_at"}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_json: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    /// Updated on every `patch` so the Library can re-sort on edit.
    pub updated_at: DateTime<Utc>,
    /// Labels sorted by name, hydrated by a second query in `get` / `list` / `search` (see [`labels_for`]). `row_to_meeting` starts with an empty `Vec`.
    #[serde(default)]
    pub labels: Vec<Label>,
}

/// What a Library card shows. Carries no note or transcript bodies, so a page of 100 stays around 40 KB.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub starred: bool,
    pub stt_engine: Option<String>,
    pub llm_model: Option<String>,
    /// `enriched_md` is set. The card flags an ended meeting that has none.
    pub has_enriched: bool,
    pub labels: Vec<Label>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Who captured an imported meeting: the owner's name, or their email when Granola has no name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_owner: Option<String>,
}

/// One page of a listing. `total` counts every match before `limit` and `offset`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeetingPage {
    pub meetings: Vec<MeetingSummary>,
    pub total: usize,
}

/// Narrows [`MeetingRepo::list_page`] and [`MeetingRepo::search_page`] in SQL, so paging and `total` see only matching rows.
#[derive(Debug, Clone, Default)]
pub struct MeetingFilter {
    pub label_id: Option<String>,
    pub starred: bool,
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

/// A finished meeting from an outside source, written whole by [`MeetingRepo::import`].
#[derive(Debug, Clone)]
pub struct ImportedMeeting {
    pub id: String,
    pub title: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub notes_md: String,
    pub enriched_md: Option<String>,
    pub transcript_json: String,
    pub source: String,
    pub source_id: String,
    /// A JSON object. `import` adds `imported_at` to it.
    pub source_json: serde_json::Value,
    /// Label names, each found or created.
    pub labels: Vec<String>,
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

    /// Writes the row, its bodies, and its labels in one transaction, so the FTS trigger runs once.
    /// `Ok(None)` when `(source, source_id)` is already present: an existing meeting is never overwritten.
    pub fn import(&self, m: ImportedMeeting) -> Result<Option<Meeting>> {
        let now = chrono::Utc::now();
        let now_ms = now.timestamp_millis();
        let mut source_json = m.source_json;
        if let Some(o) = source_json.as_object_mut() {
            o.insert("imported_at".into(), now.to_rfc3339().into());
        }
        let inserted = self.db.with_conn(|conn| -> Result<bool> {
            let tx = conn.unchecked_transaction().context("begin import tx")?;
            let n = tx
                .execute(
                    r#"INSERT INTO meetings
                           (id, title, started_at, ended_at, notes_md, enriched_md, transcript_json,
                            starred, created_at, updated_at, source, source_id, source_json)
                       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8, ?9, ?10, ?11)
                       ON CONFLICT(source, source_id) WHERE source IS NOT NULL DO NOTHING"#,
                    params![
                        m.id,
                        m.title,
                        m.started_at,
                        m.ended_at,
                        m.notes_md,
                        m.enriched_md,
                        m.transcript_json,
                        now_ms,
                        m.source,
                        m.source_id,
                        source_json.to_string()
                    ],
                )
                .context("insert imported meeting")?;
            if n == 0 {
                return Ok(false);
            }
            for name in &m.labels {
                let (label, _) = labels::find_or_create_conn(&tx, name, None)?;
                tx.execute(
                    "INSERT OR IGNORE INTO meeting_labels (meeting_id, label_id) VALUES (?1, ?2)",
                    params![m.id, label.id],
                )?;
                tx.execute(
                    "UPDATE labels SET updated_at = ?1 WHERE id = ?2",
                    params![now_ms, label.id],
                )?;
            }
            tx.commit().context("commit import")?;
            Ok(true)
        })?;
        if inserted {
            self.get(&m.id)
        } else {
            Ok(None)
        }
    }

    /// The `source_id` of every meeting already imported from `source`.
    pub fn imported_source_ids(&self, source: &str) -> Result<std::collections::HashSet<String>> {
        self.db.with_conn(|conn| -> Result<_> {
            let mut stmt =
                conn.prepare_cached("SELECT source_id FROM meetings WHERE source = ?1")?;
            let rows = stmt.query_map([source], |r| r.get(0))?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    /// The owner email shared by most meetings imported from `source`, which stands in for the user.
    /// Ties break on the email so the answer is stable.
    pub fn most_common_source_owner(&self, source: &str) -> Result<Option<String>> {
        self.db.with_conn(|conn| -> Result<_> {
            Ok(conn
                .query_row(
                    "SELECT json_extract(source_json, '$.owner.email') AS e FROM meetings \
                     WHERE source = ?1 AND e IS NOT NULL GROUP BY e ORDER BY COUNT(*) DESC, e LIMIT 1",
                    [source],
                    |r| r.get(0),
                )
                .optional()?)
        })
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

    /// A page of meetings, newest first by `started_at` with `id` breaking ties so pages stay stable.
    pub fn list_page(
        &self,
        filter: &MeetingFilter,
        limit: usize,
        offset: usize,
    ) -> Result<MeetingPage> {
        self.db.with_conn(|conn| {
            query_page(
                conn,
                &format!("meetings m WHERE {FILTER_SQL}"),
                "m.started_at DESC, m.id DESC",
                &[],
                filter,
                limit,
                offset,
            )
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

    /// `(id, cloud_hash)` for every meeting.
    pub fn cloud_hashes(&self) -> Result<Vec<(String, Option<String>)>> {
        self.db.with_conn(|conn| -> Result<_> {
            let mut stmt = conn.prepare_cached("SELECT id, cloud_hash FROM meetings")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Leaves `updated_at` alone, since a push is not an edit.
    pub fn set_cloud_hash(&self, id: &str, hash: &str) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE meetings SET cloud_hash = ?2 WHERE id = ?1",
                params![id, hash],
            )
        })?;
        Ok(())
    }

    /// Called on connect and disconnect, so a new cloud gets a full push. Pending deletions go too.
    pub fn clear_cloud_hashes(&self) -> Result<()> {
        self.db.with_conn(|conn| {
            conn.execute_batch("UPDATE meetings SET cloud_hash = NULL; DELETE FROM cloud_deleted;")
        })?;
        Ok(())
    }

    /// Ids of meetings that were pushed and then deleted here, so the cloud has not heard of the deletion yet.
    pub fn cloud_tombstones(&self) -> Result<Vec<String>> {
        self.db.with_conn(|conn| -> Result<_> {
            let mut stmt = conn.prepare_cached("SELECT meeting_id FROM cloud_deleted")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
    }

    /// Called after a pass that sent the deletion markers.
    pub fn clear_cloud_tombstones(&self) -> Result<()> {
        self.db
            .with_conn(|conn| conn.execute("DELETE FROM cloud_deleted", []))?;
        Ok(())
    }

    /// The document pushed to Yogurt Cloud, `None` for an unknown id. See [`bundle_value`].
    pub fn meeting_bundle(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let Some(m) = self.get(id)? else {
            return Ok(None);
        };
        let chat = self.db.list_chat_messages(id)?;
        Ok(Some(bundle_value(&m, &chat)))
    }

    /// FTS5 keyword search across title + notes_md + transcript_text, one page at a time.
    ///
    /// Empty or whitespace-only `query` short-circuits to [`Self::list_page`]; the Library UI relies on this when the search pill is cleared.
    ///
    /// The query is wrapped as `"<sanitized>"*`, a single prefix-matched phrase, with embedded `"` doubled (FTS5 quoting).
    /// That keeps FTS5 operators (`OR`, `NEAR`, column filters) out: typing `notes:foo` searches the literal string.
    ///
    /// Ranking: `bm25(meetings_fts)`, ties broken by `started_at DESC, id DESC`.
    /// A date-shaped query ("10/6/26") also matches every meeting that started that local day, ahead of the text matches.
    pub fn search_page(
        &self,
        query: &str,
        filter: &MeetingFilter,
        limit: usize,
        offset: usize,
    ) -> Result<MeetingPage> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return self.list_page(filter, limit, offset);
        }
        let match_expr = format!("\"{}\"*", trimmed.replace('"', "\"\""));
        let day = parse_short_date(trimmed).and_then(local_day_bounds_ms);
        let (day_start, day_end) = (day.map(|d| d.0), day.map(|d| d.1));
        const ON_DAY: &str = "(m.started_at >= :ds AND m.started_at < :de)";
        self.db.with_conn(|conn| {
            query_page(
                conn,
                &format!(
                    "meetings m LEFT JOIN (SELECT rowid AS rid, bm25(meetings_fts) AS rank \
                         FROM meetings_fts WHERE meetings_fts MATCH :q) f ON f.rid = m.rowid \
                     WHERE (f.rid IS NOT NULL OR {ON_DAY}) AND {FILTER_SQL}"
                ),
                &format!(
                    "COALESCE({ON_DAY}, 0) DESC, CASE WHEN {ON_DAY} THEN NULL ELSE f.rank END, \
                     m.started_at DESC, m.id DESC"
                ),
                &[(":q", &match_expr), (":ds", &day_start), (":de", &day_end)],
                filter,
                limit,
                offset,
            )
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
                            stt_engine, llm_model, template, source, source_id, source_json \
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
                            m.source, m.source_id, m.source_json, \
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
                        excerpt: r.get(16)?,
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

/// `{"v":1,"meeting":...,"chat":[...]}`. The hash of this document decides whether a meeting is pushed again, so it carries content only.
/// `updated_at` and each label's `position` and `updated_at` are left out because reordering labels or tagging another meeting must not change this meeting's hash.
pub fn bundle_value(m: &Meeting, chat: &[crate::chat::ChatMessage]) -> serde_json::Value {
    use serde_json::json;
    let mut meeting = serde_json::to_value(m).expect("Meeting serializes");
    let obj = meeting.as_object_mut().expect("Meeting is an object");
    obj.remove("updated_at");
    obj.insert(
        "labels".into(),
        m.labels
            .iter()
            .map(|l| json!({ "id": l.id, "name": l.name, "color": l.color }))
            .collect(),
    );
    let chat: Vec<_> = chat
        .iter()
        .map(|c| json!({ "id": c.id, "role": c.role, "content": c.content, "created_at": c.created_at }))
        .collect();
    strip_nul(json!({ "v": 1, "meeting": meeting, "chat": chat }))
}

/// Postgres jsonb rejects U+0000 (22P05), and one such meeting would block its batch on every pass.
fn strip_nul(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::String(s) => Value::String(s.replace('\0', "")),
        Value::Array(a) => Value::Array(a.into_iter().map(strip_nul).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k.replace('\0', ""), strip_nul(v)))
                .collect(),
        ),
        other => other,
    }
}

const SELECT_ALL_WHERE_ID: &str = "SELECT id, title, started_at, ended_at, \
    notes_md, enriched_md, transcript_json, starred, created_at, updated_at, \
    stt_engine, llm_model, template, source, source_id, source_json FROM meetings WHERE id = ?1";

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
    let source_json: Option<String> = r.get(15)?;
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
        source: r.get(13)?,
        source_id: r.get(14)?,
        source_json: source_json.and_then(|j| serde_json::from_str(&j).ok()),
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

const FILTER_SQL: &str = "(:label IS NULL OR EXISTS (SELECT 1 FROM meeting_labels ml \
     WHERE ml.meeting_id = m.id AND ml.label_id = :label)) AND (:starred = 0 OR m.starred = 1)";

/// Counts `from_where` (a `FROM` body whose table alias is `m`), then fetches one page of it.
/// `extra` binds the named parameters `from_where` and `order_by` use beyond `FILTER_SQL`.
fn query_page(
    conn: &Connection,
    from_where: &str,
    order_by: &str,
    extra: &[(&str, &dyn rusqlite::ToSql)],
    filter: &MeetingFilter,
    limit: usize,
    offset: usize,
) -> Result<MeetingPage> {
    let starred = i64::from(filter.starred);
    let mut named: Vec<(&str, &dyn rusqlite::ToSql)> = extra.to_vec();
    named.push((":label", &filter.label_id));
    named.push((":starred", &starred));
    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {from_where}"),
            &*named,
            |r| r.get(0),
        )
        .context("count meetings")?;
    let (limit, offset) = (limit as i64, offset as i64);
    named.push((":limit", &limit));
    named.push((":offset", &offset));
    let mut stmt = conn.prepare(&format!(
        "SELECT m.id, m.title, m.started_at, m.ended_at, m.starred, m.stt_engine, m.llm_model, \
                m.enriched_md IS NOT NULL, m.source, \
                COALESCE(json_extract(m.source_json, '$.owner.name'), json_extract(m.source_json, '$.owner.email')) \
         FROM {from_where} ORDER BY {order_by} LIMIT :limit OFFSET :offset"
    ))?;
    let mut meetings = stmt
        .query_map(&*named, |r| {
            Ok(MeetingSummary {
                id: r.get(0)?,
                title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                started_at: r.get(2)?,
                ended_at: r.get(3)?,
                starred: r.get::<_, i64>(4)? != 0,
                stt_engine: r.get(5)?,
                llm_model: r.get(6)?,
                has_enriched: r.get(7)?,
                labels: Vec::new(),
                source: r.get(8)?,
                source_owner: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let ids: Vec<String> = meetings.iter().map(|m| m.id.clone()).collect();
    let mut labels = labels_for(conn, &ids)?;
    for m in &mut meetings {
        m.labels = labels.remove(&m.id).unwrap_or_default();
    }
    Ok(MeetingPage {
        meetings,
        total: total as usize,
    })
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

    fn search(repo: &MeetingRepo, q: &str, limit: usize) -> Result<Vec<MeetingSummary>> {
        Ok(repo
            .search_page(q, &MeetingFilter::default(), limit, 0)?
            .meetings)
    }

    fn list(repo: &MeetingRepo) -> Vec<MeetingSummary> {
        repo.list_page(&MeetingFilter::default(), 100, 0)
            .unwrap()
            .meetings
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
            list(&repo)[0].llm_model.as_deref(),
            Some("MiniMax-Text-01"),
            "the summary projects the column too"
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
        let xs = list(&repo);
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
        let hits = search(&repo, "standup", 50).unwrap();
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
        let hits = search(&repo, "palette", 50).unwrap();
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
        let hits = search(&repo, "homebrew", 50).unwrap();
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
        assert_eq!(search(&repo, "homebrew", 50).unwrap().len(), 1);
        assert_eq!(search(&repo, "pricing", 50).unwrap().len(), 1);
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
        let hits = search(&repo, "findable", 50).unwrap();
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
        let hits = search(&repo, "", 50).unwrap();
        let list = list(&repo);
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
        let hits = search(&repo, "palette", 50).unwrap();
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
            search(&repo, q, 50)
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
    fn imported(source_id: &str, title: &str) -> ImportedMeeting {
        ImportedMeeting {
            id: format!("id-{source_id}"),
            title: title.into(),
            started_at: 1_000,
            ended_at: 5_000,
            notes_md: "typed".into(),
            enriched_md: Some("### Summary\n\n- zebrafish budget".into()),
            transcript_json: r#"[{"ts_ms":0,"channel":"them","text":"quokka remarks"}]"#.into(),
            source: "granola".into(),
            source_id: source_id.into(),
            source_json: serde_json::json!({ "owner": { "name": "Pat", "email": "p@example.com" } }),
            labels: vec!["Clients".into(), "clients".into(), "Q4".into()],
        }
    }

    #[test]
    fn import_writes_everything_once_and_is_searchable() {
        let repo = fresh_repo();
        let m = repo
            .import(imported("not_1", "Synthetic"))
            .unwrap()
            .unwrap();
        assert_eq!(
            (m.source.as_deref(), m.source_id.as_deref()),
            (Some("granola"), Some("not_1"))
        );
        assert_eq!(m.ended_at, Some(5_000));
        assert_eq!(m.stt_engine, None);
        assert_eq!(m.llm_model, None);
        assert_eq!(m.template, None);
        let json = m.source_json.as_ref().unwrap();
        assert_eq!(json["owner"]["email"], "p@example.com");
        assert!(json["imported_at"].is_string());
        let names: Vec<_> = m.labels.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Clients", "Q4"]);

        for q in ["quokka", "zebrafish", "typed"] {
            assert_eq!(search(&repo, q, 10).unwrap().len(), 1, "{q}");
        }
        let summary = &list(&repo)[0];
        assert_eq!(summary.source.as_deref(), Some("granola"));
        assert_eq!(summary.source_owner.as_deref(), Some("Pat"));

        assert_eq!(
            repo.imported_source_ids("granola").unwrap(),
            ["not_1".to_string()].into_iter().collect()
        );
    }

    #[test]
    fn import_never_overwrites_an_existing_meeting() {
        let repo = fresh_repo();
        let first = repo.import(imported("not_1", "Original")).unwrap().unwrap();
        repo.patch(
            &first.id,
            MeetingPatch {
                title: Some("Edited".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut again = imported("not_1", "Granola changed it");
        again.id = "another-id".into();
        assert!(repo.import(again).unwrap().is_none());
        assert_eq!(list(&repo).len(), 1);
        assert_eq!(repo.get(&first.id).unwrap().unwrap().title, "Edited");
        assert!(repo.get("another-id").unwrap().is_none());
    }

    #[test]
    fn a_failed_import_rolls_back_the_row() {
        let repo = fresh_repo();
        let mut bad = imported("not_1", "Half written");
        bad.labels = vec!["x".repeat(41)];
        assert!(repo.import(bad).is_err());
        assert!(list(&repo).is_empty());
        assert!(repo.imported_source_ids("granola").unwrap().is_empty());
    }

    #[test]
    fn meetings_recorded_here_keep_their_bundle_without_source_fields() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "Native".into(),
                ..Default::default()
            })
            .unwrap();
        let v = serde_json::to_value(&m).unwrap();
        for k in ["source", "source_id", "source_json"] {
            assert!(
                v.get(k).is_none(),
                "{k} must not change a native meeting's hash"
            );
        }
    }

    #[test]
    fn bundle_serialization_is_pinned() {
        use crate::chat::{ChatMessage, Role};
        let at = |ms| ms_to_dt(ms);
        let m = Meeting {
            id: "m1".into(),
            title: "Stand\0up".into(),
            started_at: 1000,
            ended_at: Some(2000),
            notes_md: "notes".into(),
            enriched_md: None,
            transcript_json: "[]".into(),
            starred: true,
            stt_engine: Some("local".into()),
            llm_model: None,
            template: None,
            source: None,
            source_id: None,
            source_json: None,
            created_at: at(1000),
            updated_at: at(9999),
            labels: vec![Label {
                id: "l1".into(),
                name: "Sales".into(),
                color: "blue".into(),
                position: 3,
                updated_at: 42,
            }],
        };
        let chat = ChatMessage {
            id: "c1".into(),
            meeting_id: "m1".into(),
            role: Role::User,
            content: "hi".into(),
            created_at: 1500,
        };
        assert_eq!(
            bundle_value(&m, &[chat]).to_string(),
            r#"{"chat":[{"content":"hi","created_at":1500,"id":"c1","role":"user"}],"meeting":{"created_at":"1970-01-01T00:00:01Z","ended_at":2000,"enriched_md":null,"id":"m1","labels":[{"color":"blue","id":"l1","name":"Sales"}],"llm_model":null,"notes_md":"notes","starred":true,"started_at":1000,"stt_engine":"local","template":null,"title":"Standup","transcript_json":"[]"},"v":1}"#
        );
    }

    #[test]
    fn cloud_hash_round_trip_and_bundle_ignores_updated_at() {
        let repo = fresh_repo();
        let m = repo
            .create(NewMeeting {
                title: "A".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(repo.cloud_hashes().unwrap(), [(m.id.clone(), None)]);
        let before = repo.meeting_bundle(&m.id).unwrap().unwrap();
        repo.set_cloud_hash(&m.id, "abc").unwrap();
        assert_eq!(
            repo.cloud_hashes().unwrap(),
            [(m.id.clone(), Some("abc".into()))]
        );
        assert_eq!(repo.get(&m.id).unwrap().unwrap().updated_at, m.updated_at);
        assert_eq!(repo.meeting_bundle(&m.id).unwrap().unwrap(), before);
        repo.clear_cloud_hashes().unwrap();
        assert_eq!(repo.cloud_hashes().unwrap(), [(m.id, None)]);
        assert!(repo.meeting_bundle("nope").unwrap().is_none());
    }

    #[test]
    fn deleting_a_pushed_meeting_leaves_a_tombstone_until_cleared() {
        let repo = fresh_repo();
        let mk = |t: &str| {
            repo.create(NewMeeting {
                title: t.into(),
                ..Default::default()
            })
            .unwrap()
        };
        let (pushed, never) = (mk("Pushed"), mk("Never"));
        repo.set_cloud_hash(&pushed.id, "h").unwrap();
        assert!(repo.cloud_tombstones().unwrap().is_empty());
        repo.delete(&never.id).unwrap();
        assert!(repo.cloud_tombstones().unwrap().is_empty());
        repo.delete(&pushed.id).unwrap();
        assert_eq!(repo.cloud_tombstones().unwrap(), [pushed.id]);
        repo.clear_cloud_tombstones().unwrap();
        assert!(repo.cloud_tombstones().unwrap().is_empty());
    }

    fn seed(repo: &MeetingRepo, n: usize) -> Vec<Meeting> {
        (0..n)
            .map(|i| {
                repo.create(NewMeeting {
                    title: format!("Sync {i}"),
                    started_at_unix_ms: Some(1_000 * (i as i64 + 1)),
                    ..Default::default()
                })
                .unwrap()
            })
            .collect()
    }

    #[test]
    fn list_page_reports_total_before_paging_and_orders_newest_first() {
        let repo = fresh_repo();
        let ms = seed(&repo, 7);
        let f = MeetingFilter::default();
        let p1 = repo.list_page(&f, 3, 0).unwrap();
        let p3 = repo.list_page(&f, 3, 6).unwrap();
        let past = repo.list_page(&f, 3, 30).unwrap();
        assert_eq!(p1.total, 7);
        assert_eq!(
            p1.meetings
                .iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>(),
            [&ms[6].id, &ms[5].id, &ms[4].id]
        );
        assert_eq!(p3.meetings.len(), 1);
        assert_eq!(p3.meetings[0].id, ms[0].id);
        assert!(past.meetings.is_empty());
        assert_eq!(past.total, 7);
    }

    #[test]
    fn equal_start_times_page_stably_by_id() {
        let repo = fresh_repo();
        for i in 0..5 {
            repo.create(NewMeeting {
                title: format!("Same {i}"),
                started_at_unix_ms: Some(5_000),
                ..Default::default()
            })
            .unwrap();
        }
        let f = MeetingFilter::default();
        let ids: Vec<String> = (0..5)
            .flat_map(|o| repo.list_page(&f, 1, o).unwrap().meetings)
            .map(|m| m.id)
            .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.reverse();
        assert_eq!(ids, sorted, "id DESC breaks the tie, with no repeats");
    }

    #[test]
    fn filters_apply_before_total_and_paging() {
        let repo = fresh_repo();
        let ms = seed(&repo, 6);
        let labels = crate::LabelRepo::new(repo.db.clone());
        let (l, _) = labels.find_or_create("Sales", None).unwrap();
        for m in [&ms[0], &ms[2], &ms[4]] {
            labels
                .set_for_meeting(&m.id, std::slice::from_ref(&l.id))
                .unwrap();
        }
        for m in [&ms[2], &ms[3]] {
            repo.patch(
                &m.id,
                MeetingPatch {
                    starred: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        }

        let by_label = MeetingFilter {
            label_id: Some(l.id.clone()),
            starred: false,
        };
        let page = repo.list_page(&by_label, 2, 0).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.meetings[0].id, ms[4].id);
        assert_eq!(page.meetings[0].labels[0].name, "Sales");

        let starred = MeetingFilter {
            label_id: None,
            starred: true,
        };
        let page = repo.list_page(&starred, 1, 1).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.meetings[0].id, ms[2].id);

        let both = MeetingFilter {
            label_id: Some(l.id),
            starred: true,
        };
        let page = repo.list_page(&both, 25, 0).unwrap();
        assert_eq!((page.total, page.meetings.len()), (1, 1));
        assert_eq!(page.meetings[0].id, ms[2].id);
    }

    #[test]
    fn summary_flags_enriched_notes() {
        let repo = fresh_repo();
        let ms = seed(&repo, 2);
        repo.patch(
            &ms[0].id,
            MeetingPatch {
                enriched_md: Some(Some("# Notes".into())),
                ..Default::default()
            },
        )
        .unwrap();
        let by_id: HashMap<String, bool> = list(&repo)
            .into_iter()
            .map(|m| (m.id, m.has_enriched))
            .collect();
        assert!(by_id[&ms[0].id]);
        assert!(!by_id[&ms[1].id]);
    }

    #[test]
    fn search_page_counts_the_whole_match_and_honors_filters() {
        let repo = fresh_repo();
        for i in 0..5 {
            let m = repo
                .create(NewMeeting {
                    title: format!("Budget review {i}"),
                    started_at_unix_ms: Some(1_000 * (i + 1)),
                    ..Default::default()
                })
                .unwrap();
            if i % 2 == 0 {
                repo.patch(
                    &m.id,
                    MeetingPatch {
                        starred: Some(true),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
        }
        seed(&repo, 3);
        let none = MeetingFilter::default();
        let p = repo.search_page("budget", &none, 2, 2).unwrap();
        assert_eq!(p.total, 5);
        assert_eq!(p.meetings.len(), 2);

        let starred = MeetingFilter {
            label_id: None,
            starred: true,
        };
        let p = repo.search_page("budget", &starred, 25, 0).unwrap();
        assert_eq!(p.total, 3);
        assert!(p.meetings.iter().all(|m| m.starred));

        let p = repo.search_page("  ", &none, 25, 0).unwrap();
        assert_eq!(p.total, 8, "blank query lists everything");
    }

    #[test]
    fn search_page_total_includes_day_matches_without_duplicates() {
        let repo = fresh_repo();
        let noon = |d| {
            Local
                .with_ymd_and_hms(2026, 10, d, 12, 0, 0)
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
        mk("Standup", noon(6));
        mk("Planning 10/6/26", noon(6));
        mk("Other", noon(7));
        let p = repo
            .search_page("10/6/26", &MeetingFilter::default(), 1, 1)
            .unwrap();
        assert_eq!(p.total, 2);
        assert_eq!(p.meetings.len(), 1);
    }
}
