---
name: yogurt-control
description: Read or control a meeting in a running yogurt instance. Use when handed a yogurt meeting URL (http://localhost:7878/meeting/<id>/post) or asked "what was discussed in this meeting", "summarize this yogurt meeting", "what did they say about X" - and for "start a meeting in yogurt", "start recording", "end/stop the meeting", "is yogurt recording".
---

# Read and control yogurt

[docs/AI-INTEGRATION.md](../../../docs/AI-INTEGRATION.md) is the full API surface: auth, every recipe below, and the offline fallback.
This skill is the short version - when to reach for it, and the rules the recipes don't say out loud.

## The binary

<!-- yogurt-cli:start -->
- `yogurt start` - Launch the local server and open the browser
- `yogurt doctor` - Print diagnostic info (rust, macOS, perms, providers, models) + repair actions
<!-- yogurt-cli:end -->

`yogurt start --no-open` runs the server without opening a browser tab, but still in the foreground - the caller backgrounds it themselves (tmux by convention) if they want the shell back.
There is no way yet to control an already-running instance from the CLI; drive it over the REST API below until that lands.

## First, confirm the server is up

```bash
curl -sf http://localhost:7878/api/health >/dev/null || echo "yogurt is not running"
```

If it's not running: tell the user to launch it (`yogurt start`, or `just dev`) and stop here.
Reads still work offline - see the fallback below.

## Auth

```bash
TOKEN=$(cat ~/.yogurt/session-token)
```

Send `-H "Authorization: Bearer $TOKEN"` on every call below.
Never print `$TOKEN` or send it anywhere but the local yogurt origin.

## Read a meeting

Given a meeting URL, take the origin and id straight out of it, port included: `just dev` moves the backend off 7878 when a second worktree is already running.
See AI-INTEGRATION.md's ["Read what got captured"](../../../docs/AI-INTEGRATION.md#read-what-got-captured) for the extraction and the curl recipes.

**Summary first**, almost always enough: `GET /:id/markdown` is the canonical `~/.yogurt/notes/*.md` bytes, roughly 9x smaller than the full meeting row and with no transcript.
Only fall back to the transcript (`GET /api/meetings/:id`, filtered to `transcript_json`) when the summary doesn't answer the question.

## Offline fallback (server down, summary only)

The markdown export is on disk regardless of whether the server is running.
Match on the front-matter `id`, not the filename: ids are UUIDv7 and time-ordered, so the filename suffix collides between meetings recorded minutes apart.
See AI-INTEGRATION.md's ["With the server down"](../../../docs/AI-INTEGRATION.md#with-the-server-down) for the exact `grep`.

## Start / stop a meeting

yogurt supports one active recording at a time.
Check [`GET /api/meetings/active`](../../../docs/AI-INTEGRATION.md#check-whats-recording) before `start` - if it returns a meeting, ask the user whether to stop it first rather than starting a second one.
Recipes: AI-INTEGRATION.md's ["Start a meeting"](../../../docs/AI-INTEGRATION.md#start-a-meeting) and ["Stop a meeting"](../../../docs/AI-INTEGRATION.md#stop-a-meeting).
