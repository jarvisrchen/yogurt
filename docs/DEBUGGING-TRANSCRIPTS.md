# Debugging transcripts

How to see what the transcript dock is showing, what SQLite actually stored, and why the two can legitimately differ.

For the mechanism itself - who subscribes to what, in what order - read [ARCHITECTURE.md §4](./ARCHITECTURE.md#4-sequence-record-a-meeting-live-transcript).
This doc is the operational companion: the commands to run when the dock looks wrong.

---

## The one thing to internalize

There are two transcripts, not one, and they are built by different code from the same upstream broadcast.

| | Transcript dock (UI) | `meetings.transcript_json` (SQLite) |
|---|---|---|
| Contains | finals **and** the trailing partial per channel | finals only, non-empty only |
| Built by | `mergeEvent` in the browser (`web/src/lib/ws.ts:105`) | `persist_transcript` on the server (`crates/yogurt-server/src/meetings.rs:852`) |
| Fed from | `/ws/meetings/{id}` frames, plus a one-time seed from the DB | the `transcript_tx` broadcast, directly |
| Survives reload | no, it is rebuilt from the seed plus new frames | yes, it is the source of truth |
| Timestamps | `ts_ms` as received | same `ts_ms`, the `offset_ms` for continuation sessions is applied once in `relay_transcript_events` before either consumer sees the event |

The server owns the transcript.
The browser never sends it back, so nothing the UI does can corrupt what is stored - a UI bug is always a rendering bug.

---

## Watch the persisted transcript live

```bash
scripts/tail-transcript.sh                 # newest meeting -> /tmp/yogurt-transcript.json
scripts/tail-transcript.sh <meeting-id>    # a specific meeting
OUT=~/t.txt INTERVAL=1 scripts/tail-transcript.sh
```

Leave it running during a meeting and open the output file in an editor.
It rewrites the file atomically every couple of seconds (temp file plus `mv`), so an editor that reloads on change never catches a half-written file.
One line per persisted segment: `ts_ms  channel  text`.

`persist_transcript` writes on every final, so the file grows within a second or two of each finished utterance.
If a line is in the dock but never lands here, it was a partial that never got finalized.

## Watch the raw WebSocket

This is byte-for-byte what the dock receives, partials included.

```bash
websocat "ws://localhost:7878/ws/meetings/<MEETING_ID>?token=$(cat ~/.yogurt/session-token)"
```

Frames are `{"type":"transcript","payload":{ts_ms,channel,text,is_final}}`.
`enhance_progress` and chat frames ride the same socket, so expect other `type` values interleaved.

Use this when you need to see partial churn - the DB tail cannot show it, because partials are never persisted.

## Open the DB in a visualizer

SQLite, so the "connection string" is just the file path.

```
/Users/<you>/.yogurt/db.sqlite
```

- TablePlus / DBeaver / Beekeeper: choose SQLite, point at that path, and **open read-only** so you are not competing with the running server for the write lock.
- JDBC: `jdbc:sqlite:/Users/<you>/.yogurt/db.sqlite`
- SQLAlchemy: `sqlite:////Users/<you>/.yogurt/db.sqlite`

The database runs in WAL mode, so most GUI tools will not auto-refresh mid-meeting.
Re-run the query to pick up new segments.

Flattening the JSON column into rows:

```sql
select json_extract(value, '$.ts_ms')   as ts_ms,
       json_extract(value, '$.channel') as channel,
       json_extract(value, '$.text')    as text
from meetings, json_each(meetings.transcript_json)
where meetings.id = '<meeting-id>';
```

`channel` is `"me"` for mic (you) and `"them"` for system audio (everyone else), set by `segment_json`.

---

## Known reasons the dock and the DB disagree

Work down this list before assuming a new bug.

**A repeated sentence appears once in the dock but twice in the DB.**
When the dock seeds itself from persisted history, it records the `(channel, text)` of every seeded final and drops any live final that matches (`web/src/lib/ws.ts:283`).
That exists to kill the duplicate you get when the history fetch races the WS subscribe.
It cannot tell a genuine verbatim repeat from a redelivery, so the DB keeps both and the dock keeps one.

**A line is in the dock but not in the DB.**
Almost always a partial.
`persist_transcript` skips anything where `is_final` is false or the text trims to empty.
Confirm on the raw WS - if the line never arrives with `is_final: true`, there is nothing to persist.

**Lines are missing from both.**
Look for `transcript persistence lagged` or `ws/meetings: client lagged` in the server log.
The broadcast channel holds 256 events; a consumer that falls behind gets a `Lagged(n)` and those events are gone.
Also check for the `[stt overloaded, transcript may be lossy]` status line, which means audio chunks were dropped upstream of STT entirely.

**Timestamps restart at zero mid-meeting.**
That is a stop/restart where `session_offset_ms` failed to resolve the original `started_at`.
The offset is applied in exactly one place (`relay_transcript_events`), so if the DB has it wrong the dock has it wrong too.

**The whole prior session vanished after a restart.**
`persist_transcript` seeds its accumulator from the existing column via `load_existing_segments` precisely to stop this, because `write_segments` PATCHes the entire column each time.
If it recurs, that seed is the thing to check first.

**Text renders as mojibake.**
That is a storage or sanitization bug, not a dock bug, and it will be identical in the tail file.
