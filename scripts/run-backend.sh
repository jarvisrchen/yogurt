#!/usr/bin/env bash
# yogurt run-backend - start the Rust binary in dev mode.
#
# Pair this with ./scripts/run-frontend.sh in a second terminal.
# Backend proxies non-API requests to Vite (default :5173, override with
# YOGURT_VITE_PORT or YOGURT_VITE_BASE), so the frontend script must also
# be running for the browser to render anything.
#
# Usage:
#   ./scripts/run-backend.sh                # debug build at http://localhost:7878
#   ./scripts/run-backend.sh --port 7879    # different port
#   ./scripts/run-backend.sh --no-open      # don't auto-open the browser
#   ./scripts/run-backend.sh --release      # release build (slower compile, faster runtime)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# shellcheck source=lib/port-guard.sh
source "$REPO_ROOT/scripts/lib/port-guard.sh"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
err()  { printf '\033[31m✗\033[0m %s\n' "$*" >&2; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }

EXTRA_ARGS=()
PROFILE_FLAG=""
PROFILE_DIR="debug"
PORT=7878

# Pre-scan args for --port so we can pass the resolved (possibly-changed)
# value through to the binary rather than relying on the binary's default.
i=0
ARGS=("$@")
while [ $i -lt ${#ARGS[@]} ]; do
  case "${ARGS[$i]}" in
    --release) PROFILE_FLAG="--release";  PROFILE_DIR="release" ;;
    --port)
      i=$((i+1))
      PORT="${ARGS[$i]}"
      ;;
    --port=*) PORT="${ARGS[$i]#--port=}" ;;
    -h|--help)
      sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's|^# \?||'
      exit 0
      ;;
    *)
      EXTRA_ARGS+=("${ARGS[$i]}")
      ;;
  esac
  i=$((i+1))
done

# Cargo on PATH (rustup fallback).
if ! command -v cargo >/dev/null 2>&1; then
  RUSTUP_CARGO="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo"
  if [ -x "$RUSTUP_CARGO" ]; then
    export PATH="$(dirname "$RUSTUP_CARGO"):$PATH"
  else
    err "cargo not found. Run ./scripts/setup.sh first."
    exit 1
  fi
fi

# .env.local sanity (dev mode reads it). Empty is OK - just warn.
if [ ! -f .env.local ]; then
  err ".env.local not found. Run ./scripts/setup.sh first to write a stub."
  exit 1
fi

PORT=$(ensure_port_free "backend" "$PORT") || exit 1

# Where the dev proxy sends non-API requests. `YOGURT_VITE_BASE` is what the
# binary actually reads (dev_proxy.rs); `YOGURT_VITE_PORT` is the knob
# `just dev` and run-frontend.sh share, so honour it when the base is unset.
if [ -z "${YOGURT_VITE_BASE:-}" ] && [ -n "${YOGURT_VITE_PORT:-}" ]; then
  export YOGURT_VITE_BASE="http://127.0.0.1:$YOGURT_VITE_PORT"
fi
VITE_BASE="${YOGURT_VITE_BASE:-http://127.0.0.1:5173}"

bold "Starting yogurt backend (dev mode) at http://localhost:$PORT"
dim "  Non-API requests proxy to Vite at $VITE_BASE - run ./scripts/run-frontend.sh in another terminal."
dim "  Ctrl-C to stop."
echo

if [ -n "$PROFILE_FLAG" ]; then
  cargo build $PROFILE_FLAG -p yogurt
else
  cargo build -p yogurt
fi
BIN="target/$PROFILE_DIR/yogurt"
exec "$BIN" start --dev --port "$PORT" ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}
