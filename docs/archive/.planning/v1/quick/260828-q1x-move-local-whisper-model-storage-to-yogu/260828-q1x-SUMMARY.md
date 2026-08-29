---
phase: quick-260828-q1x
plan: 01
subsystem: stt
tags: [whisper, model-storage, migration, paths]
dependency_graph:
  requires: []
  provides:
    - "~/.yogurt/models/ as the single model storage location"
    - "migrate_legacy_model one-time rename migration"
  affects:
    - crates/yogurt-stt/src/models.rs
    - crates/yogurt-stt/tests/whisper_smoke.rs
tech_stack:
  added: []
  patterns:
    - "BaseDirs::home_dir().join(\".yogurt\") path resolution (mirrors yogurt-db::paths)"
key_files:
  created: []
  modified:
    - crates/yogurt-stt/src/models.rs
    - crates/yogurt-stt/tests/whisper_smoke.rs
decisions:
  - "Migration is rename-only (atomic on-volume), never copy, never clobber, never delete; failures degrade to the SHA256-verified re-download path"
  - "ProjectDirs legacy triple confined to model_path()'s migration call site - nowhere else in crates/"
metrics:
  duration: ~5 min
  completed: 2026-08-28
---

# Quick Task 260828-q1x: Move Local Whisper Model Storage to ~/.yogurt Summary

Whisper models now resolve to `~/.yogurt/models/<filename>` via BaseDirs (same pattern as db.sqlite and the session token), with a one-time rename migration from the legacy Application Support path so existing multi-GB downloads are not re-fetched.

## Tasks

| Task | Name | Commit |
|------|------|--------|
| 1 | Repoint model_path() to ~/.yogurt/models with one-time legacy migration | 0e4641b |
| 2 | Fix stale doc comments and dedupe whisper_smoke path resolution | 0d56f72 |

## What Changed

- `model_path()` resolves `~/.yogurt/models/<spec.filename>` via `directories::BaseDirs::home_dir()`, mirroring `yogurt-db/src/paths.rs`. Same `io::Result` signature, same `create_dir_all`, `NotFound` error message aligned to "could not resolve user home directory".
- New private `migrate_legacy_model(old_dir, new_dir, filename)`: no-op if the new file exists (never clobber) or the old file is missing; otherwise `fs::rename` the model, then best-effort rename of the `.sha256` sidecar. All failures are `tracing::warn!` + degrade to re-download / re-hash.
- `directories::ProjectDirs::from("com", "yogurt", "yogurt")` now appears only inside `model_path()`'s migration call site.
- Module `# Layout` docs and `model_path()` doc comment rewritten for the new path; the "doc lie" history paragraph and inaccurate "Phase 5 set data_local_dir" claim dropped.
- `whisper_smoke.rs` reuses `models::model_path(models::lookup("small.en"))` instead of duplicating ProjectDirs resolution (runs only past the `RUN_WHISPER_SMOKE` env gate).
- Three new unit tests for the migration helper, all on `tempfile::tempdir()` - real home dir never touched.

## Verification

- `cargo test -p yogurt-stt --features local-stt`: 42 passed, 2 ignored (includes 3 new migration tests).
- `cargo build`: workspace builds clean.
- `grep -rl 'data_local_dir|Application Support' crates --include='*.rs'` matches only `crates/yogurt-stt/src/models.rs`.
- Manual post-merge step (not run here): first `model_path()` call on this machine will move the three downloaded models + sidecars from `~/Library/Application Support/com.yogurt.yogurt/models/`.

## Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

- crates/yogurt-stt/src/models.rs: FOUND
- crates/yogurt-stt/tests/whisper_smoke.rs: FOUND
- Commit 0e4641b: FOUND
- Commit 0d56f72: FOUND
