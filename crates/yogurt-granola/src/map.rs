//! Turns a Granola note into the meeting yogurt stores. Pure, so it is tested on synthetic fixtures.

use crate::model::{Note, TranscriptLine};
use chrono::DateTime;
use serde_json::json;
use uuid::{NoContext, Timestamp, Uuid};
use yogurt_db::ImportedMeeting;

pub const SOURCE: &str = "granola";
const MAX_LABEL_CHARS: usize = 40;

fn ms(rfc3339: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(rfc3339)
        .ok()
        .map(|t| t.timestamp_millis())
}

/// `Speaker A: ...`, the per-turn lines a phone recording packs into one block.
fn is_speaker_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("Speaker ") else {
        return false;
    };
    let letters: usize = rest
        .chars()
        .take_while(|c| c.is_alphabetic())
        .map(char::len_utf8)
        .sum();
    letters > 0 && rest[letters..].starts_with(':')
}

/// One segment per `Speaker X:` turn. A line without the prefix continues the turn before it.
fn split_speaker_turns(text: &str) -> Vec<String> {
    let mut turns: Vec<String> = Vec::new();
    for line in text
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty())
    {
        match turns.last_mut() {
            Some(last) if !is_speaker_line(line) => {
                last.push('\n');
                last.push_str(line);
            }
            _ => turns.push(line.trim_start().to_string()),
        }
    }
    turns
}

fn is_phone_block(lines: &[TranscriptLine]) -> bool {
    lines.len() == 1 && lines[0].text.lines().filter(|l| is_speaker_line(l)).count() >= 2
}

/// `Ok(None)` when the note has no transcript or no summary, which the import skips.
/// `Err` when its timestamps cannot be read, which fails only that note.
/// `sanitize` cleans the summary before it is stored as `enriched_md`.
/// `untitled` names a note with no title from its start time.
pub fn map_note(
    note: &Note,
    sanitize: impl Fn(&str) -> String,
    untitled: impl Fn(i64) -> String,
) -> Result<Option<ImportedMeeting>, String> {
    let lines = note.transcript.as_deref().unwrap_or(&[]);
    let summary = [&note.summary_markdown, &note.summary_text]
        .into_iter()
        .flatten()
        .map(|s| s.trim())
        .find(|s| !s.is_empty());
    let (Some(summary), false) = (summary, lines.is_empty()) else {
        return Ok(None);
    };

    let phone = is_phone_block(lines);
    let created = ms(&note.created_at);
    let started_at = if phone {
        created
    } else {
        ms(&lines[0].start_time).or(created)
    }
    .ok_or("unreadable start time")?;
    let ended_at = lines
        .iter()
        .filter_map(|l| ms(&l.end_time))
        .max()
        .unwrap_or(started_at)
        .max(started_at);

    let segments: Vec<_> = if phone {
        split_speaker_turns(&lines[0].text)
            .into_iter()
            .map(|text| json!({ "ts_ms": 0, "channel": "them", "text": text }))
            .collect()
    } else {
        lines.iter().map(|l| segment(l, started_at)).collect()
    };

    let title = note
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| untitled(started_at));

    let mut labels: Vec<String> = Vec::new();
    for f in &note.folder_membership {
        let name: String = f.name.trim().chars().take(MAX_LABEL_CHARS).collect();
        if !name.is_empty() && !labels.iter().any(|l| l.eq_ignore_ascii_case(&name)) {
            labels.push(name);
        }
    }

    let id = Uuid::new_v7(Timestamp::from_unix(
        NoContext,
        (started_at.max(0) / 1000) as u64,
        (started_at.max(0) % 1000) as u32 * 1_000_000,
    ));

    Ok(Some(ImportedMeeting {
        id: id.to_string(),
        title,
        started_at,
        ended_at,
        notes_md: note.private_notes_markdown.clone().unwrap_or_default(),
        enriched_md: Some(sanitize(summary)),
        transcript_json: serde_json::to_string(&segments).map_err(|e| e.to_string())?,
        source: SOURCE.into(),
        source_id: note.id.clone(),
        source_json: json!({
            "owner": { "name": note.owner.name, "email": note.owner.email },
            "web_url": note.web_url,
            "updated_at": note.updated_at,
        }),
        labels,
    }))
}

fn segment(l: &TranscriptLine, started_at: i64) -> serde_json::Value {
    let s = &l.speaker;
    let channel = match (s.attribution.as_deref(), s.source.as_deref()) {
        (Some(a @ ("me" | "them")), _) => a,
        (_, Some("microphone")) => "me",
        _ => "them",
    };
    let who = [&s.name, &s.diarization_label]
        .into_iter()
        .flatten()
        .map(|w| w.trim())
        .find(|w| !w.is_empty());
    let text = match who {
        Some(w) => format!("{w}: {}", l.text),
        None => l.text.clone(),
    };
    let ts_ms = ms(&l.start_time).map_or(0, |t| (t - started_at).max(0));
    json!({ "ts_ms": ts_ms, "channel": channel, "text": text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn note(v: Value) -> Note {
        serde_json::from_value(v).unwrap()
    }

    fn map(v: Value) -> Result<Option<ImportedMeeting>, String> {
        map_note(&note(v), |s| s.to_string(), |_| "Meeting 1/2/26".into())
    }

    fn line(source: &str, attr: Option<&str>, text: &str, start: &str, end: &str) -> Value {
        let mut speaker = json!({ "source": source });
        if let Some(a) = attr {
            speaker["attribution"] = a.into();
        }
        json!({ "speaker": speaker, "text": text, "start_time": start, "end_time": end })
    }

    fn base(transcript: Value) -> Value {
        json!({
            "id": "not_synthetic000001",
            "title": "Weekly sync",
            "owner": { "name": "Pat Example", "email": "pat@example.com" },
            "created_at": "2026-01-02T09:00:00Z",
            "updated_at": "2026-01-02T10:00:00Z",
            "web_url": "https://notes.example/d/1",
            "folder_membership": [{ "name": "Clients" }, { "name": "clients" }],
            "summary_markdown": "### Decisions\n\n- ship it",
            "private_notes_markdown": "my note",
            "transcript": transcript,
        })
    }

    #[test]
    fn maps_a_desktop_note() {
        let m = map(base(json!([
            line(
                "microphone",
                Some("me"),
                "hello",
                "2026-01-02T09:00:05Z",
                "2026-01-02T09:00:08Z"
            ),
            line(
                "speaker",
                Some("them"),
                "hi there",
                "2026-01-02T09:00:09.500Z",
                "2026-01-02T09:00:12Z"
            ),
        ])))
        .unwrap()
        .unwrap();
        assert_eq!(m.title, "Weekly sync");
        assert_eq!(m.started_at, 1_767_344_405_000);
        assert_eq!(m.ended_at, 1_767_344_412_000);
        assert_eq!(m.notes_md, "my note");
        assert_eq!(m.enriched_md.as_deref(), Some("### Decisions\n\n- ship it"));
        assert_eq!(m.labels, ["Clients"]);
        assert_eq!(
            (m.source.as_str(), m.source_id.as_str()),
            ("granola", "not_synthetic000001")
        );
        assert_eq!(m.source_json["owner"]["email"], "pat@example.com");
        assert_eq!(m.source_json["web_url"], "https://notes.example/d/1");
        let t: Value = serde_json::from_str(&m.transcript_json).unwrap();
        assert_eq!(
            t,
            json!([
                { "ts_ms": 0, "channel": "me", "text": "hello" },
                { "ts_ms": 4500, "channel": "them", "text": "hi there" },
            ])
        );
    }

    #[test]
    fn id_is_a_v7_uuid_stamped_with_the_start() {
        let m = map(base(json!([line(
            "microphone",
            Some("me"),
            "x",
            "2026-01-02T09:00:05Z",
            "2026-01-02T09:00:06Z"
        )])))
        .unwrap()
        .unwrap();
        let id = Uuid::parse_str(&m.id).unwrap();
        assert_eq!(id.get_version_num(), 7);
        assert_eq!(id.get_timestamp().unwrap().to_unix().0, 1_767_344_405);
    }

    #[test]
    fn channel_falls_back_to_the_audio_source_and_names_prefix_the_text() {
        let mut l1 = line(
            "microphone",
            None,
            "a",
            "2026-01-02T09:00:00Z",
            "2026-01-02T09:00:01Z",
        );
        let mut l2 = line(
            "speaker",
            None,
            "b",
            "2026-01-02T09:00:01Z",
            "2026-01-02T09:00:02Z",
        );
        l1["speaker"]["diarization_label"] = "Speaker A".into();
        l2["speaker"]["name"] = "Alice Smith".into();
        l2["speaker"]["diarization_label"] = "Speaker B".into();
        let m = map(base(json!([l1, l2]))).unwrap().unwrap();
        let t: Value = serde_json::from_str(&m.transcript_json).unwrap();
        assert_eq!(t[0]["channel"], "me");
        assert_eq!(t[0]["text"], "Speaker A: a");
        assert_eq!(t[1]["channel"], "them");
        assert_eq!(t[1]["text"], "Alice Smith: b");
    }

    #[test]
    fn a_phone_block_splits_per_speaker_turn_at_the_note_creation_time() {
        let m = map(base(json!([line(
            "microphone",
            Some("me"),
            "Speaker A: hi\nstill A talking\nSpeaker B: hello\n\nSpeaker A: bye",
            "2026-01-02T09:30:00Z",
            "2026-01-02T09:40:00Z",
        )])))
        .unwrap()
        .unwrap();
        assert_eq!(
            m.started_at, 1_767_344_400_000,
            "created_at, not the line start"
        );
        assert_eq!(m.ended_at, 1_767_346_800_000);
        let t: Value = serde_json::from_str(&m.transcript_json).unwrap();
        assert_eq!(t.as_array().unwrap().len(), 3);
        assert_eq!(
            t[0],
            json!({ "ts_ms": 0, "channel": "them", "text": "Speaker A: hi\nstill A talking" })
        );
        assert_eq!(t[1]["text"], "Speaker B: hello");
        assert_eq!(t[2]["text"], "Speaker A: bye");
    }

    #[test]
    fn non_ascii_speaker_names_split_without_panicking() {
        let m = map(base(json!([line(
            "microphone",
            Some("me"),
            "Speaker \u{dc}nal: hi\nSpeaker B: hello",
            "2026-01-02T09:30:00Z",
            "2026-01-02T09:40:00Z",
        )])))
        .unwrap()
        .unwrap();
        let t: Value = serde_json::from_str(&m.transcript_json).unwrap();
        assert_eq!(t.as_array().unwrap().len(), 2);
        assert_eq!(t[0]["text"], "Speaker \u{dc}nal: hi");
    }

    #[test]
    fn a_single_line_with_one_speaker_prefix_is_not_a_phone_block() {
        let m = map(base(json!([line(
            "microphone",
            Some("me"),
            "Speaker A: just one",
            "2026-01-02T09:00:05Z",
            "2026-01-02T09:00:06Z"
        )])))
        .unwrap()
        .unwrap();
        let t: Value = serde_json::from_str(&m.transcript_json).unwrap();
        assert_eq!(t[0]["channel"], "me");
    }

    #[test]
    fn untitled_and_unowned_notes_get_defaults() {
        let mut v = base(json!([line(
            "speaker",
            Some("them"),
            "x",
            "2026-01-02T09:00:05Z",
            "2026-01-02T09:00:06Z"
        )]));
        v["title"] = Value::Null;
        v["private_notes_markdown"] = Value::Null;
        let m = map(v).unwrap().unwrap();
        assert_eq!(m.title, "Meeting 1/2/26");
        assert_eq!(m.notes_md, "");
    }

    #[test]
    fn summary_text_is_the_fallback_and_a_missing_summary_or_transcript_is_skipped() {
        let mut v = base(json!([line(
            "speaker",
            Some("them"),
            "x",
            "2026-01-02T09:00:05Z",
            "2026-01-02T09:00:06Z"
        )]));
        v["summary_markdown"] = Value::Null;
        v["summary_text"] = "plain summary".into();
        assert_eq!(
            map(v.clone()).unwrap().unwrap().enriched_md.as_deref(),
            Some("plain summary")
        );
        v["summary_text"] = "  ".into();
        assert!(map(v).unwrap().is_none());
        assert!(map(base(json!([]))).unwrap().is_none());
        assert!(map(base(Value::Null)).unwrap().is_none());
    }

    #[test]
    fn unreadable_times_fail_the_note() {
        let mut v = base(json!([line(
            "speaker",
            Some("them"),
            "x",
            "garbage",
            "garbage"
        )]));
        v["created_at"] = "also garbage".into();
        assert!(map(v).is_err());
    }

    #[test]
    fn long_folder_names_are_truncated_to_the_label_limit() {
        let mut v = base(json!([line(
            "speaker",
            Some("them"),
            "x",
            "2026-01-02T09:00:05Z",
            "2026-01-02T09:00:06Z"
        )]));
        v["folder_membership"] = json!([{ "name": "x".repeat(60) }, { "name": "  " }]);
        let m = map(v).unwrap().unwrap();
        assert_eq!(m.labels, ["x".repeat(40)]);
    }
}
