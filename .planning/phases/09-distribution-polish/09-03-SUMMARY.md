---
phase: 09-distribution-polish
plan: 03
subsystem: cli, docs
tags: [distribution, cli, doctor, readme, release-checklist]
dependency-graph:
  requires: [09-01, 09-02]
  provides: [yogurt-doctor-subcommand, install-docs, release-runbook]
  affects: [crates/yogurt-cli]
tech-stack:
  added: []
  patterns:
    - "yogurt doctor reads the real yogurt-db SQLite settings store and the real yogurt-audio TCC permission check, not a separate config scrape"
key-files:
  created:
    - crates/yogurt-cli/src/commands/doctor.rs
    - crates/yogurt-cli/tests/doctor.rs
    - CONTRIBUTING.md
    - scripts/release-checklist.md
  modified:
    - crates/yogurt-cli/Cargo.toml
    - crates/yogurt-cli/build.rs
    - crates/yogurt-cli/src/main.rs
    - crates/yogurt-cli/src/commands/mod.rs
    - README.md
decisions:
  - "Used yogurt-db::providers::list_names / settings::load_general + yogurt-audio::permission instead of the plan's assumed ~/.yogurt/config.toml scrape -- no TOML config exists in this codebase, settings live in SQLite (Phase 5 convention)"
  - "Used the existing directories crate (already a workspace dep, already used by yogurt-db/yogurt-stt) instead of adding a new dirs dependency, per the codebase's own do-not-use-dirs convention"
  - "Used sw_vers -productVersion for macOS version instead of adding the sysinfo crate -- native platform tool, zero new dependency"
  - "Task 4 (cut v0.1.0) and the cut-tag-now/hold-tag decision checkpoint are SKIPPED per this session's explicit instruction: HOLD THE TAG. The repo stays private and no v0.1.0 tag is cut this session."
metrics:
  duration: "~1h"
  completed: 2026-08-28
---

# Phase 9 Plan 03: yogurt doctor + release docs Summary

`yogurt doctor` diagnostic/repair subcommand wired to the real settings store and TCC permission check, plus a tightened README, new CONTRIBUTING.md, and a release-day runbook -- all local verification (tests/clippy/fmt) green, no tag cut, no push.

## What shipped

### Task 1: `yogurt doctor` subcommand

- `crates/yogurt-cli/src/commands/doctor.rs` -- new module, `pub async fn run`.
- Default output prints section labels: `rust:`, `macos:`, `screen recording:`, `db path:`, `db exists:`, `providers:`, `stt:`, `models:`, `config:`, `notes:`, plus a closing line pointing at the GitHub issue tracker.
- `--json` emits `{service: "yogurt-doctor", version, rust, macos, screen_recording, db_path, db_exists, providers: [...], stt, models: [...]}`. Never includes API key values, only provider names (matches D-12).
- `--reset-screen-recording` runs `tccutil reset ScreenCapture ai.yogurt.app` (bundle ID pinned for future notarization work).
- `--check-port` binds `127.0.0.1:7878` and reports free/in-use with a `--port` suggestion.
- `--redownload-model <name>` deletes `~/.yogurt/models/ggml-<name>.bin`.
- `build.rs` extended (not replaced -- it already had the Swift rpath fix) to capture `YOGURT_RUSTC_VERSION` at compile time.
- `main.rs` gained a `Doctor(DoctorArgs)` clap variant, mirroring the existing `StartArgs` pattern (clap struct in `main.rs`, plain struct + explicit field mapping into `commands::doctor::DoctorArgs`).
- 3 new integration tests in `crates/yogurt-cli/tests/doctor.rs` using `assert_cmd` + `HOME` redirected to a tempdir (same SET-12 pattern as `tests/cli.rs`): prints all diagnostic sections, `--json` parses and has `service == "yogurt-doctor"`, `--check-port` reports a free/in-use line.
- Verified against the developer's real `~/.yogurt/`: correctly reported `providers: Minimax`, `stt: local`, and three downloaded whisper models, with zero secret leakage.

### Task 2: README, CONTRIBUTING.md, scripts/release-checklist.md

- **README.md** rewritten tight and scannable: badges, one-line quickstart, three-channel Install section (Homebrew, from source, direct download with an `xattr -d com.apple.quarantine` note for browser downloads pending notarization), First-run (4 steps), compact architecture diagram, local-state bullets, Diagnostics section (`yogurt doctor` + repair flags), Privacy posture, and a Threat model subsection (localhost trust boundary, same as the Granola desktop app). Explicitly does NOT document `cargo install yogurt` as a working command since crates.io publish is deferred (only descoped-context text `cargo install --path ...` for building from source).
- **CONTRIBUTING.md** (new): dev setup via `scripts/setup.sh`, the `just`-based dev loop (release / dev / frontend+backend split), test/lint commands, the eight-crate workspace table, code style (fmt/clippy/Conventional Commits), how to add an STT engine or LLM provider, a troubleshooting section (Keychain re-signing for unsigned dev builds, port-conflict prompts, `just reset-db`), and how to file issues via `yogurt doctor --json`.
- **scripts/release-checklist.md** (new): the release-day runbook per this session's amendments -- 4 one-time bootstrap steps (flip repo public, `gh auth login`, create the tap repo from `scripts/homebrew/`, create the `HOMEBREW_TAP_TOKEN` PAT) followed by 6 per-release steps (push main, optional dry-run, tag + push, watch the workflow, smoke-test all three channels including the direct-download quarantine step, and a Deferred section covering Apple notarization and `cargo publish` with the publish-hygiene ordering rule spelled out explicitly).

### Task 3: local verification (no push, no gh commands, per amendments)

- `YOGURT_MEMORY_KEYSTORE=1 cargo test --workspace --features yogurt-stt/local-stt` -- all suites green, 0 failures.
- `cargo clippy --workspace --features yogurt-stt/local-stt --all-targets -- -D warnings` -- clean.
- `cargo fmt --all -- --check` -- clean.
- `cargo build --release -p yogurt` then `./target/release/yogurt doctor`, `doctor --json | jq .`, `--version` -- all clean, JSON valid.

## Sample doctor output (release build, fresh tempdir HOME)

```text
yogurt doctor
version: 0.1.0
rust: rustc 1.96.0 (ac68faa20 2026-05-25)
macos: 15.6
screen recording: granted
db path: /var/folders/.../.yogurt/db.sqlite
db exists: false
providers: none configured
stt: not configured yet -- run `yogurt start` first
models: none downloaded
config: /var/folders/.../.yogurt/db.sqlite
notes: use --json for a machine-readable dump; --reset-screen-recording,
       --check-port, and --redownload-model <name> are repair actions.

paste this output into any issue at https://github.com/jarvisrchen/yogurt/issues
```

```json
{
  "service": "yogurt-doctor",
  "version": "0.1.0",
  "rust": "rustc 1.96.0 (ac68faa20 2026-05-25)",
  "macos": "15.6",
  "screen_recording": "granted",
  "db_path": "/var/folders/.../.yogurt/db.sqlite",
  "db_exists": false,
  "providers": [],
  "stt": "not configured yet -- run `yogurt start` first",
  "models": []
}
```

## Deviations from Plan

### Auto-fixed / adjusted (Rule 1/2 -- plan assumptions didn't match the real codebase)

**1. Provider/STT source: real SQLite settings store, not `~/.yogurt/config.toml`**
- **Found during:** Task 1, before writing `doctor.rs`.
- **Issue:** The plan's action text assumed a `~/.yogurt/config.toml` file with `[providers.NAME]` sections to scrape. No such file exists anywhere in this codebase -- Phase 5 established settings/providers as SQLite tables (`yogurt_db::providers`, `yogurt_db::settings`), with a comment in `yogurt-db/src/settings.rs` explicitly noting secrets never touch a config file.
- **Fix:** `doctor.rs` opens `yogurt_db::Db::open_default()` (only if `~/.yogurt/db.sqlite` already exists, to avoid creating it as a side effect of a diagnostic command) and calls `providers::list_names` + `settings::load_general` -- the exact same code path `yogurt start` uses, so the diagnostic can never drift from reality.
- **Files modified:** `crates/yogurt-cli/Cargo.toml` (added `yogurt-db` path dep), `crates/yogurt-cli/src/commands/doctor.rs`.
- **Commit:** 2496919

**2. Screen Recording status: real TCC check, not a placeholder**
- **Found during:** Task 1.
- **Issue:** The plan's action text said "screen recording status placeholder" -- but a real, already-tested `yogurt_audio::permission::has_screen_recording_permission()` function exists (`CGPreflightScreenCaptureAccess`), and `yogurt-cli` already transitively links `yogurt-audio` via `yogurt-server`.
- **Fix:** Added a direct `yogurt-audio` path dependency (no new linkage cost -- already in the dependency graph) and call the real permission check instead of shipping a stub in a diagnostic tool.
- **Files modified:** `crates/yogurt-cli/Cargo.toml`, `crates/yogurt-cli/src/commands/doctor.rs`.
- **Commit:** 2496919

**3. Dependency choices: `directories` not `dirs`, `sw_vers` not `sysinfo`**
- **Found during:** Task 1.
- **Issue:** Plan suggested adding `dirs = "5"` and `sysinfo = "0.32"`. `yogurt-db/src/paths.rs` has an explicit code comment: "do not switch to `dirs::home_dir()` (the `dirs` crate is unmaintained)" -- the workspace already standardizes on `directories`. Adding `sysinfo` (a much heavier dependency) for a single macOS version string is unnecessary when `sw_vers -productVersion` is a zero-dependency native platform call.
- **Fix:** Used the workspace's existing `directories` dependency and shelled out to `sw_vers` via `std::process::Command`, matching the ladder's "native platform feature" and "already-installed dependency" rungs.
- **Files modified:** `crates/yogurt-cli/Cargo.toml`, `crates/yogurt-cli/src/commands/doctor.rs`.
- **Commit:** 2496919

None of these change any acceptance criterion in the plan -- all `doctor` output sections, flags, and JSON shape match the plan's `must_haves` exactly.

### Session-level scope changes (per explicit user amendments, not autonomous deviations)

- Task 4 (cut v0.1.0 tag) and the preceding `checkpoint:decision` are **skipped**. The user resolved the decision as **hold-tag** before this plan ran: the repo stays private, no tag is cut, and the release runbook for actually cutting `v0.1.0` now lives in `scripts/release-checklist.md` for a future session.
- The final `checkpoint:human-verify` (DIST-09 fresh-Mac install acceptance) in the plan cannot run this session either, since it depends on a real `v0.1.0` release existing. It remains open until the release checklist is executed.
- Task 3's git push / `gh workflow run` / `gh run watch` dry-run trigger was replaced with local-only verification (test/clippy/fmt + a local release build's `doctor`/`--version` output) per the session's explicit instruction.

## Known Stubs

None -- `yogurt doctor` is fully wired to real data sources (SQLite settings, real TCC permission check, real filesystem model listing). No stub UI or hardcoded empty values ship in this plan.

## Threat Flags

None. `doctor --json` reads settings that already exist in the running app's trust boundary (localhost-only, single-user) and explicitly excludes API key values from every code path (`list_names` only ever selects the `name` column; keys live in the Keychain, never queried by this command).

## Self-Check: PASSED

- `crates/yogurt-cli/src/commands/doctor.rs` -- FOUND
- `crates/yogurt-cli/tests/doctor.rs` -- FOUND
- `CONTRIBUTING.md` -- FOUND
- `scripts/release-checklist.md` -- FOUND
- Commit `2496919` (doctor subcommand) -- FOUND in `git log --oneline`
- Commit `775e04f` (docs) -- FOUND in `git log --oneline`
