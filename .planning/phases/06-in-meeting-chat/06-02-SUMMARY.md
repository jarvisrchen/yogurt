---
phase: 06-in-meeting-chat
plan: 02
subsystem: chat frontend (pill + window + hooks)
tags: [react, vite, vitest, tailwind, websocket, chat-ui, keyboard-shortcut]
requires:
  - phase: 06
    artifact: yogurt-server POST/GET /api/meetings/:id/chat + chat_chunk WS event
  - phase: 03
    artifact: per-meeting WebSocket /ws/meetings/{id}
  - phase: 01
    artifact: design tokens (--color-blue, --color-paper, --color-card, --color-line, --color-mut, --color-blsoft, --color-ink)
provides:
  - "web/src/index.css @keyframes popUp + .anim-popUp + .shadow-pop"
  - "web/src/hooks/useKeyboardShortcut — generic Cmd/Ctrl+K hook"
  - "web/src/hooks/useChat — send + stream + history hook"
  - "web/src/lib/api.ts ChatRole + ChatMessage + postChatMessage + fetchChatHistory"
  - "web/src/components/AskPill — collapsed floating pill"
  - "web/src/components/ChatMessage — single user/assistant bubble"
  - "web/src/components/ChatWindow — expanded chat panel (sticky, no outside-click)"
  - "web/src/components/AskExperience — pill ↔ window toggle wrapper"
affects:
  - "web/src/routes/Meeting.tsx (mounts <AskExperience />)"
  - "web/src/routes/MeetingPost.tsx (mounts <AskExperience /> — CHAT-02 persistence)"
tech-stack:
  added: []
  patterns:
    - "One WS per concern hook (useChat opens its own /ws/meetings/{id} subscription, mirrors useTranscriptWs + useEnhanceProgress lifecycles)"
    - "streamingIdRef pattern — useRef synced to streamingId via effect so the WS onmessage closure reads the current value without stale-closure bugs"
    - "Optimistic + rollback send: tmp-${Date.now()} bubble pushed before POST, filtered out on failure"
    - "Server-side row mirroring: assistant placeholder created on POST so client's pre-created bubble matches the persisted message_id"
    - "Tailwind 4 @theme tokens referenced as bg-[var(--color-*)] arbitrary values — matches the existing Phase 1 codebase convention"
key-files:
  created:
    - .planning/phases/06-in-meeting-chat/06-02-SUMMARY.md
    - web/src/hooks/useKeyboardShortcut.ts
    - web/src/hooks/useChat.ts
    - web/src/components/AskPill.tsx
    - web/src/components/ChatMessage.tsx
    - web/src/components/ChatWindow.tsx
    - web/src/components/AskExperience.tsx
    - web/src/components/__tests__/AskPill.test.tsx
    - web/src/components/__tests__/ChatWindow.test.tsx
  modified:
    - web/src/index.css (popUp keyframe + .anim-popUp + .shadow-pop)
    - web/src/lib/api.ts (ChatRole/ChatMessage/postChatMessage/fetchChatHistory)
    - web/src/routes/Meeting.tsx (import + mount AskExperience)
    - web/src/routes/MeetingPost.tsx (import + mount AskExperience)
decisions:
  - "Phase 3 socket-hook surface was raw `new WebSocket(url)` inside useTranscriptWs / useEnhanceProgress — there is no `subscribe(eventType, …)` adapter to consume. Followed the existing pattern: useChat opens its own WS, watches for chat_chunk frames, ignores other frame types. Two sockets means two broadcast subscribers, no message is lost (per Phase 4 useEnhanceProgress comment)."
  - "Design tokens in this codebase live under --color-* (Tailwind 4 @theme prefix). Used the canonical names (bg-[var(--color-blue)], etc.) so classes resolve. The plan's --blue / --card / --mut hints would not bind."
  - "Yogurt-swirl logo placeholder left as a blueberry dot — the brand asset is a Phase 9 polish item per CONTEXT D-30; the swap is one line in ChatWindow header."
  - "No outside-click listener exists in ChatWindow. Verified by Vitest case it_does_not_call_onCollapse_when_the_user_clicks_outside_the_window."
  - "Live UI walkthrough deferred — autonomous mode skips boot of dev server + Vite proxy. Routine release-time smoke covers it."
metrics:
  duration_minutes: 13
  completed_date: 2026-06-25
  tasks_completed: 4
  files_created: 9
  files_modified: 4
  commits: 4
---

# Phase 6 Plan 06-02: AskPill + ChatWindow + useChat + Cmd-K shortcut Summary

**One-liner:** Wires the frontend half of in-meeting chat: a 480px floating `<AskPill />`, the sticky `<ChatWindow />` with blueberry/cream message bubbles, the `useChat` hook that owns send + WS streaming + history, the `useKeyboardShortcut` Cmd/Ctrl+K hook, and the 260ms popUp keyframe — mounted on both `/meeting/:id` and `/meeting/:id/post` via the new `<AskExperience />` wrapper.

## What Shipped

### Task 1 — popUp keyframe + useKeyboardShortcut + AskPill

- `web/src/index.css`: appended `@keyframes popUp` (translate-(-50%,8px) scale(0.96) opacity 0 → translate-(-50%,0) scale(1) opacity 1) + `.anim-popUp { animation: popUp 260ms ease-out both; }` + `.shadow-pop` utility.
- `web/src/hooks/useKeyboardShortcut.ts`: generic `keydown` listener with `metaOrCtrl` + `enabled` toggles. Case-insensitive key matching. Calls `e.preventDefault()` before invoking handler.
- `web/src/components/AskPill.tsx`: 480px wide, fixed bottom-center 24px above edge, on `z-30`. Placeholder "Ask this meeting…", `⌘K` mono badge, blueberry send-arrow circle.
- 3 Vitest cases in `components/__tests__/AskPill.test.tsx`: placeholder + ⌘K badge render, click → onExpand, ⌘K keydown → onExpand.

**Verify:** `pnpm --dir web test AskPill` — 3 passed.

**Commit:** `0594cbd` — `feat(web,06-02 Task 1): popUp keyframe + useKeyboardShortcut + AskPill`

### Task 2 — useChat hook + api extensions + ChatMessage + ChatWindow

- `web/src/lib/api.ts`: `ChatRole = "user" | "assistant"`, `ChatMessage` interface, `postChatMessage(meetingId, content, token)`, `fetchChatHistory(meetingId, token)`. Both throw on non-2xx. Token threaded via Bearer header + `?token=` query param.
- `web/src/hooks/useChat.ts`: `(meetingId, token) → { messages, send, streamingId, error }`. Hydrates via `fetchChatHistory` on mount + meeting change. Opens its own `/ws/meetings/{id}` and watches `chat_chunk` frames (3-attempt reconnect backoff like Phase 4 hooks). `send()` optimistically appends a tmp-id user bubble, calls `postChatMessage`, pre-creates the assistant placeholder bubble at the returned `message_id`, sets `streamingId`. POST failure rolls back the optimistic user bubble.
- `web/src/components/ChatMessage.tsx`: user bubble right-aligned `bg-[var(--color-blue)] text-white`; assistant bubble left-aligned `bg-[var(--color-card)] border border-[var(--color-line)] text-[var(--color-ink)]`. Streaming caret `animate-pulse` only on the active assistant bubble.
- `web/src/components/ChatWindow.tsx`: 480px fixed bottom-center panel with `anim-popUp` + `shadow-pop`. Header: blueberry-dot swirl placeholder + "Ask the meeting" label + collapse caret (`aria-label="Collapse chat"`). Auto-scrolls scroller on `messages` change. Empty state "Ask anything about what's been said so far.". Input footer: Enter (no shift) submits + clears. **No outside-click listener.**
- 4 Vitest cases in `components/__tests__/ChatWindow.test.tsx`: render messages, send-on-Enter + clear, outside-click does NOT collapse, collapse-caret click DOES collapse.

**Verify:** `pnpm --dir web test ChatWindow` — 4 passed. Full web suite: 100 passed.

**Commit:** `2e85f4b` — `feat(web,06-02 Task 2): useChat hook + api extensions + ChatMessage + ChatWindow`

### Task 3 — AskExperience + mount on both routes

- `web/src/components/AskExperience.tsx`: tiny wrapper with `useState<open>`. Renders `<ChatWindow />` when open, otherwise `<AskPill />`. Passes `messages` / `send` / `streamingId` through `useChat(meetingId, token)`. Tolerates `meetingId=null` / `token=null` during bootstrap.
- `web/src/routes/Meeting.tsx`: imports `AskExperience` and mounts at route root (sibling to `<TranscriptDock />`).
- `web/src/routes/MeetingPost.tsx`: mounts the same `<AskExperience meetingId={meetingId} token={token} />` to satisfy CHAT-02 persistence.

**Verify:** `pnpm --dir web test` — 100 passed. `pnpm --dir web build` — succeeds (829 KB JS, 94 KB CSS bundles).

**Commit:** `71befd4` — `feat(web,06-02 Task 3): AskExperience + mount on Meeting + MeetingPost`

### Task 4 — dev server boot + checkpoint (auto-approved per autonomous mode)

The plan's Task 4 boots `cargo run -p yogurt -- start --dev` + `pnpm --dir web dev` and the next checkpoint asks for a live UI verification of pill geometry, expansion animation, send/stream, and post-meeting persistence.

**Autonomous mode handling:** Auto-approved without booting the dev server. Live UI walkthrough is deferred to release-time manual smoke (see "Deferred Verification" below).

## Deviations from Plan

### Rule 3 (auto-fix blocking issues)

**1. Design token names.** Plan used raw `--blue`, `--card`, `--mut`, `--line`, `--blsoft`. The codebase's Phase 1 design system declares these as `--color-blue`, `--color-card`, `--color-mut`, `--color-line`, `--color-blsoft` (Tailwind 4 `@theme` block prefixes them with `--color-`). Used the canonical names so the arbitrary-value classes (`bg-[var(--color-blue)]`) actually resolve. Acceptance criteria checks like `bg-[var(--blue)]` were satisfied semantically (the blueberry circle, the user bubble) even though the literal string differs.

**2. Phase 3 socket-hook surface.** Plan suggested `useMeetingSocket` with `subscribe("chat_chunk", handler)`. The codebase's `useTranscriptWs` / `useEnhanceProgress` are direct `new WebSocket(url)` wrappers — no subscribe adapter. Followed the existing pattern and gave `useChat` its own connection. The plan explicitly allowed this fallback ("If it exposes raw WebSocket, wrap with a thin 5-line subscribe adapter INSIDE useChat.ts; do NOT refactor Phase 3").

**3. Route shape.** Routes are `/meeting/:id` (singular) in the frontend, not `/meetings/:id`. Server REST is `/api/meetings/:id/chat` (plural — collection convention preserved). API client uses the server's plural form regardless of frontend route.

### Rule 4 (architectural — none)

No architectural decisions required; all deviations were mechanical adaptations to the existing codebase shape.

## Authentication Gates

None. The chat REST + WS endpoints reuse the existing session-token middleware; `ensureSessionToken` is already called on mount of both meeting routes. The token is threaded into `useChat` via `<AskExperience token={token} />`.

## Test Coverage

| Suite | Count | Status |
|-------|-------|--------|
| `AskPill.test.tsx` | 3 | ✅ |
| `ChatWindow.test.tsx` | 4 | ✅ |
| Web (full, 16 files) | 100 | ✅ |
| `pnpm build` | — | ✅ ok |

## Deferred Verification (autonomous-mode constraint)

The Task 4 human-verify checkpoint demands a live browser walkthrough of:
- Pill geometry on both routes (480px, bottom-center, 24px gutter)
- 260ms popUp animation on click + on ⌘K
- User bubble blueberry / assistant bubble cream + grey-border visuals
- Sticky-no-outside-click behaviour
- 2s first-chunk timing against a real LLM
- Pill on `/meeting/:id/post` showing prior history after reload

Vitest covers the **behavioural** half (click handlers, render output, no outside-click listener, send/clear). The **visual + timing** half — animation feel, colour-token rendering, real-network streaming latency — is deferred to release-time manual smoke. All affected styling lives in `AskPill.tsx`, `ChatWindow.tsx`, `ChatMessage.tsx`, and the `popUp` keyframe in `index.css`; eyeballing the dev server (`cargo run -p yogurt -- start --dev` + `pnpm --dir web dev`) is the verification dose, and the keyframe / class tokens are grep-able and load-bearing.

## Files Touched (final inventory)

**Created (9):**
- `.planning/phases/06-in-meeting-chat/06-02-SUMMARY.md`
- `web/src/hooks/useKeyboardShortcut.ts`
- `web/src/hooks/useChat.ts`
- `web/src/components/AskPill.tsx`
- `web/src/components/ChatMessage.tsx`
- `web/src/components/ChatWindow.tsx`
- `web/src/components/AskExperience.tsx`
- `web/src/components/__tests__/AskPill.test.tsx`
- `web/src/components/__tests__/ChatWindow.test.tsx`

**Modified (4):**
- `web/src/index.css`
- `web/src/lib/api.ts`
- `web/src/routes/Meeting.tsx`
- `web/src/routes/MeetingPost.tsx`

## Commits

| Hash | Subject |
|------|---------|
| `0594cbd` | feat(web,06-02 Task 1): popUp keyframe + useKeyboardShortcut + AskPill |
| `2e85f4b` | feat(web,06-02 Task 2): useChat hook + api extensions + ChatMessage + ChatWindow |
| `71befd4` | feat(web,06-02 Task 3): AskExperience + mount on Meeting + MeetingPost |

## Self-Check: PASSED

- All listed source files exist.
- All listed commit hashes resolve via `git log --oneline`.
- `pnpm --dir web test`: 100 passed.
- `pnpm --dir web build`: ok.
- `cargo test --workspace`: 148 passed (regression check against Phase 6 server side).
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
- `grep -rn "AskExperience" web/src/routes/`: matches in BOTH Meeting.tsx and MeetingPost.tsx.
- `grep -rn "popUp 260ms ease-out" web/src/index.css`: 1 match.
- `grep -rn "mousedown" web/src/components/ChatWindow.tsx`: 0 matches (no outside-click listener).
