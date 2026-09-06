//! Granola-style meeting labels — workspace-level named tags with a color.
//!
//! A meeting can carry any number of labels (many-to-many via
//! `meeting_labels`). Labels are matched case-insensitively on name so
//! "Sales" and "sales" are the same label (`find_or_create` returns the
//! existing row rather than erroring or duplicating).
//!
//! `color` is either a palette *key* (one of [`COLORS`]) or a custom
//! `#rrggbb` hex string. Hex input is normalized to lowercase before
//! storage. The web `LabelChip` component owns the palette-key -> hex
//! mapping so a future palette refresh only touches one file; a hex
//! `color` is rendered as-is.

use crate::Db;
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Palette keys understood by the web `LabelChip`. Stored as the key, not
/// a hex string, so the web side owns the actual color values.
pub const COLORS: [&str; 6] = ["blue", "matcha", "straw", "lilac", "honey", "slate"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Custom sidebar order (UI-12), set via `LabelRepo::reorder`.
    pub position: i64,
    /// Unix ms, touched on rename/recolor and on apply/remove from a
    /// meeting (UI-12) — backs the "Last updated" sidebar sort.
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct LabelWithCount {
    #[serde(flatten)]
    pub label: Label,
    pub meeting_count: i64,
}

/// CRUD facade over the `labels` + `meeting_labels` tables.
#[derive(Clone)]
pub struct LabelRepo {
    db: Db,
}

impl LabelRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// All labels, name ASC (case-insensitive), with a `meeting_count` via
    /// a `LEFT JOIN meeting_labels ... GROUP BY` so labels with zero
    /// meetings still show up (with count 0).
    pub fn list_with_counts(&self) -> Result<Vec<LabelWithCount>> {
        self.db.with_conn(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT l.id, l.name, l.color, l.position, l.updated_at, COUNT(ml.meeting_id) \
                 FROM labels l \
                 LEFT JOIN meeting_labels ml ON ml.label_id = l.id \
                 GROUP BY l.id \
                 ORDER BY l.name COLLATE NOCASE",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(LabelWithCount {
                    label: Label {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        color: r.get(2)?,
                        position: r.get(3)?,
                        updated_at: r.get(4)?,
                    },
                    meeting_count: r.get(5)?,
                })
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })
    }

    /// Trim `name`; bail on empty or > 40 chars. If a label with the same
    /// name (case-insensitive) already exists, return it unchanged
    /// (`created = false`). Otherwise insert a fresh row (`created =
    /// true`) with a ULID id, the given `color` (or the next palette color
    /// by rotation), and `created_at = now`.
    pub fn find_or_create(&self, name: &str, color: Option<&str>) -> Result<(Label, bool)> {
        let name = validate_name(name)?;
        let color = color.map(normalize_color).transpose()?;
        self.db.with_conn(|conn| -> Result<(Label, bool)> {
            if let Some(existing) = find_by_name(conn, &name)? {
                return Ok((existing, false));
            }
            // Auto-color: first palette entry no existing label uses, so
            // adjacent labels stay visually distinct even after the user
            // recolors one; once every color is taken, cycle by count.
            let color = match &color {
                Some(c) => c.clone(),
                None => {
                    let mut stmt = conn.prepare("SELECT color FROM labels")?;
                    let used: Vec<String> = stmt
                        .query_map([], |r| r.get(0))?
                        .collect::<rusqlite::Result<_>>()?;
                    COLORS
                        .iter()
                        .find(|c| !used.iter().any(|u| u == *c))
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| COLORS[used.len() % COLORS.len()].to_string())
                }
            };
            let id = Ulid::new().to_string();
            let now_ms = chrono::Utc::now().timestamp_millis();
            // Append at the end of the custom order rather than defaulting
            // to 0, so a freshly-created label doesn't jump to the front
            // of a list the user has already hand-ordered.
            let position: i64 = conn.query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM labels",
                [],
                |r| r.get(0),
            )?;
            conn.execute(
                "INSERT INTO labels (id, name, color, created_at, position, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?4)",
                params![id, name, color, now_ms, position],
            )
            .context("insert label")?;
            Ok((
                Label {
                    id,
                    name,
                    color,
                    position,
                    updated_at: now_ms,
                },
                true,
            ))
        })
    }

    /// Update name and/or color. Same validation as create. A duplicate
    /// name (case-insensitive, different id) bails "label name already
    /// exists"; an unknown id bails "label not found".
    pub fn update(&self, id: &str, name: Option<&str>, color: Option<&str>) -> Result<Label> {
        let name = name.map(validate_name).transpose()?;
        let color = color.map(normalize_color).transpose()?;
        let id_owned = id.to_string();
        self.db.with_conn(|conn| -> Result<Label> {
            let current =
                find_by_id(conn, &id_owned)?.ok_or_else(|| anyhow::anyhow!("label not found"))?;
            if let Some(n) = name.as_ref() {
                if let Some(other) = find_by_name(conn, n)? {
                    if other.id != id_owned {
                        bail!("label name already exists");
                    }
                }
            }
            let new_name = name.unwrap_or(current.name);
            let new_color = color.unwrap_or(current.color);
            let now_ms = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "UPDATE labels SET name = ?1, color = ?2, updated_at = ?3 WHERE id = ?4",
                params![new_name, new_color, now_ms, id_owned],
            )
            .context("update label")?;
            Ok(Label {
                id: id_owned,
                name: new_name,
                color: new_color,
                position: current.position,
                updated_at: now_ms,
            })
        })
    }

    /// Set the custom sidebar order: `ids[0]` gets position 0, `ids[1]`
    /// position 1, etc. An id that doesn't exist bails "label not found"
    /// (same wording `set_for_meeting_conn` uses, so `ApiError::from`
    /// maps it to 400 the same way). Does not touch `updated_at` —
    /// reordering isn't a content change.
    pub fn reorder(&self, ids: &[String]) -> Result<()> {
        self.db.with_conn(|conn| {
            let tx = conn.unchecked_transaction().context("begin reorder tx")?;
            for (position, id) in ids.iter().enumerate() {
                let n = tx.execute(
                    "UPDATE labels SET position = ?1 WHERE id = ?2",
                    params![position as i64, id],
                )?;
                if n == 0 {
                    bail!("label not found: {id}");
                }
            }
            tx.commit().context("commit reorder tx")?;
            Ok(())
        })
    }

    /// Returns `Ok(true)` if a row was removed. `meeting_labels` rows
    /// cascade via the FK.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let id_owned = id.to_string();
        self.db.with_conn(|conn| {
            let n = conn.execute("DELETE FROM labels WHERE id = ?1", params![id_owned])?;
            Ok(n > 0)
        })
    }

    /// Replace `meeting_id`'s label set with exactly `label_ids`, inside
    /// one transaction. Unknown label ids bail "label not found". Does
    /// NOT touch `meetings.updated_at` — callers (e.g. `MeetingRepo::patch`)
    /// own that.
    pub fn set_for_meeting(&self, meeting_id: &str, label_ids: &[String]) -> Result<()> {
        self.db
            .with_conn(|conn| set_for_meeting_conn(conn, meeting_id, label_ids))
    }
}

/// Shared implementation used by both `LabelRepo::set_for_meeting` and
/// `MeetingRepo::patch` (label-only patches) so there is exactly one
/// place that knows how to rewrite a meeting's label set.
pub(crate) fn set_for_meeting_conn(
    conn: &rusqlite::Connection,
    meeting_id: &str,
    label_ids: &[String],
) -> Result<()> {
    let tx = conn.unchecked_transaction().context("begin label tx")?;
    for lid in label_ids {
        let exists: Option<i64> = tx
            .query_row("SELECT 1 FROM labels WHERE id = ?1", params![lid], |r| {
                r.get(0)
            })
            .optional()?;
        if exists.is_none() {
            bail!("label not found");
        }
    }
    let mut before_stmt =
        tx.prepare("SELECT label_id FROM meeting_labels WHERE meeting_id = ?1")?;
    let before: Vec<String> = before_stmt
        .query_map(params![meeting_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    drop(before_stmt);
    tx.execute(
        "DELETE FROM meeting_labels WHERE meeting_id = ?1",
        params![meeting_id],
    )?;
    for lid in label_ids {
        tx.execute(
            "INSERT OR IGNORE INTO meeting_labels (meeting_id, label_id) VALUES (?1, ?2)",
            params![meeting_id, lid],
        )?;
    }
    // UI-12: touch `updated_at` on every label whose association with
    // this meeting just changed (applied or removed), so "Last updated"
    // sidebar sort reflects it.
    let touched = before
        .iter()
        .filter(|l| !label_ids.contains(l))
        .chain(label_ids.iter().filter(|l| !before.contains(l)));
    let now_ms = chrono::Utc::now().timestamp_millis();
    for lid in touched {
        tx.execute(
            "UPDATE labels SET updated_at = ?1 WHERE id = ?2",
            params![now_ms, lid],
        )?;
    }
    tx.commit().context("commit label tx")?;
    Ok(())
}

fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        bail!("label name must not be empty");
    }
    if trimmed.chars().count() > 40 {
        bail!("label name must be 40 characters or fewer");
    }
    Ok(trimmed.to_string())
}

/// Accepts a palette key as-is, or a `#rrggbb` hex string (any case,
/// lowercased before storage). Anything else bails "invalid color".
fn normalize_color(color: &str) -> Result<String> {
    if COLORS.contains(&color) {
        return Ok(color.to_string());
    }
    let lower = color.to_lowercase();
    if is_hex_color(&lower) {
        return Ok(lower);
    }
    bail!("invalid color");
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn find_by_name(conn: &rusqlite::Connection, name: &str) -> rusqlite::Result<Option<Label>> {
    conn.query_row(
        "SELECT id, name, color, position, updated_at FROM labels WHERE name = ?1 COLLATE NOCASE",
        params![name],
        |r| {
            Ok(Label {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                position: r.get(3)?,
                updated_at: r.get(4)?,
            })
        },
    )
    .optional()
}

fn find_by_id(conn: &rusqlite::Connection, id: &str) -> rusqlite::Result<Option<Label>> {
    conn.query_row(
        "SELECT id, name, color, position, updated_at FROM labels WHERE id = ?1",
        params![id],
        |r| {
            Ok(Label {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
                position: r.get(3)?,
                updated_at: r.get(4)?,
            })
        },
    )
    .optional()
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meetings::{MeetingPatch, MeetingRepo, NewMeeting};

    fn fresh() -> (Db, LabelRepo, MeetingRepo) {
        let db = Db::open_in_memory().expect("open in-memory db");
        (db.clone(), LabelRepo::new(db.clone()), MeetingRepo::new(db))
    }

    #[test]
    fn find_or_create_is_case_insensitive_and_assigns_a_palette_color() {
        let (_, labels, _) = fresh();
        let (a, created_a) = labels.find_or_create("Sales", None).unwrap();
        assert!(created_a);
        assert!(COLORS.contains(&a.color.as_str()));
        let (b, created_b) = labels.find_or_create("sales", None).unwrap();
        assert!(!created_b);
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn find_or_create_rejects_empty_name() {
        let (_, labels, _) = fresh();
        let err = labels.find_or_create("   ", None).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn find_or_create_accepts_and_normalizes_hex_color() {
        let (_, labels, _) = fresh();
        let (a, _) = labels.find_or_create("Sales", Some("#A3C9FF")).unwrap();
        assert_eq!(a.color, "#a3c9ff");
    }

    #[test]
    fn find_or_create_rejects_invalid_hex_color() {
        let (_, labels, _) = fresh();
        for bad in ["#fff", "#gggggg", "blue2", "a3c9ff", "#a3c9ff0"] {
            let err = labels.find_or_create("Sales", Some(bad)).unwrap_err();
            assert!(
                err.to_string().contains("invalid color"),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn update_recolors_with_hex_and_keeps_palette_keys_working() {
        let (_, labels, _) = fresh();
        let (a, _) = labels.find_or_create("Sales", Some("blue")).unwrap();
        let recolored = labels.update(&a.id, None, Some("#123ABC")).unwrap();
        assert_eq!(recolored.color, "#123abc");
        let back = labels.update(&a.id, None, Some("matcha")).unwrap();
        assert_eq!(back.color, "matcha");
    }

    #[test]
    fn set_for_meeting_hydrates_sorted_and_counts_and_clears() {
        let (_, labels, meetings) = fresh();
        let m = meetings
            .create(NewMeeting {
                title: "T".into(),
                ..Default::default()
            })
            .unwrap();
        let (zebra, _) = labels.find_or_create("Zebra", None).unwrap();
        let (apple, _) = labels.find_or_create("Apple", None).unwrap();
        labels
            .set_for_meeting(&m.id, &[zebra.id.clone(), apple.id.clone()])
            .unwrap();
        let reloaded = meetings.get(&m.id).unwrap().unwrap();
        assert_eq!(reloaded.labels.len(), 2);
        assert_eq!(reloaded.labels[0].name, "Apple", "sorted by name");
        assert_eq!(reloaded.labels[1].name, "Zebra");

        let counts = labels.list_with_counts().unwrap();
        let apple_count = counts.iter().find(|l| l.label.id == apple.id).unwrap();
        assert_eq!(apple_count.meeting_count, 1);

        labels.set_for_meeting(&m.id, &[]).unwrap();
        let cleared = meetings.get(&m.id).unwrap().unwrap();
        assert!(cleared.labels.is_empty());
    }

    #[test]
    fn update_renames_and_rejects_duplicate_name() {
        let (_, labels, _) = fresh();
        let (a, _) = labels.find_or_create("Sales", None).unwrap();
        let (b, _) = labels.find_or_create("Support", None).unwrap();
        let renamed = labels.update(&a.id, Some("Customer"), None).unwrap();
        assert_eq!(renamed.name, "Customer");
        let err = labels.update(&b.id, Some("customer"), None).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn delete_label_removes_it_from_meeting() {
        let (_, labels, meetings) = fresh();
        let m = meetings
            .create(NewMeeting {
                title: "T".into(),
                ..Default::default()
            })
            .unwrap();
        let (l, _) = labels.find_or_create("Sales", None).unwrap();
        labels
            .set_for_meeting(&m.id, std::slice::from_ref(&l.id))
            .unwrap();
        assert!(labels.delete(&l.id).unwrap());
        let reloaded = meetings.get(&m.id).unwrap().unwrap();
        assert!(reloaded.labels.is_empty());
    }

    #[test]
    fn deleting_meeting_drops_meeting_labels_rows() {
        let (db, labels, meetings) = fresh();
        let m = meetings
            .create(NewMeeting {
                title: "T".into(),
                ..Default::default()
            })
            .unwrap();
        let (l, _) = labels.find_or_create("Sales", None).unwrap();
        labels
            .set_for_meeting(&m.id, std::slice::from_ref(&l.id))
            .unwrap();
        assert!(meetings.delete(&m.id).unwrap());
        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM meeting_labels", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn meeting_patch_with_label_ids_only_applies_labels_and_bumps_updated_at() {
        let (_, labels, meetings) = fresh();
        let m = meetings
            .create(NewMeeting {
                title: "T".into(),
                ..Default::default()
            })
            .unwrap();
        let (l, _) = labels.find_or_create("Sales", None).unwrap();
        let before = m.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let patched = meetings
            .patch(
                &m.id,
                MeetingPatch {
                    label_ids: Some(vec![l.id.clone()]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(patched.labels.len(), 1);
        assert_eq!(patched.labels[0].id, l.id);
        assert!(patched.updated_at > before);
    }

    #[test]
    fn find_or_create_appends_position_at_the_end() {
        let (_, labels, _) = fresh();
        let (a, _) = labels.find_or_create("Sales", None).unwrap();
        let (b, _) = labels.find_or_create("Support", None).unwrap();
        assert_eq!(a.position, 0);
        assert_eq!(b.position, 1);
    }

    #[test]
    fn reorder_sets_positions_in_the_given_order() {
        let (_, labels, _) = fresh();
        let (a, _) = labels.find_or_create("Alpha", None).unwrap();
        let (b, _) = labels.find_or_create("Bravo", None).unwrap();
        let (c, _) = labels.find_or_create("Charlie", None).unwrap();

        labels
            .reorder(&[c.id.clone(), a.id.clone(), b.id.clone()])
            .unwrap();

        let by_id = |id: &str| {
            labels
                .list_with_counts()
                .unwrap()
                .into_iter()
                .find(|l| l.label.id == id)
                .unwrap()
                .label
                .position
        };
        assert_eq!(by_id(&c.id), 0);
        assert_eq!(by_id(&a.id), 1);
        assert_eq!(by_id(&b.id), 2);
    }

    #[test]
    fn reorder_rejects_an_unknown_id() {
        let (_, labels, _) = fresh();
        let err = labels.reorder(&["nonesuch".to_string()]).unwrap_err();
        assert!(err.to_string().contains("label not found"));
    }

    #[test]
    fn update_touches_updated_at() {
        let (_, labels, _) = fresh();
        let (a, _) = labels.find_or_create("Sales", None).unwrap();
        let before = a.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(2));
        let renamed = labels.update(&a.id, Some("Customers"), None).unwrap();
        assert!(renamed.updated_at > before);
    }

    #[test]
    fn applying_and_removing_a_label_touches_its_updated_at() {
        let (_, labels, meetings) = fresh();
        let m = meetings
            .create(NewMeeting {
                title: "T".into(),
                ..Default::default()
            })
            .unwrap();
        let (l, _) = labels.find_or_create("Sales", None).unwrap();
        let before = l.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(2));
        labels
            .set_for_meeting(&m.id, std::slice::from_ref(&l.id))
            .unwrap();
        let after_apply = labels
            .list_with_counts()
            .unwrap()
            .into_iter()
            .find(|x| x.label.id == l.id)
            .unwrap()
            .label
            .updated_at;
        assert!(after_apply > before, "applying a label touches updated_at");

        std::thread::sleep(std::time::Duration::from_millis(2));
        labels.set_for_meeting(&m.id, &[]).unwrap();
        let after_remove = labels
            .list_with_counts()
            .unwrap()
            .into_iter()
            .find(|x| x.label.id == l.id)
            .unwrap()
            .label
            .updated_at;
        assert!(
            after_remove > after_apply,
            "removing a label touches updated_at"
        );
    }
}
