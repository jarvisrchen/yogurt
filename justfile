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

# Validate title/body/handover/ticket-checkoff, then push and open a PR: `just pr "<title>" --body-file <f> [--draft] [--dry-run]`.
pr title *rest:
    ./scripts/ship.sh pr "{{title}}" {{rest}}

# Wait for CI, squash-merge, then remove the worktree, delete the branch, and re-print the handover: `just land [pr] [--dry-run]`.
land pr='' *rest:
    ./scripts/ship.sh land {{pr}} {{rest}}

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
    # Idempotent, and lives in the common git dir, so this one call covers
    # every worktree and every agent that shells out to git (DX-5, .githooks/).
    git config core.hooksPath .githooks
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
    # pnpm writes the lockfile it installed from to node_modules/.pnpm/lock.yaml,
    # so a mismatch means the lockfile moved (a pull, a merge) since the last install.
    if ! cmp -s web/pnpm-lock.yaml web/node_modules/.pnpm/lock.yaml; then
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

# Full test suite, split across CI's rust job (test-rust) and web job (test-web). The Docker-based test-cloud-schema is CI's separate cloud-schema job and stays out of this.
test: test-rust test-web

# Rust tests, matching CI's flags.
test-rust:
    YOGURT_MEMORY_KEYSTORE=1 cargo test --workspace --features yogurt-stt/local-stt --no-fail-fast

# Web tests plus the Playwright E2E smoke against a browser-mocked backend (no API keys / live LLM). Starts Vite itself. First run needs `pnpm --dir web exec playwright install chromium`.
test-web:
    pnpm --dir web test
    pnpm --dir web e2e

# Deploy yogurt-cloud's schema.sql twice the way the build does and run its SQL test (with owner.sql prepared as set_owner) in a throwaway postgres:16 container, once with Supabase's old default grants and once with none. Then apply origin/main's schema over seeded rows, this branch's deploy twice, main's schema again, and this branch's deploy once more, checking search and a laptop push after each. Needs Docker. CI's cloud-schema job runs this.
test-cloud-schema:
    #!/usr/bin/env bash
    set -euo pipefail
    main_schema=$(mktemp)
    id=$(docker run --rm -d -e POSTGRES_PASSWORD=postgres postgres:16)
    trap 'docker stop "$id" >/dev/null; rm -f "$main_schema"' EXIT
    ready=
    for _ in $(seq 60); do
        docker exec "$id" pg_isready -U postgres -h 127.0.0.1 >/dev/null 2>&1 && ready=1 && break
        sleep 1
    done
    [ -n "$ready" ] || { echo "test-cloud-schema: postgres did not become ready in 60s" >&2; exit 1; }
    psql() { docker exec -i -e PGOPTIONS='-c client_min_messages=warning' "$id" psql -U postgres -v ON_ERROR_STOP=1 -q "$@"; }
    # Like the build: the schema in one transaction, then search catch-up calls, each its own transaction, until none is left.
    deploy() {
        psql -1 -d "$1" < schema.sql
        for _ in $(seq 100); do
            [ "$(psql -d "$1" -tAc 'select yogurt_private.yogurt_search_catch_up(2000)')" = 0 ] && return
        done
        echo "test-cloud-schema: search catch-up did not finish" >&2
        return 1
    }
    cd crates/yogurt-cloud
    docker cp schema.sql "$id:/schema.sql"
    for mode in on off; do
        db="defaults_$mode"
        echo "== old default grants: $mode"
        psql -c "create database $db" -d postgres
        psql -d "$db" -v old_defaults="$mode" < tests/supabase_stub.sql
        deploy "$db"
        deploy "$db"
        { echo "prepare set_owner(text) as"; cat owner.sql; cat tests/schema_test.sql; } | psql -d "$db"
    done
    echo "== upgrade from main, then revert to it"
    git rev-parse -q --verify origin/main >/dev/null || git fetch -q --depth=1 origin main:refs/remotes/origin/main
    git show origin/main:crates/yogurt-cloud/schema.sql > "$main_schema"
    psql -c "create database upgrade" -d postgres
    psql -d upgrade -v old_defaults=off < tests/supabase_stub.sql
    psql -d upgrade < "$main_schema"
    psql -d upgrade < tests/upgrade_seed.sql
    psql -d upgrade -c "create extension unaccent schema public"
    deploy upgrade
    deploy upgrade
    psql -d upgrade -v phase=upgrade -v search=true < tests/upgrade_test.sql
    psql -d upgrade < "$main_schema"
    psql -d upgrade -v phase=revert -v search=false < tests/upgrade_test.sql
    deploy upgrade
    psql -d upgrade -v phase=reupgrade -v search=true < tests/upgrade_test.sql

# Run yogurt-cloud's #[ignore]d integration tests against a local Supabase stack (project yogurt-cloud-test, ports 56420-56429). Starts the stack if it is not running (slow the first time), drops the yogurt_ tables and every auth user, reapplies schema.sql and owner.sql for owner@yogurt-cloud.test, then runs the tests single-threaded. Leaves the stack running: stop it with `supabase stop --workdir crates/yogurt-cloud`. Needs Docker and the supabase CLI.
test-cloud:
    #!/usr/bin/env bash
    set -euo pipefail
    wd=crates/yogurt-cloud
    if ! docker ps --format '{{{{.Names}}' | grep -qx supabase_db_yogurt-cloud-test; then
        supabase start --workdir "$wd" -x studio,imgproxy,realtime,storage-api,edge-runtime,logflare,vector,supavisor,mailpit,postgres-meta
    fi
    eval "$(supabase status --workdir "$wd" -o env)"
    psql() { docker exec -i -e PGOPTIONS='-c client_min_messages=warning' supabase_db_yogurt-cloud-test psql -U postgres -v ON_ERROR_STOP=1 -q "$@"; }
    psql <<'SQL'
    drop table if exists yogurt_versions, yogurt_pushes, yogurt_devices, yogurt_browsers, yogurt_owner cascade;
    delete from auth.users;
    SQL
    psql < "$wd/schema.sql"
    sed "s/[$]1/'owner@yogurt-cloud.test'/" "$wd/owner.sql" | psql
    YOGURT_CLOUD_TEST_URL="$API_URL" YOGURT_CLOUD_TEST_PUBLISHABLE_KEY="$PUBLISHABLE_KEY" YOGURT_CLOUD_TEST_SECRET_KEY="$SECRET_KEY" \
        cargo test -p yogurt-cloud -- --ignored --test-threads=1

# Drives the real yogurt binary through Yogurt Cloud connect, push, edit, delete, a second laptop, and disconnect against the local Supabase stack (see test-cloud for the stack). Builds target/debug/yogurt first. Needs Docker, the supabase CLI, curl, jq, and python3. Leaves the stack running: stop it with `supabase stop --workdir crates/yogurt-cloud`.
test-cloud-e2e:
    cargo build -p yogurt
    ./scripts/test-cloud-e2e.sh

# Run the cloud/ webapp's Playwright e2e against a local Supabase stack (project yogurt-cloud-web, ports 56430-56439, mail catcher on 56434). Starts the stack if it is not running, then the specs reset the yogurt_ tables and auth users themselves. Runs phone and desktop widths in light and dark and writes screenshots to cloud/e2e/screenshots/. Leaves the stack running: stop it with `supabase stop --workdir cloud`. Needs Docker and the supabase CLI.
test-cloud-web:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! docker ps --format '{{{{.Names}}' | grep -qx supabase_db_yogurt-cloud-web; then
        supabase start --workdir cloud -x studio,imgproxy,realtime,storage-api,edge-runtime,logflare,vector,supavisor,postgres-meta
    fi
    pnpm --dir cloud install --frozen-lockfile
    pnpm --dir cloud exec playwright test

# Hardware smoke tests - opens a real SCK + mic capture pipeline, needs Screen Recording (+ Microphone) grants. NEVER runs under `just test` or CI: every test here is `#[ignore]`d and re-checks $YOGURT_HW_TESTS itself, so this recipe is the only sanctioned way to run them. Skips (does not fail) the whisper smoke test if ~/.yogurt/models/ggml-small.en.bin isn't downloaded. A fresh worktree's `target/debug/yogurt` is a new TCC identity, so the first run here may need a one-time Screen Recording/Microphone grant in System Settings; the start/stop test bounds itself to 45s and names this if it times out.
test-hw:
    #!/usr/bin/env bash
    set -euo pipefail
    YOGURT_HW_TESTS=1 cargo test -p yogurt --test ctl_smoke --features yogurt-stt/local-stt -- --ignored
    cargo test -p yogurt-audio --test permission -- --ignored
    if [ -f "$HOME/.yogurt/models/ggml-small.en.bin" ]; then
        RUN_WHISPER_SMOKE=1 cargo test -p yogurt-stt --test whisper_smoke --features local-stt -- --ignored --nocapture
    else
        echo "test-hw: skipping whisper smoke test -- ~/.yogurt/models/ggml-small.en.bin not downloaded (Settings > Transcription > Local to download it)"
    fi

# Clippy + rustfmt + check-docs (all read-only) - same as CI's rust-job lint gate.
lint: check-docs
    cargo fmt --all -- --check
    cargo clippy --workspace --features yogurt-stt/local-stt --all-targets -- -D warnings
    ./scripts/tests/ticket_test.sh
    ./scripts/ticket.sh --check
    ./scripts/tests/docs-only_test.sh
    ./scripts/tests/task_test.sh
    ./scripts/tests/release_test.sh
    ./scripts/tests/ship_test.sh
    ./scripts/tests/hooks_test.sh
    ./scripts/tests/check-published_test.sh

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

# Does what's published still match what's documented - tap formula version, release assets, README model lines, model mirror URLs. Needs the network; not part of `just lint`.
check-published *args:
    ./scripts/check-published.sh {{args}}

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
