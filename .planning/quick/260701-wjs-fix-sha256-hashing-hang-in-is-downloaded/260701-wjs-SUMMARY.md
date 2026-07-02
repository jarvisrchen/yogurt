---
phase: quick
plan: 260701-wjs
subsystem: stt
tags: [performance, tokio, sha256, whisper-models]
requires: []
provides:
  - "sidecar .sha256 marker scheme for model verification"
  - "cheap is_downloaded (no per-call hashing)"
  - "spawn_blocking wrapping for list_models and select_stt"
affects: [yogurt-stt, yogurt-server]
key-files:
  modified:
    - crates/yogurt-stt/src/models.rs
    - crates/yogurt-server/src/api/stt_models.rs
    - crates/yogurt-server/src/meetings.rs
key-decisions:
  - "Marker format is a single line '<lowercase-hash> <len>'; length guards against truncated files reusing a stale marker"
  - "Marker IO failures are non-fatal (tracing::warn); worst case is re-hashing on the next check"
  - "list_models degrades to an empty list on JoinError instead of returning 500"
metrics:
  duration: "~10 min"
  tasks: 2
  files: 3
completed: 2026-07-01
---

# Quick Task 260701-wjs: Fix SHA-256 Hashing Hang in is_downloaded Summary

Sidecar `<filename>.sha256` markers make `is_downloaded` an O(1) stat + tiny-file read instead of hashing multi-GB whisper models on every call, and both server call sites now run on `spawn_blocking`.

## What Changed

### Task 1: Sidecar marker + cheap is_downloaded (yogurt-stt) - TDD

- RED commit `1405c82`: five failing tests covering marker short-circuit, legacy self-heal, stale-length fallback, corrupt-file rejection, and missing file.
- GREEN commit `d60af23`: `marker_path` / `write_marker` / `read_marker` helpers plus the path-injectable `is_downloaded_at` core.
- Marker is written at all three verification points: `download_to` fast-path, post-download verify, and the legacy self-heal inside `is_downloaded_at`.
- `sha256::hash_file` and the registry are unchanged; all pre-existing tests still pass.

### Task 2: Server call sites onto spawn_blocking (yogurt-server)

- Commit `41ea94b`: `list_models` maps registry to view inside one `spawn_blocking`; `Registry::start` wraps `select_stt` the same way.
- Even the one-time legacy migration hash (about a minute for the 3 GB model in debug builds) can no longer starve tokio workers.
- `select_stt` doc comment updated to describe the marker scheme and the spawn_blocking requirement for async callers.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Plan verify command misses the models module**
- **Found during:** Task 1 RED phase
- **Issue:** `cargo test -p yogurt-stt` does not compile `models.rs` because the module is gated behind the non-default `local-stt` feature.
- **Fix:** ran the marker tests via `cargo test -p yogurt-stt --features local-stt --lib` for both RED and GREEN gates; the plan's plain command was also run and passes.
- **Files modified:** none
- **Commit:** n/a (verification-only adjustment)

## Verification

- `cargo build -p yogurt`: passes.
- `cargo test -p yogurt-stt --features local-stt --lib`: 20 passed, 0 failed (includes the 5 new marker tests).
- `cargo test -p yogurt-stt -p yogurt-server --no-fail-fast`: 26 suites pass; one pre-existing environmental failure, see below.
- `grep spawn_blocking` shows both call sites wrapped (stt_models.rs:78, meetings.rs:253).
- Manual curl smoke skipped: the running dev server (PID 23141) is the pre-fix binary and is CPU-pegged; restarting it was out of scope per constraints.

## Pre-existing Environmental Failure (not caused by this change)

`yogurt-server tests/embedded.rs::it_returns_bad_gateway_in_dev_mode_when_vite_is_down` expects nothing listening on :5173, but the user's live Vite dev server (node PID 23093) occupies that port, so the dev proxy returns 200 instead of 502.
The test touches only the Vite dev proxy and is unrelated to STT; it will pass once the dev server is stopped.

## TDD Gate Compliance

- RED gate: `1405c82` test(260701-wjs) with 5 failing tests confirmed.
- GREEN gate: `d60af23` feat(260701-wjs) with all tests passing.
- REFACTOR: not needed; implementation landed clean.

## Known Stubs

None.

## Self-Check: PASSED

- Commits 1405c82, d60af23, 41ea94b all present on gsd/autonomous.
- All three modified source files committed; only the SUMMARY (docs artifact, orchestrator commits) plus pre-existing unrelated changes remain uncommitted.
