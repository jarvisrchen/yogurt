---
phase: 1
slug: design-system
status: draft
shadcn_initialized: false
preset: none
created: 2026-06-25
---

# Phase 1 — UI Design Contract

> Visual and interaction contract for the Yogurt design system. This is the **canonical reference** for Phases 2-9 — every token, primitive, and copywriting decision below is inherited by downstream UI work. Source of truth: PRD §16 (Brand & Visual Design System).

---

## Design System

| Property | Value |
|----------|-------|
| Tool | none (custom Tailwind 4 `@theme` block; shadcn evaluation deferred per CONTEXT D-13) |
| Preset | not applicable |
| Component library | none (custom primitives: `Logo`, `Button`, `Pill`, `RecordingBadge`, `ProviderChip`, `Card`, `BrowserChrome`) |
| Icon library | none / inline SVG + Unicode glyphs (Lucide vs Phosphor selection deferred to Phase 7 per CONTEXT D-13) |
| Font | Instrument Serif (display) + Hanken Grotesk (body/UI) + JetBrains Mono (technical captions) — all bundled via `@fontsource/*` (no Google Fonts CDN, privacy posture) |

**Stack notes:**

- Tailwind 4 CSS-first config — all tokens declared in `web/src/index.css` inside a single `@theme { … }` block. **No `tailwind.config.ts`.**
- Every CSS variable auto-generates Tailwind utilities: `--color-paper` → `bg-paper`/`text-paper`/`border-paper`; `--radius-card` → `rounded-card`; `--animate-recpulse` → `animate-recpulse`.
- Blueberry theme only in v1 (PRD §15). Strawberry + Matcha-dark documented but not shipped.
- Single phase deliverable: `/style-guide` route renders every token + primitive as a long scrollable showcase (the human-eyeball validation gate).

---

## Spacing Scale

Declared values (multiples of 4 — PRD §16.4):

| Token | Value | Usage |
|-------|-------|-------|
| xs | 4px | Icon gaps, inline padding, tab-group inner track padding |
| sm | 8px | Compact element spacing, pill inner padding |
| (—) | 12px | Pill horizontal padding, card inner tight rows |
| md | 16px | Default element spacing, card padding sm |
| lg | 24px | Section padding, card padding md |
| xl | 32px | Layout gaps, card padding lg |
| 2xl | 48px | Major section breaks |
| 3xl | 64px | Page-level spacing (showcase section breaks; recommended `space-y-16`) |

**Exceptions (downstream phase-specific widths, documented for forward reference — NOT used in Phase 1):**

- 660px max-width — centered notes column (live meeting / post-meeting, Phase 4)
- 480px — Ask pill anchored bottom-center (Phase 6)
- 330px — Transcript dock width docked to right edge (Phase 3)
- 212px — Settings + Library sidebar width (Phase 5 + Phase 7)
- 44px — Logo mark canvas (renders cleanly at 19px and 60px scale-down/up)
- 42px — BrowserChrome header height
- 42px / 24px / 130px — notes column padding triplet (Phase 4)

---

## Typography

Source: PRD §16.3. Three families, type scale anchored to design board.

| Role | Family | Size | Weight | Line Height | Usage |
|------|--------|------|--------|-------------|-------|
| Hero display | Instrument Serif | 52px | 400 | 1.05 | Welcome screen hero, library empty state heading |
| Section title | Instrument Serif | 30-38px | 400 | 1.1 | Screen titles, marketing headlines |
| Card title | Hanken Grotesk | 16-21px | 700 | 1.3 | Card headers, settings group titles |
| Body | Hanken Grotesk | 13-17px | 400-500 | 1.5 | Note text, paragraph copy, list items |
| Label | Hanken Grotesk | 11-13px | 600 | 1.3 | Form labels, button text base, small badges |
| Button | Hanken Grotesk | 13.5px | 600 | 1 (leading-none) | All button variants |
| Caption | Hanken Grotesk | 11-13px | 500 | 1.4 | Helper text, metadata under cards |
| Mono caption | JetBrains Mono | 10.5-12px | 400-500 | 1.4 | Timestamps, CLI output, `localhost:7878`, file paths |

**Font weights loaded (PRD §16.3, ten woff2 files via `@fontsource/*`):**

- Instrument Serif: 400, 400-italic
- Hanken Grotesk: 400, 500, 600, 700, 800
- JetBrains Mono: 400, 500, 600

**Wordmark spec:** `yogurt` — Instrument Serif, lowercase, `letter-spacing: -0.01em`.

---

## Color

Source: PRD §16.2 (Blueberry theme — the only v1 theme).

| Role | Value | Usage |
|------|-------|-------|
| Dominant (60%) | `#FBF7EF` (`--color-paper`) | App background, hero surfaces, onboarding left column |
| Secondary (30%) | `#FFFFFF` (`--color-card`) | Cards, surfaces over paper, onboarding right column |
| Accent (10%) | `#5B4FC7` (`--color-blue`, "blueberry") | Primary CTAs, active nav, transcript deep-link underline, provider card border (active state) |
| Destructive | `#E07A66` (`--color-straw`, "strawberry") | Recording indicator dot + border, permission-denied / error accent |

**Accent (blueberry) reserved EXPLICITLY for:**

1. Primary `Button` background ("New meeting", "End meeting", "Take me to my meetings →", "Re-enhance", "Generate")
2. Active-nav background (via soft-blueberry `#ECE9FB` for the wash, blueberry text on top)
3. Transcript deep-link affordance — `1.5px dotted #C9B8F0` underline on `↳ HH:MM` markers (Phase 4)
4. Provider card border when active/selected (Phase 5)
5. Logo mark circle fill (`Logo.tsx` inline SVG)

**Destructive (strawberry) reserved EXPLICITLY for:**

1. `RecordingBadge` border + pulsing dot (PRD §16.6)
2. Permission-denied state ("Yogurt can't hear the call yet", Phase 7 STATE-02)
3. Logo mark strawberry-dot accent at spoon tip (`Logo.tsx`)

**Full token palette (use in implementation; declared in `@theme`):**

| Token | Hex | Role |
|-------|-----|------|
| `--color-paper` | `#FBF7EF` | App background |
| `--color-card` | `#FFFFFF` | Card surface |
| `--color-ink` | `#211D18` | User notes, headings, primary text (the "black" in black-user/grey-AI) |
| `--color-grey` | `#A89F90` | AI-added text, secondary captions (the "grey" in black-user/grey-AI) |
| `--color-line` | `#EBE3D5` | Borders, dividers |
| `--color-blue` | `#5B4FC7` | Blueberry primary |
| `--color-blsoft` | `#ECE9FB` | Soft blueberry — active-nav wash, pill backgrounds, "enhancing" banner |
| `--color-straw` | `#E07A66` | Strawberry destructive/recording |
| `--color-strsoft` | `#FBE6E0` | Soft strawberry — error/warning surfaces |
| `--color-matcha` | `#5E9E73` | Local-only / privacy / success badges |
| `--color-mtsoft` | `#E7F0E8` | Soft matcha — "Local-only · on" pill background |
| `--color-mut` | `#8A8174` | Muted text on cards, ghost-button text |

**Semantic mapping (downstream phases):**

- **Black-user / grey-AI hero treatment (PRD §16.7):** user-authored note text uses `--color-ink`; LLM-added text uses `--color-grey`. This pair is the single most load-bearing visual contract in the product.
- **Lilac (`#ECE9FB` soft blueberry):** background for the "✨ AI enhances these when you hit End" enhancing banner (Phase 4).
- **Secondary button border:** `#D9D0C0` — slightly warmer than `--color-line` per PRD §16.6 (kept inline, not promoted to its own token).

**Elevation (PRD §16.4):**

| Token | Value | Usage |
|-------|-------|-------|
| `--shadow-card` | `0 2px 6px rgba(40,30,15,0.08)` | Cards |
| `--shadow-pop` | `0 12px 30px -10px rgba(40,30,15,0.22)` | Chat window, popover (Phase 6) |
| `--shadow-window` | `0 26px 60px -28px rgba(40,30,15,0.4)` | Modals, `BrowserChrome` mockup chrome |
| `--shadow-button-blue` | `0 2px 8px rgba(91,79,199,0.3)` | Primary `Button` (applied as arbitrary value, not a `shadow-*` utility) |

**Border radius (PRD §16.4):**

| Token | Value | Usage |
|-------|-------|-------|
| `--radius-chip` | 6px | Small chip |
| `--radius-button` | 9px | Buttons, inputs |
| `--radius-card` | 14px | Cards |
| `--radius-pill` | 999px | Pills, `RecordingBadge`, `ProviderChip` |

**Motion (PRD §16.5):**

| Token | Duration | Easing | Use |
|-------|----------|--------|-----|
| `--animate-recpulse` | 1.4s | ease-in-out infinite | Recording dot pulse, enhancing dot pulse |
| `--animate-blink` | 1.0s | step-end infinite | Cursor blink for active editor / streaming partial |
| `--animate-shimmer` | 1.25s | linear infinite | Skeleton placeholders during enhance streaming |
| `--animate-wave` | 1.0s | ease-in-out infinite | 3-bar audio wave on live transcript tab |
| `--animate-float` | 3.5s | ease-in-out infinite | Empty-state logo gentle float |
| `--animate-pop-up` | 260ms | `cubic-bezier(0,0,0.2,1)` (`--ease-pop`) | Chat window expanding from Ask pill (Phase 6) |
| `--animate-slide-in-right` | 340ms | `cubic-bezier(0.2,0.7,0.2,1)` (`--ease-slide`) | Transcript dock opening from right edge (Phase 3) |

Phase 1 wires all 7 animations but does NOT bind them to runtime state — the showcase verifies they run; downstream phases bind them: Phase 2 wires `recpulse` to recording state, Phase 3 wires `slide-in-right` to transcript dock, Phase 4 wires `shimmer` to enhance streaming.

---

## Copywriting Contract

Voice: warm + editorial + a touch dev-tooly. Mono captions set the technical tone ("100% on-device", "localhost:7878", `~/.yogurt/notes/`); Instrument Serif headlines set the editorial tone.

| Element | Copy |
|---------|------|
| Primary CTA (library empty) | "Start your first meeting" + `⌘N` keyboard hint (Phase 7) |
| Empty state heading | "No meetings yet" (Phase 7 STATE-01) |
| Empty state body | Mono caption hinting at notes location: `~/.yogurt/notes/` |
| Error state | "Yogurt can't hear the call yet" — followed by next-step copy directing the user to System Settings → Privacy → Screen Recording (Phase 7 STATE-02) |
| Destructive confirmation | "Delete this meeting? This removes notes, transcript, and markdown export. This cannot be undone." |
| Recording badge | Pulsing strawberry dot + mono timer in `MM:SS` format (e.g., `12:04`) |
| Local-mode badge | "Local-only · on" — matcha-soft pill in library sidebar |
| Enhancing banner | "✨ AI enhances these when you hit End" — soft-blueberry pill under editor (Phase 4) |
| Wordmark | `yogurt` (lowercase, always) — Instrument Serif, `-0.01em` letter-spacing |
| Personality line (marketing) | *"A local-first, open-source meeting copilot. Granola's augmented-notes UX, the privacy posture inverted — your audio never leaves the machine."* |

**Phase 1 `/style-guide` showcase copy:**

- Section labels use Hanken 700 card-title size (16-21px).
- Each token row uses mono captions for the variable name (e.g., `--color-blue · #5B4FC7`) and Hanken body for the usage description.
- Motion preview row uses 160px-wide animation cells; scripted entries (`pop-up`, `slide-in-right`) render a muted-italic placeholder rather than an empty cell.

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | none | not required (shadcn not initialized — `shadcn_initialized: false`) |
| Any third-party | none | not applicable |

**Notes:**

- Custom primitives only. All seven Phase 1 components (`Logo`, `Button`, `Pill`, `RecordingBadge`, `ProviderChip`, `Card`, `BrowserChrome`) are hand-written from PRD §16 specs; no external component code is copied or vendored.
- **shadcn evaluation deferred per CONTEXT D-13.** If shadcn is introduced in a later phase, that phase's UI-SPEC MUST gate on `shadcn view <block>` + diff against any custom override before merge.
- Logo SVG lifted verbatim from `yogurt-app-design/project/Yogurt Design Board.dc.html` line 32 (44×44 viewBox, blueberry circle `#5B4FC7`, white spoon path, strawberry dot `#E07A66`) — design-board source, not a third-party registry.
- BrowserChrome traffic-light dots use real macOS hex values: `#FF5F57` close, `#FEBC2E` minimize, `#28C840` maximize.

---

## Component Primitive APIs (Phase 1 contract)

All components live in `web/src/components/` with colocated `*.test.tsx`. Total: 34 passing Vitest + @testing-library/react tests.

| Component | Variants / Props | Notes |
|-----------|------------------|-------|
| `Logo` | `size?: number` (default 44) | Inline SVG; spoon-and-swirl mark; renders cleanly at 19px and 60px |
| `Button` | `variant: "primary" \| "secondary" \| "ghost"` (default primary), `disabled`, `onClick`, `type`, standard HTML button attrs | Primary = blueberry/white, Secondary = white/ink with `#D9D0C0` border, Ghost = transparent/muted. `disabled:opacity-50`. Ink variant deferred to Phase 4. |
| `Pill` | `tone?: "neutral" \| "blue" \| "matcha" \| "straw"` (default neutral), `children` | Base 999-radius pill. Tone maps to soft-bg + matching text color. |
| `RecordingBadge` | (named export wrapping `Pill`) | Strawberry-bordered pill with pulsing dot + mono timer. |
| `ProviderChip` | (named export wrapping `Pill`) | 8px-radius soft-tone chip with dot indicator + provider name. |
| `Card` | `padding?: "sm" \| "md" \| "lg"` (16/24/32), `active?: boolean` | White surface, `rounded-card` (14px), `shadow-card`. `active` adds blueberry border. |
| `BrowserChrome` | `url?: string`, `children` | Fake-Safari wrapper: 42px header, `#F4EEE3` bg, 3 traffic-light dots, centered URL pill. Wraps children as the "browser body". |

**Deferred to later phases:**

- Tab group (Notes / Summary / Transcript) — Phase 4 (only needed by post-meeting view)
- Button `variant="ink"` — Phase 4 (live meeting top bar)
- Real icon library — Phase 7 (Lucide vs Phosphor decision)

---

## Validation Gate

The `/style-guide` route IS the phase acceptance gate. It MUST render:

1. **Color tokens** — every `--color-*` swatch with hex + variable name + usage caption
2. **Typography** — all three families at every loaded weight, with specimen pangrams
3. **Spacing** — visual ruler showing 4/8/12/16/24/32/48 stops
4. **Border radius** — 4 swatches (chip/button/card/pill)
5. **Elevation** — 3 cards demonstrating card/pop/window shadows
6. **Motion** — 7 animation preview cells (160px wide each)
7. **Logo** — rendered at 19/44/60/120px to verify crispness across scales
8. **Buttons** — all 3 variants in default + hover + disabled states
9. **Pills / Badges / Chips** — all 4 tones + `RecordingBadge` + `ProviderChip`
10. **Cards** — sm/md/lg padding + default vs `active`
11. **BrowserChrome** — wrapping a placeholder body

Manual eyeball validation only — Playwright visual-regression diffing is Phase 9+ polish.

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting: PASS
- [ ] Dimension 2 Visuals: PASS
- [ ] Dimension 3 Color: PASS
- [ ] Dimension 4 Typography: PASS
- [ ] Dimension 5 Spacing: PASS
- [ ] Dimension 6 Registry Safety: PASS

**Approval:** pending
