---
name: yogurt-control
description: Start, stop, or check a live meeting recording in a running yogurt instance (localhost:7878). Use when asked to "start a meeting in yogurt", "start recording", "end/stop the meeting", "is yogurt recording", or to record a Zoom/Meet/etc. call via yogurt.
---

# Control yogurt

Reference: [docs/AI-INTEGRATION.md](../../../docs/AI-INTEGRATION.md) is the full API surface. This skill only covers the common start/stop/check path - read the doc if you need more than that.

## First, confirm the server is up

```bash
curl -sf http://localhost:7878/api/health >/dev/null || echo "yogurt is not running"
```

yogurt has no CLI to start it headlessly - if it's not running, tell the user to launch the app (or `just dev`) and stop here.

## Auth

```bash
TOKEN=$(cat ~/.yogurt/session-token)
```

Send `-H "Authorization: Bearer $TOKEN"` on every call below. Never print `$TOKEN` or send it anywhere but `localhost:7878`.

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
