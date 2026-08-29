---
phase: 05-llm-client-settings-keychain
plan: 02
subsystem: secrets + db + bootstrap
tags: [sqlite, keychain, dotenvy, providers, settings, bootstrap, cold-boot]
requires:
  - phase: 00
    artifact: yogurt-server (AppState, Storage at ~/.yogurt/db.sqlite)
  - phase: 04
    artifact: AppState (markdown_exporter, prompts, meetings, session, storage)
provides:
  - "crates/yogurt-db (Db + providers + settings + keychain)"
  - "yogurt-server::state::AppState extended with { db, keys }"
  - "yogurt-server::state::AppState::production_warmed() — 5s eager-load (SET-10)"
  - "yogurt-server::bootstrap::seed_from_env() — YOGURT_*_API_KEY → providers + Keychain (idempotent)"
  - "yogurt-cli --dev gated .env.local loader (SET-11)"
affects:
  - "crates/yogurt-server/src/lib.rs (AppState moved to state.rs, run_with_config wires warmed + bootstrap)"
  - "crates/yogurt-server/tests/{meeting_ws,meeting_ws_auth,e2e_synthetic_audio}.rs (extended AppState literals)"
tech-stack:
  added:
    - rusqlite_migration: 1
    - keyring: 3
    - ulid: 1 (serde)
    - dotenvy: 0.15
  patterns:
    - spawn_blocking + tokio::time::timeout for sync FFI on the tokio runtime
    - trait-object key store (ApiKeyStore) with prod (KeychainStore) and test (MemoryKeyStore) impls
    - rusqlite_migration with include_str! for single-binary distribution
    - partial unique index for "at most one active per kind" invariant
key-files:
  created:
    - crates/yogurt-db/Cargo.toml
    - crates/yogurt-db/migrations/V001__initial.sql
    - crates/yogurt-db/src/lib.rs
    - crates/yogurt-db/src/migrations.rs
    - crates/yogurt-db/src/paths.rs
    - crates/yogurt-db/src/providers.rs
    - crates/yogurt-db/src/settings.rs
    - crates/yogurt-db/src/keychain.rs
    - crates/yogurt-db/tests/migrations.rs
    - crates/yogurt-db/tests/providers.rs
    - crates/yogurt-db/tests/settings.rs
    - crates/yogurt-db/tests/keychain.rs
    - crates/yogurt-db/tests/keychain_live.rs
    - crates/yogurt-server/src/state.rs
    - crates/yogurt-server/src/bootstrap.rs
    - crates/yogurt-server/tests/cold_boot.rs
    - crates/yogurt-server/tests/bootstrap.rs
  modified:
    - Cargo.toml (workspace members += yogurt-db; deps += rusqlite_migration, keyring, ulid, dotenvy)
    - crates/yogurt-server/Cargo.toml (deps += yogurt-db; futures-util promoted to main deps by 05-01)
    - crates/yogurt-server/src/lib.rs (AppState removed; pub use state::AppState; run_with_config calls production_warmed + seed_from_env)
    - crates/yogurt-cli/Cargo.toml (deps += dotenvy)
    - crates/yogurt-cli/src/main.rs (--dev gated dotenvy::from_filename(".env.local") at top of main)
    - crates/yogurt-server/tests/meeting_ws.rs (extended AppState literal)
    - crates/yogurt-server/tests/meeting_ws_auth.rs (extended AppState literal)
    - crates/yogurt-server/tests/e2e_synthetic_audio.rs (extended AppState literal)
decisions:
  - "AppState lives in state.rs (not lib.rs) — satisfies plan acceptance criteria + clean separation of concerns"
  - "yogurt-db migration runs in same ~/.yogurt/db.sqlite file as Phase 0 storage — disjoint table sets so independent runners coexist safely"
  - "SET-10 mitigation: warm_keychain wraps spawn_blocking in 5s timeout; on expiry continues with cold cache (warns, doesn't abort)"
  - "ENV_PRESETS limited to LLM-kind for v1 (Minimax, OpenAI, OpenRouter); STT presets deferred to Phase 8 when kind='stt' surface lands"
  - "bootstrap is non-fatal on Keychain set failure — logs warning, user re-enters via Settings UI"
metrics:
  duration_minutes: 16
  completed_date: 2026-06-25
  tasks_completed: 3
  files_created: 17
  files_modified: 8
  commits: 3
---

# Phase 5 Plan 05-02: yogurt-db + AppState eager-load + .env.local bootstrap Summary

**One-liner:** Ships the `yogurt-db` crate (SQLite providers + settings + Keychain wrapper), extends `AppState` with `db` + `keys` fields and a 5s cold-boot Keychain warm-up (SET-10), and adds the `--dev`-gated `.env.local` → `seed_from_env` provider bootstrap (SET-11) — release builds never read `.env.local`.

## What Shipped

### Task 1 — `yogurt-db` crate

Commit `71f3c96`. New library crate at `crates/yogurt-db/` with:

- **V001 migration** (`migrations/V001__initial.sql`): `settings(key,value)` KV table + `providers(id,name,base_url,model,kind,is_active,created_at)` table. Partial unique index `idx_providers_one_active_per_kind ON providers(kind) WHERE is_active=1` enforces the "single active LLM" invariant at the DB layer (PRD §5.6 / §9). Seeds default `general.port=7878` + `general.open_browser_on_start=true` + `audio.input_device=''` via `INSERT OR IGNORE`.
- **`providers` module**: `Provider`/`NewProvider` types, `insert/list/list_names/active/set_active/update/delete`. `set_active` runs in `BEGIN IMMEDIATE` so partial failure rolls back (the partial unique index would otherwise reject the second UPDATE mid-transaction). `PRESETS` const slice with five v1 entries (Minimax, OpenAI, Ollama (local), LM Studio (local), OpenRouter).
- **`settings` module**: KV get/set (`ON CONFLICT(key) DO UPDATE`), typed `General { port, open_browser_on_start, audio_input_device }` load, `GeneralPatch` save with load-after-write to avoid lost-update bugs.
- **`keychain` module**: `ApiKeyStore` trait (`Send + Sync`) with `get/set/delete` + default `masked()` returning `••••XXXX` (last 4 unicode chars, `\u{2022}` × 4 prefix). `KeychainStore` (real macOS Keychain via `keyring` 3.x) + `MemoryKeyStore` (in-memory fake for tests). `SERVICE="yogurt"` namespacing so brew uninstall/reinstall doesn't leak keys.
- **`keychain-live` feature flag** gates `tests/keychain_live.rs` which roundtrips against the real Keychain (manual: `cargo test -p yogurt-db --features keychain-live -- --ignored`).

Coexists with Phase 0 `yogurt-server::storage` by managing a disjoint set of tables in the same `~/.yogurt/db.sqlite` file (no name collisions; both use `CREATE TABLE IF NOT EXISTS`).

**Tests:** 13 passing across 4 files (2 migrations + 4 providers + 4 settings + 3 keychain).

### Task 2 — `AppState` + 5s Keychain warm-up (SET-10)

Commit `df3c922`. Moves the existing `AppState` from `lib.rs` to a dedicated `crates/yogurt-server/src/state.rs` module and extends it with two new fields:

```rust
pub struct AppState {
    // ... Phase 4 fields preserved verbatim ...
    pub db: Db,                          // Phase 5: providers + settings
    pub keys: Arc<dyn ApiKeyStore>,      // Phase 5: Keychain (or MemoryKeyStore in tests)
}
```

**SET-10 mitigation** (the load-bearing piece):

```rust
async fn warm_keychain(state: &AppState) {
    // Walk providers from the cheap in-memory SQLite.
    let providers = providers::list(&state.db)?;
    // Move the keyring FFI off the tokio scheduler — it's sync and can hang
    // for seconds during user prompts.
    let warm = tokio::task::spawn_blocking(move || {
        for p in providers { let _ = keys.get(&p.id); }
    });
    // Bounded 5s budget per SET-10. On timeout we log + continue with cold
    // cache — request handlers fall back to on-demand `keys.get` (pre-mitigation behavior).
    let _ = tokio::time::timeout(Duration::from_secs(5), warm).await;
}
```

`AppState::production_warmed(ProductionConfig)` runs `production()` then `warm_keychain()`. `run_with_config` now calls `production_warmed` (not `production`) before mounting routes.

Re-exported `pub use state::AppState` from `lib.rs` so existing `use crate::AppState` call sites keep compiling.

**Tests:**
- New `crates/yogurt-server/tests/cold_boot.rs` (2 tests): warm path completes within 5s budget wrapped in 6s outer timeout (regression guard) — one with no providers, one with a seeded provider.
- Existing `tests/{meeting_ws,meeting_ws_auth,e2e_synthetic_audio}.rs` extended with `db: Db::open_in_memory()` + `keys: Arc::new(MemoryKeyStore::default())` (Rule 3 auto-fix — required to keep the compile clean after the AppState extension).
- All 67 yogurt-server tests pass.

### Task 3 — `--dev`-gated `.env.local` loader + `seed_from_env` bootstrap (SET-11)

Commit `8087f18`. Two pieces:

**`yogurt-cli/src/main.rs`**:
```rust
// Raw arg check (BEFORE Cli::parse) so loaded env vars are visible to clap
// AND to bootstrap::seed_from_env later in the boot chain.
if std::env::args().any(|a| a == "--dev") {
    let _ = dotenvy::from_filename(".env.local");
}
```
Release builds invoked without `--dev` NEVER read `.env.local` — brew users never have one. The grep-style test `cli_main_only_loads_env_local_when_dev_flag_passed` is the regression guard against future drift.

**`yogurt-server/src/bootstrap.rs`**:
- `ENV_PRESETS` const slice maps `YOGURT_{MINIMAX,OPENAI,OPENROUTER}_API_KEY` → preset providers. Limited to LLM-kind for v1 (STT presets deferred to Phase 8 when `kind='stt'` surface lands).
- `seed_from_env(state)` is idempotent (case-insensitive name match against existing rows → `SeedReport.skipped`). First-seeded LLM wins the active slot (Minimax is first by design — matches the user's `.env.local` convention).
- Keychain `set` failures are logged but non-fatal; the user can re-enter via Settings UI in Plan 05-03/04.

`run_with_config` calls `bootstrap::seed_from_env(&state)` AFTER `production_warmed` and BEFORE `axum::serve`, with structured logging of seeded/skipped names.

**Tests** (`crates/yogurt-server/tests/bootstrap.rs` — 4 passing):
- `it_seeds_minimax_from_env`: end-to-end seed + active + keystore read returns `"sk-test-minimax-12345"`.
- `it_is_idempotent`: second run reports `seeded=[], skipped=["Minimax"]`; row count stays at 1.
- `it_does_not_override_existing_active`: with both MINIMAX + OPENAI set, Minimax wins active; OpenAI's key still stored.
- `cli_main_only_loads_env_local_when_dev_flag_passed`: SET-11 grep regression — fails if a future refactor drops the `--dev` guard.

A `tokio::sync::Mutex` (not `std::sync::Mutex`) serializes the env-var-mutating tests so `clippy::await_holding_lock` stays green and the env mutations never race across parallel test threads.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking issue] Extended existing `AppState` struct literals in three test files**
- **Found during:** Task 2 verification (`cargo build -p yogurt-server --tests`)
- **Issue:** Three pre-existing test files (`meeting_ws.rs`, `meeting_ws_auth.rs`, `e2e_synthetic_audio.rs`) constructed `AppState { ... }` via struct literal syntax — adding the new `db` + `keys` fields without updating them caused E0063 "missing fields" errors.
- **Fix:** Added `db: Db::open_in_memory().unwrap(), keys: Arc::new(MemoryKeyStore::default())` to each literal with a `// Phase 5 (Plan 05-02)` comment explaining why. Test semantics unchanged.
- **Files modified:** `crates/yogurt-server/tests/{meeting_ws,meeting_ws_auth,e2e_synthetic_audio}.rs`
- **Commit:** `df3c922` (rolled into the Task 2 commit since these were direct collateral)

**2. [Rule 3 - Blocking issue] Used `tokio::sync::Mutex` for env-var serialization in bootstrap tests**
- **Found during:** Task 3 clippy gate
- **Issue:** First version of `bootstrap.rs` tests used `std::sync::Mutex` around the env-var-mutating section; `clippy::await_holding_lock` correctly flagged that the guard was held across `seed_from_env(...).await`.
- **Fix:** Swapped to `tokio::sync::Mutex` (async-aware) and changed `lock().unwrap()` to `lock().await`. Mutex still serves the same purpose: prevent parallel tests from racing on shared `std::env`.
- **Files modified:** `crates/yogurt-server/tests/bootstrap.rs`
- **Commit:** `8087f18`

### Architectural notes (no Rule 4 escalation)

- **Plan said "create `state.rs` with `pub struct AppState { db, keys }`" but Phase 4 already had AppState in `lib.rs` with seven other fields.** Resolved by moving AppState (all fields) to `state.rs` and re-exporting from `lib.rs`. This satisfies the plan's literal acceptance criteria (file path + field presence) AND preserves Phase 4 wiring. Not a deviation — the plan explicitly noted "the exact diff depends on what Phase 4 did to lib.rs; the conceptual change is …".
- **`ProviderKind` enum doesn't exist in yogurt-db.** The plan flagged this as expected — V001 uses a `kind TEXT DEFAULT 'llm'` column instead. ENV_PRESETS therefore omits the kind field (all v1 presets are LLM); STT presets land when Phase 8 introduces `kind='stt'`.
- **STT presets deferred from ENV_PRESETS.** The plan's table included Deepgram/AssemblyAI/Groq but acknowledged they need the kind enum. Since v1 ships LLM-only providers (the providers CRUD has no STT surface yet), restricting ENV_PRESETS to LLM is the correct conservative read. STT bootstrap lands in Phase 8.

## Authentication Gates

None encountered. The Keychain warm-up path uses `MemoryKeyStore` in tests (no real Keychain prompts). Manual Keychain verification is documented in `crates/yogurt-db/tests/keychain_live.rs` for a developer to run with `cargo test -p yogurt-db --features keychain-live -- --ignored` once the Settings UI lands.

## Known Stubs

None. The `bootstrap.rs` placeholder created in Task 2 was replaced with the real `seed_from_env` in Task 3 within the same plan.

## Threat Flags

None. The plan stays within the established threat model:
- Keys NEVER stored in `settings` or `providers` rows — only in Keychain via `ApiKeyStore::set`.
- `ApiKeyStore::masked` is the only key-derived value exposed (Plan 05-03/04 will wire `••••XXXX` into `/api/settings*` responses).
- `.env.local` only read in `--dev` mode (SET-11 regression test enforces).
- `keyring` calls confined to `spawn_blocking` — never block the tokio reactor during a request.

## Verification Results

| Gate | Result |
|------|--------|
| `cargo test -p yogurt-db` | ✅ 13 passed (2 migrations + 4 providers + 4 settings + 3 keychain) |
| `cargo test -p yogurt-server --test cold_boot` | ✅ 2 passed (5s budget verified) |
| `cargo test -p yogurt-server --test bootstrap` | ✅ 4 passed (incl. SET-11 release-safety grep) |
| `cargo test --workspace --features yogurt-audio/synthetic` | ✅ all green (no regressions) |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ clean |
| `cargo fmt --all -- --check` | ✅ clean |
| `cargo build --release -p yogurt` | ✅ universal release binary builds |
| `pnpm --dir web test` | ✅ 92 passed (web untouched) |
| `pnpm --dir web build` | (not run — web frontend unchanged this plan) |

## Manual Verification (deferred)

The SET-10 < 5s wall-clock cold-boot test on a fresh Mac is documented in the plan's `<verification>` block as a manual step. The automated test (`tests/cold_boot.rs`) exercises the warm path against `MemoryKeyStore` to prove the timeout machinery works, but the real macOS Keychain prompt timing requires a hardware run. Recommended manual run before shipping:

```bash
# Fresh state — clean DB and keychain entries first
rm -f ~/.yogurt/db.sqlite*
security delete-generic-password -s yogurt 2>/dev/null

# Time the boot path
time cargo run -p yogurt --release -- start --dev --no-open
# Watch for the "yogurt-server starting" log line. Expect < 5s wall.
```

And the no-key-leak smoke test (deferred to Plan 05-03 which adds `/api/settings*` routes):

```bash
# After Plan 05-03 ships /api/settings
curl -s -H "Authorization: Bearer $TOKEN" localhost:7878/api/settings | jq .providers
# Expect: each provider has api_key_masked="••••XXXX" and NO raw key field.
```

## Self-Check: PASSED

Verified before writing summary:

| Check | Result |
|-------|--------|
| `crates/yogurt-db/Cargo.toml` exists | FOUND |
| `crates/yogurt-db/migrations/V001__initial.sql` exists | FOUND |
| `crates/yogurt-db/src/{lib,migrations,paths,providers,settings,keychain}.rs` exist | FOUND |
| `crates/yogurt-db/tests/{migrations,providers,settings,keychain,keychain_live}.rs` exist | FOUND |
| `crates/yogurt-server/src/state.rs` exists | FOUND |
| `crates/yogurt-server/src/bootstrap.rs` exists | FOUND |
| `crates/yogurt-server/tests/cold_boot.rs` exists | FOUND |
| `crates/yogurt-server/tests/bootstrap.rs` exists | FOUND |
| Commit `71f3c96` (Task 1) | FOUND |
| Commit `df3c922` (Task 2) | FOUND |
| Commit `8087f18` (Task 3) | FOUND |
| `pub struct AppState` contains `db: Db` and `keys: Arc<dyn ApiKeyStore>` | VERIFIED |
| `tokio::time::timeout(Duration::from_secs(5)` present in state.rs warm_keychain | VERIFIED |
| `tokio::task::spawn_blocking` present in state.rs warm_keychain | VERIFIED |
| `dotenvy::from_filename(".env.local")` in main.rs guarded by `std::env::args` --dev check | VERIFIED (asserted by test) |
| `YOGURT_MINIMAX_API_KEY` literal in bootstrap.rs | VERIFIED |
| `https://api.minimax.io/v1` literal in bootstrap.rs | VERIFIED |
