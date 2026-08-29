---
phase: 05-llm-client-settings-keychain
plan: 04
subsystem: settings-page-remaining-sections + smoke-test
tags: [react, tanstack-query, settings, transcription, audio, general, vitest, smoke]
requires:
  - phase: 05
    plan: 03
    artifact: "/api/settings* routes + Settings page Model section + sidebar"
provides:
  - "STTPicker (Transcription card pair: Cloud selected + Local 'Coming in v1')"
  - "AudioSection (input device dropdown wired to /api/audio/devices + PATCH persist)"
  - "GeneralSection (port 1024–65535 + open-browser-on-start, both PATCH persist)"
  - "Settings page now renders all four sections (Model/Transcription/Audio/General)"
  - "Vitest smoke test: Settings renders active card + masked key + Local-only pill"
affects:
  - "web/src/routes/Settings.tsx (replaces plan 05-03 placeholders with the three section components)"
  - "crates/yogurt-server/tests/embedded.rs (clippy doc fix, surfaced by Task 5 sweep)"
  - "crates/yogurt-cli/tests/cli.rs (HOME tempdir isolation, surfaced by Task 5 sweep)"
  - "crates/yogurt-cli/Cargo.toml (+ tempfile dev-dep)"
tech-stack:
  added: []
  patterns:
    - "useQuery(['audio-devices'], audioApi.devices) + useMutation calling settingsApi.patch — same pattern as ProviderCard's key-save mutation"
    - "defaultValue + onBlur/onChange for uncontrolled inputs that PATCH on commit (avoids a controlled-input refetch cascade)"
    - "vi.mock('../lib/api/settings') with a fixed SettingsView fixture for backend-free component tests"
key-files:
  created:
    - web/src/components/settings/STTPicker.tsx
    - web/src/components/settings/AudioSection.tsx
    - web/src/components/settings/GeneralSection.tsx
    - web/src/routes/Settings.test.tsx
  modified:
    - web/src/routes/Settings.tsx
    - crates/yogurt-server/tests/embedded.rs
    - crates/yogurt-cli/Cargo.toml
    - crates/yogurt-cli/tests/cli.rs
decisions:
  - "Smoke-test fixture uses Ollama (localhost) as the active provider so the Local-only pill renders — Minimax would suppress it (api.minimax.io is non-localhost). Active provider name 'Local Ollama' is distinct from the 'Ollama (local)' preset chip to keep getByText unambiguous."
  - "STTPicker is a visual-only contract for now: Local is opacity-60 with a 'Coming in v1' matcha badge; the actual STT routes land in Phase 8."
  - "Inputs are uncontrolled (defaultValue + onBlur/onChange) to avoid every keystroke triggering a PATCH + queryInvalidate cascade. Port only patches when the parsed value differs from current."
  - "Settings.tsx renders <AudioSection general={data.general} /> directly (no wrapping <section>) because AudioSection owns its own h2 header — matches GeneralSection symmetry."
metrics:
  duration_minutes: 45
  completed_date: 2026-06-25
  tasks_completed: 5
  tasks_auto_approved_in_autonomous_mode: 1
  task_no_op_in_autonomous_mode: 1
  files_created: 4
  files_modified: 4
  commits: 4
---

# Phase 5 Plan 05-04: Settings Transcription/Audio/General + smoke test + workspace green Summary

**One-liner:** Completes the Settings page with the three remaining sections — STTPicker (Cloud selected, Local "Coming in v1"), AudioSection (device dropdown wired to `/api/audio/devices` + PATCH persist), GeneralSection (port + open-browser-on-start, both PATCH persist) — adds a Vitest smoke test that proves the active card + masked key + Local-only pill render against a mocked SettingsView, and lands the Task 5 workspace-green sweep including two pre-existing SET-12 follow-up fixes (clippy doc-list lint in `embedded.rs`, HOME-tempdir isolation for subprocess-spawning CLI tests).

## What Shipped

### Task 1 — STTPicker + AudioSection + GeneralSection

Commit `6842000`. Three new components under `web/src/components/settings/`:

- **`STTPicker.tsx`**: two-column grid.
  - Cloud card: `rounded-xl border-[1.5px] border-[var(--blue)] bg-white p-5` with serif "Cloud" header + mono "Selected" badge (blueberry text). Sub-pills: Deepgram active (blsoft bg + blueberry text), AssemblyAI / Groq with neutral borders.
  - Local card: `rounded-xl border border-neutral-300 bg-neutral-50 p-5 opacity-60` with serif "Local · whisper.cpp" + matcha "Coming in v1" badge (`bg-[var(--matchasoft)] text-[var(--matcha)]`). Sub-pills: tiny.en / small.en / medium.en / large-v3 in mono neutral-400.
- **`AudioSection.tsx`**: `useQuery(['audio-devices'], audioApi.devices)` + `useMutation` calling `settingsApi.patch({ audio_input_device })`. Renders `<select>` with `<option value="">System default</option>` + `devices.data?.map(...)`. Bottom caption: "System audio is captured via ScreenCaptureKit — no extra setup."
- **`GeneralSection.tsx`**: serif "General" header. Port input `<input type="number" min={1024} max={65535}>` with `defaultValue={general.port}` and `onBlur` that PATCHes only if value parsed differently (no spurious writes on tab-out without edit). Checkbox `defaultChecked={general.open_browser_on_start}` with `onChange` PATCH. Caption: "Port change applies on next `yogurt start`."

`web/src/routes/Settings.tsx` modified to import the three components and replace the plan-05-03 placeholders. The Transcription branch wraps STTPicker in `<section className="space-y-4"><h2 className="font-serif text-[28px] leading-none">Transcription</h2><STTPicker /></section>`; AudioSection and GeneralSection render directly (they own their own h2 headers).

### Task 2 — Vitest smoke test

Commit `6557af0`. `web/src/routes/Settings.test.tsx`:

- `vi.mock('../lib/api/settings')` returns a fixed `SettingsView` with one active "Local Ollama" provider (`base_url: http://localhost:11434/v1`, `api_key_masked: "••••WXYZ"`), one inactive Minimax (no key), and one Ollama (local) preset chip.
- Renders `<Settings />` inside a `<QueryClientProvider>` with `retry: false`.
- Asserts:
  - `getByText("Local Ollama")` — the active card mounted with the provider name.
  - `getByText("••••WXYZ")` — the masked key surfaces in the card.
  - `getByText(/Local-only · on/)` — the matcha pill renders because the only active provider is localhost.
  - `queryByText(/sk-/)` is null — no raw `sk-` prefix anywhere in the tree.

**Fixture safety note:** the plan's original fixture suggestion mixed an active Minimax with the "Local-only · on" assertion, which would fail because Minimax is non-localhost. Per the plan's own WARNING block, the test ships with a localhost Ollama as the active provider, matching the SidebarNav pill semantics ("no active provider has a non-localhost base_url"). The active provider's name is "Local Ollama" — distinct from the "Ollama (local)" preset chip text — so `getByText` returns a single unambiguous match. Vitest infrastructure (`vitest`, `jsdom`, `@testing-library/react`, `@testing-library/jest-dom`) was already installed in `web/package.json` from a prior phase; no separate infra commit was needed.

### Task 3 — Start dev servers (no-op in autonomous mode)

Autonomous mode does not spawn long-running interactive servers. The functional contract that this task was meant to enable (visual + end-to-end) is enforced indirectly by Task 2's smoke test and by the class-marker greps in plan 05-04's `<acceptance_criteria>` blocks.

### Task 4 — Human-verify checkpoint (AUTO-APPROVED in autonomous mode)

The plan's manual walkthrough — clicking through all four sections, pasting a real Minimax key, opening Keychain Access.app to confirm the entry, triggering `/api/enhance` to verify a real Minimax response — was auto-approved per autonomous-mode policy. The auto-approval is justified by:

1. **Component-level class assertions** are enforced by `<acceptance_criteria>` greps:
   - `STTPicker.tsx` contains `Coming in v1` ✅ and `border-[1.5px] border-[var(--blue)]` ✅
   - `AudioSection.tsx` contains `audioApi.devices` ✅ and `audio_input_device` ✅
   - `GeneralSection.tsx` contains `open_browser_on_start` ✅, `min={1024}` ✅, `max={65535}` ✅
   - `Settings.tsx` references all three new components ✅
2. **The no-key-leak invariant** is enforced by plan 05-03's load-bearing integration test `api_responses_never_include_the_raw_api_key` — still passing.
3. **The Settings page renders correctly without a server** is enforced by Task 2's vitest smoke.

The deferred manual checks (live Minimax round-trip, Keychain Access.app entry inspection) are documented below under "Deferred / Manual Verification."

### Task 5 — Final format + lint + workspace test sweep

Commit `8b7a580` (Rule 3 auto-fix collateral). The sweep ran:

| Gate | Result |
|------|--------|
| `cargo fmt --all` | no changes |
| `cargo clippy --all-targets -- -D warnings` | ✅ clean (after the `embedded.rs` doc fix below) |
| `cargo test --workspace` | ✅ 143 passed, 1 ignored, 45 suites, 5.49s |
| `pnpm --dir web test` | ✅ 93 passed (14 files, 5.17s) |
| `pnpm --dir web build` | ✅ built in 1.56s |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking issue] clippy::doc_lazy_continuation in `crates/yogurt-server/tests/embedded.rs`**
- **Found during:** Task 5 `cargo clippy --all-targets -- -D warnings`
- **Issue:** SET-12's doc comment for the embedded test file had a wrapped bullet starting with `//! + ephemeral-port ...` which clippy parsed as a list continuation and demanded indented wraps. Indenting actually made it angrier (the indentation pushed clippy into wanting deeper nesting).
- **Fix:** Reflowed the bullet into a single paragraph (replaced `tempdir + ephemeral-port` with `tempdir and ephemeral-port`). Semantics preserved, list structure removed.
- **Files modified:** `crates/yogurt-server/tests/embedded.rs`
- **Commit:** `8b7a580`

**2. [Rule 3 — Blocking issue] CLI subprocess tests touched real `~/.yogurt/db.sqlite`**
- **Found during:** Task 5 `cargo test --workspace`
- **Issue:** Two of the four tests in `crates/yogurt-cli/tests/cli.rs` (`it_reports_port_conflict_with_friendly_error`, `it_does_not_suggest_port_0_at_upper_boundary`) spawned `yogurt start` as a subprocess. The subprocess opened `~/.yogurt/db.sqlite` (which on this developer's machine was at `user_version=2` from prior dev runs). The test binary's bundled migrations only know about V001, so the subprocess crashed with `MigrationDefinition(DatabaseTooFarAhead)` — masking the actual port-conflict assertion the tests are trying to make. SET-12 fixed the in-process integration tests via `RunConfig.app_db_path` but did NOT cover subprocess-spawning CLI tests (no `RunConfig` plumbing to the subprocess).
- **Fix:** Each subprocess-spawning CLI test now creates a `tempfile::tempdir()` and passes it as `HOME` to the spawned process. The `yogurt-db` paths module uses `directories::BaseDirs::home_dir()` which honors `$HOME`, so the subprocess creates a fresh `~/.yogurt/db.sqlite` (in the tempdir) that the bundled migrations can apply cleanly. Added `tempfile = { workspace = true }` to `crates/yogurt-cli/Cargo.toml` dev-deps. Also applied to `it_starts_server_and_serves_health` proactively (same shape — would have failed under the same conditions).
- **Files modified:** `crates/yogurt-cli/Cargo.toml`, `crates/yogurt-cli/tests/cli.rs`
- **Commit:** `8b7a580`

### Architectural notes (no Rule 4 escalation)

- **No Settings.tsx restructure:** the plan said to replace the placeholders inline. Done — AudioSection and GeneralSection render directly (they own their own h2 headers), Transcription wraps STTPicker in a section that holds its own header. Symmetric and tight.
- **No msw harness used:** Task 2's smoke test uses `vi.mock('../lib/api/settings')` rather than the msw infrastructure already installed for plan 05-03. The component-level mock is cheaper and tests the exact contract we care about (Settings → settingsApi). msw remains available for higher-fidelity tests in future plans.

## Authentication Gates

None encountered. The `/api/audio/devices` route is gated by the existing session-token middleware from Phase 4; AudioSection's `audioApi.devices` call assumes that wiring carries a valid token at runtime, which Phase 2's bootstrap arranged. No Keychain prompts during component tests (the settings client is mocked).

## Known Stubs

None. The three new components fully render and persist their state via PATCH. The "Coming in v1" badge on the Local STT card is intentional and visually self-documenting — it's a v2 deferral, not a hidden stub.

## Threat Flags

None. The plan stays within the established threat model:
- Audio device IDs are non-sensitive routing identifiers.
- Port number and open-browser-on-start are config primitives that round-trip through SQLite (no secret material).
- The smoke test fixture deliberately uses `••••WXYZ` (the masked-key sentinel format) rather than any plaintext key, and the test asserts no `sk-` prefix anywhere in the rendered DOM.

## Verification Results

| Gate | Result |
|------|--------|
| `pnpm --dir web build` (after Task 1) | ✅ built in 1.68s |
| `pnpm --dir web test -- Settings` (Task 2) | ✅ 1 test passed |
| `cargo fmt --all -- --check` | ✅ clean |
| `cargo clippy --all-targets -- -D warnings` | ✅ clean (after embedded.rs fix) |
| `cargo test --workspace` | ✅ 143 passed, 1 ignored, 45 suites, 5.49s |
| `pnpm --dir web test` (final) | ✅ 93 passed (14 files, 5.17s) |
| `pnpm --dir web build` (final) | ✅ built in 1.56s |
| `rg "Coming in v1" web/src/components/settings/STTPicker.tsx` | ✅ hit |
| `rg "border-\[1\.5px\] border-\[var\(--blue\)\]" web/src/components/settings/STTPicker.tsx` | ✅ hit |
| `rg "audioApi\.devices.*audio_input_device" web/src/components/settings/AudioSection.tsx` | ✅ both hit |
| `rg "open_browser_on_start.*min=\{1024\}.*max=\{65535\}" web/src/components/settings/GeneralSection.tsx` | ✅ all three hit |
| `rg "STTPicker\|AudioSection\|GeneralSection" web/src/routes/Settings.tsx` | ✅ all three referenced |

## Deferred / Manual Verification (autonomous-mode boundary)

The following items require live network or interactive macOS UI and are deferred to a release-time manual smoke:

1. **Live Minimax round-trip** (network): paste the real `MINIMAX_API_KEY` into Settings → Model → ProviderCard, click `Set active`, then trigger `/api/enhance` (or navigate to a Phase-4 meeting) and confirm `enriched_md` is real Minimax output rather than the Phase-4 MockLLM placeholder.

2. **Keychain Access.app visual verification** (macOS UI): after setting a key in Settings, open Keychain Access.app → search "yogurt" → confirm one entry exists under service="yogurt", account=the provider's ULID. Authenticate when prompted and confirm the stored password matches the pasted key byte-for-byte.

3. **Cross-session persistence smoke**: change Port to 7879 → refresh page → port still shows 7879. Toggle "Open browser on start" → refresh → state preserved. (The unit/integration tests cover the PATCH wire format; this confirms the SQLite round-trip in a real boot.)

Recommended smoke recipe before release:

```bash
# Fresh state
rm -f ~/.yogurt/db.sqlite*
security delete-generic-password -s yogurt 2>/dev/null

# Boot dev mode
cargo run -p yogurt -- start --dev --no-open &
pnpm --dir web dev &
open http://localhost:5173/settings
```

## Self-Check: PASSED

Verified before writing summary:

| Check | Result |
|-------|--------|
| `web/src/components/settings/STTPicker.tsx` exists | FOUND |
| `web/src/components/settings/AudioSection.tsx` exists | FOUND |
| `web/src/components/settings/GeneralSection.tsx` exists | FOUND |
| `web/src/routes/Settings.test.tsx` exists | FOUND |
| `web/src/routes/Settings.tsx` imports all three new components | VERIFIED |
| Commit `6842000` (Task 1) | FOUND |
| Commit `6557af0` (Task 2) | FOUND |
| Commit `8b7a580` (Task 5 collateral fixes) | FOUND |
| Vitest smoke test passes | VERIFIED |
| Workspace `cargo test` 143 passed | VERIFIED |
| `pnpm --dir web test` 93 passed | VERIFIED |
| `cargo clippy --all-targets -- -D warnings` clean | VERIFIED |
| SET-07 / SET-08 / SET-09 requirements satisfied | VERIFIED via component shape + persistence wiring |
