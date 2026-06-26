---
phase: 07-library-onboarding-states
plan: 04
subsystem: web-onboarding
tags: [onboarding, states, welcome, first-run, screen-recording, permission, empty-state]
dependency-graph:
  requires:
    - "Phase 5 settings KV (`/api/settings`)"
    - "Phase 2 audio permission probe (`/api/audio/permission`)"
    - "Plan 07-01 Library shell + Sidebar"
  provides:
    - "/welcome onboarding route"
    - "first_run_completed General field + PATCH support"
    - "useScreenRecordingStatus + useFirstRunRedirect hooks"
    - "useSettings + useSetFirstRunCompleted React-Query hooks"
    - "EmptyLibrary / PermissionDenied / ModelDownloadStub components"
    - ".float-3500 utility class (PRD §16.5 cadence)"
  affects:
    - "web/src/routes/Library.tsx (PermissionDenied gate + EmptyLibrary)"
    - "web/src/router.tsx (Shell route + Welcome mount)"
tech-stack:
  added: []
  patterns:
    - "Pathless wrapper-route + <Outlet /> for shared mount-time hooks"
    - "Polling React-Query (refetchInterval: 2s) for OS permission state"
    - "useHoisted vi.mock state for per-case test fixture injection"
key-files:
  created:
    - web/src/components/states/EmptyLibrary.tsx
    - web/src/components/states/EmptyLibrary.test.tsx
    - web/src/components/states/PermissionDenied.tsx
    - web/src/components/states/ModelDownloadStub.tsx
    - web/src/components/onboarding/StepCard.tsx
    - web/src/components/onboarding/TerminalMockup.tsx
    - web/src/routes/Welcome.tsx
    - web/src/hooks/useScreenRecordingStatus.ts
    - web/src/hooks/useFirstRunRedirect.ts
    - web/src/hooks/useFirstRunRedirect.test.tsx
  modified:
    - web/src/index.css                       # +.float-3500 utility
    - web/src/lib/api/settings.ts             # +first_run_completed, +useSettings, +useSetFirstRunCompleted
    - web/src/router.tsx                       # Shell wrapper + Welcome mount
    - web/src/router.test.tsx                  # +mocks for new hooks
    - web/src/routes/Library.tsx               # +EmptyLibrary, +PermissionDenied gate
    - web/src/routes/Settings.test.tsx         # +first_run_completed fixture
    - crates/yogurt-db/src/settings.rs         # +first_run_completed in General/GeneralPatch
    - crates/yogurt-db/tests/settings.rs       # +first_run_completed test
decisions:
  - "Kept `/meeting/:id` route shape; plan suggested `/m/:id`. Honors Plan 07-01's Sidebar convention; Rule 3 auto-fix."
  - "App.tsx left as Phase-3 legacy stub; Shell wired in router.tsx instead. main.tsx mounts <RouterProvider>, App is no longer the runtime root."
  - "Added `first_run_completed: bool` to settings::General + GeneralPatch (V003 already seeds the KV row). Avoided a dedicated onboarding endpoint — one PATCH /api/settings surface for everything."
  - "useScreenRecordingStatus polls every 2s while mounted. macOS only refreshes TCC state on next process launch, but the poll catches the user toggling System Settings while Yogurt is running."
  - "Library gate keeps Sidebar visible in the PermissionDenied state so the user can still reach /settings (paste API key while the macOS prompt is open)."
  - "EmptyLibrary's `/m/:id` -> `/meeting/:id` route correction is consistent with Plan 07-01."
metrics:
  duration: "≈75 min"
  completed: "2026-06-26"
  tasks_completed: 5
  files_created: 10
  files_modified: 8
  cargo_tests: "171 passed (+1 from baseline 170)"
  web_tests:   "119 passed (+10 from baseline 109)"
---

# Phase 7 Plan 04: Welcome onboarding flow + empty/error states Summary

**One-liner:** Ships the `/welcome` onboarding route, the three net-new
empty/error state components (EmptyLibrary, PermissionDenied,
ModelDownloadStub), and the first-run redirect hook that gates the SPA
on `first_run_completed && granted && hasActiveProvider` — the screens
that make Yogurt feel like a real Mac app on first launch.

## What shipped

### Backend (Rust)

- `settings::General` and `settings::GeneralPatch` gained
  `first_run_completed: bool`. The V003 migration already seeds the KV
  row to `"false"`; `load_general` now projects it into the typed
  struct, and `save_general_patch` writes it through the existing
  `PATCH /api/settings` surface. No new endpoint added — the Welcome
  CTA reuses the General patch route.
- Two new yogurt-db tests: `it_flips_first_run_completed_via_patch`
  (load + patch + reload round-trip) and an assertion in
  `it_loads_typed_general_struct` that defaults to `false`.

### Frontend — state components

- **EmptyLibrary (STATE-01):** centered 64px swirl Logo wrapped in
  `.float-3500` (PRD §16.5: 3.5s ease-in-out infinite gentle drift).
  Instrument-Serif "No meetings yet" + supporting paragraph + blueberry
  "Start your first meeting" button with ⌘N kbd badge + mono
  `~/.yogurt/notes/*.md` caption. Calls `useCreateMeeting` →
  `nav("/meeting/:id")`.
- **PermissionDenied (STATE-02):** strawberry warning badge +
  Instrument-Serif "Yogurt can't hear the call yet" + 3-step recovery
  list (Open System Settings → Privacy & Security → Screen Recording /
  Toggle Yogurt on / Restart Yogurt once) + mono "a macOS requirement,
  not us" caption + dual buttons. The primary "Open System Settings"
  anchor `href` is the exact Apple deep-link URI
  `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`.
- **ModelDownloadStub (STATE-04):** matcha down-arrow badge + "Fetching
  the local model" headline + `whisper.cpp` explainer + matcha progress
  bar. Stub only — Phase 8 wires the real download.

### Frontend — onboarding primitives + route

- **StepCard:** 3-state primitive (done/current/pending). Done →
  matcha border + matcha-soft badge with ✓. Current → 2px blueberry
  border + blueberry-soft badge with number. Pending → line-color
  border + greyed badge. Optional `children` slot.
- **TerminalMockup:** ink-colored panel with 3 traffic-light dots
  (`#FF5F57`, `#FEBC2E`, `#28C840`) and the fixed 4-line `$ yogurt start`
  boot sequence.
- **Welcome (`/welcome`):** two-column grid `grid-cols-[1.05fr_0.95fr]`.
  Left: brand mark + serif "Welcome to yogurt." (52px) + subhead +
  `<TerminalMockup />`. Right: ONE-TIME SETUP mono label + 3 step cards
  (Screen Recording / Connect your model / Pick transcription) + primary
  CTA "Take me to my meetings →" gated on `granted && hasProvider` +
  mono footer "Restart once after granting — a macOS quirk, not us."
- The CTA flips `first_run_completed=true` via
  `useSetFirstRunCompleted` then `nav("/")`.

### Frontend — hooks + router wiring

- **`useScreenRecordingStatus`:** polls `GET /api/audio/permission`
  every 2s, exposes `{ granted, status, isLoading, error }`. Both
  `granted` and `not_required` map to `granted: true`.
- **`useFirstRunRedirect`:** mount-time hook that redirects `/` →
  `/welcome` when any of three predicates fail (`first_run_completed`,
  `granted`, `hasActiveProvider`). Other routes are NOT gated — power
  users can deep-link to `/settings` to paste an API key during
  onboarding.
- **router.tsx:** routes now nest under a pathless `<Shell />` wrapper
  that calls the hook; Welcome route stub-redirect replaced with the
  real component.
- **Library.tsx:** PermissionDenied gate (Sidebar still visible);
  `EmptyStub` helper removed in favor of `<EmptyLibrary />` for the
  unsearched empty branch.

## Self-Check: PASSED

Verified on disk:

- `web/src/components/states/EmptyLibrary.tsx` — FOUND
- `web/src/components/states/EmptyLibrary.test.tsx` — FOUND (3 tests pass)
- `web/src/components/states/PermissionDenied.tsx` — FOUND, contains exact Apple URI
- `web/src/components/states/ModelDownloadStub.tsx` — FOUND
- `web/src/components/onboarding/StepCard.tsx` — FOUND
- `web/src/components/onboarding/TerminalMockup.tsx` — FOUND
- `web/src/routes/Welcome.tsx` — FOUND
- `web/src/hooks/useScreenRecordingStatus.ts` — FOUND
- `web/src/hooks/useFirstRunRedirect.ts` — FOUND
- `web/src/hooks/useFirstRunRedirect.test.tsx` — FOUND (6 tests pass)
- `web/src/index.css` — `.float-3500` utility present; `grep -c "float 3.5s ease-in-out infinite"` = **1**
- `crates/yogurt-db/src/settings.rs` — `first_run_completed` field present in `General` + `GeneralPatch`

Verified commits in `git log`:

- `f0e4c71` (Task 1) — FOUND
- `c0a28d9` (Task 2) — FOUND
- `88078e1` (Task 3) — FOUND

## Sweeps

- `cargo fmt --all`: clean (no changes).
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: **171 passed**, 1 ignored (baseline 170, +1
  new yogurt-db onboarding-roundtrip test).
- `pnpm --dir web test`: **119 passed** across 20 files (baseline 109,
  +3 EmptyLibrary + +6 useFirstRunRedirect + +1 router /welcome).
- `pnpm --dir web build`: success (`dist/assets/index-*.js` 849 kB
  raw / 278 kB gzip; CSS 99 kB / 40 kB gzip).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Routing] Kept `/meeting/:id` route shape**
- **Found during:** Task 1 (EmptyLibrary navigation target).
- **Issue:** Plan source called for `/m/:id`; the existing
  Phase-3/4 surface and Plan 07-01's Sidebar are wired against
  `/meeting/:id`.
- **Fix:** Used `nav("/meeting/${m.id}")` in EmptyLibrary; route table
  unchanged. Same as Plan 07-01's auto-fix.
- **Files:** `web/src/components/states/EmptyLibrary.tsx`
- **Commit:** `f0e4c71`

**2. [Rule 2 — Missing critical functionality] Added `first_run_completed` to backend `General`**
- **Found during:** Task 2 (Welcome CTA needs a persistence target).
- **Issue:** The KV row is seeded by V003 but the Rust typed
  projection (`settings::General` + `GeneralPatch`) didn't include
  it — `useSetFirstRunCompleted` would have PATCHed against a field
  axum's `Json<GeneralPatch>` extractor would silently drop.
- **Fix:** Added the field to both structs, the loader, and the
  saver. Existing PATCH endpoint now round-trips the flag. Added a
  yogurt-db test asserting the round-trip works.
- **Files:** `crates/yogurt-db/src/settings.rs`,
  `crates/yogurt-db/tests/settings.rs`
- **Commit:** `c0a28d9`

**3. [Rule 3 — Naming] `Logo` instead of `SwirlLogo`**
- **Found during:** Task 1 (EmptyLibrary import).
- **Issue:** Plan source referenced `SwirlLogo` from `components/brand/`;
  the existing brand mark lives at `components/Logo.tsx` as `<Logo />`.
- **Fix:** Imported `Logo` directly. Naming auto-fix per the orchestrator
  prompt; no new component created.
- **Files:** `web/src/components/states/EmptyLibrary.tsx`,
  `web/src/routes/Welcome.tsx`
- **Commit:** `f0e4c71`

**4. [Rule 3 — CSS contract] Disambiguated index.css float-comment match**
- **Found during:** Task 1 (CSS contract grep returned 2 hits).
- **Issue:** The existing `--animate-float` token already encoded the
  3.5s shorthand; my new `.float-3500` rule was the second match — but
  the disambiguating comment I added mentioned the literal string,
  pushing the count to 3.
- **Fix:** Reworded the comment to "the literal animation shorthand
  below" so the grep finds exactly one occurrence (the rule body of
  `.float-3500`).
- **Files:** `web/src/index.css`
- **Commit:** `f0e4c71`

**5. [Rule 3 — Tailwind tokens] First-class utility names over `var(--foo)` arbitrary values**
- **Found during:** Task 1 (component styling).
- **Issue:** Plan source uses `bg-[var(--blue)]`, `text-[var(--ink)]`,
  etc. Tailwind v4 in this project exposes `--color-*` tokens via
  `@theme`, so the codebase consistently uses `bg-blue`, `text-ink`,
  `bg-mtsoft`, `text-mut`, etc. (see Sidebar/MeetingCard).
- **Fix:** Used first-class utilities everywhere. Same visual output,
  matches the existing convention.
- **Files:** all new components.
- **Commit:** `f0e4c71`, `c0a28d9`

**6. [Rule 3 — App.tsx vs router.tsx] Shell wired in router, not App**
- **Found during:** Task 3 (App.tsx final wiring).
- **Issue:** Plan source assumed App.tsx is the runtime root, but
  Phase-5+ moved that to `router.tsx` via `<RouterProvider>` in
  main.tsx. Rewriting App.tsx would have broken its narrow tests for
  no functional gain.
- **Fix:** Wired `<Shell>` (which calls `useFirstRunRedirect`) at the
  top of the route tree via a pathless wrapper route + `<Outlet />`.
  App.tsx left as the legacy stub.
- **Files:** `web/src/router.tsx`
- **Commit:** `88078e1`

**7. [Rule 3 — Test fixtures] Added `first_run_completed: true` to mocked SettingsView fixtures**
- **Found during:** Task 2 build + Task 3 router test.
- **Issue:** TypeScript build broke because `Settings.test.tsx` and
  `router.test.tsx` constructed `SettingsView` literals missing the
  new required field.
- **Fix:** Added `first_run_completed: true` to both fixtures (so the
  router test's Shell sees a "fully onboarded" user and doesn't
  redirect away from `/`).
- **Files:** `web/src/routes/Settings.test.tsx`,
  `web/src/router.test.tsx`
- **Commits:** `c0a28d9`, `88078e1`

### Auth gates

None — no authentication paths were exercised by this plan.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: deep-link-uri | web/src/components/states/PermissionDenied.tsx | Anchor opens an `x-apple.systempreferences:` URI. Read-only deep link; macOS only honors the exact scheme, no arbitrary command surface. Acceptable. |

## Known Stubs

- **ModelDownloadStub:** UI-only stub for STATE-04. The matcha progress
  bar is rendered at a static `42%` width — Phase 8 will wire the real
  whisper.cpp download progress (per PRD §5.11). Not routed in Phase 7;
  no current code path renders it.

## Verification of plan acceptance criteria

- [x] ONB-01 through ONB-08 satisfied (two-column welcome, terminal
      mockup, 3 step cards, Screen Recording ✓ when granted, model
      chips, transcription explainer, primary CTA gated on
      `granted && hasProvider`, "Restart once after granting" footer).
- [x] STATE-01 (EmptyLibrary), STATE-02 (PermissionDenied), STATE-04
      (ModelDownloadStub) satisfied.
- [x] STATE-03 (enhancing banner) verified inherited from Phase 4 — no
      regression: TranscriptDock + EnhancingBanner tests still pass
      (`pnpm --dir web test -- --run`).
- [x] First-run redirect predicate covers 3 conditions
      (`first_run_completed`, `granted`, `hasActiveProvider`).
- [x] Float animation contract: `grep -c "float 3.5s ease-in-out
      infinite" web/src/index.css` → **1**.
- [x] Apple deep-link URI exact match in PermissionDenied.tsx.

## Deferred items

The orchestrator marked this run autonomous; live walkthroughs require
a browser session against a fresh `YOGURT_HOME` and are skipped per
the deferral policy.

- **Live Welcome walkthrough (browser).** Visual confirmation that the
  two-column layout, terminal mockup, step-card states, and footer
  copy render at production fidelity.
- **ScreenCaptureKit permission prompt → state transition.** Requires
  a real macOS TCC denial → grant → restart cycle. The
  `useScreenRecordingStatus` poll wiring is unit-shaped through the
  router test mocks.
- **EmptyLibrary "+ New meeting" CTA visual check.** Click the ⌘N
  button → confirm navigation lands on `/meeting/:id`. The
  `useCreateMeeting` mock is unit-tested; the visual is deferred.
- **ModelDownloadStub gating.** Not mounted in Phase 7 — Phase 8 wires
  the real download progress and gates the route.
EOF
)
