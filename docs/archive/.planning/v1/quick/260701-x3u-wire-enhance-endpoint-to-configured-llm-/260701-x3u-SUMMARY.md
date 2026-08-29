---
phase: quick-260701-x3u
plan: 01
subsystem: server
tags: [llm, enhance, keychain, providers]
requires: []
provides:
  - "llm_openai::from_active_provider() resolution helper"
  - "enhance handler wired to env -> provider -> mock priority chain"
affects: []
tech-stack:
  added: []
  patterns: ["spawn_blocking for Keychain reads (SET-10)"]
key-files:
  created: []
  modified:
    - crates/yogurt-server/src/llm_openai.rs
    - crates/yogurt-server/src/enhance.rs
decisions:
  - "Store-level key read errors collapse into the same actionable Err arm as missing keys (both bail naming the provider, never the key value)"
metrics:
  duration: ~10 min
  completed: 2026-07-01
---

# Quick Task 260701-x3u: Wire Enhance Endpoint to Configured LLM Summary

Enhance now resolves its LLM via env override -> active provider row + Keychain key -> MockLlm, with unreadable keys returning an actionable 502 instead of silently mocking.

## What Was Done

### Task 1: from_active_provider() helper (TDD)

- RED (`ff84d43`): three `#[tokio::test]` cases in a `#[cfg(test)]` module in `llm_openai.rs` against `Db::open_in_memory()` + `MemoryKeyStore`; helper stubbed with `todo!()` so the crate compiles while tests fail.
- GREEN (`da8f2e0`): `pub async fn from_active_provider(db, keys) -> anyhow::Result<Option<OpenAiCompatClient>>`:
  - no active provider -> `Ok(None)`
  - active provider + readable key -> `Ok(Some(client))` built from the row's base_url/model + the key
  - active provider, missing/unreadable key -> `Err` naming the provider (name + id), never the key value (T-x3u-01)
  - key fetch wrapped in `tokio::task::spawn_blocking` (SET-10, T-x3u-02)
  - module docs updated: `from_env()` is the dev override, this is the production path

Note: the plan referenced `yogurt_db::providers::create(...)`; the actual API is `providers::insert(db, NewProvider)` - tests use `insert` + `set_active`.

### Task 2: enhance handler three-tier chain (`ca126eb`)

- Step 4 of `enhance()` now: `from_env()` (unchanged, highest priority) -> `from_active_provider()` -> `MockLlm`.
- `Ok(None)` branch logs `tracing::warn!("no LLM provider configured; enhance is using MockLlm")`.
- `Err` branch emits `enhance_progress {phase: "error", message: "Enhance failed: ..."}` on `meeting.events_tx` and returns 502; never falls to mock.
- Stale "until Plan 05-02" comment block and module header (line 5) rewritten to describe the actual chain. BL-5 timeout wrapper unchanged, still only around `.complete()`.
- No changes to chat.rs, state.rs, or `AppState.llm`.

## Verification

- `cargo test -p yogurt-server --lib llm_openai`: 3/3 pass.
- `cargo clippy -p yogurt-server --lib`: no issues.
- `cargo build -p yogurt`: success.
- `cargo test -p yogurt-server`: all suites green except the documented environmental flake `embedded.rs::it_returns_bad_gateway_in_dev_mode_when_vite_is_down` (live Vite dev server on :5173 during the run - expected, ignored per plan).
- `grep -c "from_active_provider" crates/yogurt-server/src/enhance.rs` = 2.

## Deviations from Plan

**1. [Rule 1 - Bug] Test used `unwrap_err()` on a `Result` whose Ok type lacks `Debug`**
- **Found during:** Task 1 RED
- **Issue:** `OpenAiCompatClient` doesn't derive `Debug`, so `unwrap_err()` doesn't compile.
- **Fix:** matched on the `Result` explicitly instead of adding a `Debug` derive to `yogurt-llm`.
- **Files modified:** crates/yogurt-server/src/llm_openai.rs
- **Commit:** ff84d43

Otherwise executed as written.

## Commits

| Commit | Subject |
| --- | --- |
| ff84d43 | test(quick-260701-x3u): add failing tests for from_active_provider resolution |
| da8f2e0 | feat(quick-260701-x3u): implement from_active_provider resolution helper |
| ca126eb | feat(quick-260701-x3u): wire enhance handler to env -> provider -> mock chain |

## Self-Check: PASSED
