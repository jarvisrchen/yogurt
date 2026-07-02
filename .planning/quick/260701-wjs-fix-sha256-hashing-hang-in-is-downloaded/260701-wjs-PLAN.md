---
phase: quick
plan: 260701-wjs
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/yogurt-stt/src/models.rs
  - crates/yogurt-server/src/api/stt_models.rs
  - crates/yogurt-server/src/meetings.rs
autonomous: true
requirements: [QUICK-FIX]
must_haves:
  truths:
    - "GET /api/stt/models returns in milliseconds when large models are on disk, not minutes"
    - "'+ New meeting' (Registry::start local-STT preflight) does not stall the tokio runtime"
    - "A corrupt model file (hash mismatch) still reports downloaded=false"
    - "A pre-fix on-disk model with no marker self-heals: one final hash, marker written, cheap thereafter"
  artifacts:
    - path: "crates/yogurt-stt/src/models.rs"
      provides: "sidecar .sha256 marker helpers + cheap is_downloaded + tests"
      contains: "is_downloaded_at"
    - path: "crates/yogurt-server/src/api/stt_models.rs"
      provides: "list_models registry->view mapping on spawn_blocking"
      contains: "spawn_blocking"
    - path: "crates/yogurt-server/src/meetings.rs"
      provides: "select_stt preflight on spawn_blocking"
      contains: "spawn_blocking"
  key_links:
    - from: "crates/yogurt-server/src/api/stt_models.rs"
      to: "yogurt_stt::models::is_downloaded"
      via: "to_view inside tokio::task::spawn_blocking"
      pattern: "spawn_blocking"
    - from: "crates/yogurt-server/src/meetings.rs"
      to: "select_stt"
      via: "tokio::task::spawn_blocking(move || select_stt(&stt_settings))"
      pattern: "spawn_blocking.*select_stt"
---

<objective>
Fix the backend-wide hang caused by `is_downloaded()` SHA-256 hashing multi-GB whisper model files on every call inside async handlers.
With ggml-large-v3.bin (3 GB) on disk, each call is ~a minute of pure CPU in debug builds; the UI polls `GET /api/stt/models` (4 registry entries per request), requests pile up on tokio workers, and the whole runtime starves - "+ New meeting" hangs.

Root cause is fully diagnosed (stack confirmed via `sample` of the hung process). Do NOT re-investigate.

Fix design is locked:
1. Sidecar marker file `<filename>.sha256` (single line `<hash> <len>`) written after every successful full-file verification.
2. `is_downloaded` becomes cheap: existence + marker hash/length check, no hashing. Legacy migration path hashes once and self-heals the marker.
3. Both server call sites move onto `tokio::task::spawn_blocking` so even the one-time legacy hash cannot stall the runtime.

Purpose: unblock the server; "+ New meeting" and the Settings model picker respond instantly.
Output: patched yogurt-stt + yogurt-server, new unit tests for the marker logic.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@crates/yogurt-stt/src/models.rs
@crates/yogurt-stt/src/sha256.rs
@crates/yogurt-server/src/api/stt_models.rs
@crates/yogurt-server/src/meetings.rs
</context>

<tasks>

<task type="auto" tdd="true">
  <name>Task 1: Sidecar marker + cheap is_downloaded in yogurt-stt</name>
  <files>crates/yogurt-stt/src/models.rs</files>
  <behavior>
    All tests live in the existing `#[cfg(test)] mod tests` in models.rs, using `tempfile::tempdir()` (already a dev-dep, see sha256.rs tests) and the path-injectable inner fn so `~/.yogurt/models` is never touched.
    Use `sha256::hash_bytes(payload)` as the expected hash for fixtures.
    - Test 1 (marker skips hashing): file whose CONTENT does NOT match the expected hash, but a marker containing the expected hash + the file's actual byte length -> `is_downloaded_at` returns true. This proves the hash was skipped (a hash check would have failed).
    - Test 2 (self-heal): file whose content matches the expected hash, NO marker -> returns true AND the marker file now exists containing `<hash> <len>`.
    - Test 3 (stale marker): marker has the right hash but the WRONG length (simulate truncated/partial file by writing a marker with len+1) and content matches -> re-hash fallback returns true and marker is rewritten with the correct length.
    - Test 4 (corrupt file): file content does NOT match expected hash, no marker -> returns false and no marker is written.
    - Test 5 (missing file): nonexistent path -> false.
  </behavior>
  <action>
    In `crates/yogurt-stt/src/models.rs`:

    1. Add private marker helpers next to `is_downloaded`:
       - `fn marker_path(model: &Path) -> PathBuf` - append `.sha256` to the file name (e.g. `ggml-large-v3.bin` -> `ggml-large-v3.bin.sha256`). Use `OsString` push or `format!("{}.sha256", file_name)`; do NOT use `with_extension` (it would replace `.bin`).
       - `fn write_marker(model: &Path, hash: &str)` - best-effort: read `fs::metadata(model).len()`, write single line `<lowercase-hash> <len>` to the marker path. All failures are non-fatal (log via `tracing::warn!` if tracing is already a dep of yogurt-stt, otherwise silently ignore - check Cargo.toml, do not add a dep for this).
       - `fn read_marker(model: &Path) -> Option<(String, u64)>` - read + parse the single line; any IO/parse failure -> None.
    2. Refactor `is_downloaded(spec)` into a thin wrapper over a path-injectable inner fn:
       `fn is_downloaded_at(path: &Path, expected_sha256: &str) -> bool` (private; tests are in-module so no `pub` needed).
       Signature takes the expected hash as `&str`, NOT `&ModelSpec` - `ModelSpec` fields are `&'static str` and test fixtures need runtime hash strings.
       Logic:
       - `path` missing or `fs::metadata` fails -> false (existence short-circuit; no stale-marker cleanup needed).
       - marker readable AND marker hash `eq_ignore_ascii_case(expected_sha256)` AND marker len == current `metadata.len()` -> true, NO hashing.
       - otherwise (marker missing/invalid/stale) legacy migration: `sha256::hash_file(path)`; on match write marker (self-heal) and return true; on mismatch or IO error return false.
       `is_downloaded(spec)` stays `pub fn is_downloaded(spec: &ModelSpec) -> bool` and just resolves `model_path(spec)` then delegates to `is_downloaded_at(&path, spec.sha256)`. Update its doc comment to describe the marker scheme (the "source of truth for the UI" sentence stays true).
    3. Write the marker at the two other verification points, using the ACTUAL computed lowercase hash:
       - `download_to` fast-path (~line 228): after `existing.eq_ignore_ascii_case(expected_sha256)` matches, call `write_marker(dest, &existing)` before `return Ok(())`.
       - post-download verify (~line 322): after the hash matches (i.e. after the mismatch early-return), call `write_marker(dest, &actual)` before `Ok(())`.
    4. `sha256::hash_file` itself is unchanged. Registry, `lookup`, `model_path`, download logic otherwise unchanged. Existing tests do not assume per-call hashing (they only test the registry), so none need updating - verify this holds after the change.
    Keep comment density in line with the existing file style.
  </action>
  <verify>
    <automated>cargo test -p yogurt-stt</automated>
  </verify>
  <done>New marker tests pass; existing yogurt-stt tests still pass; `is_downloaded` no longer hashes when a valid marker is present; all three verification points write the marker.</done>
</task>

<task type="auto">
  <name>Task 2: Move server call sites onto spawn_blocking</name>
  <files>crates/yogurt-server/src/api/stt_models.rs, crates/yogurt-server/src/meetings.rs</files>
  <action>
    Even post-fix, the one-time legacy migration hash (3 GB file, debug build) can take ~a minute - it must never run on a tokio worker.

    1. `crates/yogurt-server/src/api/stt_models.rs` `list_models` (~line 72):
       move the WHOLE registry->view mapping into one `tokio::task::spawn_blocking`:
       `tokio::task::spawn_blocking(|| models::REGISTRY.iter().map(to_view).collect::<Vec<_>>())` then `.await`.
       `REGISTRY` is `&'static`, so the closure needs no captures. On `JoinError` (only possible via panic) fall back to `Vec::new()` with a one-line comment - do not 500 the model picker over a panic in a filesystem probe.
       `to_view` and its wire-shape test are unchanged.
    2. `crates/yogurt-server/src/meetings.rs` `Registry::start` (~line 246):
       replace `let stt_spec = select_stt(&stt_settings).context("select STT adapter")?;` with
       `let stt_spec = tokio::task::spawn_blocking(move || select_stt(&stt_settings)).await.context("join select_stt")?.context("select STT adapter")?;`
       `stt_settings` is not used after this line (verified), so moving it into the closure compiles cleanly. `select_stt` itself stays a sync fn - the existing branch-coverage tests at ~lines 678-740 keep calling it directly, unchanged.
    3. Update the `select_stt` doc comment (~line 78) which currently says "verifies SHA256": note it now checks the sidecar `.sha256` marker (hashing only on legacy migration) and that callers must invoke it via `spawn_blocking`.
  </action>
  <verify>
    <automated>cargo build -p yogurt && cargo test -p yogurt-stt -p yogurt-server</automated>
  </verify>
  <done>Workspace binary builds; scoped test suites pass (do NOT use `cargo test --workspace` - it has a pre-existing unrelated failure, D-INT-01 synthetic feature gating); no `is_downloaded`/`select_stt` call remains on the async runtime path outside `spawn_blocking`.</done>
</task>

</tasks>

<verification>
- `cargo build -p yogurt` succeeds.
- `cargo test -p yogurt-stt -p yogurt-server` passes (scoped; `--workspace` is known-broken by unrelated D-INT-01).
- `grep -n "spawn_blocking" crates/yogurt-server/src/api/stt_models.rs crates/yogurt-server/src/meetings.rs` shows both call sites wrapped.
- Manual smoke (optional, if a large model is on disk): `time curl -s localhost:7878/api/stt/models` completes in < 1s on the second call (first call may pay the one-time migration hash, off-runtime).
</verification>

<success_criteria>
- `is_downloaded` performs zero hashing when a valid `<filename>.sha256` marker matches the spec hash and current file length.
- Legacy files (no marker) hash exactly once, then self-heal a marker.
- Corrupt files (hash mismatch) still report not-downloaded and never write a marker.
- Neither `GET /api/stt/models` nor `Registry::start` can starve the tokio runtime, even during the one-time migration hash.
- Marker write failures are non-fatal everywhere.
</success_criteria>

<output>
Create `.planning/quick/260701-wjs-fix-sha256-hashing-hang-in-is-downloaded/260701-wjs-SUMMARY.md` when done.
</output>
