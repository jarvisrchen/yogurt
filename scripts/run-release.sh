#!/usr/bin/env bash
# yogurt run-release — start the release binary, single process.
#
# This is the "what a brew user gets" mode: one binary, no Vite, no .env.local
# (release builds skip dotenvy per SET-11). API keys must be entered via
# the Settings UI on first run.
#
# Use this for validation / acceptance testing of the shipped product.
# For active web-UI development, use run-backend.sh + run-frontend.sh.
#
# Usage:
#   ./scripts/run-release.sh                # release binary at http://localhost:7878
#   ./scripts/run-release.sh --port 7879    # different port
#   ./scripts/run-release.sh --no-open      # don't auto-open the browser

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# shellcheck source=lib/port-guard.sh
source "$REPO_ROOT/scripts/lib/port-guard.sh"

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
err()  { printf '\033[31m✗\033[0m %s\n' "$*" >&2; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }

EXTRA_ARGS=()
PORT=7878

i=0
ARGS=("$@")
while [ $i -lt ${#ARGS[@]} ]; do
  case "${ARGS[$i]}" in
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

# Build if needed.
BIN="target/release/yogurt"
if [ ! -x "$BIN" ]; then
  bold "Binary missing at $BIN — building (8-15 min on first run)."
  cargo build --release
fi

# Optional dev signing: a stable `yogurt-dev` identity keeps macOS Keychain
# "Always Allow" grants valid across rebuilds. Shipped releases get real
# notarized signing in the Phase 9 pipeline; this is for local builds only.
# See README "Keychain prompts (macOS)".
if security find-identity -v -p codesigning 2>/dev/null | grep -q "yogurt-dev"; then
  if codesign --force --sign "yogurt-dev" "$BIN" 2>/dev/null; then
    dim "  signed with yogurt-dev identity — Keychain grants persist across rebuilds"
  else
    dim "  yogurt-dev signing failed — continuing unsigned"
  fi
fi

PORT=$(ensure_port_free "release" "$PORT") || exit 1

bold "Starting yogurt (release mode) at http://localhost:$PORT"
dim "  Single binary with embedded web bundle — no Vite, no .env.local."
dim "  Enter API keys via the Settings UI on first run."
dim "  Ctrl-C to stop."
echo

exec "$BIN" start --port "$PORT" ${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}
