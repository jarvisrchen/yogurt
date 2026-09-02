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

`GET /api/health` needs no token and returns `{"status":"ok","service":"yogurt-server","version":"0.7.0","mode":"release"}` if the binary is up (`mode` is `"dev"` under `just dev` / `yogurt start --dev`).
If it isn't up, `yogurt ctl status` says so - see "Controlling yogurt from the CLI" below for the full `ctl` surface, which covers everything in this document plus meeting detection and window matching.

## Controlling yogurt from the CLI

Everything below this section is the raw HTTP surface, useful if you're scripting against yogurt directly.
If you have the `yogurt` binary on `$PATH` (brew install, or a debug build), `yogurt ctl` is the same surface as real subcommands - it resolves the port, reads the token, and formats errors with a `help:` line instead of a bare `curl` exit code:

```bash
yogurt ctl status                                  # is a server up, what's recording, what's detected
yogurt ctl meeting list [--limit N]
yogurt ctl meeting new [--title T] [--start]
yogurt ctl meeting start <id|last>
yogurt ctl meeting stop [<id|url|last>]
yogurt ctl meeting show|summary|transcript <id|url|last>
yogurt ctl meeting enhance <id|url|last>
yogurt ctl detect [dismiss]
yogurt ctl windows
```

`--json` on any subcommand gets machine-readable output; `--port`/`$YOGURT_PORT` picks the instance when more than one is running (`just dev` prints the `$YOGURT_PORT` line to use).
`<id|url|last>` accepts a bare id, a full meeting URL, or `last` for the most recently created meeting; read commands fall back to the local database (`source: db`) when no server answers.
See the README's [Command line](../README.md#command-line) section for the full flag reference.
The rest of this document still applies when `ctl` doesn't have a subcommand for what you need yet (full CRUD, `PATCH`, `search`) - `ctl`'s second slice (`docs/.planning/agent-workflow.md`) is where those land.

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

If you were handed a URL like `http://127.0.0.1:7878/meeting/<id>/post`, that is all you need - the origin and the meeting id are both in it. Take the port from the URL rather than assuming 7878, since `just dev` moves the backend when a second worktree is already running.

```bash
BASE=$(echo "$URL" | sed -E 's#(https?://[^/]+)/.*#\1#')
ID=$(echo "$URL" | sed -E 's#.*/meeting/([^/?#]+).*#\1#')
```

**Summary first.** `GET /:id/markdown` returns the canonical `~/.yogurt/notes/*.md` bytes - YAML front-matter plus the enhanced summary, and no transcript:

```bash
curl -s "$BASE/api/meetings/$ID/markdown" -H "Authorization: Bearer $TOKEN" | sed -E 's/<[^>]+>//g'
```

The `sed` strips the `<span data-ai-grey …>` wrappers `yogurt-notes`' renderer emits so the editor can tint AI-authored blocks (see MTG-3). They mean nothing outside the browser and roughly double the byte count; stripping them leaves the `↳ 02:24` transcript deep-links readable. On a representative meeting that is 849 bytes against 7,864 for the full row - so prefer this endpoint over `GET /api/meetings/:id` whenever the question is about what was *said*, not about the row's metadata.

An empty body under the front-matter means the meeting was never enhanced (`enriched_md` is null and the user typed no notes), not that the meeting was empty.

**Transcript only when the summary is not enough.** It ships inside the meeting row, so filter it out of the response rather than reading the row:

```bash
curl -s "$BASE/api/meetings/$ID" -H "Authorization: Bearer $TOKEN" \
  | jq -r '.transcript_json | fromjson | .[]
           | "\((.ts_ms/1000|floor|tostring)) \(.channel): \(.text)"'
```

`channel` is `me` (mic) or `them` (system audio); `ts_ms` is milliseconds from recording start, which is what the summary's `↳ mm:ss` links point at.

The JSON form's `transcript_json` and `notes_md` fields are live during recording; `enriched_md` is only populated after the meeting has been enhanced (`POST /api/meetings/:id/enhance`) or the user has hit that button in the UI.

### With the server down

The markdown export is on disk whether or not yogurt is running. Match the front-matter id, not the filename - meeting ids are UUIDv7 and therefore time-ordered, so the `-<id6>` suffix in `<date>-<slug>-<id6>.md` collides between meetings recorded minutes apart:

```bash
grep -l "^id: $ID$" ~/.yogurt/notes/*.md
```

There is no on-disk transcript; that lives only in `~/.yogurt/db.sqlite`.

Full CRUD (`GET /api/meetings` list, `PATCH`/`DELETE /api/meetings/:id`, `GET /api/meetings/search?q=`) exists too - see `crates/yogurt-server/src/api/meetings.rs` for the complete surface if you need more than start/stop/read.

## What agents should not try to do

These follow from the [hard constraints](../AGENTS.md#hard-constraints-never-violate) in AGENTS.md, restated for anything calling the API rather than editing the code:

- Don't read or transmit `~/.yogurt/keys.json` (LLM/STT provider API keys) - nothing in the API surface exposes it, and there's no legitimate reason for a caller to need it.
- Don't script around the single-active-recording model (e.g. hammering `/start` on multiple ids) - the server doesn't guard against it and you'll get overlapping audio captures.
- The session token is a local secret, equivalent to filesystem access. Don't put it in a request to anything other than `localhost:7878`, and don't log it.
