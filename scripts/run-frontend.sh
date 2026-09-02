#!/usr/bin/env bash
# yogurt run-frontend - start the Vite dev server with HMR.
#
# Pair this with ./scripts/run-backend.sh in another terminal.
# Vite serves the React UI on :5173 and the backend proxies to it.
# Open the browser at http://localhost:7878 (NOT 5173 - auth lives on
# the backend) once both are up.
#
# Usage:
#   ./scripts/run-frontend.sh           # vite at http://localhost:5173

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# shellcheck source=lib/port-guard.sh
source "$REPO_ROOT/scripts/lib/port-guard.sh"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
err()  { printf '\033[31m✗\033[0m %s\n' "$*" >&2; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }

case "${1:-}" in
  -h|--help)
    sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's|^# \?||'
    exit 0
    ;;
esac

if command -v corepack >/dev/null 2>&1; then
  PNPM=(corepack pnpm)
elif command -v pnpm >/dev/null 2>&1; then
  PNPM=(pnpm)
else
  err "pnpm not found. Run ./scripts/setup.sh first."
  exit 1
fi

if [ ! -d web/node_modules ]; then
  bold "web/node_modules missing - running pnpm install"
  (cd web && "${PNPM[@]}" install --frozen-lockfile)
fi

# Vite's port is no longer fixed: the backend proxy target is
# `YOGURT_VITE_BASE` and vite.config.ts reads `YOGURT_VITE_PORT`, so a
# second worktree can run its own pair. `just dev` resolves the pair and
# passes both down; a bare run of this script falls back to 5173/7878.
WANTED_VITE_PORT="${YOGURT_VITE_PORT:-5173}"
VITE_PORT=$(ensure_port_free "vite" "$WANTED_VITE_PORT") || exit 1
export YOGURT_VITE_PORT="$VITE_PORT"
BACKEND_PORT="${YOGURT_BACKEND_PORT:-7878}"

bold "Starting Vite dev server at http://127.0.0.1:$VITE_PORT"
dim "  Open http://localhost:$BACKEND_PORT in the browser - backend proxies to here."
if [ "$VITE_PORT" != "5173" ]; then
  dim "  Pair it with: YOGURT_VITE_BASE=http://127.0.0.1:$VITE_PORT ./scripts/run-backend.sh --port $BACKEND_PORT"
fi
dim "  Ctrl-C to stop."
echo

# Force IPv4 bind: the backend proxy hits http://127.0.0.1:<port> (not ::1).
# Without --host 127.0.0.1 Vite binds IPv6-only on some macs, breaking the proxy.
exec "${PNPM[@]}" --dir web dev --host 127.0.0.1
