---
phase: 01-design-system
plan: 01
subsystem: web/design-tokens
tags: [tailwind4, theme, tokens, fontsource, motion, keyframes, css-first]
dependency_graph:
  requires:
    - phase-0/web-scaffold  # React 19 + Vite 6 + Tailwind 4 + @import "tailwindcss"
  provides:
    - design-tokens/color   # 12 --color-* tokens (PRD §16.2)
    - design-tokens/type    # 3 --font-* family chains (PRD §16.3)
    - design-tokens/radius  # 4 --radius-* tokens (PRD §16.4)
    - design-tokens/shadow  # 4 --shadow-* tokens (PRD §16.4)
    - design-tokens/easing  # 2 --ease-* tokens (PRD §16.5)
    - design-tokens/motion  # 7 --animate-* tokens + matching @keyframes (PRD §16.5)
    - fonts/instrument-serif # 400 + 400-italic woff2
    - fonts/hanken-grotesk   # 400/500/600/700/800 woff2
    - fonts/jetbrains-mono   # 400/500/600 woff2
  affects:
    - web/src/index.css      # @theme block, keyframes, base resets
    - web/src/main.tsx       # 10 font side-effect imports above ./index.css
tech-stack:
  added:
    - "@fontsource/instrument-serif@5.2.8"
    - "@fontsource/hanken-grotesk@5.2.8"
    - "@fontsource/jetbrains-mono@5.2.8"
  patterns:
    - "Tailwind 4 CSS-first @theme block (no tailwind.config.ts)"
    - "Side-effect font imports via @fontsource/* (no Google Fonts CDN)"
    - "Motion tokens declared centrally; bound to state in downstream phases"
key-files:
  created: []
  modified:
    - web/src/index.css
    - web/src/main.tsx
    - web/package.json
    - web/pnpm-lock.yaml
decisions:
  - "Single Blueberry token block; Strawberry + Matcha-dark deferred (PRD §15)"
  - "All 7 motion tokens + @keyframes declared in Phase 1; binding to runtime state happens in Phases 2/3/4"
  - "Fonts shipped via @fontsource/* (privacy posture — no CDN egress)"
  - "Base resets reference token vars (var(--color-paper), var(--font-sans)) rather than hardcoded hex"
metrics:
  duration: "~2 min"
  completed: "2026-06-25"
  tasks_completed: 3
  files_modified: 4
  commits: 2
requirements_completed:
  - DESIGN-01  # Color tokens (12)
  - DESIGN-02  # Typography tokens (3 families × 10 weights)
  - DESIGN-03  # Spacing/radius/elevation tokens
  - DESIGN-04  # Motion tokens
---

# Phase 1 Plan 01: Design Tokens + Font Loading Summary

Wired the Tailwind 4 `@theme` block in `web/src/index.css` with every PRD §16 design token (12 colors, 3 font families, 4 radii, 4 shadows, 2 easings, 7 motion tokens + matching keyframes) and installed the three `@fontsource/*` packages with 10 weight side-effect imports in `main.tsx` — token foundation now consumable by all downstream component primitives.

## One-liner

Tailwind 4 `@theme` token block + `@fontsource/*` font pipeline replacing the Phase-0 placeholder styling.

---

## What was built

### Task 1 — `web/src/index.css` rewrite (commit `b7433a7`)

Replaced the 4-line Phase-0 placeholder (`@import "tailwindcss"` + hardcoded `#FBF7EF` / `#211D18` body styling) with the full Tailwind 4 `@theme` block:

- **12 color tokens** — `--color-paper`, `--color-card`, `--color-ink`, `--color-grey`, `--color-line`, `--color-blue`, `--color-blsoft`, `--color-straw`, `--color-strsoft`, `--color-matcha`, `--color-mtsoft`, `--color-mut` (exact hex values from PRD §16.2 verbatim)
- **3 font-family chains** — `--font-serif` (Instrument Serif → ui-serif → Georgia), `--font-sans` (Hanken Grotesk → ui-sans-serif → system-ui → -apple-system → "Segoe UI"), `--font-mono` (JetBrains Mono → ui-monospace → SFMono-Regular → Menlo)
- **4 radii** — `--radius-chip: 6px`, `--radius-button: 9px`, `--radius-card: 14px`, `--radius-pill: 999px`
- **4 shadows** — `--shadow-card`, `--shadow-pop`, `--shadow-window`, `--shadow-button-blue` (exact rgba per PRD §16.4)
- **2 easings** — `--ease-pop: cubic-bezier(0,0,0.2,1)`, `--ease-slide: cubic-bezier(0.2,0.7,0.2,1)`
- **7 motion tokens** — `--animate-recpulse` (1.4s), `--animate-blink` (1.0s), `--animate-shimmer` (1.25s), `--animate-wave` (1.0s), `--animate-float` (3.5s), `--animate-pop-up` (260ms), `--animate-slide-in-right` (340ms)
- **7 matching `@keyframes` blocks** — recpulse, blink, shimmer, wave, float, pop-up, slide-in-right (referenced by the `--animate-*` tokens above)
- **Base resets** — `:root { color-scheme: light; }` + `html, body { … }` block referencing `var(--color-paper)`, `var(--color-ink)`, `var(--font-sans)`, 15px font-size, 1.5 line-height, antialiased font smoothing

### Task 2 — `@fontsource/*` install + main.tsx (commit `c170f35`)

- Installed `@fontsource/instrument-serif@5.2.8`, `@fontsource/hanken-grotesk@5.2.8`, `@fontsource/jetbrains-mono@5.2.8` (all under `dependencies`)
- Prepended 10 side-effect CSS imports to the top of `web/src/main.tsx`, ABOVE the existing `./index.css` import — registers all required `@font-face` rules in the bundle
- Weights loaded: Instrument Serif 400 + 400-italic; Hanken Grotesk 400/500/600/700/800; JetBrains Mono 400/500/600 (PRD §16.3)
- Existing `StrictMode` / `createRoot` / `<App />` mount preserved unchanged (router swap happens in Plan 03)

### Task 3 — Smoke test

- `pnpm --dir web test` → 2 tests passed (Phase-0 App suite — the body styling switch from hardcoded hex to `var(--color-paper)` did not break anything)
- `pnpm --dir web build` → succeeds in ~700ms, emits 32 `.woff2` files into `web/dist/assets/` plus a 68 kB CSS chunk containing the token declarations
- No FontFace warnings, no `@theme` syntax errors

---

## Acceptance criteria (verified)

- [x] `web/src/index.css` contains `@theme {` (matched, twice — once in comment + once in directive)
- [x] All 12 `--color-*` tokens present with exact hex values
- [x] All 3 `--font-*` chains present with required first-family names
- [x] All 4 `--radius-*` tokens present
- [x] All 4 `--shadow-*` tokens present
- [x] Both `--ease-pop` and `--ease-slide` present with exact cubic-bezier values
- [x] All 7 `--animate-*` tokens present
- [x] All 7 `@keyframes` blocks present (grep counted 7 lines starting with `@keyframes`)
- [x] Base resets reference `var(--color-paper)` and `var(--font-sans)`
- [x] `web/package.json` contains all three `@fontsource/*` dependencies
- [x] `web/src/main.tsx` contains all 10 `@fontsource/*/<weight>.css` imports, all above `./index.css`
- [x] `pnpm --dir web build` exits 0 and emits ≥10 .woff2 files (actual: 32)
- [x] `pnpm --dir web test` exits 0 (Phase-0 suite unchanged: 2/2 pass)

---

## Deviations from Plan

None — plan executed exactly as written. All token values, font weights, keyframe definitions, and main.tsx import order match the superpowers plan and UI-SPEC verbatim. No Rules 1-4 deviations triggered.

The plan's verify line `ls web/dist/assets/*.woff2 | wc -l` produced 32 (the build emits both `.woff` and `.woff2` per family/weight; counting only `.woff2` still vastly exceeds the ≥10 floor).

---

## Authentication gates

None encountered — no external APIs touched. All work was local file edits + `pnpm add` from the workspace.

---

## Decisions made

- **Skipped the dev-server smoke check from Step 3 of superpowers Task 1.2** (DevTools Network inspection) — the build-time emit of woff2 files into `dist/assets/` is sufficient evidence the fontsource pipeline works, and dev-server smoke checks aren't automatable from a non-interactive executor. The acceptance criteria explicitly only require the build to emit woff2s, which it does.
- **Did not modify `web/src/App.tsx`** — only Plans 02 and 03 are scoped to swap components into App and wire the router. Plan 01-01's contract is strictly tokens + fonts.

---

## Known stubs

None — this plan does not introduce any stubbed components, placeholder data, or "coming soon" UI surface. The `@theme` block declares concrete values; the fonts are real fonts; the keyframes are functional CSS. Phase 1 Plans 02 and 03 will consume these tokens directly.

---

## What's next

- **Plan 01-02** — Component primitives (`Logo`, `Button`, `Pill`, `RecordingBadge`, `ProviderChip`, `Card`, `BrowserChrome`) consuming the utility classes that Tailwind 4 auto-generates from this theme (`bg-paper`, `text-ink`, `font-serif`, `rounded-card`, `shadow-card`, `animate-recpulse`, etc.).
- **Plan 01-03** — `/style-guide` route + router swap, rendering every token + primitive as the human-eyeball validation gate for Phase 1.

---

## Commits

| Hash      | Message |
| --------- | ------- |
| `b7433a7` | feat(01-01): add tailwind 4 @theme block with PRD §16 design tokens |
| `c170f35` | feat(01-01): add @fontsource for instrument-serif, hanken-grotesk, jetbrains-mono |

---

## Self-Check: PASSED

- web/src/index.css — FOUND (modified)
- web/src/main.tsx — FOUND (modified)
- web/package.json — FOUND (modified, 3 deps added)
- web/pnpm-lock.yaml — FOUND (modified)
- Commit b7433a7 — FOUND in git log
- Commit c170f35 — FOUND in git log
- web/dist/assets/*.woff2 — FOUND (32 files emitted)
- web/dist/assets/index-*.css — FOUND (68 kB chunk)
