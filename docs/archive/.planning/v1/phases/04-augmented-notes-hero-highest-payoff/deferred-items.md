# Deferred items — Phase 04

## Pre-existing test parallelization issue (out of scope for Plan 04-03)

`crates/yogurt-server/tests/embedded.rs` contains 4 tests that all call
`yogurt_server::run()` (the non-config entry point) which resolves the
DB path + session-token path + (Phase 4) the per-meeting notes dir to
the real `~/.yogurt/` paths. When cargo runs these tests in parallel
they collide on those shared paths, producing transient "Connection
refused" failures.

**Mitigation:** Run with `--test-threads=1` (`cargo test --test embedded
-- --test-threads=1` passes 4/4 reliably).

**Permanent fix (NOT for Plan 04-03):** Either gate these tests behind
`--ignored` + a serial setup helper, OR migrate them to `run_with_config`
with tempdir paths (matching `meeting_rest.rs` / `audio_api.rs` style).
That is out of scope here — the tests already failed in parallel on
`bbbb583` (the parent commit of Plan 04-03) and the path-resolution
change in Plan 04-03 only adds one more shared resource (notes_dir) to
an already-shared set (db_path + session_token_path).
