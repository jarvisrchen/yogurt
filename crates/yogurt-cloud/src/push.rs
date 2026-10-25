//! One push pass (design Flow 4): list what the cloud holds for this device, send the meetings whose content changed, and mark the ones that disappeared.

use crate::{Error, Project, Result, Session};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Versions per insert, and the body bytes after which a batch closes early so one request stays small.
const BATCH_ROWS: usize = 100;
const BATCH_BYTES: usize = 1 << 20;

pub struct LocalMeeting {
    pub id: String,
    /// The hash stored after the last successful push, `None` if never pushed.
    pub cloud_hash: Option<String>,
    pub hash: String,
    pub body: Value,
}

impl LocalMeeting {
    pub fn new(id: String, cloud_hash: Option<String>, body: Value) -> Self {
        let hash = hash_body(&body);
        Self {
            id,
            cloud_hash,
            hash,
            body,
        }
    }
}

/// `serde_json` orders object keys, so the same content always hashes the same.
pub fn hash_body(body: &Value) -> String {
    let bytes = serde_json::to_vec(body).expect("a Value serializes");
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub changed: Vec<String>,
    pub deleted: Vec<String>,
}

/// `skip` is the meeting being recorded: it is neither pushed nor, if the cloud already has it, marked deleted.
/// A meeting the cloud no longer lists counts as changed even when its hash matches, so a purged device heals on the next pass.
pub fn plan(local: &[LocalMeeting], cloud_ids: &[String], skip: Option<&str>) -> Plan {
    let in_cloud: HashSet<&str> = cloud_ids.iter().map(String::as_str).collect();
    let here: HashSet<&str> = local.iter().map(|m| m.id.as_str()).collect();
    let mut changed: Vec<String> = local
        .iter()
        .filter(|m| Some(m.id.as_str()) != skip)
        .filter(|m| m.cloud_hash.as_deref() != Some(&m.hash) || !in_cloud.contains(m.id.as_str()))
        .map(|m| m.id.clone())
        .collect();
    let mut deleted: Vec<String> = cloud_ids
        .iter()
        .filter(|id| !here.contains(id.as_str()) && Some(id.as_str()) != skip)
        .cloned()
        .collect();
    changed.sort();
    deleted.sort();
    Plan { changed, deleted }
}

type Row = (Value, Option<(String, String)>);

/// Renews the access token when it is within 5 minutes of expiring, so a long pass never trips a 401.
async fn fresh(
    project: &Project,
    session: &mut Session,
    on_refresh: &mut impl FnMut(&Session) -> std::result::Result<(), String>,
) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    if session.expires_at - now < 300 {
        *session = project.refresh(&session.refresh_token).await?;
        on_refresh(session).map_err(Error::Local)?;
    }
    Ok(())
}

/// Closes a batch at 100 rows or 1 MiB of bodies.
fn batches(rows: Vec<Row>) -> Vec<Vec<Row>> {
    let mut out: Vec<Vec<Row>> = Vec::new();
    let mut bytes = 0;
    for row in rows {
        if out
            .last()
            .is_none_or(|b| b.len() >= BATCH_ROWS || bytes >= BATCH_BYTES)
        {
            out.push(Vec::new());
            bytes = 0;
        }
        bytes += row.0["body"].to_string().len();
        out.last_mut().expect("a batch was just pushed").push(row);
    }
    out
}

/// `session` is updated in place when it is renewed.
/// `on_refresh` runs with every renewed session, so the caller can persist the rotated refresh token before it is used.
/// `stored` runs after each batch commits with the `(id, hash)` of its changed meetings, so a later failure leaves earlier batches recorded.
/// A pass that finds nothing to do starts no push, so the history stays free of empty pushes.
/// A failing batch's error names its first and last meeting id.
pub async fn run(
    project: &Project,
    session: &mut Session,
    device_id: &str,
    local: &[LocalMeeting],
    skip: Option<&str>,
    mut on_refresh: impl FnMut(&Session) -> std::result::Result<(), String>,
    mut stored: impl FnMut(&[(String, String)]) -> std::result::Result<(), String>,
) -> Result<Plan> {
    let args = json!({ "device_id": device_id });
    fresh(project, session, &mut on_refresh).await?;
    let cloud: Vec<String> = project
        .rpc_all(session, "yogurt_device_meetings", &args, "")
        .await?
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect();
    let plan = plan(local, &cloud, skip);
    if plan.changed.is_empty() && plan.deleted.is_empty() {
        return Ok(plan);
    }
    fresh(project, session, &mut on_refresh).await?;
    let push_id = project
        .rpc(session, "yogurt_start_push", &args)
        .await?
        .as_i64()
        .ok_or_else(|| Error::Network("start_push returned no push id".into()))?;

    let by_id: HashMap<&str, &LocalMeeting> = local.iter().map(|m| (m.id.as_str(), m)).collect();
    let row = |id: &String, m: Option<&&LocalMeeting>| -> Row {
        let body = m.map_or(Value::Null, |m| m.body.clone());
        let hash = m.map(|m| (m.id.clone(), m.hash.clone()));
        (
            json!({ "meeting_id": id, "push_id": push_id, "device_id": device_id, "body": body }),
            hash,
        )
    };
    let rows = plan
        .changed
        .iter()
        .map(|id| row(id, by_id.get(id.as_str())))
        .chain(plan.deleted.iter().map(|id| row(id, None)))
        .collect();

    for batch in batches(rows) {
        fresh(project, session, &mut on_refresh).await?;
        let ids = (
            batch[0].0["meeting_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            batch[batch.len() - 1].0["meeting_id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        );
        let (rows, hashes): (Vec<Value>, Vec<Option<(String, String)>>) = batch.into_iter().unzip();
        project
            .insert(session, "yogurt_versions", &Value::Array(rows))
            .await
            .map_err(|e| match e {
                Error::Http {
                    status,
                    code,
                    message,
                } => Error::Http {
                    status,
                    code,
                    message: format!("{message} (batch of meetings {} to {})", ids.0, ids.1),
                },
                other => other,
            })?;
        stored(&hashes.into_iter().flatten().collect::<Vec<_>>()).map_err(Error::Local)?;
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(id: &str, stored: Option<&str>, body: Value) -> LocalMeeting {
        LocalMeeting::new(id.into(), stored.map(str::to_string), body)
    }
    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plan_sends_new_and_edited_and_marks_vanished() {
        let same = m("a", None, json!({"n": 1}));
        let same = m("a", Some(&same.hash.clone()), json!({"n": 1}));
        let edited = m("b", Some("stale"), json!({"n": 2}));
        let fresh = m("c", None, json!({"n": 3}));
        let p = plan(&[same, edited, fresh], &ids(&["a", "b", "gone"]), None);
        assert_eq!(p.changed, ids(&["b", "c"]));
        assert_eq!(p.deleted, ids(&["gone"]));
    }

    #[test]
    fn plan_with_nothing_to_do_is_empty() {
        let a = m("a", None, json!(1));
        let a = m("a", Some(&a.hash.clone()), json!(1));
        assert_eq!(plan(&[a], &ids(&["a"]), None), Plan::default());
    }

    #[test]
    fn plan_resends_a_meeting_the_cloud_lost() {
        let a = m("a", None, json!(1));
        let a = m("a", Some(&a.hash.clone()), json!(1));
        assert_eq!(plan(&[a], &[], None).changed, ids(&["a"]));
    }

    #[test]
    fn plan_skips_the_recording_meeting_both_ways() {
        let p = plan(&[m("rec", None, json!(1))], &[], Some("rec"));
        assert_eq!(p, Plan::default());
        let p = plan(&[], &ids(&["rec"]), Some("rec"));
        assert_eq!(p, Plan::default());
    }

    #[test]
    fn plan_with_no_local_meetings_deletes_everything() {
        assert_eq!(plan(&[], &ids(&["b", "a"]), None).deleted, ids(&["a", "b"]));
    }

    #[test]
    fn hash_follows_content_only() {
        assert_eq!(
            hash_body(&json!({"b": 1, "a": 2})),
            hash_body(&json!({"a": 2, "b": 1}))
        );
        assert_ne!(hash_body(&json!({"a": 1})), hash_body(&json!({"a": 2})));
        assert_eq!(hash_body(&json!(null)).len(), 64);
    }
}

#[cfg(test)]
mod batch_tests {
    use super::*;

    #[test]
    fn batches_close_on_row_and_byte_limits() {
        let row = |n: usize| (json!({ "body": "x".repeat(n) }), None);
        let by_rows = batches((0..250).map(|_| row(1)).collect());
        assert_eq!(
            by_rows.iter().map(Vec::len).collect::<Vec<_>>(),
            [100, 100, 50]
        );
        let by_bytes = batches((0..5).map(|_| row(BATCH_BYTES / 2)).collect());
        assert_eq!(by_bytes.iter().map(Vec::len).collect::<Vec<_>>(), [2, 2, 1]);
        assert!(batches(Vec::new()).is_empty());
    }
}
