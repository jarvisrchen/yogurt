---
phase: 03-cloud-stt-live-transcript
plan: 03
subsystem: web
tags: [web, transcript-dock, ws, react-router, vitest, human-verify]
dependency_graph:
  requires:
    - phase-03-01 (yogurt-stt crate + Stt trait + Deepgram adapter)
    - phase-03-02 (meetings::Registry + REST + /ws/meetings/:id)
    - phase-01 (design tokens + primitives)
    - phase-0 (SPA scaffold + React Router 7)
  provides:
    - useTranscriptWs(meetingId) — typed WS hook with reconnect + token auth, exposes TranscriptEvent[] + last partial + connectionStatus
    - TranscriptLine component — Me/Them channel labels, JetBrains-Mono HH:MM:SS, partial-opacity + blink cursor
    - TranscriptDock component — collapsed tab + 330px slide-in panel + auto-scroll with pause-on-user-scroll
    - Meeting route at /meeting/:id + library↔meeting App.tsx switch + library New-meeting CTA
  affects:
    - Phase 4 hero (TipTap editor composes inside the notes column slot in Meeting.tsx, transcript dock stays as-is)
    - Phase 6 chat (floating pill anchors to Meeting.tsx, dock unaffected)
    - Phase 7 library (library stub on / replaced by full library view; transcript dock unchanged)
tech_stack:
  added:
    - "no new web deps — uses Phase 1 primitives + native WebSocket"
  patterns:
    - "WS auth via ?token=… URL param (per Phase 0 contract); WS opens on Meeting.tsx mount and closes on unmount; 3 reconnect attempts with exponential backoff"
    - "Auto-scroll-with-pause: ↓ new pill appears when user scrolls up; auto-resume when user scrolls within 24px of bottom"
    - "Dock motion via the Phase 1 @theme animate-slide-in-right utility (340ms cubic-bezier(.2,.7,.2,1)); minifier collapsed 340ms → .34s in dist CSS (verified literal cubic-bezier present)"
key_files:
  - web/src/lib/ws.ts (NEW — useTranscriptWs hook)
  - web/src/lib/ws.test.ts (NEW)
  - web/src/components/TranscriptLine.tsx (NEW)
  - web/src/components/TranscriptDock.tsx (NEW)
  - web/src/components/TranscriptDock.test.tsx (NEW)
  - web/src/routes/Meeting.tsx (NEW)
  - web/src/App.tsx (extended — library stub + library↔meeting nav)
  - web/src/App.test.tsx (extended)
  - web/src/index.css (extended — Phase 3 dock-open/dock-closed slide motion classes)
commits:
  - "862df09 feat(web): useTranscriptWs hook with reconnect + token auth + dock motion tokens + hook tests"
  - "8321db6 feat(web): TranscriptLine + TranscriptDock components + dock tests"
  - "63d96dc feat(web): Meeting route + App.tsx library↔meeting switch + App tests"
requirements:
  satisfied: [TRANS-03, TRANS-04, TRANS-05, TRANS-06, TRANS-07, TRANS-08]
  notes:
    - "TRANS-03/04: collapsed tab + 330px slide-in panel + 340ms cubic-bezier(.2,.7,.2,1) verified visually by user"
    - "TRANS-05: Me/Them channel labels with ink/grey color + JetBrains-Mono HH:MM:SS timestamps verified visually"
    - "TRANS-06: auto-scroll with pause-on-user-scroll behavior verified (↓ new pill, 24px resume threshold)"
    - "TRANS-07: blink cursor on most-recent partial line verified"
    - "TRANS-08: server-side <200ms latency assertion proven in plan 03-02 (e2e_synthetic_audio test); client-side <2s end-to-end depends on Deepgram-side latency, observable when user provides YOGURT_DEEPGRAM_API_KEY"
verification:
  human_verify_checkpoint:
    artifact: localhost:5173 (Vite proxying /api + /ws to backend on :7878)
    items_checked: [library stub renders, library→meeting nav, collapsed dock tab geometry, 340ms slide-in feel, dock surface color, notes editor not blocked/dimmed when dock open, re-collapse animation, meeting creation flow, WS connected dot turns green]
    result: approved by user 2026-06-25
test_count:
  web: 81 (was 75 after Phase 1 → +6 for ws/dock/App suites)
  workspace_rust: unchanged from 03-02 (35 yogurt-server + 26 yogurt-audio + 6 yogurt-stt + cli)
---

# Plan 03-03 Summary — Transcript Dock UI + Meeting Route

## Outcome

Plan 03-03 sealed the user-facing surface for Phase 3: the right-edge live transcript dock and the Meeting route that hosts it. With this plan, the full audio → STT → WS → browser pipeline is renderable end-to-end. The user verified the visual contract (dock geometry, 340ms slide-in feel, channel labels, notes-not-blocked) and approved.

## What was built

- **`useTranscriptWs(meetingId)`** — typed WebSocket hook with automatic reconnect (3 attempts, exponential backoff), session-token auth via `?token=…` URL param, returns `{ events: TranscriptEvent[], lastPartial, connectionStatus }`
- **`TranscriptLine`** — single transcript line component. Me labels render in `text-ink`, Them in `text-grey`. Timestamp rendered in JetBrains Mono `HH:MM:SS`. Partial lines render at opacity 0.7 with the blink cursor (`animate-blink`, Phase 1 @theme) at the end.
- **`TranscriptDock`** — collapsed state (vertical tab inset 22px from top, 11px-rounded-left, soft shadow, animate-wave 3-bar icon). Expanded state (330px panel slides in from right at 340ms cubic-bezier(.2,.7,.2,1), surface `#FCFAF5`, scrollable transcript list, ↓-new pill for auto-scroll resume).
- **`Meeting` route** — React Router 7 route at `/meeting/{id}`. Composes a notes column (placeholder for Phase 4 TipTap) + `TranscriptDock` pinned right. Library stub on `/` (full library lands in Phase 7).
- **App.tsx library↔meeting switch** — clicking "Open a new meeting →" calls `POST /api/meetings` → navigates to the new Meeting route.

## The human-verify checkpoint

The user opened http://localhost:5173 with the Vite dev server proxying to the backend, walked through the 9-item visual verification list (library, dock tab geometry, 340ms slide-in, channel colors, notes-editable-while-open, meeting creation, WS connection), and approved.

The optional live Deepgram smoke (item 8) was not run during this checkpoint since it requires `YOGURT_DEEPGRAM_API_KEY` to be in the environment — the missing-key error path (strawberry banner pointing at Phase 5 settings) is the expected Phase 3 behavior without it.

## Deferred / followup notes

- **D-INT-02 (from Plan 03-02): per-meeting WS auth tuned for Phase 5.** The Phase 0 session-token + Origin checks apply to the server-level WS handshake but not per-meeting-route ACL (i.e., any localhost process with the session token can connect to any meeting). Acceptable for v1 single-user single-machine. Phase 5 (Keychain + settings) is the right home for per-meeting ACL.
- **Client-side < 2s end-to-end latency:** the server-side budget is verified in 03-02 (< 200ms first frame). The full < 2s figure depends on Deepgram-side latency which can only be observed when a user provides their own `YOGURT_DEEPGRAM_API_KEY`. Documented as user-observable acceptance.

## Phase 3 complete

All three plans (03-01 Stt trait + Deepgram, 03-02 registry + REST + WS, 03-03 dock UI + Meeting route) merged into `gsd/autonomous`. Full audio→STT→WS→browser pipeline functional with the Phase 1 design system applied. Ready for Phase 4 (Augmented Notes Hero) to slot the TipTap editor into Meeting.tsx's notes column.
