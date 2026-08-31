# AI integration

How an AI agent (Claude Code, a script, another tool) drives yogurt from the outside: starting and stopping meetings, reading transcripts and notes, without touching the UI.

Read [ARCHITECTURE.md](ARCHITECTURE.md) first if you need to know *why* the server is shaped this way.
This doc only covers *how to call it*.

## The short version

yogurt is a single `axum` server on `http://localhost:7878`.
Every `/api/*` route except `/api/health` and `/api/session-token` requires a bearer token, which lives on disk at `~/.yogurt/session-token`.
An agent running on the same machine can read that file directly - it's already inside the trust boundary the server assumes (see `require_session_token` in `crates/yogurt-server/src/routes.rs`).

```bash
TOKEN=$(cat ~/.yogurt/session-token)
curl -s http://localhost:7878/api/health   # confirm the server is up first
```

Every example below sends `Authorization: Bearer $TOKEN`.

## Is yogurt running?

`GET /api/health` needs no token and returns `{"status":"ok","service":"yogurt-server"}` if the binary is up.
If it isn't, there's nothing else you can do - yogurt has no CLI and no daemon-start-on-demand; the user has to launch the app (or `just dev`) themselves.

## Start a meeting

Two calls: create the meeting row, then start the recording stream against its id.

```bash
ID=$(curl -s -X POST http://localhost:7878/api/meetings \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"title":"Standup with Zoom"}' | jq -r .id)

curl -s -X POST "http://localhost:7878/api/meetings/$ID/start" \
  -H "Authorization: Bearer $TOKEN"
```

`title` is optional - an empty body creates "Untitled meeting". `start` begins capturing mic + system audio and transcribing live; from this point audio is being recorded until you call `stop`.

If you're also joining a Zoom/Meet/etc. call as part of this, launch that separately (`open "zoommtg://..."`, an AppleScript, whatever) - yogurt has no meeting-platform integration and doesn't need one; it captures system audio regardless of which app is making the sound.

## Check what's recording

```bash
curl -s http://localhost:7878/api/meetings/active -H "Authorization: Bearer $TOKEN"
```

Returns `null` if nothing is recording, otherwise the active meeting's `id`, `title`, `started_at`, and `stt_engine`. Use this before `start` if you're not sure whether a meeting is already live - yogurt supports one active recording at a time.

## Stop a meeting

```bash
curl -s -X POST "http://localhost:7878/api/meetings/$ID/stop" -H "Authorization: Bearer $TOKEN"
```

Idempotent-ish: stopping twice doesn't error, but only the first call stamps `ended_at`.

## Read what got captured

```bash
# Raw notes + transcript fields as JSON
curl -s "http://localhost:7878/api/meetings/$ID" -H "Authorization: Bearer $TOKEN"

# The canonical markdown file (front-matter + body), same bytes as ~/.yogurt/notes/*.md
curl -s "http://localhost:7878/api/meetings/$ID/markdown" -H "Authorization: Bearer $TOKEN"
```

The JSON form's `transcript_json` and `notes_md` fields are live during recording; `enriched_md` is only populated after the meeting has been enhanced (`POST /api/meetings/:id/enhance`) or the user has hit that button in the UI.

Full CRUD (`GET /api/meetings` list, `PATCH`/`DELETE /api/meetings/:id`, `GET /api/meetings/search?q=`) exists too - see `crates/yogurt-server/src/api/meetings.rs` for the complete surface if you need more than start/stop/read.

## What agents should not try to do

These follow from the [hard constraints](../AGENTS.md#hard-constraints-never-violate) in AGENTS.md, restated for anything calling the API rather than editing the code:

- Don't read or transmit `~/.yogurt/keys.json` (LLM/STT provider API keys) - nothing in the API surface exposes it, and there's no legitimate reason for a caller to need it.
- Don't script around the single-active-recording model (e.g. hammering `/start` on multiple ids) - the server doesn't guard against it and you'll get overlapping audio captures.
- The session token is a local secret, equivalent to filesystem access. Don't put it in a request to anything other than `localhost:7878`, and don't log it.
