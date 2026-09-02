#!/usr/bin/env bash
# refresh-model-hashes - download every whisper.cpp model from HuggingFace
# and print the SHA256 each one has TODAY. Use the output to update the
# placeholder hashes in `crates/yogurt-stt/src/models.rs`.
#
# Why this exists: the registry ships with placeholder hashes pinned to
# a 2026-06 snapshot. HuggingFace LFS blobs can drift (re-encoded by the
# upstream maintainer, etc.), so a release ritual is to run this script
# right before tagging v1.0 (and again for each release after) to confirm
# the four pinned hashes still match upstream. If any drift, paste the
# `actual` value into the matching ModelSpec.
#
# Cost: ~5.2 GB total bandwidth (75 MB + 487 MB + 1.5 GB + 3.1 GB) and
# however long that takes on your connection. Re-runnable; uses a temp
# dir that's cleaned up on exit.
#
# Usage:
#   ./scripts/refresh-model-hashes.sh                # all four models
#   ./scripts/refresh-model-hashes.sh tiny.en small.en   # subset by name

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# (name, url) pairs.
declare -a MODELS=(
  "tiny.en          https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin"
  "small.en         https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin"
  "medium.en        https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.en.bin"
  "large-v3         https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin"
  "large-v3-turbo   https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin"
)

# Filter by name args if provided.
if [ $# -gt 0 ]; then
  FILTER=" $* "
  FILTERED=()
  for row in "${MODELS[@]}"; do
    name=$(awk '{print $1}' <<<"$row")
    if [[ "$FILTER" == *" $name "* ]]; then
      FILTERED+=("$row")
    fi
  done
  MODELS=("${FILTERED[@]}")
fi

TMPDIR=$(mktemp -d -t yogurt-models-XXXXXX)
trap 'rm -rf "$TMPDIR"' EXIT

bold() { printf '\033[1m%s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m %s\n' "$*"; }
mut()  { printf '\033[2m  %s\033[0m\n' "$*"; }

bold "Refreshing SHA256 hashes for ${#MODELS[@]} whisper.cpp model(s)"
mut "tmpdir: $TMPDIR"
echo

declare -a RESULTS=()
for row in "${MODELS[@]}"; do
  name=$(awk '{print $1}' <<<"$row")
  url=$(awk '{print $2}' <<<"$row")
  dest="$TMPDIR/ggml-$name.bin"

  bold "$name"
  mut "url: $url"
  curl -fL --progress-bar -o "$dest" "$url"
  hash=$(shasum -a 256 "$dest" | awk '{print $1}')
  size=$(du -h "$dest" | awk '{print $1}')
  ok "$size  $hash"
  RESULTS+=("$name  $hash")
  echo
done

bold "Paste these into crates/yogurt-stt/src/models.rs (REGISTRY):"
echo
for r in "${RESULTS[@]}"; do
  printf '  %s\n' "$r"
done
echo
mut "Each line replaces the sha256 field in the matching ModelSpec."
mut "After editing, rebuild and re-run \`cargo test -p yogurt-stt --features local-stt models\`."
