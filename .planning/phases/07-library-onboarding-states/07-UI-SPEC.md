---
phase: 7
slug: library-onboarding-states
status: draft
shadcn_initialized: false
preset: none
created: 2026-06-25
---

# Phase 7 — UI Design Contract

> Visual and interaction contract for the Library home, the `/welcome` onboarding flow, and the empty/permission-denied/model-download state surfaces. This is the largest UI phase — it owns most of the canonical product copy and the load-bearing 3.5s float animation. Source of truth: PRD §5.9 / §5.10 / §5.11 + §16, the design board (`yogurt-app-design/project/Yogurt Design Board.dc.html`), and Phase 1 tokens.

---

## Design System

Inherits from `.planning/phases/01-design-system/01-UI-SPEC.md`. No new tokens.

| Property | Value |
|----------|-------|
| Tool | none (Tailwind 4 `@theme` block from Phase 1) |
| Preset | not applicable |
| Component library | none — custom primitives only |
| Icon library | Lucide (`lucide-react`, PRD §16.9) — first phase to actually pull the dependency; Settings/Kebab icons (`Settings`, `MoreHorizontal`, `Search`, `Star`, `ChevronRight`, `Copy`, `FolderOpen`, `Trash2`, `Download`, `AlertTriangle`, `Check`, `Plus`) come from here |
| Font | Phase 1 stack: Instrument Serif + Hanken Grotesk + JetBrains Mono |

**Inherited primitives reused this phase:**

- `Logo` / `SwirlLogo` (sidebar header, Welcome left, EmptyLibrary center floating)
- `Button` (primary blueberry, secondary outlined)
- `Pill` (`tone="matcha"` for "Local-only · on", `tone="blue"` for active-nav backgrounds)
- `Card` (meeting cards, step cards, model download card)
- `BrowserChrome` (test fixtures only — not a runtime wrapper inside the app)

**New custom components introduced this phase (no registry):**

`Sidebar`, `MeetingCard`, `Avatar`, `DateGroup`, `Greeting`, `SearchPill`, `InlineTitle`, `MeetingCardActions` (kebab overflow), `StepCard`, `TerminalMockup`, `EmptyLibrary`, `PermissionDenied`, `ModelDownloadDialog`.

---

## Spacing Scale

Reference Phase 1 (`xs 4 · sm 8 · md 16 · lg 24 · xl 32 · 2xl 48 · 3xl 64`, plus the 12px intermediate). All paddings in this phase MUST use these stops.

**Phase-7-specific exceptions (extracted from design board, declared so deltas trip review):**

| Width / dimension | Value | Where |
|-------------------|-------|-------|
| Sidebar fixed width | **212px** | `Sidebar` — matches Phase 1 forward-reference and PRD §16.8 |
| Sidebar inner padding | 20px / 16px | top-bottom / left-right |
| Sidebar nav row padding | 8px / 10px | active + inactive rows |
| Main pane padding | 26px / 30px | top / sides |
| Avatar canvas | **42px × 42px** | `Avatar` — load-bearing, drives card row height |
| Avatar radius | 10px | `rounded-[10px]` (one off-token exception — between `--radius-button` 9 and `--radius-card` 14) |
| Avatar initials font-size | 18px | Instrument Serif inside Avatar |
| Meeting card padding | 16px / 18px | vertical / horizontal |
| Meeting card radius | 13px | one off-token exception (between button 9 and card 14, matches design board) |
| Meeting card gap | 16px | between avatar / title block / right badge cluster |
| Date group label margin | 24px top / 10px bottom | `DateGroup` separator rhythm |
| Date group label letter-spacing | 0.1em | uppercase mono |
| Search pill padding | 8px / 12px | rounded-[9px] |
| Welcome two-column grid | **`1.05fr 0.95fr`** | PRD §16.8 onboarding invariant |
| Welcome left column padding | 46px / 48px | per design board |
| Welcome right column padding | 42px / 44px | per design board |
| StepCard padding | 15px / 16px | radius 12px |
| StepCard gap (between cards) | 14px | `flex-col gap-14` |
| StepCard number/check badge | 26px circle | radius `999` |
| EmptyLibrary logo canvas | 56px (rendered) | (also 64px allowed per CONTEXT D-18; design board uses 56px — adopt 56px for parity) |
| EmptyLibrary headline top margin | 20px | below floating logo |
| PermissionDenied card max-width | 760px | full-screen warning surface inside chrome |
| PermissionDenied alert badge | 46px square, 12px radius | strawberry-soft fill |
| ModelDownloadDialog card width | **440px** | matches design board exactly |
| ModelDownloadDialog progress bar height | 8px | matcha fill on `#EFEAE0` track, radius 5px |
| ModelDownloadDialog icon badge | 38px square, 10px radius | matcha-soft fill |
| TerminalMockup padding | 16px / 18px | radius 12px on ink (`#211D18`) background |

---

## Typography

Reference Phase 1. Phase-7 surfaces resolve to these exact roles:

| Surface | Family | Size | Weight | Notes |
|---------|--------|------|--------|-------|
| Greeting headline ("Good afternoon, Dana") | Instrument Serif | 30px | 400 | per design board — between card-title (16-21) and hero (52); rendered above caption |
| Greeting caption ("14 meetings · all on this Mac") | Hanken Grotesk | 13.5px | 400 | `--color-mut` |
| Welcome hero ("Welcome to yogurt.") | Instrument Serif | 38px | 400 | line-height 1.05; design board uses 38px (NOT the 52px hero size) |
| Welcome subhead one-liner | Hanken Grotesk | 15px | 400 | line-height 1.55; `#6B6256` |
| EmptyLibrary headline ("No meetings yet") | Instrument Serif | 28px | 400 | margin-top 20px below floating logo |
| EmptyLibrary supporting body | Hanken Grotesk | 14.5px | 400 | max-width 340px |
| PermissionDenied headline | Instrument Serif | 32px | 400 | follows the 46px alert badge |
| PermissionDenied body paragraph | Hanken Grotesk | 15px | 400 | line-height 1.55, max-width 520px |
| PermissionDenied numbered step | Hanken Grotesk | 14px | 400 | ink text; `<b>` inline weight 700 |
| ModelDownloadDialog title ("Downloading small.en") | Hanken Grotesk | 16px | 700 | |
| ModelDownloadDialog mono caption ("whisper.cpp · 487 MB") | JetBrains Mono | 11.5px | 400 | `--color-mut` |
| ModelDownloadDialog body | Hanken Grotesk | 12.5px | 400 | `--color-mut`, line-height 1.5 |
| Meeting card title | Hanken Grotesk | 15.5px | 700 | `--color-ink` |
| Meeting card meta ("2:00 PM · 38 min · enhanced") | Hanken Grotesk | 13px | 400 | `--color-mut` |
| Avatar initials | Instrument Serif | 18px | 400 | tinted text per avatar palette |
| Sidebar nav row (inactive) | Hanken Grotesk | 13.5px | 400 | `#6B6256` |
| Sidebar nav row (active) | Hanken Grotesk | 13.5px | 600 | `--color-blue` |
| Sidebar "+ New meeting" button | Hanken Grotesk | 13.5px | 600 | matches Phase 1 primary button spec |
| "Local-only · on" pill | Hanken Grotesk | 12px | 600 | `#5d6b5f` (matcha-deep) |
| Sidebar Settings row | Hanken Grotesk | 13px | 400 | `--color-mut` |
| `⚙ Settings` glyph | Lucide `Settings` | 14px | n/a | inline beside text |
| FOLDERS section label | JetBrains Mono | 10.5px | 600 | uppercase, letter-spacing 0.1em, `--color-mut` |
| Folder row | Hanken Grotesk | 13px | 400 | `#6B6256`, count in JetBrains Mono 11px |
| Date group label (`TODAY` / `YESTERDAY` / `EARLIER`) | JetBrains Mono | 10.5px | 600 | uppercase, letter-spacing 0.1em, `--color-mut` |
| ONE-TIME SETUP label | JetBrains Mono | 11px | 600 | uppercase, letter-spacing 0.12em, `--color-grey` |
| StepCard title | Hanken Grotesk | 14.5px | 700 | `--color-ink` |
| StepCard body | Hanken Grotesk | 13px | 400 | `#5d6b5f` (done) / `#6B6256` (current/pending), line-height 1.45 |
| TerminalMockup content | JetBrains Mono | 13px | 400 | line-height 1.7; colors: prompt `--color-mut`, command `#EDEAE2`, success `#7FBE8C`, pending `#C9B8F0` |
| Welcome footer note ("Restart once after granting — a macOS quirk, not us.") | Hanken Grotesk | 12px | 400 | `--color-grey`, centered |
| Search pill placeholder | Hanken Grotesk | 13px | 400 | `--color-grey` until typed |
| File-path mono caption ("`~/.yogurt/notes/*.md`") | JetBrains Mono | 11.5px | 400 | `--color-grey` |
| `⌘N` kbd hint inside button | JetBrains Mono | 11px | 400 | `rgba(255,255,255,0.22)` chip on blueberry button |
| Local badge ("local") on meeting card | Hanken Grotesk | 11px | 600 | matcha text on matcha-soft, radius 6, padding 4/9 |
| Right meta-badge count (folder count, etc.) | JetBrains Mono | 11px | 400 | `--color-mut` |

---

## Color

Reference Phase 1 palette. **No new color tokens.** Phase-7 reservations of accent / destructive / matcha:

### Blueberry (`--color-blue` = `#5B4FC7`)

Reserved EXPLICITLY for:

1. **Active-nav row text** in `Sidebar` (with `--color-blsoft` `#ECE9FB` wash behind it)
2. **Primary CTAs** in this phase:
   - "+ New meeting" (sidebar primary button, with `shadow: 0 2px 8px rgba(91,79,199,.3)`)
   - "Start your first meeting" (EmptyLibrary)
   - "Take me to my meetings →" (Welcome onboarding bottom)
   - "Open System Settings" (PermissionDenied primary CTA)
3. **Current-step StepCard border** (1.5px solid) + 26px badge fill on `StepCard variant="current"`
4. **Provider chip active state** ("Minimax" on Step 2 — `#ECE9FB` bg + `--color-blue` text)
5. **Avatar tint #1** — blueberry-soft (`--color-blsoft` `#ECE9FB`) background + `--color-blue` Instrument-Serif initials (e.g., "WS")

### Lilac / soft blueberry (`--color-blsoft` = `#ECE9FB`)

- Active nav row background in `Sidebar` (only)
- Avatar tint #1 background
- Provider chip active background
- Phase 4 enhancing banner background (already in place — Phase 7 just verifies it still renders)

### Matcha (`--color-matcha` = `#5E9E73`)

Reserved EXPLICITLY for:

1. **"Local-only · on" pill** — `--color-mtsoft` `#E7F0E8` background, `#5d6b5f` text, 8px circle dot in `--color-matcha`, radius 8px (NOT pill 999 — design board uses radius 8 on this sidebar pill)
2. **"✓ granted" badge** on Welcome StepCard Step 1 (done state) — 26px circle in `--color-matcha`, white check, on `#E7F0E8` card background with `#CFE3D4` border
3. **ModelDownloadDialog down-arrow badge** + progress-bar fill
4. **`local` badge** on `MeetingCard` (matcha text on matcha-soft, radius 6)
5. **Avatar tint #3** — matcha-soft (`--color-mtsoft` `#E7F0E8`) background + `--color-matcha` initials (e.g., "D1")
6. **TerminalMockup success lines** (`✓ server live on :7878`, `✓ opening your browser…`) — `#7FBE8C` (matcha-light, terminal-only literal — design board specific)

### Strawberry (`--color-straw` = `#E07A66`)

Reserved EXPLICITLY for:

1. **PermissionDenied alert icon** — 46px badge in `--color-strsoft` `#FBE6E0` (a.k.a. `#FBEDE9` in design board variant — use the Phase-1 token `--color-strsoft`), Lucide `AlertTriangle` glyph in `--color-straw`
2. **Avatar tint #2** — strawberry-soft (`--color-strsoft`) background + `--color-straw` initials (e.g., "JR")

### Paper / cream / ink

- `--color-paper` (`#FBF7EF`) — main pane background (Library main column), sidebar background, Welcome LEFT column
- `--color-card` (`#FFFFFF`) — meeting cards, step cards, model download card, Welcome RIGHT column
- `--color-ink` (`#211D18`) — all body headings, ordered-step number badges (PermissionDenied), TerminalMockup background
- `--color-mut` / `--color-grey` — captions, helper text

### Per-meeting `Avatar` tint cycle (deterministic by `hash(ulid) % 3`)

The design board renders three concrete tint pairings. Phase 7 implements a deterministic cycle keyed off the ULID hash so the same meeting always gets the same tint.

| Index | Background | Initials color | Source token | Example from design board |
|-------|------------|----------------|--------------|---------------------------|
| 0 | `#ECE9FB` (`--color-blsoft`) | `#5B4FC7` (`--color-blue`) | blueberry | "WS" (Weekly sync) |
| 1 | `#FBEDE9` (`--color-strsoft`) | `#E07A66` (`--color-straw`) | strawberry | "JR" (Interview backend) |
| 2 | `#E7F0E8` (`--color-mtsoft`) | `#5E9E73` (`--color-matcha`) | matcha | "D1" (Design crit) |

Cycle is intentionally LIMITED to three to honor the brand palette (no off-token pastels). Lilac/cream variants reserved for the design-board fourth slot were considered and **rejected** — they overlap perceptually with blueberry-soft.

### Accent reserved for (master summary, must match every other accent table in the project)

`blueberry`: active-nav text, primary CTAs (+ New meeting, Start your first meeting, Take me to my meetings →, Open System Settings), current-step StepCard border, provider chip active, avatar tint #1, logo mark circle (inherited).

`strawberry`: PermissionDenied alert icon + badge, avatar tint #2, RecordingBadge (inherited from Phase 2).

`matcha`: Local-only pill, Welcome ✓ granted badge, ModelDownloadDialog arrow + progress, MeetingCard `local` badge, avatar tint #3.

---

## Copywriting Contract

This phase owns the largest share of canonical product copy. **All strings below are VERBATIM and load-bearing.** Any change requires a UI-SPEC amendment.

### Library (home view)

| Element | Copy |
|---------|------|
| Greeting (morning) | `Good morning, you` |
| Greeting (afternoon) | `Good afternoon, you` |
| Greeting (evening) | `Good evening, you` |
| Greeting name token | `you` (default; no `/api/me` in v1 — see CONTEXT D-03) |
| Greeting caption (plural) | `{N} meetings · all on this Mac` |
| Greeting caption (singular) | `1 meeting · all on this Mac` |
| Search pill placeholder | `Search notes & transcripts` |
| Search pill icon | Lucide `Search` (12-14px), `--color-grey` |
| Sidebar wordmark | `yogurt` (Instrument Serif, lowercase, `-0.01em` letter-spacing) |
| Sidebar primary CTA | `+ New meeting` |
| Sidebar nav row (active) | `All meetings` |
| Sidebar nav row (secondary) | `Starred` |
| Sidebar FOLDERS section label | `FOLDERS` (uppercase mono) |
| Sidebar hardcoded folders | `Work` (8) · `Hiring` (3) · `1:1s` (5) — with `title="Coming in v1.1"` tooltip per CONTEXT D-02 |
| Sidebar local-mode pill | `Local-only · on` |
| Sidebar settings row | `⚙ Settings` (Lucide `Settings` glyph + label) |
| Date group label — same calendar day | `TODAY` |
| Date group label — previous calendar day | `YESTERDAY` |
| Date group label — older | `EARLIER` |
| Meeting card meta (with enrich) | `{time} · {N} min · enhanced` |
| Meeting card meta (without enrich) | `{time} · {N} min` |
| Meeting card right badge | `local` (matcha pill, always present in v1 per CONTEXT D-06) |
| Inline-edit fallback title | `Untitled meeting` |
| Kebab action 1 | `Copy markdown` |
| Kebab action 2 | `Reveal in Finder` |
| Kebab action 3 | `Delete` |
| Delete confirmation | `Delete this meeting from the library? The markdown file in ~/.yogurt/notes/ stays put.` |

> **Note:** The CONTEXT preserves the canonical Phase-1 alternative destructive copy "Delete this meeting? This removes notes, transcript, and markdown export. This cannot be undone." That copy is the Phase-1 reservation. Phase 7's actually-shipped behavior keeps the markdown file in place (per CONTEXT D-10 + D-24), so the more accurate, more honest copy above replaces it inside the Library kebab flow. **Both strings are sanctioned; the Phase-7 string is the one that appears in product.**

### Onboarding (`/welcome`)

| Element | Copy |
|---------|------|
| Hero headline | `Welcome to yogurt.` (Instrument Serif, 38px, period included) |
| Hero subhead | `Two streams, one set of notes, zero bots in the call. Everything below happens on this Mac.` |
| Terminal line 1 (prompt + command) | `$ yogurt start` |
| Terminal line 2 (success) | `✓ server live on :7878` |
| Terminal line 3 (success) | `✓ opening your browser…` |
| Terminal line 4 (pending, blink cursor) | `→ waiting for screen-recording grant` |
| Right column label | `ONE-TIME SETUP` (uppercase mono, letter-spacing 0.12em) |
| Step 1 title | `Screen Recording` |
| Step 1 body (done) | `Granted. This is how yogurt hears the other side of the call — no meeting bot required.` |
| Step 2 title | `Connect your model` |
| Step 2 body | `Bring your own key — OpenAI-compatible. Nothing is built in.` |
| Step 2 provider chips | `Minimax` (active, blueberry) · `Ollama` · `OpenAI` · `OpenRouter` (inactive, paper-border) |
| Step 3 title | `Pick transcription` |
| Step 3 body | `Cloud Deepgram for speed, or fully-local whisper.cpp.` |
| Primary CTA | `Take me to my meetings →` (arrow is literal `→` U+2192, not Lucide) |
| Disabled CTA state | same label, blueberry @ 40% opacity, `cursor-not-allowed` |
| Footer note | `Restart once after granting — a macOS quirk, not us.` |

### State surfaces

| Element | Copy |
|---------|------|
| **STATE-01 EmptyLibrary** headline | `No meetings yet` |
| **STATE-01** body | `Start one and Yogurt listens to both sides of the call — no bot joins. Your notes and audio stay on this Mac.` |
| **STATE-01** primary CTA | `Start your first meeting` (followed by `⌘N` kbd chip) |
| **STATE-01** kbd hint | `⌘N` (literal U+2318 + N, JetBrains Mono, white-translucent chip inside blueberry button) |
| **STATE-01** file-path caption | `notes saved to ~/.yogurt/notes/*.md` |
| **STATE-02 PermissionDenied** headline | `Yogurt can't hear the call yet` |
| **STATE-02** body paragraph | `Screen Recording is off, so the other side of your meeting is silent. Turn it on — this is the permission that lets Yogurt capture system audio without a bot.` |
| **STATE-02** numbered step 1 | `System Settings → Privacy & Security → Screen Recording` (bold whole phrase) |
| **STATE-02** numbered step 2 | `Toggle Yogurt on` (bold "Yogurt") |
| **STATE-02** numbered step 3 | `Restart Yogurt once` + muted suffix `— a macOS requirement, not us` |
| **STATE-02** primary CTA | `Open System Settings` |
| **STATE-02** secondary CTA | `Restart Yogurt` |
| **STATE-04 ModelDownloadDialog** title | `Downloading small.en` |
| **STATE-04** mono caption | `whisper.cpp · 487 MB` (Phase 8 swaps `487` for live size; Phase 7 ships the literal stub) |
| **STATE-04** progress mono row (stub) | `0 / 487 MB` · ` ` (right slot empty until Phase 8 wires bytes/sec/ETA) |
| **STATE-04** body | `One-time download, stored in ~/.yogurt/models. Most users stay on cloud STT and never see this.` |
| **STATE-04** secondary action | `Cancel` |
| **STATE-04** tertiary action | `Run in background` |

### Motion timings (load-bearing — verified by snapshot test)

| Animation | Class | Duration | Easing | Notes |
|-----------|-------|----------|--------|-------|
| **EmptyLibrary logo float** | `.float-3500` (alias for Phase 1 `animate-float`) | **3.5s** | `ease-in-out infinite` | Class name encodes the duration as bait — Phase 7 plan calls for snapshot asserting computed CSS `animation: float 3.5s ease-in-out infinite` |
| Terminal "waiting" cursor | `animate-blink` | 1.0s | `step-end infinite` | Inherited Phase 1 |
| Sidebar nav row hover | none | n/a | n/a | No transition — instant state swap per design board |
| Search pill focus | none | n/a | n/a | Phase 9 may add focus ring; Phase 7 ships bare |

### Disabled-state mechanics

| Surface | Disabled visual | Enabled condition |
|---------|-----------------|-------------------|
| Welcome "Take me to my meetings →" | `opacity-40`, `cursor-not-allowed` | `granted && hasActiveProvider` (CONTEXT D-17) |
| Inline title submit on blank | resets to fallback `Untitled meeting`, no error UI | always commits |
| Search pill on empty results | shows date-grouped empty list with no headline change | Phase 7 deliberately no-ops "no results" UX (research-flagged for v1.1) |

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | none | not required (shadcn not initialized — `shadcn_initialized: false`) |
| Any third-party UI | none | not applicable |
| Lucide icons | `Settings`, `MoreHorizontal`, `Search`, `Star`, `Plus`, `Copy`, `FolderOpen`, `Trash2`, `Download`, `AlertTriangle`, `Check`, `ChevronRight` | icons are MIT — no shadcn diff needed, but bundle size MUST be tree-shaken (no `import * from "lucide-react"`) |

**Notes:**

- 13 custom components introduced (see Design System table) — all hand-written from PRD §§5.9–5.11 + design board lines 222-277 (Library), 167-220 (Welcome), 685-732 (EmptyLibrary + PermissionDenied), 629-637 (ModelDownloadDialog).
- The Welcome `TerminalMockup` lifts its color palette (`#211D18` bg, `#EDEAE2` command, `#7FBE8C` success, `#C9B8F0` pending, `#8A8174` prompt) from design board line 190 verbatim. These are terminal-specific literals, not promoted to tokens.
- The PermissionDenied step-list uses ink-filled 24px circles with white numerals — distinct from the Welcome StepCard 26px circles (which use blueberry/matcha tint). Both are intentional; do NOT consolidate.
- `Avatar` is the only new component with a deterministic-hash dependency. The hash function (`hash(ulid) % 3`) MUST be the same on the server (when generating preview thumbnails in Phase 8+) and the client. Phase 7 implements client-only; the hash impl lives in `web/src/lib/avatar-tint.ts` so a future Rust port can mirror it.
- The hardcoded folders in `Sidebar` are visual placeholders only — they MUST carry `title="Coming in v1.1"` HTML attribute (browser native tooltip) so it's immediately clear this is not interactive yet.

---

## Phase-7 Interaction Contracts (binding)

For checker reference — these are NOT decorative copy, they're testable interaction rules:

1. **First-run redirect (`useFirstRunRedirect`)** — when `!first_run_completed || !screenRecordingGranted || !hasActiveProvider`, `/` redirects to `/welcome`. After Welcome's CTA, `first_run_completed = true` is PATCHed and the user is `navigate("/")`-ed.
2. **`Local-only · on` pill visibility** — pill renders iff `settings.providers.every(p => p.kind !== "cloud" || !p.active)`. The moment any cloud provider goes active, the pill vanishes (no fade, instant — privacy contract: never lie about cloud state).
3. **Permission denied "Open System Settings" link** — MUST link to EXACTLY `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`. Playwright assertion is non-negotiable.
4. **Inline title commit** — `onBlur` and `Enter` both commit. `Escape` reverts. Empty-after-trim resets to `Untitled meeting` and PATCHes that literal string.
5. **Search debounce** — 200ms (CONTEXT D-13). Query passes through FTS5 sanitization.
6. **Kebab actions** — `Copy markdown` uses `navigator.clipboard.writeText` (no fallback toast in Phase 7; toast deferred to Phase 9). `Reveal in Finder` POSTs `/api/meetings/:id/reveal` — no client-side handling of result. `Delete` opens confirmation, then `DELETE /api/meetings/:id`, then optimistic list update.
7. **Greeting time-of-day** — `hour < 12` → morning, `hour < 18` → afternoon, else evening. Recomputes on each Library mount (no real-time tick).
8. **EmptyLibrary `⌘N` hotkey** — Phase 7 ships the visual hint only; the actual `cmd+n` global hotkey lands in Phase 9 polish. The kbd chip is real, the binding is not. (Click on the button still creates a meeting.)

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting: PASS
- [ ] Dimension 2 Visuals: PASS
- [ ] Dimension 3 Color: PASS
- [ ] Dimension 4 Typography: PASS
- [ ] Dimension 5 Spacing: PASS
- [ ] Dimension 6 Registry Safety: PASS

**Approval:** pending
