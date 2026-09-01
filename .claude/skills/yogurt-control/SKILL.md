---
name: yogurt-control
description: Read or control a meeting in a running yogurt instance. Use when handed a yogurt meeting URL (http://localhost:7878/meeting/<id>/post) or asked "what was discussed in this meeting", "summarize this yogurt meeting", "what did they say about X" - and for "start a meeting in yogurt", "start recording", "end/stop the meeting", "is yogurt recording".
---

# Read and control yogurt

Reference: [docs/AI-INTEGRATION.md](../../../docs/AI-INTEGRATION.md) is the full API surface. This skill covers the common read and start/stop paths - read the doc if you need more than that.

## First, confirm the server is up

```bash
curl -sf http://localhost:7878/api/health >/dev/null || echo "yogurt is not running"
```

yogurt has no CLI to start it headlessly - if it's not running, tell the user to launch the app (or `just dev`) and stop here. For reads only, there is an offline fallback at the bottom of this file.

## Auth

```bash
TOKEN=$(cat ~/.yogurt/session-token)
```

Send `-H "Authorization: Bearer $TOKEN"` on every call below. Never print `$TOKEN` or send it anywhere but the local yogurt origin.

## Read a meeting from its URL

Given a link like `http://127.0.0.1:7878/meeting/01a0594d-.../post` (the `/post` suffix is optional - a live meeting URL has the same shape), take the origin and the id straight out of the URL. Take the port from the URL too, not from this file: `just dev` moves the backend off 7878 when a second worktree is already running.

```bash
URL="<the URL the user gave you>"
BASE=$(echo "$URL" | sed -E 's#(https?://[^/]+)/.*#\1#')
ID=$(echo "$URL" | sed -E 's#.*/meeting/([^/?#]+).*#\1#')
```

**Summary first.** `GET /:id/markdown` returns the canonical `~/.yogurt/notes/*.md` bytes - YAML front-matter plus the AI-enhanced summary, and no transcript. That is the whole point: it is roughly 9x smaller than the meeting row and is almost always enough to answer the question.

```bash
curl -s "$BASE/api/meetings/$ID/markdown" -H "Authorization: Bearer $TOKEN" | sed -E 's/<[^>]+>//g'
```

The `sed` strips the `<span data-ai-grey …>` wrappers the enhance renderer emits to tint AI-authored blocks in the editor. They carry no meaning outside the browser and roughly double the token cost. Stripping them leaves the `↳ 02:24` transcript deep-links intact, which is how you find *where* in the transcript to look if you need more.

An empty body under the front-matter means the meeting was never enhanced (`enriched_md` is null and the user typed no notes). Go to the transcript.

**Transcript only if the summary is not enough.** It lives in the meeting row, so ask for the row and let `jq` keep only the transcript - the rest of the row never reaches your context:

```bash
curl -s "$BASE/api/meetings/$ID" -H "Authorization: Bearer $TOKEN" \
  | jq -r '.transcript_json | fromjson | .[]
           | "\((.ts_ms/1000|floor|tostring)) \(.channel): \(.text)"'
```

`channel` is `me` (mic) or `them` (system audio), `ts_ms` is milliseconds from recording start. On a long meeting, `grep` this rather than reading all of it.

404 means the id is not in the database; 403 means the token is missing or stale.

## Offline fallback (server down, summary only)

The markdown export is on disk regardless of whether the server is running. Match on the front-matter id, **not** the filename: ids are UUIDv7 and therefore time-ordered, so the `-<id6>` filename suffix collides between meetings recorded minutes apart.

```bash
grep -l "^id: $ID$" ~/.yogurt/notes/*.md
```

The transcript has no on-disk copy - it is only in `~/.yogurt/db.sqlite`.

## Check what's recording (do this before starting)

```bash
curl -s http://localhost:7878/api/meetings/active -H "Authorization: Bearer $TOKEN"
```

`null` means nothing is live. yogurt only supports one active recording at a time - if this returns a meeting, don't call `/start` again; ask the user whether they want to stop it first.

## Start a meeting

```bash
ID=$(curl -s -X POST http://localhost:7878/api/meetings \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"title":"<meeting title>"}' | jq -r .id)

curl -s -X POST "http://localhost:7878/api/meetings/$ID/start" -H "Authorization: Bearer $TOKEN"
```

If the user also wants a Zoom/Meet/etc. call joined, do that as a separate step (e.g. `open "zoommtg://..."`) - yogurt captures system audio regardless of which app produces it, no meeting-platform integration needed. Tell the user recording has started once `/start` succeeds.

## Stop the active meeting

Get the id from `/api/meetings/active` if you don't already have it, then:

```bash
curl -s -X POST "http://localhost:7878/api/meetings/$ID/stop" -H "Authorization: Bearer $TOKEN"
```
