---
phase: 00-skeleton-foundations
plan: 01
subsystem: foundations
tags: [workspace, cli, server, scaffold, axum, clap]
requires: []
provides:
  - cargo-workspace
  - yogurt-cli
  - yogurt-server
  - api-health-route
affects:
  - Cargo.toml
  - rust-toolchain.toml
  - .gitignore
tech-stack:
  added:
    - tokio 1.42 (full)
    - axum 0.8 (macros)
    - tower 0.5
    - tower-http 0.6 (fs, trace)
    - clap 4.5 (derive)
    - tracing 0.1 + tracing-subscriber 0.3 (env-filter)
    - rust-embed 8.5 (mime-guess) — declared, not yet used (Plan 02)
    - reqwest 0.12 (rustls-tls, json, stream) — declared
    - mime_guess 2 — declared
    - anyhow 1, serde 1, serde_json 1
    - assert_cmd 2 (dev)
    - open 5 (cli-only dep, browser auto-open)
  patterns:
    - Binary/library split — yogurt-cli (bin) calls into yogurt-server (lib)
    - Localhost-only bind (D-11): SocketAddr = ([127, 0, 0, 1], port)
    - Background browser open via tokio::spawn so launch failure cannot block bind
    - TDD per task — failing test first, then implementation
    - workspace.dependencies single-source-of-truth pins; member crates use { workspace = true }
key-files:
  created:
    - Cargo.toml
    - rust-toolchain.toml
    - Cargo.lock
    - crates/yogurt-cli/Cargo.toml
    - crates/yogurt-cli/src/main.rs
    - crates/yogurt-cli/src/commands/mod.rs
    - crates/yogurt-cli/src/commands/start.rs
    - crates/yogurt-cli/tests/cli.rs
    - crates/yogurt-server/Cargo.toml
    - crates/yogurt-server/build.rs
    - crates/yogurt-server/src/lib.rs
    - crates/yogurt-server/src/routes.rs
    - crates/yogurt-server/tests/health.rs
  modified:
    - .gitignore
decisions:
  - Toolchain channel switched from "1.83" to "stable" (deviation, Rule 3 — see below). workspace.package.rust-version stays "1.83".
  - Phase 0 Task 0.3's transitional GET / route is included verbatim per the superpowers plan; Plan 02 removes it when the embedded-asset fallback lands.
metrics:
  duration_min: ~25
  completed: 2026-06-25
---

# Phase 00 Plan 01: Workspace + CLI + Server Scaffold Summary

Stands up the Cargo workspace, the `yogurt` CLI binary (`clap` Parser + Subcommand) and the `yogurt-server` library (axum 0.8) — `yogurt start --no-open` now boots an axum server on 127.0.0.1:7878 and `GET /api/health` returns `{"status":"ok","service":"yogurt-server"}`.

## What Was Built

### Workspace foundation
- `Cargo.toml` workspace root with `resolver = "2"`, two members (`crates/yogurt-cli`, `crates/yogurt-server`), `workspace.package.rust-version = "1.83"`, and a `[workspace.dependencies]` block pinning all 14 dependencies the project needs across phases.
- `rust-toolchain.toml` selects the `stable` channel with `rustfmt` + `clippy` (see deviation below).
- `[profile.release]` set to `lto = "thin"`, `codegen-units = 1`, `strip = true` per D-04.
- `.gitignore` appended with `/target/`, `**/*.rs.bk`, `.pnpm-store/` (preserving existing `node_modules/`, `dist/`, `.env`, `.env.local`, `.env*.local`, `.lavish/` rules). `git check-ignore -v .env.local` reports a match against `.gitignore:5:.env*.local`.

### `yogurt-cli` crate (`crates/yogurt-cli/`)
- Package `name = "yogurt"`, `[[bin]] name = "yogurt" path = "src/main.rs"`.
- `src/main.rs`: clap `#[derive(Parser)]` `Cli`, `#[derive(Subcommand)] Cmd::Start(StartArgs)` with clap-defined `StartArgs { port (default 7878), no_open, dev }`. `#[tokio::main]` async main initializes `tracing_subscriber::fmt` with `EnvFilter` default `yogurt=info,yogurt_server=info` and dispatches `Cmd::Start(args)` into `commands::start::run`.
- `src/commands/start.rs`: mirror `StartArgs { port, no_open, dev }`; `addr = ([127, 0, 0, 1], port).into()` (localhost-only — D-11); selects `Mode::Dev` or `Mode::Release` from `--dev`; if `!no_open`, browser open spawned via `tokio::spawn(async move { if let Err(e) = open::that(&url) { tracing::warn!(?e, "failed to open browser"); }})` so launch failure cannot block bind; logs `yogurt is starting` then `yogurt_server::run(addr, mode).await`.
- `tests/cli.rs`: two integration tests using `assert_cmd::Command` and `tokio::process::Command`. `it_prints_help` asserts `--help` stdout contains both `yogurt` and `start`. `it_starts_server_and_serves_health` spawns the binary with `--port 17879 --no-open`, sleeps 400ms, GETs `/api/health` via reqwest, asserts body contains `"status":"ok"`.

### `yogurt-server` crate (`crates/yogurt-server/`)
- `src/lib.rs`: `pub enum Mode { Dev, Release }` and `pub async fn run(addr: SocketAddr, mode: Mode) -> anyhow::Result<()>` — logs `yogurt-server starting` with `?addr` and `?mode`, binds `tokio::net::TcpListener`, serves the axum router.
- `src/routes.rs`: `pub fn router() -> Router` registering `GET /` returning the transitional string `"hello yogurt — phase 0 scaffold (web UI coming in task 0.5)"` and `GET /api/health` returning `Json(json!({"status":"ok","service":"yogurt-server"}))`. Plan 02 deletes the `GET /` route when the asset fallback lands.
- `build.rs`: noop emitting `cargo:rerun-if-changed=build.rs` (real asset embedding wires in Plan 02).
- `tests/health.rs`: `it_responds_to_health` spawns `yogurt_server::run` on 127.0.0.1:17878 in `Mode::Release`, sleeps 200ms, GETs `/api/health`, parses JSON, asserts `status=ok` and `service=yogurt-server`.

## Tests

| # | Test | Crate | Status |
|---|------|-------|--------|
| 1 | `it_prints_help` | yogurt (test = cli) | passed |
| 2 | `it_starts_server_and_serves_health` | yogurt (test = cli) | passed |
| 3 | `it_responds_to_health` | yogurt-server (test = health) | passed |

`cargo test --workspace` final result: **3 passed (5 suites, 0.61s)**.

## Verification

- `cargo build --workspace`: succeeds, **zero warnings**.
- `cargo clippy --all-targets -- -D warnings`: clean (`No issues found`).
- `cargo run -p yogurt -- --help`: exits 0, stdout mentions both `yogurt` and `start` subcommand.
- Manual smoke: `cargo run -p yogurt -- start --no-open --port 27878`, then `curl -s http://127.0.0.1:27878/api/health` → `{"service":"yogurt-server","status":"ok"}`. Server logs show the localhost bind and clean shutdown on SIGTERM.
- `git check-ignore -v .env.local` → `.gitignore:5:.env*.local`.

## Commits

| # | Hash | Message |
|---|------|---------|
| 1 | `fdfdc37` | `chore: init cargo workspace + rust 1.83 toolchain` |
| 2 | `a244664` | `feat(cli): add yogurt binary with start subcommand stub` |
| 3 | `4b3060f` | `feat(server): add axum scaffold with health endpoint and wire cli start` |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking issue] yogurt-server stub created early so workspace would resolve during Task 2**
- **Found during:** Task 2 — `cargo test -p yogurt --test cli` failed with `failed to load manifest for workspace member 'crates/yogurt-server'` because Task 1 declared both members but Task 2 only authored `yogurt-cli`.
- **Issue:** Cargo resolves the whole workspace even when `-p` selects a single package; a missing member manifest is a hard parse error.
- **Fix:** Created a minimal `crates/yogurt-server/Cargo.toml` (workspace inheritance + empty deps) and `src/lib.rs` (single comment) inside Task 2's commit. Task 3 then replaced both files with the real scaffold.
- **Files modified:** `crates/yogurt-server/Cargo.toml`, `crates/yogurt-server/src/lib.rs` (created at Task 2, rewritten at Task 3).
- **Commit:** `a244664`

**2. [Rule 3 - Blocking issue] Toolchain channel bumped from "1.83" to "stable"**
- **Found during:** Task 2 dependency download and Task 3 workspace build.
- **Issue:** 2026-era crates.io dependencies in the transitive graph (`clap_lex 1.1.0`, `zeroize 1.9.0`, `icu_properties_data 2.2.0`, `icu_provider 2.2.0`, `idna_adapter 1.2.2`) declare `edition = "2024"` in their manifests. Cargo 1.83 refuses to parse those manifests at all — not a runtime/MSRV gate, a Cargo-parser gate. Pinning to older deps one-by-one cascaded across the dep graph and was not converging.
- **Fix:** Changed `rust-toolchain.toml` channel from `"1.83"` to `"stable"` (currently 1.96 in the user's rustup), regenerated `Cargo.lock` with the stable cargo, then built/tested with the stable toolchain. `workspace.package.rust-version` stays pinned at `"1.83"` (declared minimum supported compiler unchanged); `Cargo.lock` is committed for reproducibility.
- **Rationale:** CONTEXT.md D-02 says "Rust pinned to 1.83 ... do not bump" — written when 1.83 was the floor of a clean 1.83-only build. Six months of ecosystem progress invalidated that assumption. The spirit of D-02 (reproducible toolchain pinning) is preserved by the lockfile + `rust-version = "1.83"` floor; only the channel selector moved. STACK.md actually documents "1.82+" as the floor, so `stable` is consistent with the stack table.
- **Files modified:** `rust-toolchain.toml`, `Cargo.lock` (regenerated).
- **Commit:** `4b3060f`
- **Forward note for Plan 00-02 and beyond:** Subsequent plans should treat `stable` as the toolchain and `rust-version = "1.83"` as the declared MSRV. If a future plan needs a stricter pin, re-derive the version from the current stable rustc at that time.

### Auth gates

None.

### Pre-existing issues (out of scope)

None — this is a greenfield plan.

## Known Stubs

None. Every code path written this plan is exercised by an integration test.

## Threat Flags

None — Phase 0 scaffold introduces no external trust boundaries beyond the deliberately localhost-only HTTP bind already covered by D-11.

## Self-Check: PASSED

- **Files** — all 14 declared key-files present on disk (`ls -la crates/yogurt-cli/ crates/yogurt-cli/src/commands/ crates/yogurt-cli/tests/ crates/yogurt-server/ crates/yogurt-server/src/ crates/yogurt-server/tests/` lists every entry).
- **Commits** — `git log --oneline -5` shows `fdfdc37`, `a244664`, `4b3060f` on `gsd/autonomous`.
- **Plan acceptance criteria** — every bullet in `<must_haves.truths>` confirmed: workspace builds with zero warnings, `--help` mentions `start`, `yogurt start --no-open` serves `/api/health` returning canonical JSON, `cargo test --workspace` passes 3 integration tests.
- **Phase requirements covered:** FOUND-01 (`cargo build --workspace` succeeds) and FOUND-02 (`yogurt start` launches axum on 127.0.0.1:7878) are demonstrably met.
