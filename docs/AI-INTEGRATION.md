# AI integration

How an AI agent (Claude Code, a script, another tool) drives yogurt from the outside: starting and stopping meetings, reading transcripts and notes, without touching the UI.

Read [ARCHITECTURE.md](ARCHITECTURE.md) first for *why* the server is shaped this way; this doc only covers *how to call it*.

Prefer [`yogurt ctl`](../README.md#command-line) over the raw routes below - it resolves the port and token for you.
The routes are what it wraps, for scripting directly or for what `ctl` doesn't cover yet.

## Auth

Every `/api/*` route except `/api/health` and `/api/session-token` requires a bearer token, on disk at `~/.yogurt/session-token`.
An agent on the same machine can read that file directly - it's already inside the trust boundary the server assumes (see `require_session_token` in `crates/yogurt-server/src/routes.rs`).
The token is a local secret, equivalent to filesystem access: never send it anywhere but `localhost:7878`, never log it.

```bash
TOKEN=$(cat ~/.yogurt/session-token)
curl -s http://localhost:7878/api/meetings/active -H "Authorization: Bearer $TOKEN"
```

## Routes

`GET /api/health` returns `{"status":"ok","service":"yogurt-server","version":"0.7.0","mode":"release"}` (`"dev"` under `just dev`/`--dev`).
`ctl` never sets or reveals a key or the token; no `ctl` column entry means only `crates/yogurt-server/src/api/meetings.rs` covers it.

| Method | Path | Purpose | `ctl` |
|---|---|---|---|
| GET | `/api/health` | liveness | - |
| GET | `/api/meetings` | list | `ctl meeting list` |
| POST | `/api/meetings` | create (seedable, below) | `ctl meeting new` |
| GET | `/api/meetings/search` | search | `ctl meeting search` |
| GET | `/api/meetings/active` | what's recording | `ctl status` |
| GET | `/api/meetings/{id}` | full row | `ctl meeting show` |
| PATCH | `/api/meetings/{id}` | update title/notes | - |
| DELETE | `/api/meetings/{id}` | delete | `ctl meeting delete` |
| GET | `/api/meetings/{id}/markdown` | notes export (summary) | `ctl meeting summary` |
| POST | `/api/meetings/{id}/start` | start recording | `ctl meeting start` |
| POST | `/api/meetings/{id}/stop` | stop recording | `ctl meeting stop` |
| POST | `/api/meetings/{id}/enhance` | generate notes | `ctl meeting enhance` |
| GET | `/api/templates` | note formats for enhance | - |
| POST | `/api/meetings/{id}/mic-muted` | mute/unmute mic | `ctl meeting mute` |
| POST | `/api/meetings/{id}/echo` | echo mic to an output device: `{enabled?, device?}` | - |
| GET | `/api/audio/output-devices` | output devices for the echo | - |
| POST | `/api/meetings/{id}/chat` | ask a question (streamed) | - |
| GET | `/api/meetings/detected` | detected meeting | `ctl detect` |
| POST | `/api/meetings/detected/dismiss` | dismiss prompt | `ctl detect dismiss` |
| GET/PATCH | `/api/settings` | settings | `ctl settings get`/`set` |
| GET | `/api/settings/providers` | list providers | `ctl provider list` |
| POST | `/api/settings/providers/{id}/activate` | activate provider | `ctl provider activate` |
| POST | `/api/settings/providers/{id}/test` | test provider key | `ctl provider test` |
| GET | `/api/stt/models` | list STT models | `ctl models list` |
| POST | `/api/stt/models/{name}/download` | download model | `ctl models download` |
| DELETE | `/api/stt/models/{name}` | delete model | `ctl models delete` |
| GET | `/ws/meetings/{id}` | transcript/enhance/chat/audio frames | `ctl ws <id>`, `ctl meeting transcript --follow` |
| GET | `/ws` | model-download frames | `ctl ws` |

## Fixture meetings

`POST /api/meetings` optionally takes `transcript_json` - `{ts_ms, channel, text}` segments in the column's stored shape (see [DEBUGGING-TRANSCRIPTS.md](DEBUGGING-TRANSCRIPTS.md)) - and `ended: true`, to seed a finished meeting without recording anything.
A malformed `transcript_json` 400s naming the bad field; no `stt_engine` is stamped, since nothing was recorded.
`notes_md` seeds the user's raw notes on the same row (`ctl meeting new --notes-file`), so a following `/enhance` with `transcript_json: "[]"` runs exactly what End meeting runs.
`ctl meeting new --transcript-file <segments.json>` and `--from-script <script>` wrap this - see [MODEL-EVAL.md](MODEL-EVAL.md).

`/enhance`'s body also takes an optional `template` field: `"auto"` or omitted auto-detects the note format from the transcript, or pass one of the ids `GET /api/templates` lists to force it.
The response and the meeting row both carry the resolved `template`.
`ctl meeting enhance --template <id>` forces it from the CLI.

## WebSocket frames

Both sockets share a `{"type": "<snake_case>", ...}` discriminator.
`/ws/meetings/{id}`: `transcript` (`ts_ms`, `channel`, `text`, `is_final`), `enhance_progress`, `chat_chunk`, `audio_level`, `stt_error`.
`/ws`: `stt_model_download_progress`/`complete`/`error`.
`ctl ws` prints one frame per line from either; `--types` filters by `type`.

## What agents should not try to do

Restated from AGENTS.md's [hard constraints](../AGENTS.md#hard-constraints-never-violate):

- Don't read or transmit `~/.yogurt/keys.json` - nothing in the API exposes it.
- Don't script around the single-active-recording model (hammering `/start` on multiple ids) - the server won't stop you, and captures will overlap.
