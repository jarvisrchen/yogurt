# yogurt — task runner. Install with `brew install just`, then run from
# anywhere in the repo. `just` with no args lists every recipe.

# Default: list recipes. Runs when you type `just` with no recipe name.
default:
    @just --list --unsorted

# ── Run modes ────────────────────────────────────────────────────────

# Start the release binary at http://localhost:7878 (single process, embedded web bundle, paste keys via Settings UI).
release *args:
    ./scripts/run-release.sh {{args}}

# Start backend + Vite together in this terminal — reads .env.local, Ctrl-C stops both cleanly.
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    # Ctrl-C in the foreground process group should kill both children.
    # Belt + suspenders: on EXIT, kill any remaining jobs.
    trap 'kill $(jobs -p) 2>/dev/null; wait 2>/dev/null; true' EXIT INT TERM
    ./scripts/run-frontend.sh &
    # Wait for Vite to bind so the backend proxy doesn't 502 on first request.
    for _ in {1..20}; do
        if curl -sf -o /dev/null http://127.0.0.1:5173/; then break; fi
        sleep 0.5
    done
    ./scripts/run-backend.sh

# Backend only — debug binary in dev mode, expects Vite already on :5173.
backend *args:
    ./scripts/run-backend.sh {{args}}

# Vite dev server only — :5173, paired with `just backend`.
frontend:
    ./scripts/run-frontend.sh

# ── Setup + build ────────────────────────────────────────────────────

# One-time prereq check + .env.local stub + web build + release build (idempotent).
setup:
    ./scripts/setup.sh

# Re-run setup without the slow cargo build (faster iteration).
setup-quick:
    ./scripts/setup.sh --skip-build

# Build the release binary without running it.
build:
    cargo build --release

# Build the web bundle without running it.
build-web:
    pnpm --dir web build

# ── Quality gates ────────────────────────────────────────────────────

# Full test suite (cargo + web) — what CI runs.
test:
    YOGURT_MEMORY_KEYSTORE=1 cargo test --workspace --features yogurt-stt/local-stt
    pnpm --dir web test

# Just the Rust tests.
test-rust:
    YOGURT_MEMORY_KEYSTORE=1 cargo test --workspace --features yogurt-stt/local-stt

# Just the web tests.
test-web:
    pnpm --dir web test

# Playwright E2E smoke — drives the real SPA against a browser-mocked backend
# (no API keys / live LLM). Starts Vite itself. First run needs
# `pnpm --dir web exec playwright install chromium`.
test-e2e:
    pnpm --dir web e2e

# Clippy + rustfmt check (read-only) — same as CI's lint gate.
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --features yogurt-stt/local-stt --all-targets -- -D warnings

# Auto-format Rust code (mutates files).
fmt:
    cargo fmt --all

# ── Maintenance ──────────────────────────────────────────────────────

# Free disk by removing all build artifacts — re-run `just build` after.
clean:
    cargo clean
    rm -rf web/dist

# Drop incremental compile cache only (frees ~3 GB, keeps most build output).
clean-incremental:
    find target -name "incremental" -type d -exec rm -rf {} + 2>/dev/null || true

# Wipe the user database — next launch routes to /welcome onboarding again (~/.yogurt/keys.json stays).
reset-db:
    rm -rf ~/.yogurt/db.sqlite ~/.yogurt/db.sqlite-wal ~/.yogurt/db.sqlite-shm
    @echo "  ✓ ~/.yogurt/db.sqlite removed — next launch starts fresh"

# Download every whisper.cpp model from HuggingFace and print the current SHA256 to paste into REGISTRY.
refresh-model-hashes *args:
    ./scripts/refresh-model-hashes.sh {{args}}

# ── Model evals ──────────────────────────────────────────────────────

# Speak the fixed eval conversation through the speaker (start a meeting first; see docs/MODEL-EVAL.md).
eval-play *args:
    ./scripts/eval/play.sh {{args}}

# Judge two enhanced meetings that were fed the same audio: `just eval-compare <url-or-id> <url-or-id>`.
eval-compare a b:
    ./scripts/eval/compare.sh {{a}} {{b}}
