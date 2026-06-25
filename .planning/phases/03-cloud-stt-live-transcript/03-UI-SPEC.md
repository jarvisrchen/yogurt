---
phase: 3
slug: cloud-stt-live-transcript
status: draft
shadcn_initialized: false
preset: none
created: 2026-06-25
---

# Phase 3 — UI Design Contract

> Visual and interaction contract for the right-edge Live Transcript dock — the first significant in-meeting UI surface. Streams WebSocket transcript events into a collapsible side panel while leaving the notes column fully editable.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | none (custom dock component, hand-written React + Tailwind) |
| Preset | not applicable |
| Component library | none (no shadcn/Radix in this phase — custom slide-in panel + tab) |
| Icon library | Lucide-React (Phase 1 baseline) — chevron glyph (`›`/`‹`) for tab toggle; 3-bar wave is hand-rolled `<span>` rectangles, not an icon |
| Font | Inherits Phase 1 stack: Hanken Grotesk (UI body/labels), JetBrains Mono (timestamps + technical captions). No new fonts. |

**Inherits from `.planning/phases/01-design-system/01-UI-SPEC.md`. No new tokens.** Per Phase 3 D-14/D-16, hex values are inlined at the component layer here; Phase 1's CSS-variable refactor is a mechanical sed-and-replace later.

---

## Spacing Scale

Inherits Phase 1's 4-base scale verbatim (declared on the Design Board at 4 / 8 / 12 / 16 / 24 / 32 / 48):

| Token | Value | Usage |
|-------|-------|-------|
| xs | 4px | Icon gap inside tab, transcript-line label-to-text gap (`gap:8px` close-rounded) |
| sm | 8px | Channel-label / timestamp inline gap, transcript-line internal `margin-bottom:2px` between label row and body |
| md | 16px | Dock header bottom margin (`margin-bottom:16px`) |
| lg | 24px | (not used in this phase) |
| xl | 32px | (not used in this phase) |
| 2xl | 48px | (not used in this phase) |
| 3xl | 64px | (not used in this phase) |

**Phase-specific spacing values pulled from Design Board mockup** (these are exact pixel readings from the transcript-dock element, not new tokens):

| Surface | Value | Source |
|---------|-------|--------|
| Transcript dock panel width | **330px** | PRD §5.2 + §16.8 + Design Board line 343 (`width:330px`). Load-bearing exact pixel — not on the 4px scale. |
| Tab horizontal padding | 16px | Design Board line 300 (`padding:10px 16px`) |
| Tab vertical padding | 10px | Design Board line 300 |
| Tab inset from notes top | 22px | Design Board line 300 (`top:22px`) |
| Tab border-radius (left side only) | 11px / 0 / 0 / 11px | Design Board line 300 (`border-radius:11px 0 0 11px`) |
| Tab inner element gap | 10px | Design Board line 300 (`gap:10px`) — between wave glyph, label, chevron |
| Dock panel padding | 22px 20px | Design Board line 343 (`padding:22px 20px`) |
| Dock header margin-bottom | 16px | Design Board line 344 |
| Transcript-line vertical gap | 13px | Design Board line 348 (`gap:13px` inside scroll container) |
| Transcript-line label-row → body gap | 2px | Design Board line 349 (`margin-bottom:2px`) |
| Notes column right gutter (reserves tab) | 28px (`pr-7` Tailwind) | Phase 3 D-15 — keeps notes from sliding under the closed tab |
| Wave-bar width | 2.5px (panel) · 3px (tab) | Design Board lines 301 + 345 — bars in the closed tab are slightly chunkier than the inline header wave |
| Wave-bar height | 12px (panel) · 13px (tab) | Same |
| Wave-bar gap | 2px | Both |

Tab z-index: `z-30`. Panel z-index: `z-20` with `position:fixed` so layout doesn't reflow when the dock opens (Phase 3 D-15).

---

## Typography

Inherits Phase 1 type system. Roles **used by Phase 3**:

| Role | Family | Size | Weight | Line Height | Usage in this phase |
|------|--------|------|--------|-------------|---------------------|
| Transcript body | Hanken Grotesk | 13.5px | 400 | 1.45 | The actual spoken text of each line. `#211D18` for "Me" finals; `#8A8174` for "Them" finals (Design Board chose this slightly-warmer-than-pure-grey body text for "Them"; tokens table treats it as the body-on-grey reading of `--grey`). |
| Channel label | Hanken Grotesk | 11px | 700 | 1.0 | "Me" / "Them" tag on each line. Colour swaps by channel (see Color). |
| Timestamp | JetBrains Mono | 10.5px | 400 | 1.0 | `HH:MM:SS` from meeting start, `#A89F90`. **Always JetBrains Mono — never the UI sans-serif.** |
| Dock header eyebrow | JetBrains Mono | 11px | 400 | 1.0 (`letter-spacing:.1em`) | "LIVE TRANSCRIPT" caps label at the top of the open panel. |
| Tab label | Hanken Grotesk | 13px | 600 | 1.0 | "Live transcript" text on the closed right-edge tab. |
| Tab chevron | system glyph | 14px | — | 0 | `‹` (open intent) on the closed tab; `›` (close intent) inside the open panel header. |
| Empty-state heading | Hanken Grotesk | 14.5px | 700 | 1.45 | Pre-first-event placeholder. |
| Empty-state body | Hanken Grotesk | 13px | 400 | 1.45 | Pre-first-event subcopy. |
| Error toast | Hanken Grotesk | 13px | 500 | 1.45 | Reconnect / unavailable banner inside dock panel. |

**Timestamp format contract:** `HH:MM:SS`, two-digit zero-padded, derived from `(event.ts_ms − meeting.started_at_ms) / 1000`. **Never wall-clock.** Computed client-side from `TranscriptEvent.ts_ms` and the meeting's `created_at_ms` (mock e.g. `00:11:55`).

---

## Color

Inherits Phase 1 palette. Phase 3 references tokens **inline as hex** at the component layer (D-14 / D-16) — Phase 1's CSS-variable layer is a later refactor pass.

| Role | Value | Usage in this phase (explicit list — nothing else) |
|------|-------|---------------------------------------------------|
| Paper | `#FCFAF5` | Dock panel surface background (slightly warmer than app `#FBF7EF` paper — Design Board picked this so the dock reads as a sibling surface to the notes column, not the app shell). |
| White | `#FFFFFF` | Closed tab background. |
| Ink | `#211D18` | "Me" channel label text · "Me" final transcript body text · Tab label "Live transcript" |
| Grey | `#A89F90` | "Them" channel label text · timestamp text · dock header eyebrow "LIVE TRANSCRIPT" · tab chevron · cursor-blink glyph on most-recent partial |
| Body-grey (reading of grey) | `#8A8174` | "Them" final transcript body text (Design Board reads slightly darker than label-grey for legibility) |
| Line | `#EBE3D5` | Dock panel left-border (1px) · tab border (1px, no right edge: `border-right:none`) |
| Blueberry | `#5B4FC7` | **Wave-glyph bars 1 + 2** on both the closed tab and the open-panel header. Reserved accent: pulse animation only. |
| Blueberry-soft | `#C9B8F0` | **Wave-glyph bar 3** (the trailing bar reads softer — Design Board lines 301 + 345). |
| Tab shadow | `rgba(40,30,15,.08)` | `box-shadow:-8px 8px 22px rgba(40,30,15,.08)` on the closed tab — anchors it visually to the right edge. |
| (Partial-event opacity) | `opacity: 0.7` | Non-final transcript events render at 70% opacity; finals render at 100% (Phase 3 D-18). No new hex — this is opacity-on-existing-text-color. |

**Accent reserved for:** the 3-bar wave glyph (closed tab + open header) and nothing else in this phase. No buttons, no links, no hover states use blueberry in the dock — the only "interactive" surfaces are the tab toggle and the in-panel close chevron, both rendered in grey (`#A89F90`) per the Design Board.

**Destructive:** none in this phase. No delete, no destructive confirm. Phase 7 may add a "clear transcript history" action; not here.

**Cursor blink contract** (D-18 / Design Board line 352): the most-recent non-final line for each channel ends with a `7px × 13px` `#A89F90` block animated `blink 1s step-end infinite`. This is the "still listening" signal — it disappears the moment that segment is replaced with its final-confirmed text.

---

## Copywriting Contract

| Element | Copy |
|---------|------|
| Primary CTA | (none — transcript is read-only) |
| Tab label (closed state) | `Live transcript` |
| Dock header eyebrow (open state) | `LIVE TRANSCRIPT` (uppercase, JetBrains Mono, letter-spacing `.1em`) |
| Channel label — mic | `Me` |
| Channel label — system audio | `Them` |
| Empty state heading | `Waiting for audio…` |
| Empty state body | `Speak into your mic or play system audio. Lines appear within 2 seconds.` |
| Error state — reconnecting | `Transcript dropped connection. Reconnecting…` |
| Error state — unrecoverable | `Transcript unavailable — check Settings → Transcription` |
| Destructive confirmation | (none in this phase) |

**Copy invariants:**

- The strings "Me" and "Them" are **textual, not just colour-coded** — both must always render. PRD §5.2 and Phase 3 specifics explicitly forbid dropping the labels in favour of colour alone (Granola itself shows both label and colour because diarization is a v1 anti-goal; channel-source labels are how we tell two voices apart).
- The dock header text is uppercase **`LIVE TRANSCRIPT`** (with letter-spacing), but the closed-tab text is title-case **`Live transcript`**. Design Board distinguishes these intentionally — the closed tab is a button-shaped call-to-open; the open-panel header is a section label.
- The "Waiting for audio…" copy uses a Unicode horizontal ellipsis `…` (U+2026), not three dots.
- The reconnecting message ends with the same ellipsis; the unrecoverable message uses an em-dash `—` (U+2014) followed by the Settings deep-link hint as plain text (no actual link in Phase 3 — Settings UI ships Phase 5; this copy just tells the user where to go).

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | (none) | not required |
| Third-party | (none) | not required |

Phase 3 introduces no shadcn registry blocks and no third-party UI primitives. The dock is a hand-written React component (`TranscriptDock.tsx`) consisting of:

- A `position: fixed` right-edge tab (closed state).
- A `position: fixed` 330px-wide panel (open state) animated in via a hand-defined `@keyframes slideInRight` block in `web/src/index.css`.
- A child `TranscriptLine.tsx` component per event.
- A `useTranscriptWs` hook (`web/src/hooks/ws.ts`) backing the data flow.

All visual primitives (the chevron, the wave bars, the cursor-blink) are inline SVG or styled `<span>` — no registry surface to gate.

---

## Motion

Phase 3 introduces one motion contract and reuses two from Phase 1:

| Animation | Duration | Easing | Surface | Notes |
|-----------|----------|--------|---------|-------|
| `slideInRight` | **`340ms`** | **`cubic-bezier(.2, .7, .2, 1)`** | Dock panel opens from right edge | **LOAD-BEARING.** PRD §16.5 specs these exact values. The compiled CSS at `web/dist/assets/*.css` **MUST** literally contain both `340ms` and `cubic-bezier(.2, .7, .2, 1)` — Task 3.10 step 4 of the superpowers plan greps for them as an acceptance check. Triggered by `.dock-open` class added to the panel root on mount. Defined inline in `web/src/index.css` for Phase 3 (Phase 1's token layer moves it to a `@theme` block later). |
| `wave` | 1.0s | `ease-in-out` (staggered 0ms / 0.2s / 0.4s) | 3-bar wave glyph in tab + open-panel header | Reused from Phase 1 motion tokens (PRD §16.5). The two leading bars (`#5B4FC7`) and trailing bar (`#C9B8F0`) animate `transform-origin: bottom` scale to create the equalizer pulse. |
| `blink` | 1.0s | `step-end` | Cursor block on most-recent partial transcript line | Reused from Phase 1. Solid `7px × 13px` `#A89F90` rectangle appended inline at end of the partial body text. Communicates "still listening." |

**No closing animation** — the panel disappears via `display: none` toggle on the `open` state flip. (Phase 1 can add a symmetrical `slideOutRight` later if it feels abrupt; Phase 3 ships the slide-in only because the superpowers plan only specs the slide-in animation.)

**`prefers-reduced-motion` posture:** not implemented in Phase 3 (no PRD/plan requirement). Reduced-motion handling lands as a Phase 1 follow-up across the design-system layer.

---

## Layout & Interaction Contract

These are not standard UI-SPEC sections but are load-bearing for Phase 3 and worth pinning explicitly here so the checker can verify them.

### Layout rules
- Notes column wrapper applies `pr-7` (28px) to reserve the closed-tab gutter. When the dock opens, the **notes column does NOT reflow** — the 330px panel is `position: fixed; right: 0;` and overlays on top via `z-30` (D-15).
- Notes column **is NOT dimmed** when the dock is open. No backdrop, no opacity drop. The notes editor remains fully focusable and editable throughout.
- Max-width of the notes content stays at `660px` (matches Design Board line 307) regardless of dock state.

### Auto-scroll behaviour (D-17)
- Transcript list auto-scrolls to bottom on every new event by default.
- A `stickyRef` ref starts `true`; an `onScroll` handler flips it to `false` when the user scrolls up beyond a 24px threshold from the bottom (`scrollHeight - scrollTop - clientHeight >= 24`).
- While `stickyRef.current === false`, new events arrive without auto-scroll, preserving the user's read position.
- Scrolling back to within 24px of the bottom flips `stickyRef` back to `true` and auto-scroll resumes.

### Partial-vs-final rendering (D-18)
- `is_final: true` events render at `opacity: 1` and append to the list as a new locked line.
- `is_final: false` events render at `opacity: 0.7` and **replace** the previous partial **for that channel** in-place (the `mergeEvent` reducer in `useTranscriptWs` handles the upsert by `(channel, partial-key)`).
- Only the latest partial per channel may show the cursor-blink glyph at a time.

### Tab ↔ panel state
- Default state: tab visible, panel hidden.
- Click tab → tab hides, panel slides in (`slideInRight`).
- Click chevron inside panel header → panel hides, tab re-appears (no animation).
- State held in a parent `useState<boolean>` on `Meeting.tsx`; no global store needed in this phase.

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting: PASS
- [ ] Dimension 2 Visuals: PASS
- [ ] Dimension 3 Color: PASS
- [ ] Dimension 4 Typography: PASS
- [ ] Dimension 5 Spacing: PASS
- [ ] Dimension 6 Registry Safety: PASS

**Approval:** pending
