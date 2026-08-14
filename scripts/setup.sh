#!/usr/bin/env bash
# yogurt setup — one-time prereq + build script.
#
# Usage:
#   ./scripts/setup.sh                 # full setup: check tools, install web deps, build release binary
#   ./scripts/setup.sh --skip-build    # do everything except the cargo build (faster iteration)
#
# Idempotent: safe to re-run. Stops on the first failure with an actionable message.

set -euo pipefail

# Resolve repo root regardless of where the script is invoked from.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SKIP_BUILD=false
SKIP_JUST=false
for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=true ;;
    --no-just)    SKIP_JUST=true ;;
    -h|--help)
      sed -n '2,8p' "${BASH_SOURCE[0]}" | sed 's|^# \?||'
      echo ""
      echo "Flags:"
      echo "  --skip-build   do everything except the cargo release build"
      echo "  --no-just      do not install the just task runner via brew"
      exit 0
      ;;
    *)
      echo "✗ unknown flag: $arg" >&2
      exit 2
      ;;
  esac
done

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$*"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$*"; }
err()  { printf '  \033[31m✗\033[0m %s\n' "$*" >&2; }

# ── 1. Prereqs ────────────────────────────────────────────────────
bold "[1/5] Checking prerequisites"

# macOS version: >= 13.
mac_ver=$(sw_vers -productVersion 2>/dev/null || echo "0.0")
mac_major=${mac_ver%%.*}
if [ "$mac_major" -lt 13 ] 2>/dev/null; then
  err "macOS $mac_ver — yogurt needs macOS 13 (Ventura) or newer for ScreenCaptureKit audio."
  exit 1
fi
ok "macOS $mac_ver"

# Rust: cargo on PATH OR at the rustup toolchain location.
if ! command -v cargo >/dev/null 2>&1; then
  RUSTUP_CARGO="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo"
  if [ -x "$RUSTUP_CARGO" ]; then
    export PATH="$(dirname "$RUSTUP_CARGO"):$PATH"
    warn "added rustup toolchain to PATH for this session"
  else
    err "cargo not found. Install Rust: https://rustup.rs"
    exit 1
  fi
fi
ok "cargo $(cargo --version | awk '{print $2}')"

# Node and pnpm.
if ! command -v node >/dev/null 2>&1; then
  err "node not found. Install Node 20.19+: brew install node"
  exit 1
fi
node_ver=$(node --version | sed 's/^v//')
ok "node $node_ver"

if ! command -v pnpm >/dev/null 2>&1; then
  err "pnpm not found. Install: brew install pnpm  (or: npm install -g pnpm)"
  exit 1
fi
ok "pnpm $(pnpm --version)"

# ── 2. just task runner (auto-install via brew) ──────────────────
bold "[2/5] Ensuring just task runner is installed"

if [ "$SKIP_JUST" = "true" ]; then
  warn "skipped per --no-just; use ./scripts/run-*.sh directly"
elif command -v just >/dev/null 2>&1; then
  ok "just $(just --version | awk '{print $2}') (already installed)"
elif command -v brew >/dev/null 2>&1; then
  echo "    Installing 'just' via Homebrew (gives you 'just release', 'just dev', etc.)"
  if brew install just >/tmp/yogurt-just-install.log 2>&1; then
    ok "just $(just --version 2>/dev/null | awk '{print $2}') installed"
  else
    warn "brew install just failed — see /tmp/yogurt-just-install.log. You can still use ./scripts/run-*.sh directly."
  fi
else
  warn "Homebrew not found — skipping 'just' install. Install brew from https://brew.sh, then 'brew install just' (or use ./scripts/run-*.sh directly)."
fi

# ── 3. .env.local stub ───────────────────────────────────────────
bold "[3/5] Provisioning .env.local"

if [ ! -f .env.local ]; then
  cat > .env.local <<'EOF'
# yogurt — local dev API keys. NOT committed (see .gitignore).
# Required for cloud transcript (Phase 3):
YOGURT_DEEPGRAM_API_KEY=

# Pick ONE LLM provider for augmented notes + chat (Phase 4 / Phase 6):
YOGURT_OPENAI_API_KEY=
# YOGURT_MINIMAX_API_KEY=
# YOGURT_OPENROUTER_API_KEY=
EOF
  chmod 600 .env.local
  warn "wrote .env.local stub — fill in your Deepgram key and at least one LLM key before running"
else
  ok ".env.local already exists (not overwriting)"
fi

# ── 4. Web bundle ─────────────────────────────────────────────────
bold "[4/5] Installing web dependencies + building bundle"

pnpm --dir web install --silent
ok "web deps installed"

pnpm --dir web build >/tmp/yogurt-web-build.log 2>&1 || {
  err "web build failed — see /tmp/yogurt-web-build.log"
  tail -20 /tmp/yogurt-web-build.log >&2
  exit 1
}
ok "web bundle built → web/dist/"

# ── 4. Rust release binary ────────────────────────────────────────
if [ "$SKIP_BUILD" = "true" ]; then
  bold "[5/5] Skipping cargo build (--skip-build)"
  dim "    run ./scripts/run-release.sh and it will build on demand"
else
  bold "[5/5] Building release binary (this is the slow step — 8-15 min on first run)"
  echo "    yogurt-server already pins yogurt-stt with local-stt enabled, so no extra flags."

  # Disk space sanity check.
  avail_gb=$(df -g . | awk 'NR==2 {print $4}')
  if [ "$avail_gb" -lt 8 ] 2>/dev/null; then
    warn "only ${avail_gb}G free on this volume; the build typically needs ~10G. Continuing anyway."
  fi

  cargo build --release
  ok "binary built → target/release/yogurt"
fi

echo
bold "Done."
if command -v just >/dev/null 2>&1; then
  echo "Next: just release   (single binary at http://localhost:7878)"
  echo "      just dev       (UI dev with HMR, one terminal)"
  echo "      just           (no args — lists every recipe)"
else
  echo "Next: ./scripts/run-release.sh   (single binary at http://localhost:7878)"
  echo "      ./scripts/run-backend.sh + ./scripts/run-frontend.sh   (UI dev with HMR)"
  echo "      brew install just   (then 'just' lists every recipe — recommended)"
fi
