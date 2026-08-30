#!/usr/bin/env bash
# yogurt setup — one-time prereq + build script.
#
# Usage:
#   ./scripts/setup.sh                 # full setup: check tools, install web deps, build release binary
#   ./scripts/setup.sh --skip-build    # do everything except the cargo build (faster iteration)
#
# Idempotent: safe to re-run. Stops on the first failure with an actionable message.

set -euo pipefail

EXPECTED_NODE_VERSION="22.14.0"
EXPECTED_PNPM_VERSION="9.15.4"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"

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

if ! command -v brew >/dev/null 2>&1; then
  err "Homebrew not found. Install it from https://brew.sh, then rerun setup."
  exit 1
fi

brew_install() {
  local package="$1"
  if ! brew list --formula "$package" >/dev/null 2>&1; then
    echo "    Installing '$package' via Homebrew"
    brew install "$package"
  fi
}

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

# Rust: bootstrap rustup from Homebrew so rust-toolchain.toml controls the
# exact compiler used by setup and subsequent cargo commands. The formula is
# keg-only (and no longer ships rustup-init), so its bin dir must be on PATH;
# the cargo shim there auto-installs the pinned toolchain on first use.
if ! command -v rustup >/dev/null 2>&1; then
  brew_install rustup
  export PATH="$(brew --prefix rustup)/bin:$PATH"
  warn "add to your shell profile: export PATH=\"\$(brew --prefix rustup)/bin:\$PATH\""
fi
if ! command -v cargo >/dev/null 2>&1; then
  RUSTUP_CARGO="$(rustup which cargo 2>/dev/null || true)"
  if [ -x "$RUSTUP_CARGO" ]; then
    export PATH="$(dirname "$RUSTUP_CARGO"):$PATH"
    warn "added rustup toolchain to PATH for this session"
  else
    err "cargo bootstrap failed. Try: rustup toolchain install 1.96.0"
    exit 1
  fi
fi
ok "cargo $(cargo --version | awk '{print $2}')"

# Node and pnpm.
if ! command -v node >/dev/null 2>&1; then
  brew_install node@22
  export PATH="$(brew --prefix node@22)/bin:$PATH"
fi
node_ver=$(node --version | sed 's/^v//')
if [ "${node_ver%%.*}" -lt 22 ] 2>/dev/null; then
  brew_install node@22
  export PATH="$(brew --prefix node@22)/bin:$PATH"
  node_ver=$(node --version | sed 's/^v//')
fi
if [ "${node_ver%%.*}" -lt 22 ] 2>/dev/null; then
  err "node $node_ver - yogurt needs Node 22 or newer"
  exit 1
fi
if [ "$node_ver" = "$EXPECTED_NODE_VERSION" ]; then
  ok "node $node_ver"
else
  warn "node $node_ver (CI pins $EXPECTED_NODE_VERSION; compatible for local setup)"
fi

if ! command -v corepack >/dev/null 2>&1; then
  npm install --global corepack
fi
corepack enable
if command -v corepack >/dev/null 2>&1; then
  pnpm_version=$(corepack pnpm --version)
  pnpm_runner=(corepack pnpm)
elif command -v pnpm >/dev/null 2>&1; then
  pnpm_version=$(pnpm --version)
  pnpm_runner=(pnpm)
else
  err "pnpm not found. Install Node $EXPECTED_NODE_VERSION, then run: corepack enable"
  exit 1
fi
if [ "$pnpm_version" != "$EXPECTED_PNPM_VERSION" ]; then
  err "pnpm $pnpm_version - yogurt requires pnpm $EXPECTED_PNPM_VERSION"
  err "Run: corepack enable && corepack prepare pnpm@$EXPECTED_PNPM_VERSION --activate"
  exit 1
fi
ok "pnpm $pnpm_version"

# cmake: whisper-rs-sys builds whisper.cpp via CMake (local STT is always
# compiled into the release binary), so the cargo build fails without it.
if ! command -v cmake >/dev/null 2>&1; then
  brew_install cmake
fi
ok "cmake $(cmake --version | head -1 | awk '{print $3}')"

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
  warn "wrote .env.local stub — only 'just dev' (dev mode) reads it; 'just release' ignores it, paste keys in the Settings UI instead"
else
  ok ".env.local already exists (not overwriting)"
fi

# ── 4. Web bundle ─────────────────────────────────────────────────
bold "[4/5] Installing web dependencies + building bundle"

(
  cd web
  "${pnpm_runner[@]}" install --frozen-lockfile --silent
)
ok "web deps installed"

(
  cd web
  "${pnpm_runner[@]}" build
) >/tmp/yogurt-web-build.log 2>&1 || {
  err "web build failed — see /tmp/yogurt-web-build.log"
  tail -20 /tmp/yogurt-web-build.log >&2
  exit 1
}
ok "web bundle built → web/dist/"

# ── 5. Rust release binary ────────────────────────────────────────
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

  cargo build --release --locked
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
