# Phase 1: Design System - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace the throwaway Phase-0 styling with the proper Yogurt design system: wire Tailwind 4 with the brand tokens from PRD §16 via the `@theme` directive, ship the three font families (Instrument Serif, Hanken Grotesk, JetBrains Mono) via `@fontsource/*`, build the core component primitives (`Logo`, `Button`, `Pill`, `RecordingBadge`, `ProviderChip`, `Card`, `BrowserChrome`), and stand up a `/style-guide` route that renders every token + component as visual proof the system is wired correctly.

This phase is **frontend-only** (no Rust changes). The `/style-guide` route is the phase gate — a single long scrollable page that doubles as living documentation and human-eyeball validation surface. No user-facing screens (library, settings, onboarding, empty-states) are built here; those are downstream phases that compose these primitives.

</domain>

<decisions>
## Implementation Decisions

### Token system structure
- **D-01:** Tokens live in `web/src/index.css` inside a Tailwind 4 `@theme { … }` block — no separate `tailwind.config.ts`. CSS-first config is the Tailwind 4 way.
- **D-02:** Every token becomes a Tailwind utility automatically: `--color-blue` → `bg-blue` / `text-blue` / `border-blue`, `--radius-card` → `rounded-card`, `--shadow-card` → `shadow-card`, `--animate-recpulse` → `animate-recpulse`, etc.
- **D-03:** **Blueberry theme only.** Strawberry and Matcha-dark themes (PRD §16.2) are documented but explicitly deferred (PRD §15 — "Blueberry only in v1"). Ship one token block, not three.
- **D-04:** Base CSS resets (html/body bg, color, font-family, antialiasing) sit outside `@theme` and reference the variables directly — they are not Tailwind utilities, they are global defaults.

### Font loading
- **D-05:** Fonts ship via `@fontsource/*` packages (`@fontsource/instrument-serif`, `@fontsource/hanken-grotesk`, `@fontsource/jetbrains-mono`), imported as side-effect imports in `web/src/main.tsx` **above** `./index.css`. This embeds the woff2 files in the Vite bundle — no Google Fonts, no CDN, no external network calls (privacy posture).
- **D-06:** Weights to load: Instrument Serif 400 + 400-italic; Hanken Grotesk 400/500/600/700/800; JetBrains Mono 400/500/600 (per PRD §16.3). Total: 10 woff2 files.

### Motion tokens
- **D-07:** All 7 animations (`recpulse`, `blink`, `shimmer`, `wave`, `float`, `pop-up`, `slide-in-right`) are declared as `--animate-*` variables inside `@theme` with matching `@keyframes` blocks immediately after. The `--ease-pop` (cubic-bezier(0,0,0.2,1)) and `--ease-slide` (cubic-bezier(0.2,0.7,0.2,1)) easings are also declared as tokens.
- **D-08:** Motion is wired but not bound to runtime state in this phase — the showcase proves the animations work; Phase 2 wires `recpulse` to actual recording state, Phase 3 wires `slide-in-right` to transcript dock, Phase 4 wires `shimmer` to enhance state.

### Component primitive choices
- **D-09:** Six components ship in `web/src/components/`: `Logo` (SVG mark), `Button` (primary/secondary/ghost variants), `Pill` (base + `RecordingBadge` and `ProviderChip` named exports), `Card` (sm/md/lg padding + optional `active` border), `BrowserChrome` (fake-Safari mockup wrapper).
- **D-10:** Each component is colocated with a `*.test.tsx` Vitest + @testing-library/react smoke test. The full suite ends at 34 passing tests.
- **D-11:** **Tab group** (PRD §16.6 lists it as a primitive) is **deferred to Phase 4** — only the post-meeting view needs it.
- **D-12:** **End-meeting "ink" Button variant** (PRD §16.6) is deferred to Phase 4 — Phase 4 will extend `Button` with `variant="ink"` when needed. Document the deferral inline.

### Icon system (DESIGN-06)
- **D-13:** **No real icon library in Phase 1.** Lucide vs Phosphor selection is **deferred to Phase 7** (per superpowers plan §"Out of scope"). Phase 1 uses inline SVGs (`Logo`, traffic-light dots in `BrowserChrome`) and Unicode glyphs (`⌘K`, `↳`, `+`, `→`, `⌄`, `✨`, `⚙`) in the showcase to demonstrate the visual surface. DESIGN-06 coverage in Phase 1 is "icon strategy chosen and demonstrated"; full library installation lives in Phase 7.

### Routing
- **D-14:** React Router 7 (note: package renamed from `react-router-dom` to `react-router`) with `createBrowserRouter` + `RouterProvider` pattern. Two routes in v1: `/` (Phase 0 hello-world updated to compose new primitives) and `/style-guide`.
- **D-15:** `router.tsx` is split out of `main.tsx` so it can be tested in isolation via `createMemoryRouter`.

### Showcase screen approach
- **D-16:** Single long scrollable page at `/style-guide` — no tabs, no fancy navigation, no automated screenshot diffing. Sections separated by `<hr>`-equivalent spacing. Composes the already-tested primitives; no new tests for the showcase itself.
- **D-17:** Showcase sections (in order): Color tokens, Typography, Spacing, Border radius, Elevation, Motion, Logo at multiple sizes, Buttons, Pills/Badges/Chips, Cards, BrowserChrome. Each section pulls its data from a const array at the top of `StyleGuide.tsx` for clarity.

### Claude's Discretion
- Exact spacing between showcase sections (recommended `space-y-16`).
- Whether to add a sticky table-of-contents (not required — vertical scroll is fine).
- Exact `disabled:opacity` value on Button (50 in plan).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Authoritative implementation source
- `docs/superpowers/plans/2026-06-25-yogurt-phase-1-design-system.md` — **Source of truth.** Step-by-step task list (1.1–1.10) with exact code snippets for every file modification, exact token hex values, exact font weights, exact component APIs, exact test assertions, exact commit messages. Plans below reference this by Task number.

### Design tokens & visual system
- `docs/PRD.md` §16 (Brand & Visual Design System) — full token spec: §16.1 logo, §16.2 colors, §16.3 typography, §16.4 spacing/radius/elevation, §16.5 motion, §16.6 components
- `docs/PRD.md` §15 — "Blueberry only in v1" theme constraint
- `docs/PRD.md` §5.3, §5.4, §5.6, §5.9, §5.10 — usage contexts that the primitives must satisfy

### Project planning
- `.planning/REQUIREMENTS.md` "Design System" section — DESIGN-01 through DESIGN-06 acceptance criteria
- `.planning/ROADMAP.md` "### Phase 1: Design System" — phase boundary + 4-bullet success criteria
- `.planning/PROJECT.md` — overall project context, tech-stack constraints

### Prior phase
- `docs/superpowers/plans/2026-06-25-yogurt-phase-0-skeleton.md` — Phase 0 scaffold (React 19 + Vite + Tailwind 4 placeholder, `web/src/index.css` with `@import "tailwindcss";`, `App.tsx` TipTap demo). Phase 1 builds on this without redoing any of it.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets (from Phase 0)
- `web/src/index.css`: contains `@import "tailwindcss";` and placeholder body styling (`background: #FBF7EF; color: #211D18;`) — Task 1.1 replaces the placeholder with the full `@theme` block and base resets that reference token variables.
- `web/src/main.tsx`: standard StrictMode + createRoot mount; Task 1.2 adds 10 font side-effect imports above `./index.css`, Task 1.8 swaps the root element from `<App />` to `<RouterProvider router={router} />`.
- `web/src/App.tsx`: TipTap StarterKit demo + `fetchHealth()` useEffect; Task 1.8 rewrites to compose `Logo`, `Card`, `Pill`, and a `<Link>` to `/style-guide`.
- `web/src/App.test.tsx`: existing Phase-0 selectors (`text-neutral-500`, etc.) are now stale; Task 1.8 Step 8 rewrites assertions to target the new markup and wraps `<App />` in `<MemoryRouter>`.
- `web/src/lib/api.ts`: `fetchHealth()` exists from Phase 0 and is mocked in tests via `vi.mock('./lib/api', …)`.

### Established Patterns
- **Test colocation:** `<Component>.test.tsx` lives beside `<Component>.tsx`. Use Vitest + @testing-library/react. `it('<does thing>', …)` naming.
- **Side-effect CSS imports:** Phase 0 already uses `import "./index.css"` in `main.tsx`; font imports follow the same pattern.
- **No `tailwind.config.ts`:** Tailwind 4's CSS-first config means everything lives in `index.css`.

### Integration Points
- **`/style-guide` SPA fallback:** Phase 0's `assets.rs` already returns embedded `index.html` for any non-API route, so the React Router client-side `/style-guide` route works in release builds without backend changes.
- **No Rust changes:** `cargo test --workspace` should pass unchanged at end of phase.
- **No new backend endpoints, no schema changes, no Cargo dependencies touched.**

</code_context>

<specifics>
## Specific Ideas

- The Logo SVG is lifted verbatim from `yogurt-app-design/project/Yogurt Design Board.dc.html` line 32: 44×44 viewBox, blueberry circle (`fill="#5B4FC7"`), white spoon-curve path, strawberry dot at the spoon tip (`fill="#E07A66"`).
- The `BrowserChrome` traffic-light dots use the real macOS hex values: `#FF5F57` close, `#FEBC2E` minimize, `#28C840` maximize.
- The "secondary" Button border color is `#D9D0C0` — slightly warmer than `--color-line` per PRD §16.6 intent.
- The `--shadow-button-blue` (`0 2px 8px rgba(91,79,199,0.3)`) is applied inline on `bg-blue` buttons as an arbitrary Tailwind value, not as a `shadow-*` utility, to avoid a Tailwind utility for a single-use shadow.
- StyleGuide section: motion preview row uses a 160px-wide animation cell so all 7 examples align visually — the two "scripted" entries (`pop-up`, `slide-in-right`) render a muted-italic placeholder rather than an empty cell.

</specifics>

<deferred>
## Deferred Ideas

- **Settings page UI / Model section cards** — Phase 5
- **Library home view + sidebar layout** — Phase 7
- **Onboarding 3-step welcome flow** — Phase 7
- **Empty / error / enhancing / model-download state screens** — Phase 7 (Phase 4 wires the enhancing motion)
- **TipTap custom marks** (`aiGrey`, `transcriptTs` deep-link node) — Phase 4
- **Tab group primitive** — Phase 4 (only needed by post-meeting view)
- **Real icon system selection** (Lucide vs Phosphor vs custom) — Phase 7
- **Strawberry + Matcha-dark themes** — deferred per PRD §15 (potential v1.x add-on)
- **Light/dark mode toggle** — single Blueberry theme in v1
- **Live `recpulse` dot tied to actual recording state** — Phase 2 wires it; Phase 1 only verifies the animation runs
- **End-meeting "ink" Button variant** — Phase 4 extends `Button` when the meeting view needs it
- **Playwright visual regression / screenshot diffing** — Phase 9+ polish

</deferred>

---

*Phase: 01-design-system*
*Context gathered: 2026-06-25*
