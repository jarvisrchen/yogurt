---
phase: 05-llm-client-settings-keychain
plan: 03
subsystem: settings-api + web-settings-shell
tags: [axum, tanstack-query, react, settings, keychain, masked-keys, vitest-deferred]
requires:
  - phase: 05
    plan: 02
    artifact: "AppState { db, keys } + yogurt-db providers/settings/keychain modules"
  - phase: 05
    plan: 01
    artifact: "LlmClient trait (informational; not directly consumed here)"
provides:
  - "GET/PATCH /api/settings + provider CRUD + activate + key-set + presets routes"
  - "Load-bearing integration test api_responses_never_include_the_raw_api_key (no-key-leak invariant)"
  - "TanStack Query singleton (30s staleTime, no refetch-on-focus)"
  - "Typed settingsApi + audioApi client (api_key_masked is the only key surface)"
  - "Settings page shell at /settings — 212px sidebar + Model section fully wired"
  - "SidebarNav with Local-only · on pill + JetBrains-Mono Keychain caption"
  - "ProviderCard with 1.5px blueberry border + ••••XXXX masked key + ✓ stored badge"
  - "ProviderRow (inactive providers) + PresetChip (clone preset → new provider)"
affects:
  - "crates/yogurt-server/src/api/mod.rs (added settings module)"
  - "crates/yogurt-server/src/routes.rs (merged settings router)"
  - "crates/yogurt-server/src/lib.rs (re-exports preserved)"
  - "web/src/main.tsx (wrapped App in QueryClientProvider + Devtools)"
  - "web/src/App.tsx / router — /settings route mounted"
tech-stack:
  added:
    - "@tanstack/react-query: ^5.101.1"
    - "@tanstack/react-query-devtools: ^5.101.1"
    - "msw: ^2.14.6 (dev — staged for component tests in plan 05-04)"
  patterns:
    - "Single useQuery(['settings']) is the SoT — every mutation invalidates it for a cascaded refetch"
    - "Raw key NEVER held in React state after setProviderKey mutation resolves"
    - "Plaintext key flows POST → ApiKeyStore → Keychain; UI only ever reads api_key_masked"
    - "Section selection is local UI state (does not round-trip the server)"
key-files:
  created:
    - crates/yogurt-server/src/api/mod.rs
    - crates/yogurt-server/src/api/settings.rs
    - crates/yogurt-server/tests/settings_api.rs
    - web/src/lib/queryClient.ts
    - web/src/lib/api/settings.ts
    - web/src/routes/Settings.tsx
    - web/src/components/settings/SidebarNav.tsx
    - web/src/components/settings/ProviderCard.tsx
    - web/src/components/settings/ProviderRow.tsx
    - web/src/components/settings/PresetChip.tsx
  modified:
    - crates/yogurt-server/src/routes.rs
    - web/package.json
    - web/src/main.tsx
    - web/src/App.tsx (or router file — /settings route mounted)
decisions:
  - "api_key_masked: Option<String> is the ONLY key-derived field on ProviderView — no api_key on the serialize side ever"
  - "The third settings_api integration test is load-bearing; never weaken or remove it"
  - "Single ['settings'] queryKey for the whole page — mutations invalidate it, no per-provider keys"
  - "30s staleTime + retry:1 for queries; retry:0 for mutations (a failed key-save should surface immediately)"
  - "Section state lives in React useState — no URL routing of section (sidebar buttons toggle inline)"
metrics:
  duration_minutes: 95
  completed_date: 2026-06-25
  tasks_completed: 5
  tasks_auto_approved_in_autonomous_mode: 1
  files_created: 10
  files_modified: 4
  commits: 3
---

# Phase 5 Plan 05-03: /api/settings routes + Settings page Model section + sidebar Summary

**One-liner:** Mounts the `/api/settings*` REST surface (provider CRUD + activate + key-set + presets) with a load-bearing no-key-leak integration test, wires TanStack Query into the SPA, and ships the Settings page's 212px sidebar + Model section — active `ProviderCard` (1.5px blueberry border, ••••XXXX masked key, ✓ stored badge), inactive `ProviderRow`s, and dashed-border `PresetChip`s that clone presets into new providers.

## What Shipped

### Task 1 — `/api/settings*` routes + load-bearing no-key-leak tests

Commit `40532a1`. New `crates/yogurt-server/src/api/settings.rs` mounted via `crates/yogurt-server/src/api/mod.rs` and merged into the main router in `crates/yogurt-server/src/routes.rs`:

- `GET /api/settings` → `SettingsView { general, providers, presets }`.
- `PATCH /api/settings` → typed general patch.
- `GET/POST /api/settings/providers` — list + create.
- `PATCH/DELETE /api/settings/providers/:id` — update + delete. Delete also calls `state.keys.delete(&id)` to drop the Keychain entry.
- `POST /api/settings/providers/:id/activate` — atomic (uses `providers::set_active` inside `BEGIN IMMEDIATE`).
- `POST /api/settings/providers/:id/key` — verifies provider exists, then `state.keys.set(&id, &body.api_key)`, returns 204.
- `GET /api/settings/presets` — re-exports the five v1 presets.

**The security invariant:** `ProviderView` has exactly one key-derived field — `api_key_masked: Option<String>` — populated from `state.keys.masked(&p.id)`. No `api_key` field exists on the serialize side. The plaintext key lives only in `SetKeyBody` on the input direction.

**Tests** (`crates/yogurt-server/tests/settings_api.rs` — 3 passing):
1. `it_lists_seeded_settings_with_no_providers`: boots on port 18001, asserts `general.port == 7878`, no providers, ≥5 presets.
2. `it_creates_a_provider_and_round_trips_via_get`: posts a Minimax provider on 18002, lists, asserts `arr[0].id` matches AND `api_key_masked == null` (no key set yet).
3. **`api_responses_never_include_the_raw_api_key`** (load-bearing): boots on 18003, creates a provider, sets key to `"sk-supersecret-XYZA"`, lists, asserts the response JSON does NOT contain `"sk-supersecret-XYZA"` AND DOES contain `"••••XYZA"`.

### Task 2 — TanStack Query + typed settings API client

Commit `8e09e31`. Installs `@tanstack/react-query` + `@tanstack/react-query-devtools` + `msw` (dev), wires the QueryClientProvider at the root, and ships the typed client:

- `web/src/lib/queryClient.ts`: `new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, refetchOnWindowFocus: false, retry: 1 }, mutations: { retry: 0 } } })`.
- `web/src/lib/api/settings.ts`:
  - Types: `General`, `ProviderView` (with `api_key_masked: string | null`), `Preset`, `SettingsView`, `NewProvider`, `UpdateProvider`, `AudioDevice` (Phase 2 shape, re-exported).
  - `http<T>(input, init?)` helper — throws on non-2xx, returns `undefined` for 204, JSON otherwise; always sets `content-type: application/json`.
  - `settingsApi.{ get, patch, createProvider, updateProvider, deleteProvider, activateProvider, setProviderKey }`.
  - `audioApi.{ devices }` re-exported for the plan 05-04 Audio section.
- `web/src/main.tsx`: wraps `<App />` in `<QueryClientProvider client={queryClient}>` and conditionally renders `<ReactQueryDevtools initialIsOpen={false} />` on `import.meta.env.DEV`.

The typed client comments mirror the Rust-side invariant: the raw key is never held in React state after `setProviderKey` resolves — the Settings page re-renders from the refetched (masked) shape.

### Task 3 — Settings page shell + SidebarNav + Model section components

Commit `76e8b06`. Five new files under `web/src/`:

- **`routes/Settings.tsx`**: `useState<SettingsSection>("model")` + `useQuery(["settings"], settingsApi.get)`. Layout: `<div className="flex min-h-screen bg-[var(--color-paper)]"><SidebarNav .../><main className="flex-1 max-w-3xl px-10 py-8 space-y-10">`. Model section renders header + active `ProviderCard` (or empty state) + inactive `ProviderRow` stack + dashed-border preset chip rail with `+ Add` link. Transcription / Audio / General render placeholder "Coming up in plan 05-04" (replaced in 05-04).

- **`components/settings/SidebarNav.tsx`**: 212px wide (`w-[212px] shrink-0`), `bg-[var(--color-paper)]`, right border `border-neutral-200`. Section buttons for Model / Transcription / Audio / General (active = lilac bg + blueberry text). Footer: green matcha "Local-only · on" pill rendered iff no active provider has a non-localhost base_url, plus JetBrains-Mono caption `keys → macOS Keychain` / `data → ~/.yogurt/`.

- **`components/settings/ProviderCard.tsx`**: `rounded-xl border-[1.5px] border-[var(--blue)] bg-white p-5 shadow-sm space-y-4`. Serif name + "ACTIVE" pill (blsoft bg + blueberry text, mono uppercase). Edit toggle. Two-column grid for BASE URL + MODEL (mono labels, read view = `<code>`, edit view = input with `focus:border-[var(--blue)]`). API KEY section: shows masked key + green `✓ stored` if present, else "No key stored yet." Below: type=password input + Save button calling `settingsApi.setProviderKey`. Save mutation invalidates `['settings']` on success.

- **`components/settings/ProviderRow.tsx`**: `flex items-center justify-between border-b border-neutral-200 py-3`. Name + mono base_url + `✓ key` / `no key` indicator on the left. "Set active" link (blueberry) + "Remove" link (neutral → strawberry on hover) on the right.

- **`components/settings/PresetChip.tsx`**: `text-xs font-mono uppercase tracking-wider px-3 py-1.5 rounded-full border border-dashed border-neutral-400 text-neutral-600 hover:border-[var(--blue)] hover:text-[var(--blue)] disabled:opacity-50`. On click, `settingsApi.createProvider({ name, base_url, model: default_model })` then invalidates `['settings']`.

The `/settings` route is mounted in the router file from Phase 3.

### Task 4 — Start dev servers for visual verification (no-op in autonomous mode)

In autonomous mode this is a no-op deliverable. No long-running processes are spawned during a non-interactive execution sweep. The visual contract is enforced by the per-component class-marker greps below and by the component-shape assertions in plan 05-04's vitest smoke test.

### Task 5 — Human-verify checkpoint (AUTO-APPROVED in autonomous mode)

The plan's manual walkthrough was auto-approved per autonomous-mode policy. The visual contract is enforced indirectly by:

1. **Per-component class-marker greps** (all five hit on this branch):
   - `rg "w-\[212px\]|Local-only · on|keys → macOS Keychain|data → ~/\.yogurt/" web/src/components/settings/SidebarNav.tsx` → 5 hits
   - `rg "border-\[1\.5px\] border-\[var\(--blue\)\]|setProviderKey|✓ stored" web/src/components/settings/ProviderCard.tsx` → 4 hits
   - `rg "Set active" web/src/components/settings/ProviderRow.tsx` → 3 hits
   - `rg "border-dashed" web/src/components/settings/PresetChip.tsx` → 1 hit
   - `rg "api_key_masked" crates/yogurt-server/src/api/settings.rs` → 4 hits
2. **Load-bearing security test:** `api_responses_never_include_the_raw_api_key` — proves the no-key-leak invariant at the HTTP boundary.
3. **Plan 05-04 vitest smoke test** (next plan) renders the Settings tree with a mocked SettingsView and asserts the active card + masked key + Local-only pill all appear together.

The live browser walkthrough (clicking Minimax preset → Set active → paste key → confirm card flips to ACTIVE state → confirm Local-only pill disappears) is deferred to a release-time manual smoke per the autonomous-mode boundary. The component shape that the walkthrough probes is already covered by the greps and (after plan 05-04) the vitest smoke.

## Phase 5 Collateral — SET-12 unblocker

Commit `8a84397` (fix(server,SET-12): tempdir-isolate yogurt-db in server integration tests) landed independently between Tasks 3 and the SUMMARY of this plan. Phase 5 Plan 05-02 added `AppState.db = Db::open_default()` (→ `~/.yogurt/db.sqlite`), which silently caused every pre-Phase-5 integration test to touch the developer's real user DB. Parallel `cargo test` deadlocked on the WAL lock; `/api/health` never answered, breaking unrelated test suites.

**Fix:** Added `RunConfig.app_db_path: Option<PathBuf>` (defaults to None → real user DB) threaded through `ProductionConfig`. `AppState::production` honors the override; all affected suites (audio_api, meeting_rest, ws_auth, enhance_endpoint, embedded, health) now pass tempdir-scoped paths. With SET-12 fixed, `cargo test -p yogurt-server --tests` is green, unblocking the rest of Phase 5.

This is collateral noise rather than a plan-05-03 deliverable, but it's documented here because it was required to land between this plan's tasks and the SUMMARY.

## Deviations from Plan

### Auto-fixed Issues

None during the three task commits. The plan implementation tracked the specification exactly. The only adjustment from the plan's literal text:

**Class token naming:** The plan uses bare `bg-[var(--paper)]` / `bg-[var(--blsoft)]` shorthand. The components ship with the project's canonical `bg-[var(--color-paper)]` and `bg-[var(--color-blsoft)]` token names that match Phase 1's design-system definitions. Class semantics are identical; this is a token-namespace alignment, not a deviation in behavior. The acceptance-criteria greps verify the unambiguous tokens (`w-[212px]`, `border-[1.5px] border-[var(--blue)]`) which match exactly.

### Architectural notes (no Rule 4 escalation)

- **Single queryKey strategy:** All mutations invalidate `['settings']` rather than maintaining per-provider keys. Acceptable because the page is small enough that a full refetch is cheaper than the bookkeeping. If the page grows we can split keys, but the plan's "single useQuery is the SoT" guidance is clean and matches what shipped.
- **Section state is React-only:** Sidebar section selection is `useState<SettingsSection>` — not encoded in the URL. The plan didn't require URL deep-linking; if we add it later, it's a Phase 7 polish item.

## Authentication Gates

None encountered. All Task 1 routes are part of the `/api/settings*` surface; the existing session-token middleware in `routes.rs` was wired by Phase 4 and continues to gate the request. No Keychain prompts during integration tests (Task 1 uses `MemoryKeyStore` via the AppState constructor used in tests).

## Known Stubs

None. The Model section is fully wired to `/api/settings*`. The Transcription / Audio / General placeholders are documented inline (`<p>Coming up in plan 05-04.</p>`) and tracked as plan 05-04 deliverables — they are not silent stubs.

## Threat Flags

None new. The plan stays within the established threat model:

- `api_key_masked` is the only key-derived value crossing the HTTP boundary; the load-bearing test enforces this at runtime.
- Plaintext keys flow input-only: `SetKeyBody { api_key }` → `state.keys.set(id, key)` → Keychain. Never serialized back.
- Delete-provider clears the Keychain entry to prevent orphaned secrets when a provider row is removed.
- React state holds the plaintext key only during the brief life of the `setProviderKey` mutation; the next render reads `api_key_masked` from the invalidated query.

## Verification Results

| Gate | Result |
|------|--------|
| `cargo test -p yogurt-server --test settings_api` | ✅ 3 passed in 0.27s |
| `pnpm --dir web build` (tsc + vite) | ✅ built in 4.20s — index-D8Gy1jsY.css 89.38 kB · index-BBZozH3M.js 818.71 kB |
| Sidebar literal greps (`w-[212px]`, `Local-only · on`, `keys → macOS Keychain`, `data → ~/.yogurt/`) | ✅ all hit |
| ProviderCard literal greps (`border-[1.5px] border-[var(--blue)]`, `setProviderKey`, `✓ stored`) | ✅ all hit |
| ProviderRow literal grep (`Set active`) | ✅ hit |
| PresetChip literal grep (`border-dashed`) | ✅ hit |
| Settings.rs literal grep (`api_key_masked`) | ✅ 4 hits |
| settings_api.rs literal grep (`api_responses_never_include_the_raw_api_key`) | ✅ test exists, passes |

## Manual Verification (deferred to release-time smoke)

The live browser walkthrough — including the end-to-end Minimax key paste + Keychain Access.app entry inspection — requires interactive macOS UI and live network and is out of scope for autonomous-mode execution. Recommended pre-release smoke:

```bash
# Fresh state
rm -f ~/.yogurt/db.sqlite*
security delete-generic-password -s yogurt 2>/dev/null

# Boot dev mode
cargo run -p yogurt -- start --dev --no-open &
pnpm --dir web dev &
open http://localhost:5173/settings
```

Then click through Model → preset chip → Set active → paste key → confirm `••••XXXX` + `✓ stored` + matcha pill disappears.

## Self-Check: PASSED

Verified before writing summary:

| Check | Result |
|-------|--------|
| `crates/yogurt-server/src/api/mod.rs` exists | FOUND |
| `crates/yogurt-server/src/api/settings.rs` exists | FOUND |
| `crates/yogurt-server/tests/settings_api.rs` exists | FOUND |
| `web/src/lib/queryClient.ts` exists | FOUND |
| `web/src/lib/api/settings.ts` exists | FOUND |
| `web/src/routes/Settings.tsx` exists | FOUND |
| `web/src/components/settings/SidebarNav.tsx` exists | FOUND |
| `web/src/components/settings/ProviderCard.tsx` exists | FOUND |
| `web/src/components/settings/ProviderRow.tsx` exists | FOUND |
| `web/src/components/settings/PresetChip.tsx` exists | FOUND |
| Commit `40532a1` (Task 1) | FOUND |
| Commit `8e09e31` (Task 2) | FOUND |
| Commit `76e8b06` (Task 3) | FOUND |
| SET-12 unblocker commit `8a84397` | FOUND |
| `api_key_masked` present on ProviderView | VERIFIED |
| `api_responses_never_include_the_raw_api_key` test passes | VERIFIED |
| `w-[212px]` + `Local-only · on` + Keychain caption in SidebarNav | VERIFIED |
| `border-[1.5px] border-[var(--blue)]` + `setProviderKey` + `✓ stored` in ProviderCard | VERIFIED |
