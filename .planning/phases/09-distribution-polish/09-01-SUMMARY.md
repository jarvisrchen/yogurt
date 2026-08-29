---
phase: 09-distribution-polish
plan: 01
subsystem: release-engineering
tags: [ci, github-actions, cargo, distribution]
dependency-graph:
  requires: []
  provides: [ci-workflow, release-workflow, crates-io-metadata]
  affects: [09-02-homebrew-tap-cargo-publish, 09-03-doctor-and-ship-gate]
tech-stack:
  added: []
  patterns:
    - "GitHub Actions matrix build (aarch64-apple-darwin + x86_64-apple-darwin) for macOS release binaries"
    - "workflow_dispatch dry-run input pattern for testing release pipelines before real tag pushes"
key-files:
  created:
    - .github/workflows/release.yml
  modified:
    - .github/workflows/ci.yml
    - Cargo.toml
    - crates/yogurt-cli/Cargo.toml
decisions:
  - "Apple Developer ID enrollment + codesign/notarytool/stapler descoped per user amendment: binaries ship ad-hoc-signed; Homebrew/curl/cargo installs never set the quarantine xattr so Gatekeeper never evaluates them. Comment in release.yml documents the follow-up insertion point."
  - "Did not push to origin/main or trigger any GitHub Actions run (repo is private, macOS minutes are 10x paid, origin/main lacks the code) -- verification done entirely locally: YAML parses, cargo build/test/clippy green, tarball+strip+sha256 mechanics smoke-tested by hand."
  - "cargo publish -p yogurt --dry-run cannot fully succeed yet -- yogurt-server (and other workspace path deps) aren't published to crates.io, so cargo's dry-run registry resolution fails after packaging succeeds. Manifest metadata itself is complete and warning-free (packaging step succeeds). Full publish ordering is Plan 09-02's job (D-10)."
  - "Reused and repaired the pre-existing .github/workflows/ci.yml (added outside GSD tracking in an earlier session) rather than overwriting it: added the missing 'build web before compiling rust-embed consumers' step (a real bug -- web/dist is gitignored so a fresh CI checkout would fail to compile), pinned toolchain to 1.83, added CARGO_TERM_COLOR/RUST_BACKTRACE/INSTA_UPDATE env, kept the existing yogurt-stt/local-stt feature-gated clippy+test (superior to the plan's plain version)."
metrics:
  duration: ~35 min
  completed: 2026-08-28
---

# Phase 9 Plan 01: Distribution Polish - CI + Release Pipeline Summary

Hand-rolled GitHub Actions release pipeline (tag `v*` push or `workflow_dispatch` dry-run) that matrix-builds unsigned `aarch64-apple-darwin` + `x86_64-apple-darwin` tarballs and attaches them to a GitHub Release, plus a repaired `ci.yml` and crates.io-ready workspace metadata.

## What Was Built

### Task 1: Workspace Cargo.toml metadata + panic=abort

- `Cargo.toml` `[workspace.package]` gained `description`, `homepage`, `documentation`, `readme`, `categories`, `keywords`.
- `[profile.release]` gained `panic = "abort"`.
- `crates/yogurt-cli/Cargo.toml` now inherits all six metadata fields via `*.workspace = true` (previously hardcoded its own `description` string, which also contained an em-dash — replaced with a plain dash per project style).
- `yogurt-server` path dependency now also specifies `version = "0.1.0"` — required by `cargo publish`'s manifest verification step (path deps must carry a version requirement).

### Task 2: `.github/workflows/ci.yml` repair

A `ci.yml` already existed on this branch (added in an earlier, non-GSD-tracked session). Rather than replace it, fixed a real bug and layered in the plan's requirements:

- **Bug fix:** the `rust` job compiled (`clippy`, `test`) without first building `web/dist`. `yogurt-server`'s `#[derive(RustEmbed)] #[folder = "../../web/dist/"]` requires that folder to exist at compile time, and it's gitignored — a fresh CI checkout would fail to compile. Added a `pnpm --dir web install && pnpm --dir web build` step before `clippy`/`test`.
- Pinned Rust toolchain to `1.83` (matches `rust-version` in workspace `Cargo.toml`) instead of floating `stable`.
- Added `CARGO_TERM_COLOR=always`, `RUST_BACKTRACE=1`, `INSTA_UPDATE=no` workflow-level env.
- Gated `pull_request` trigger to `branches: [main]` explicitly.
- Added a `build` step to the `web` job (previously only `install`/`typecheck`/`test`).
- Kept the existing `--features yogurt-stt/local-stt` on the `rust` job's `clippy` and `test` steps (exercises the local-STT feature-gated code path) rather than reverting to the plan's plain `cargo clippy --workspace --all-targets` — the existing version is strictly more coverage.

### Task 3: `.github/workflows/release.yml`

New file. Two jobs:

- **`build`** (matrix `aarch64-apple-darwin` / `x86_64-apple-darwin`, `fail-fast: false`, `macos-latest`): checkout → Rust 1.83 with target → `Swatinem/rust-cache@v2` → pnpm 9 + Node 20 → build web assets → `cargo build --release --target <target> -p yogurt` → `strip` → tarball + `shasum -a 256` → `actions/upload-artifact@v4`.
- **`release`** (`needs: build`, `ubuntu-latest`, `permissions: contents: write`): downloads all `yogurt-*` artifacts, concatenates `*.sha256` into `SHA256SUMS`, resolves the release version from the tag (or a `dryrun-<timestamp>` placeholder for `workflow_dispatch`), prints a dry-run manifest when applicable, and otherwise creates a GitHub Release via `softprops/action-gh-release@v2` with both tarballs + `SHA256SUMS` attached.
- Triggers: `push: tags: ["v*"]` and `workflow_dispatch` with a `dry-run: boolean, default: true` input.
- **No codesign / notarytool / stapler steps** — Apple Developer Program enrollment is descoped per user amendment. A top-of-file comment documents why (Homebrew/curl/cargo installs never set the quarantine xattr, so Gatekeeper never evaluates these binaries) and names the exact insertion point (between "strip" and "tarball") for adding signing back later without touching job structure.

## Deviations from Plan

### Plan Amendments (user-directed, not autonomous deviations)

The user descoped Apple Developer Program enrollment before this execution began. Per explicit instructions:

1. Skipped the `checkpoint:human-action` task (Apple Developer ID enrollment + repo secrets) entirely.
2. Task 3 omits codesign, notarytool, and stapler steps completely.
3. No dead `ai.yogurt.app` bundle-ID config was added to release.yml (nothing in this workflow consumes it after removing the signing steps).
4. Task 4 (push to `ci/release-pipeline` branch, open a PR, merge, trigger a real Actions run) was skipped entirely — no `git push`, no `gh` commands were run. The repo is private (macOS Actions minutes billed at 10x) and `origin/main` does not yet have this code; the orchestrator handles pushing and remote verification later.
5. The `checkpoint:human-verify` task (download dry-run artifacts, check `spctl`) was replaced with local verification: both workflow YAML files parse via Ruby's YAML library, `cargo build --release -p yogurt` and `pnpm --dir web build` succeed, and the tarball/strip/sha256 mechanics were hand-verified against a locally built `aarch64-apple-darwin` binary (extracted, ran `--version` and `--help` cleanly).
6. `release.yml` supports both `push: tags: ["v*"]` and `workflow_dispatch` with a `dry-run` boolean input (default `true`), as required.

### Auto-fixed Issues

**1. [Rule 1 - Bug] `ci.yml` rust job did not build `web/dist` before compiling**
- **Found during:** Task 2
- **Issue:** `crates/yogurt-server/src/assets.rs` uses `#[derive(RustEmbed)] #[folder = "../../web/dist/"]`, which requires the folder to physically exist at compile time. `web/dist` is gitignored, so a fresh CI checkout's `clippy`/`test` steps would fail to compile.
- **Fix:** Added a `pnpm --dir web install --frozen-lockfile && pnpm --dir web build` step before the `clippy`/`test` steps in the `rust` job.
- **Files modified:** `.github/workflows/ci.yml`
- **Commit:** c794d0c

**2. [Rule 1 - Bug] Em-dash in `yogurt-cli` description string**
- **Found during:** Task 1
- **Issue:** The hardcoded `description` field in `crates/yogurt-cli/Cargo.toml` used an em-dash ("—"), which the project's global style guide (`~/.claude/CLAUDE.md`) forbids in any text authored/touched.
- **Fix:** Replaced with a plain dash when moving the field to `description.workspace = true` (source string updated in the workspace `Cargo.toml`).
- **Files modified:** `Cargo.toml`, `crates/yogurt-cli/Cargo.toml`
- **Commit:** a53876b

**3. [Rule 3 - Blocking] `cargo publish --dry-run` manifest verification failed on missing path-dep version**
- **Found during:** Task 1
- **Issue:** `cargo publish -p yogurt --dry-run` errored with "all dependencies must have a version requirement specified when publishing" for the `yogurt-server` path dependency.
- **Fix:** Added `version = "0.1.0"` alongside the existing `path = "../yogurt-server"` in `crates/yogurt-cli/Cargo.toml`.
- **Files modified:** `crates/yogurt-cli/Cargo.toml`
- **Commit:** a53876b
- **Residual:** `cargo publish -p yogurt --dry-run` still cannot fully succeed — after packaging succeeds (confirming metadata completeness), cargo's registry-resolution step fails because `yogurt-server` itself is not yet published to crates.io. This is out of scope for Plan 09-01 (which only asks for metadata completeness) and is exactly the ordering problem Plan 09-02 / CONTEXT D-10 is scoped to solve (leaf crates must publish before `yogurt-cli`).

## Verification Results

| Check | Result |
|---|---|
| `cargo build --workspace --release` | Pass |
| `cargo publish -p yogurt --dry-run` (metadata completeness) | Pass — packaging succeeds with no missing-field warnings; full registry dry-run blocked by unpublished workspace deps (see deviation 3) |
| `./target/release/yogurt --help` | Pass — clean output, metadata-derived about text |
| `.github/workflows/ci.yml` YAML parses (Ruby YAML) | Pass |
| `.github/workflows/release.yml` YAML parses (Ruby YAML) | Pass |
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --workspace --features yogurt-stt/local-stt --all-targets -- -D warnings` | Pass — no issues |
| `cargo test --workspace --features yogurt-stt/local-stt --no-fail-fast` (`YOGURT_MEMORY_KEYSTORE=1`) | Pass — 279 passed, 3 ignored (55 suites) |
| `pnpm --dir web build` | Pass |
| Tarball/strip/sha256 mechanics (hand-verified against local `aarch64-apple-darwin` release binary) | Pass — extracted binary runs `--version` and `--help` cleanly |

## Known Stubs

None — this plan produces CI/release configuration, not application UI/data surfaces.

## Threat Flags

None — no new network endpoints, auth paths, or trust-boundary schema changes. The release workflow only reads repo-local files and the (already-scoped) `GITHUB_TOKEN`/`contents: write` permission used by `softprops/action-gh-release`.

## Self-Check: PASSED

- FOUND: `.github/workflows/ci.yml`
- FOUND: `.github/workflows/release.yml`
- FOUND: `Cargo.toml` (modified)
- FOUND: `crates/yogurt-cli/Cargo.toml` (modified)
- FOUND commit a53876b (feat(09-01): add crates.io metadata + panic=abort release profile)
- FOUND commit c794d0c (fix(09-01): CI builds web/dist before compiling rust-embed consumers)
- FOUND commit 389f78c (feat(09-01): add release.yml matrix build + tarball pipeline)
