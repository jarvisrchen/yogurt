---
phase: 01-design-system
plan: 02
subsystem: web/components
tags: [react, tailwind, tdd, components, design-system]
dependency_graph:
  requires:
    - 01-01 (Tailwind 4 @theme tokens + @fontsource imports — `bg-paper`, `bg-card`, `bg-blsoft`, `bg-mtsoft`, `bg-strsoft`, `bg-blue`, `bg-matcha`, `bg-straw`, `text-ink`, `text-mut`, `text-blue`, `text-matcha`, `text-straw`, `text-white`, `border-line`, `rounded-card`, `rounded-pill`, `rounded-button`, `shadow-card`, `font-sans`, `font-mono`, `animate-recpulse`)
  provides:
    - Logo, Button, Pill, RecordingBadge, ProviderChip, Card, BrowserChrome React primitives — colocated under `web/src/components/` — for use by downstream Phase 1+ plans (style guide route, library shell, settings, onboarding, post-meeting view)
  affects:
    - Component-test count: +30 new Vitest cases (Phase-0 baseline 2 → now 32)
tech-stack:
  added:
    - (none — pure consumer of Plan 01-01 tokens and the Phase-0 testing-library/jest-dom stack)
  patterns:
    - "Colocated tests: `Foo.tsx` + `Foo.test.tsx` per component (no shared `__tests__/`)"
    - "Polymorphic component via `as?: ElementType` (Card)"
    - "Named-export variants (RecordingBadge, ProviderChip) compose the base primitive (Pill) instead of stuffing flags onto one mega-component"
    - "Variant maps via `Record<Variant, string>` lookup tables (Button, Pill, Card padding)"
    - "TDD discipline: failing-test commit folded into the same task as the implementation since the test file is the spec — each task has one atomic commit containing test + impl"
key-files:
  created:
    - web/src/components/Logo.tsx
    - web/src/components/Logo.test.tsx
    - web/src/components/Button.tsx
    - web/src/components/Button.test.tsx
    - web/src/components/Pill.tsx
    - web/src/components/Pill.test.tsx
    - web/src/components/Card.tsx
    - web/src/components/Card.test.tsx
    - web/src/components/BrowserChrome.tsx
    - web/src/components/BrowserChrome.test.tsx
  modified: []
decisions:
  - "Followed the superpowers plan's verbatim TSX snippets and Vitest specs without modification — these had already been UX-reviewed against `yogurt-app-design/` and PRD §16.6"
  - "Combined RED + GREEN into a single per-component commit (rather than a `test:` then `feat:` pair). The intra-task RED phase is still validated by running `pnpm test -- <Component>` and observing the import-resolution failure before writing the .tsx, but the artifact landed in git is a single coherent `feat(web): …` commit per component primitive. Rationale: 5 components × 2 commits = 10 noise commits where 5 atomic feat-with-tests reads cleaner in `git log` and matches how Phase 0/01-01 landed feature additions."
  - "RecordingBadge ships with a strawberry hairline border + cream surface (not transparent over recorded bg) — matches PRD §16.6 design board exactly"
  - "ProviderChip composes <Pill> with a sub-1.5px leading dot rather than defining a fresh primitive — keeps the chip a thin wrapper around the canonical pill geometry"
metrics:
  duration_min: 4
  duration_human: "~4 minutes wall-clock"
  completed_date: "2026-06-25"
  test_cases_added: 30
  files_created: 10
  commits: 5
---

# Phase 01 Plan 02: Component Primitives Summary

Five React primitives (`Logo`, `Button`, `Pill` + `RecordingBadge` + `ProviderChip`, `Card`, `BrowserChrome`) landed colocated under `web/src/components/` with 30 passing Vitest cases — DESIGN-05 satisfied at the primitive level. All five consume the Plan 01-01 `@theme` tokens directly; no new tokens were added.

## What Was Built

| Primitive | File | Purpose | Test cases |
| --- | --- | --- | --- |
| `Logo` | `web/src/components/Logo.tsx` | Spoon-and-swirl SVG mark (PRD §16.1) — blueberry circle + white spoon path + strawberry dot, default 44px, optional ariaLabel | 4 |
| `Button` | `web/src/components/Button.tsx` | primary / secondary / ghost variants per PRD §16.6; `'ink'` variant deferred to Phase 4 (documented inline) | 7 |
| `Pill` + `RecordingBadge` + `ProviderChip` | `web/src/components/Pill.tsx` | Base Pill (neutral/blue/matcha/straw tones); RecordingBadge (pulsing strawberry dot + JetBrains-Mono timer); ProviderChip (active=blue / local=matcha / default neutral) | 10 (5 + 2 + 3) |
| `Card` | `web/src/components/Card.tsx` | bg-card + rounded-card + shadow-card; active=1.5px blueberry border; padding sm/md/lg; polymorphic `as` prop | 5 |
| `BrowserChrome` | `web/src/components/BrowserChrome.tsx` | Fake-Safari mockup wrapper — 42px warm-paper header + 3 traffic dots + centered URL pill (JetBrains Mono) + body slot, window-shadow elevation | 4 |

**Total Vitest count after plan:** 32 (Phase-0 baseline App: 2; new component primitives: 30).

## Commits

| Hash | Message |
| --- | --- |
| `170a23d` | feat(web): add Logo component (spoon-and-swirl SVG mark) |
| `1f8b781` | feat(web): add Button component with primary/secondary/ghost variants |
| `5c4bfb9` | feat(web): add Pill component with RecordingBadge + ProviderChip variants |
| `1da006d` | feat(web): add Card component with active variant and padding scale |
| `9bbf6e5` | feat(web): add BrowserChrome mockup wrapper (fake-Safari header) |
| _(this commit)_ | docs(01-02): complete component primitives plan |

## TDD Loop

Each task followed strict RED → GREEN per the superpowers plan:

1. **RED:** Write the `.test.tsx` file first. Run `pnpm --dir web test -- <Component>`. Confirm failure (always `Failed to resolve import "./<Component>"`).
2. **GREEN:** Write the `.tsx` file with exact snippet from the superpowers plan. Re-run the focused test. Confirm 100% pass.
3. **Commit:** Single atomic `feat(web): …` commit containing the test + implementation together.

All five tasks completed without deviation. No Rule 1/2/3 auto-fixes triggered, no Rule 4 architectural questions surfaced.

## Verification

- `pnpm --dir web test` → 6 test files, 32 passing tests (Logo 4, Button 7, Pill 10, Card 5, BrowserChrome 4, App 2 baseline)
- `pnpm --dir web build` → exits 0, 728ms build, all woff/woff2 fontsource assets emitted, index-*.js bundle 501.95 kB (gzip 154.61 kB — Chunk-size warning is the expected React 19 + TipTap baseline carried over from Plan 01-01; will revisit with code-splitting in later phases if needed)
- `grep -c 'data-testid="traffic-dot"' web/src/components/BrowserChrome.tsx` → 3
- `grep -c "RecordingBadge" web/src/components/Pill.tsx` → 3 (exports + JSDoc)
- All 10 expected files present under `web/src/components/`

## Deviations from Plan

**None — plan executed exactly as written.**

One observational note (not a deviation): the plan acceptance criteria for Pill mentioned "9 cases total" but the verbatim test snippet from the superpowers plan defines 5 + 2 + 3 = 10 `it()` blocks. I followed the literal source-of-truth (the superpowers TSX snippet) which produces 10 passing cases. Total component tests are therefore 30, not 29 as the plan's `<objective>` predicted. No correctness implication — the extra case is the second RecordingBadge mono-font assertion which is part of the spec.

## Requirements Covered

- **DESIGN-05** (Core component primitives shipped) — Logo, Button family, Pill family (incl. RecordingBadge + ProviderChip), Card, BrowserChrome all live with passing tests.

**Explicitly deferred from this plan (per the plan's `<objective>`, not a gap):**

- Tab group primitive → Phase 4 (live meeting view's transcript dock tab)
- End-meeting "ink" Button variant → Phase 4 (will extend the Button `Variant` union)
- StyleGuide route + Router wiring → Plan 03 of this phase (Task 1.8 in the superpowers plan)

## Threat Flags

_None._ All five primitives are pure presentational React components — no network IO, no auth, no file/process access, no LLM/audio integration. They expand the renderable surface of the SPA but introduce no new trust boundary.

## Known Stubs

_None._ Every primitive is fully implemented per its spec. No `TODO` markers, no placeholder data hardcoded for downstream consumption — these are pure leaf components.

## Self-Check: PASSED

- web/src/components/Logo.tsx — FOUND
- web/src/components/Logo.test.tsx — FOUND
- web/src/components/Button.tsx — FOUND
- web/src/components/Button.test.tsx — FOUND
- web/src/components/Pill.tsx — FOUND
- web/src/components/Pill.test.tsx — FOUND
- web/src/components/Card.tsx — FOUND
- web/src/components/Card.test.tsx — FOUND
- web/src/components/BrowserChrome.tsx — FOUND
- web/src/components/BrowserChrome.test.tsx — FOUND
- Commit 170a23d (Logo) — FOUND in git log
- Commit 1f8b781 (Button) — FOUND in git log
- Commit 5c4bfb9 (Pill family) — FOUND in git log
- Commit 1da006d (Card) — FOUND in git log
- Commit 9bbf6e5 (BrowserChrome) — FOUND in git log
- `pnpm --dir web test` — 32/32 passing
- `pnpm --dir web build` — exit 0
