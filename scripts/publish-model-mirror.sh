#!/usr/bin/env bash
# publish-model-mirror - mirror whisper.cpp models onto a GitHub release so
# a machine that cannot reach huggingface.co can still install them (AUD-4).
#
# The mirror lives on ONE tag that never moves (`models-v1`), not on each
# app release. Pinning it per app version would make every release re-fetch
# the weights from HuggingFace in CI - reintroducing the dependency this
# removes - and put a duplicate copy on every release page. Identity is
# already pinned by the SHA256 in REGISTRY, which `download_to` hard-fails
# on, so a versioned URL buys nothing the hash does not already guarantee.
#
# The expected hash is read out of `crates/yogurt-stt/src/models.rs` rather
# than repeated here: one source of truth, and a mismatch means upstream
# drifted and the registry needs attention BEFORE anything is published.
#
# Assets are the raw `.bin` files only. The `.sha256` sidecar yogurt reads
# to skip re-hashing is written by the Homebrew formula at install time,
# from the hash the formula already pins - nothing to upload, nothing to
# keep in sync.
#
# Usage:
#   ./scripts/publish-model-mirror.sh                 # tiny.en + small.en
#   ./scripts/publish-model-mirror.sh medium.en       # a specific model
#
# Re-runnable: an existing release is added to, not recreated.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODELS_RS="$REPO_ROOT/crates/yogurt-stt/src/models.rs"
TAG="models-v1"
REPO="jarvisrchen/yogurt"

# Default set: every model that fits. small.en matters most - it is the
# seeded default (V005), so without it a machine that cannot reach
# HuggingFace has no working out-of-the-box local model. Pass names to
# mirror a subset. large-v3 CANNOT be mirrored: 2.88 GiB is over GitHub's
# 2 GiB per-asset cap, and it is the only model with no Homebrew path.
MODELS=("${@:-}")
if [ "${#MODELS[@]}" -eq 0 ] || [ -z "${MODELS[0]}" ]; then
  MODELS=(tiny.en small.en medium.en large-v3-turbo)
fi

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$*"; }
mut()  { printf '\033[2m  %s\033[0m\n' "$*"; }
die()  { printf '\033[31mERROR\033[0m %s\n' "$*" >&2; exit 1; }

command -v gh >/dev/null || die "gh CLI not found"
gh auth status >/dev/null 2>&1 || die "gh is not authenticated - run 'gh auth login'"

# Pull the pinned sha256 for a model out of the REGISTRY: find the
# `filename:` line for it, then the next `sha256:` value in that ModelSpec.
pinned_sha() {
  awk -v want="ggml-$1.bin" '
    $0 ~ "filename: \"" want "\"" { found = 1 }
    found && /sha256: "/ {
      match($0, /"[0-9a-f]{64}"/)
      print substr($0, RSTART + 1, 64)
      exit
    }
  ' "$MODELS_RS"
}

TMPDIR=$(mktemp -d -t yogurt-mirror-XXXXXX)
trap 'rm -rf "$TMPDIR"' EXIT

bold "Mirroring ${#MODELS[@]} model(s) to $REPO @ $TAG"
echo

ASSETS=()
for name in "${MODELS[@]}"; do
  file="ggml-$name.bin"
  expected=$(pinned_sha "$name")
  [ -n "$expected" ] || die "no sha256 pinned for '$name' in models.rs - is that a real model name?"

  bold "$name"

  # Check BEFORE downloading, not after. HuggingFace serves the Git LFS
  # pointer at `raw/`, and its `oid sha256` IS the blob's hash while `size`
  # is the blob's byte count - about 130 bytes of traffic to learn both.
  # Doing this after the transfer means burning 3 GB to discover the file
  # is too large to upload, which is exactly what large-v3 does.
  pointer=$(curl -fsL --max-time 30 \
    "https://huggingface.co/ggerganov/whisper.cpp/raw/main/$file") \
    || die "could not read the LFS pointer for $name - is that a real model?"
  upstream_sha=$(awk '/^oid sha256:/ { print substr($2, 8) }' <<<"$pointer")
  bytes=$(awk '/^size / { print $2 }' <<<"$pointer")
  [ -n "$upstream_sha" ] && [ -n "$bytes" ] || die "unparseable LFS pointer for $name:
$pointer"

  [ "$upstream_sha" = "$expected" ] || die "$name hash drift
  pinned   $expected (models.rs)
  upstream $upstream_sha (huggingface.co)
Fix REGISTRY first - publishing this would mirror bytes the app refuses to load."

  # GitHub rejects a release asset over 2 GiB.
  [ "$bytes" -le 2147483648 ] || die "$name is $bytes bytes, over GitHub's 2 GiB \
asset cap - it cannot be mirrored this way. Use large-v3-turbo instead."

  mut "sha256 $expected"
  mut "$bytes bytes, matches models.rs"
  curl -fL --progress-bar \
    -o "$TMPDIR/$file" \
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$file"

  # Re-hash what actually landed: the pointer proves what upstream intends
  # to serve, not what survived the transfer.
  actual=$(shasum -a 256 "$TMPDIR/$file" | awk '{print $1}')
  [ "$actual" = "$expected" ] || die "$name downloaded corrupt
  expected $expected
  actual   $actual"

  ok "verified"
  ASSETS+=("$TMPDIR/$file")
  echo
done

if gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
  bold "Uploading to existing $TAG"
  gh release upload "$TAG" "${ASSETS[@]}" --repo "$REPO" --clobber
else
  bold "Creating $TAG"
  # --latest=false so the model mirror never displaces the app release as
  # the repo's "Latest" - that badge is what points users at the binary.
  gh release create "$TAG" "${ASSETS[@]}" \
    --repo "$REPO" \
    --title "whisper.cpp model mirror" \
    --latest=false \
    --notes "Byte-identical mirror of the whisper.cpp ggml models yogurt uses for local transcription, so installs on networks that cannot reach huggingface.co still work (AUD-4).

Weights are MIT, from https://huggingface.co/ggerganov/whisper.cpp. Every file's SHA256 is pinned in \`crates/yogurt-stt/src/models.rs\` and verified by the app on download.

This tag never moves. Do not delete it: released binaries resolve their models against it."
fi

echo
ok "done"
for a in "${ASSETS[@]}"; do
  mut "https://github.com/$REPO/releases/download/$TAG/$(basename "$a")"
done
