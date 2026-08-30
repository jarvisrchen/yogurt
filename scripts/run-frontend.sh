#!/usr/bin/env bash
# yogurt run-frontend — start the Vite dev server with HMR.
#
# Pair this with ./scripts/run-backend.sh in another terminal.
# Vite serves the React UI on :5173 and the backend proxies to it.
# Open the browser at http://localhost:7878 (NOT 5173 — auth lives on
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
  bold "web/node_modules missing — running pnpm install"
  (cd web && "${PNPM[@]}" install --frozen-lockfile)
fi

# Vite's port is wired into the backend proxy at compile time (5173).
# If 5173 is busy we cannot pick a different port — the backend proxy
# would still send to 5173 and 502. So we ask to kill/abort, not "next port".
YOGURT_PORT_POLICY="${YOGURT_PORT_POLICY:-ask}"
if [ "$YOGURT_PORT_POLICY" = "next" ]; then
  err "vite must run on 5173 (hardcoded in backend proxy) — YOGURT_PORT_POLICY=next is unsupported here."
  exit 1
fi
VITE_PORT=$(ensure_port_free "vite" 5173) || exit 1
if [ "$VITE_PORT" != "5173" ]; then
  err "vite must run on 5173 — got $VITE_PORT. The backend proxy is hard-coded to 5173."
  exit 1
fi

bold "Starting Vite dev server at http://127.0.0.1:5173"
dim "  Open http://localhost:7878 in the browser — backend proxies to here."
dim "  Ctrl-C to stop."
echo

# Force IPv4 bind: backend proxy hits http://127.0.0.1:5173 (not ::1).
# Without --host 127.0.0.1 Vite binds IPv6-only on some macs, breaking the proxy.
exec "${PNPM[@]}" --dir web dev --host 127.0.0.1
