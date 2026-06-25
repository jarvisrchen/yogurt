---
phase: 01-design-system
plan: 03
subsystem: web
tags: [routing, design-system, showcase, phase-gate]
requires:
  - "Phase 0 axum SPA-fallback at GET /* serving embedded web/dist (assets.rs)"
  - "Plan 01-01: Tailwind 4 @theme tokens in web/src/index.css"
  - "Plan 01-02: Logo, Button, Pill+RecordingBadge+ProviderChip, Card, BrowserChrome primitives in web/src/components/"
  - "@fontsource side-effect imports preserved in main.tsx"
provides:
  - "React Router 7 mounted via RouterProvider"
  - "/ → App (Phase-1 styled hello using Logo + Pill + Card + Link)"
  - "/style-guide → full token + component showcase (phase-gate eyeball surface)"
  - "Public seam: routes export from web/src/router.tsx for future test reuse"
affects:
  - "Future phases consume the router by adding entries to web/src/router.tsx"
  - "Phase 4 will replace path='/' element to the library home; current App becomes scaffold reference"
tech_stack:
  added:
    - "react-router@7.18.0 (NOT react-router-dom — package was renamed in v7)"
  patterns:
    - "createBrowserRouter + RouterProvider (v6.4+ data-router pattern)"
    - "createMemoryRouter for in-test routing without a window"
    - "Local vi.mock('./lib/api') in router.test.tsx because mocks don't leak across files"
key_files:
  created:
    - "web/src/router.tsx — route table + browser router"
    - "web/src/router.test.tsx — 2 createMemoryRouter smoke tests"
    - "web/src/routes/StyleGuide.tsx — full showcase (~445 lines)"
  modified:
    - "web/src/main.tsx — render RouterProvider instead of App"
    - "web/src/App.tsx — compose Logo + Pill + Card + Link; preserve LO-03 inline-error path via tone toggle"
    - "web/src/App.test.tsx — wrap App in MemoryRouter, add /style-guide href assertion"
    - "web/package.json + web/pnpm-lock.yaml — add react-router@^7"
decisions:
  - "Preserved Phase 0 LO-03 inline healthError state (toggle Pill tone between matcha/straw) rather than dropping it as the superpowers plan implied — Rule 2 correctness preservation"
  - "Added an Iconography section to /style-guide that the original superpowers plan did not have; satisfies DESIGN-06 explicitly with inline-SVG + Unicode-glyph demonstrations and a comment pointing at CONTEXT D-13 (Lucide/Phosphor selection deferred to Phase 7)"
  - "Used Pill tone toggling for server-status (matcha on success, straw on error) rather than a separate Banner primitive — matches the dot/badge vocabulary already in the design system without inventing new shapes"
metrics:
  duration: "~10 min"
  completed: "2026-06-25"
  tasks_completed: 4
  files_created: 3
  files_modified: 5
  tests_total: 35
  tests_passing: 35
  cargo_tests_passing: 28
  smoke_endpoints_passing: 3
---

# Phase 1 Plan 03: Routing + Style-Guide Showcase Summary

**One-liner:** React Router 7 mounted via `createBrowserRouter`; `/` renders a Phase-1-styled hello composing Plan-02 primitives; `/style-guide` renders every PRD §16 token group + every Phase-1 component variant on a single scrollable page — the phase-gate eyeball surface satisfying DESIGN-01 through DESIGN-06.

## What Was Built

### Routing layer

- **`web/src/router.tsx`** (~18 lines) — exports `routes: RouteObject[]` and `router = createBrowserRouter(routes)`. Maps `/` → `<App />` and `/style-guide` → `<StyleGuide />`. Public `routes` export lets tests build `createMemoryRouter(routes, { initialEntries: [...] })` without duplicating the route table.
- **`web/src/router.test.tsx`** (~32 lines) — 2 tests covering both routes via `createMemoryRouter`. Locally mocks `./lib/api.fetchHealth` (mocks don't leak across files in Vitest).
- **`web/src/main.tsx`** — swaps `<App />` to `<RouterProvider router={router} />` inside `<StrictMode>`. All 10 `@fontsource` side-effect imports preserved.

### Phase-1 styled hello (`/`)

- **`web/src/App.tsx`** — rewritten to compose Plan-02 primitives:
  - Header: `<Logo size={44} ariaLabel="Yogurt" />` + Instrument-Serif "yogurt" headline (44px) + caption containing `<Link to="/style-guide">/style-guide →</Link>`
  - Server status: `<Pill tone="matcha">` (success) or `<Pill tone="straw">` (error) wrapping `<code className="font-mono">` health string
  - Editor: `<Card padding="md">` wrapping `<EditorContent />`
- **Preserved LO-03 inline error handling** (added in Phase 0 by `feat(web): surface health check errors inline`) by toggling Pill tone instead of dropping the error path. The superpowers plan implied a simpler success-only path; keeping the error branch is a Rule-2 correctness preservation.
- **`web/src/App.test.tsx`** — wraps `<App />` in `<MemoryRouter>` (required for `<Link>` to render); 3 cases: headline renders, health response shows, `/style-guide` link has `href="/style-guide"`.

### `/style-guide` showcase (~445 lines)

A single scrollable page rendering every token group and every component variant:

| Section | What it renders |
|---|---|
| Color tokens | 12 swatches (paper/card/ink/grey/line/blue/blsoft/straw/strsoft/matcha/mtsoft/mut) with hex + use |
| Typography | Instrument Serif 52/38/30/20-italic; Hanken Grotesk 400/500/600/700/800; JetBrains Mono 400/500/600 |
| Spacing | 4-base ladder 4/8/12/16/24/32/48 px blueberry blocks |
| Border radius | 4 cards (chip 6 / button 9 / card 14 / pill 999) |
| Elevation | 3 shadows (`shadow-card`, `shadow-pop`, `shadow-window`) |
| Motion | 7 token rows: recpulse / blink / shimmer / wave / float live; pop-up + slide-in-right marked "scripted" |
| Logo | 5 sizes (19 / 24 / 32 / 44 / 60 px) |
| Buttons | Primary, secondary, ghost — each with multiple labels + a disabled variant |
| Pills | All 4 base tones + 3 RecordingBadge timers + 5 ProviderChip variants (active / neutral / local) |
| Cards | sm / md / lg paddings; `active` variant with blueberry hairline |
| BrowserChrome | Wraps a Logo + welcome heading mock at `localhost:7878/welcome` |
| Iconography (DESIGN-06) | Inline SVG Logo at 3 sizes + Unicode glyphs (⌘K, ↳, ✓, ✨, ⌄, ⚙) inside Button/Pill, with deferral note → CONTEXT D-13 |
| Footer | PRD §16 attribution + last-updated date in mono |

Local `Section` helper component standardizes h2 (Instrument-Serif 30px) + caption + content slot.

## Tasks Executed

| Task | Description | Commit |
|---|---|---|
| 1 | Install react-router@^7 + create router.tsx + router.test.tsx + placeholder StyleGuide.tsx | `67d5f47` |
| 2 | Mount RouterProvider in main.tsx; rewrite App.tsx + App.test.tsx | `dfdcfcc` |
| 3 | Expand StyleGuide.tsx to full showcase | `38c762d` |
| 4 | Full Phase-1 acceptance gate — tests + build + release smoke + cargo hygiene (no file changes) | — |

## Verification Results

| Check | Result |
|---|---|
| `pnpm --dir web test` | **35 passed** (Logo 4 + Button 7 + Pill 10 + Card 5 + BrowserChrome 4 + router 2 + App 3) |
| `pnpm --dir web build` | exit 0; `web/dist/index.html` present; `web/dist/assets/index-*.css` contains `--color-blue:#5b4fc7`; 31 `.woff2` files (target ≥10); total `web/dist/` = 1.5 MB |
| `cargo test --workspace` | **28 passed** across 9 suites in 0.91s (Phase-0 suite unchanged) |
| `cargo fmt --all --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean — no issues |
| `cargo build --release` | exit 0 (2 crates compiled in 8.84s) |
| Release smoke: `GET /` | **HTTP 200**, returns `index.html` (389 B) containing `id="root"` |
| Release smoke: `GET /style-guide` | **HTTP 200**, returns `index.html` (389 B) containing `id="root"` (SPA fallback works) |
| Release smoke: `GET /api/health` | **HTTP 200** → `{"service":"yogurt-server","status":"ok"}` |

> Note: test count is 35 (one more than the plan's projected 34) because the existing `Pill.test.tsx` carries 10 cases, not 9. No change required — already passing pre-Plan-03.

## Requirements Satisfied

| ID | How |
|---|---|
| **DESIGN-01** | 12 color swatches in the Color Tokens section render every PRD §16.2 token with exact hex values |
| **DESIGN-02** | Typography section renders Instrument Serif (4 sizes), Hanken Grotesk (5 weights), JetBrains Mono (3 weights) — visibly distinct families |
| **DESIGN-03** | Spacing (4-base ladder), Radius (4 scales), Elevation (3 shadows) sections all rendered |
| **DESIGN-04** | 7 motion tokens declared in `@theme`; 5 visibly animate (recpulse / blink / shimmer / wave / float); pop-up + slide-in-right marked "scripted — see chat / transcript" per PRD usage |
| **DESIGN-05** | Logo, Button (primary/secondary/ghost), Pill (4 tones), RecordingBadge, ProviderChip (active/neutral/local), Card (sm/md/lg + active), BrowserChrome — all rendered in their respective sections |
| **DESIGN-06** | Inline SVG (Logo at 3 sizes, BrowserChrome traffic-lights) + Unicode glyphs (⌘K, ↳, ✓, ✨, ⌄, ⚙, +, →) demonstrated in Iconography section with deferral note → CONTEXT D-13 (Lucide vs Phosphor selection in Phase 7) |

## Deviations from Plan

### Auto-fixed / preserved (Rule 2 — correctness)

**1. [Rule 2 - Critical functionality] Preserved LO-03 inline error handling on `/`**
- **Found during:** Task 2
- **Issue:** The superpowers-plan App.tsx body shown at lines 1220-1271 dropped the Phase-0 `healthError` state added by commit `feat(web): surface health check errors inline` (LO-03). Reverting to the plan literal would have re-introduced the silent-failure UX the LO-03 fix was specifically created to prevent.
- **Fix:** Kept `healthError` state + the `.catch()` setter, and surfaced the error branch via Pill `tone="straw"` (vs `tone="matcha"` on success). Pill copy switches between `"unreachable — <message>"` and `"<service> <status>"`. Visually consistent with the design system; functionally preserves LO-03.
- **Files modified:** `web/src/App.tsx`
- **Commit:** `dfdcfcc`

### Plan additions (orchestrator guidance)

**2. [Orchestrator additive] Iconography section added to /style-guide**
- **Found during:** Task 3
- **Reason:** The orchestrator prompt explicitly required an Iconography demo section satisfying DESIGN-06 with both inline SVG (3-4 sizes) and Unicode glyphs (3-4 in visual context), plus a comment pointing to CONTEXT D-13. The superpowers plan's StyleGuide snippet did not contain this section.
- **Implementation:** Added a 13th `<Section>` rendering Logo at 19/32/60 px and a grid of Button + Pill variants using ⌘K, ↳, ✓, ✨, ⌄, ⚙. Explanatory paragraph + JSDoc top-of-file comment point to CONTEXT D-13.
- **Files modified:** `web/src/routes/StyleGuide.tsx`
- **Commit:** `38c762d`

### Environment

- `cargo` was not on the default PATH; resolved by sourcing `/opt/homebrew/Cellar/rustup/1.29.0_2/bin` + `/Users/rchen/.rustup/toolchains/stable-aarch64-apple-darwin/bin` inline. No project change required.

## Known Stubs

None. Every component on `/style-guide` is wired to real Phase-1 primitives. The two "scripted" motion tokens (pop-up, slide-in-right) are intentionally non-live in the showcase — they're declarative `@theme` tokens consumed at runtime by Phase 3 (chat) and Phase 6 (transcript dock); a static preview would misrepresent their use. This is documented inline ("scripted — see chat / transcript") rather than faked with a placeholder animation.

## File Inventory

**Created:**
- `web/src/router.tsx` (18 lines)
- `web/src/router.test.tsx` (32 lines)
- `web/src/routes/StyleGuide.tsx` (445 lines)

**Modified:**
- `web/src/main.tsx` (23 → 24 lines; render path swap + RouterProvider import)
- `web/src/App.tsx` (56 → 73 lines; primitives + Link + Pill-tone error toggle)
- `web/src/App.test.tsx` (26 → 39 lines; MemoryRouter wrap + link assertion)
- `web/package.json` (1 dependency added)
- `web/pnpm-lock.yaml` (regenerated)

## Self-Check: PASSED

- [x] `web/src/router.tsx` exists (verified)
- [x] `web/src/router.test.tsx` exists (verified)
- [x] `web/src/routes/StyleGuide.tsx` exists (verified)
- [x] `web/src/main.tsx` contains `RouterProvider router={router}` (verified)
- [x] `web/src/App.tsx` contains `Link` import from `"react-router"` and `to="/style-guide"` (verified)
- [x] Commit `67d5f47` exists (Task 1)
- [x] Commit `dfdcfcc` exists (Task 2)
- [x] Commit `38c762d` exists (Task 3)
- [x] All 35 web tests pass
- [x] All 28 cargo tests pass
- [x] Release smoke: 3/3 endpoints HTTP 200 (`/`, `/style-guide`, `/api/health`)
