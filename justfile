# yogurt - task runner. Install with `brew install just`, then run from
# anywhere in the repo. `just` with no args lists every recipe.

# Default: list recipes. Runs when you type `just` with no recipe name.
default:
    @just --list --unsorted

# ── Task lifecycle ───────────────────────────────────────────────────

# Create (or resume) a worktree + branch for a ticket ID or a docs/release slug, bootstrapped and ready.
start *args:
    ./scripts/task.sh start {{args}}

# List every worktree: path, branch, PR state, listening ports, dirty/removable.
worktrees *args:
    ./scripts/task.sh worktrees {{args}}

# Run `just dev` in a tmux window, print the port pair once it's healthy (resumes if already running).
dev-bg *args:
    ./scripts/task.sh dev-bg {{args}}

# Stop the tmux window `just dev-bg` started.
dev-stop *args:
    ./scripts/task.sh dev-stop {{args}}

# ── Run modes ────────────────────────────────────────────────────────

# Start the release binary at http://localhost:7878 (single process, embedded web bundle, paste keys via Settings UI).
release *args:
    ./scripts/run-release.sh {{args}}

# Start backend + Vite together in this terminal - picks a free port pair so a second worktree can run alongside, bootstraps first, Ctrl-C stops both cleanly.
dev: bootstrap
    #!/usr/bin/env bash
    set -euo pipefail
    source ./scripts/lib/port-guard.sh
    # Resolve BOTH ports here, once, and hand them to the two scripts. The
    # pair has to agree: the backend proxies non-API requests to Vite, and
    # Vite proxies /api and /ws back to the backend. Default policy is
    # `next` rather than `ask` - the common reason :7878 is busy is another
    # worktree, and moving over is what you wanted anyway.
    export YOGURT_PORT_POLICY="${YOGURT_PORT_POLICY:-next}"
    VITE_PORT=$(ensure_port_free "vite" "${YOGURT_VITE_PORT:-5173}")
    BACKEND_PORT=$(ensure_port_free "backend" "${YOGURT_BACKEND_PORT:-7878}")
    export YOGURT_VITE_PORT="$VITE_PORT"
    export YOGURT_BACKEND_PORT="$BACKEND_PORT"
    export YOGURT_VITE_BASE="http://127.0.0.1:$VITE_PORT"
    # D5: an echo, not an export -- a recipe cannot export into the
    # caller's shell, so this is copy-paste bait for `--port`/$YOGURT_PORT.
    # `just dev-bg` reads both lines back from the tmux pane.
    echo "YOGURT_PORT=$BACKEND_PORT   # pass --port or set this for yogurt ctl"
    echo "YOGURT_VITE_PORT=$VITE_PORT"
    # Both ports are already free, so the scripts' own guards pass through.
    # Ctrl-C in the foreground process group should kill both children.
    # HUP too: `just dev-stop` kills the tmux window running this recipe,
    # which sends HUP to the pane's shell, not INT/TERM.
    # Belt + suspenders: on EXIT, kill any remaining jobs.
    trap 'kill $(jobs -p) 2>/dev/null; wait 2>/dev/null; true' EXIT INT TERM HUP
    ./scripts/run-frontend.sh &
    # Wait for Vite to bind so the backend proxy doesn't 502 on first request.
    for _ in {1..20}; do
        if curl -sf -o /dev/null "http://127.0.0.1:$VITE_PORT/"; then break; fi
        sleep 0.5
    done
    ./scripts/run-backend.sh --port "$BACKEND_PORT"

# Backend only - debug binary in dev mode, expects Vite already up (:5173, or $YOGURT_VITE_PORT).
backend *args:
    ./scripts/run-backend.sh {{args}}

# Vite dev server only - :5173 (or $YOGURT_VITE_PORT), paired with `just backend`.
frontend:
    ./scripts/run-frontend.sh

# ── Setup + build ────────────────────────────────────────────────────

# Make a fresh worktree runnable - restores .env.local, node_modules and web/dist (all gitignored, so they don't come across). No-ops once present; `just dev` depends on it.
bootstrap:
    #!/usr/bin/env bash
    set -euo pipefail
    # The main checkout, from any worktree: .git/ lives there, linked
    # worktrees only get a .git file pointing into it.
    MAIN="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")"
    if [ ! -f .env.local ]; then
        if [ -f "$MAIN/.env.local" ]; then
            cp "$MAIN/.env.local" .env.local
            echo "bootstrap: copied .env.local from $MAIN"
        else
            echo "bootstrap: no .env.local here or in $MAIN - run ./scripts/setup.sh to write a stub" >&2
            exit 1
        fi
    fi
    if [ ! -d web/node_modules ]; then
        echo "bootstrap: installing web deps"
        pnpm --dir web install --frozen-lockfile
    fi
    # rust-embed's #[folder = "../../web/dist/"] derive needs this to exist
    # before anything compiles yogurt-server.
    if [ ! -d web/dist ]; then
        echo "bootstrap: building web bundle (rust-embed needs web/dist)"
        pnpm --dir web build
    fi

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

# Full test suite - what CI runs, split across the rust job (test-rust) and the web job (test-web).
test: test-rust test-web

# Rust tests, matching CI's flags.
test-rust:
    YOGURT_MEMORY_KEYSTORE=1 cargo test --workspace --features yogurt-stt/local-stt --no-fail-fast

# Web tests plus the Playwright E2E smoke against a browser-mocked backend (no API keys / live LLM). Starts Vite itself. First run needs `pnpm --dir web exec playwright install chromium`.
test-web:
    pnpm --dir web test
    pnpm --dir web e2e

# Clippy + rustfmt + check-docs (all read-only) - same as CI's rust-job lint gate.
lint: check-docs
    cargo fmt --all -- --check
    cargo clippy --workspace --features yogurt-stt/local-stt --all-targets -- -D warnings
    ./scripts/tests/ticket_test.sh
    ./scripts/ticket.sh --check
    ./scripts/tests/docs-only_test.sh
    ./scripts/tests/task_test.sh
    ./scripts/tests/release_test.sh

# Web typecheck (read-only) - same as CI's web-job lint gate; that job has no cargo.
lint-web:
    pnpm --dir web typecheck

# Doc drift: documented /api paths, `just` recipes, links, repo paths, em dash, size budgets. Run by `just lint` and .github/workflows/docs.yml.
check-docs:
    ./scripts/check-docs.sh

# Auto-format Rust code (mutates files).
fmt:
    cargo fmt --all

# ── Maintenance ──────────────────────────────────────────────────────

# docs/TODO.md backlog: list open items, or `ticket <ID>` / `ticket next <PREFIX>` / `ticket done <ID> --note-file <path>` / `ticket --check`.
ticket *args:
    ./scripts/ticket.sh {{args}}

# Free disk by removing all build artifacts - re-run `just build` after.
clean:
    cargo clean
    rm -rf web/dist

# Drop incremental compile cache only (frees ~3 GB, keeps most build output).
clean-incremental:
    find target -name "incremental" -type d -exec rm -rf {} + 2>/dev/null || true

# Wipe the user database - next launch routes to /welcome onboarding again (~/.yogurt/keys.json stays).
reset-db:
    rm -rf ~/.yogurt/db.sqlite ~/.yogurt/db.sqlite-wal ~/.yogurt/db.sqlite-shm
    @echo "  ✓ ~/.yogurt/db.sqlite removed - next launch starts fresh"

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
