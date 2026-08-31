#!/usr/bin/env bash
# render-model-formula — print the Homebrew formula for one mirrored
# whisper.cpp model (AUD-4). Copy the output into the tap repo at
# `Formula/yogurt-model-<name>.rb`.
#
# Generated rather than hand-written so the SHA256 has exactly one source
# of truth: `crates/yogurt-stt/src/models.rs`. A formula whose hash was
# copied by hand and later drifted would install bytes the app then
# refuses to load, with nothing pointing at the formula as the cause.
#
# Why a formula per model instead of bundling weights into the main one:
# `brew install yogurt` has to stay 11.7 MB for people using cloud STT.
# Homebrew dropped per-formula options years ago, so opt-in means a
# separate formula.
#
# Usage:
#   ./scripts/render-model-formula.sh small.en
#   ./scripts/render-model-formula.sh small.en > ../homebrew-yogurt/Formula/yogurt-model-small-en.rb

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODELS_RS="$REPO_ROOT/crates/yogurt-stt/src/models.rs"
TAG="models-v1"
REPO="jarvisrchen/yogurt"
REPO_TAP="jarvisrchen/yogurt"

[ $# -eq 1 ] || { echo "usage: $(basename "$0") <model-name>   e.g. small.en" >&2; exit 2; }
NAME="$1"
FILE="ggml-$NAME.bin"

SHA=$(awk -v want="$FILE" '
  $0 ~ "filename: \"" want "\"" { found = 1 }
  found && /sha256: "/ { match($0, /"[0-9a-f]{64}"/); print substr($0, RSTART + 1, 64); exit }
' "$MODELS_RS")
[ -n "$SHA" ] || { echo "no sha256 pinned for '$NAME' in models.rs" >&2; exit 1; }

# `tiny.en` -> `yogurt-model-tiny-en` -> class YogurtModelTinyEn.
SLUG="yogurt-model-$(printf '%s' "$NAME" | tr '.' '-')"
CLASS=$(printf '%s' "$SLUG" | awk -F- '{ for (i=1;i<=NF;i++) printf toupper(substr($i,1,1)) substr($i,2); print "" }')

cat <<FORMULA
class $CLASS < Formula
  desc "whisper.cpp $NAME model for yogurt local transcription"
  homepage "https://github.com/$REPO"
  url "https://github.com/$REPO/releases/download/$TAG/$FILE"
  version "1"
  sha256 "$SHA"
  license "MIT"

  # Mirrored from https://huggingface.co/ggerganov/whisper.cpp so this
  # installs on a network that cannot reach HuggingFace. github.com is
  # already proven reachable here - it served the yogurt binary.

  # Tap-qualified: an unqualified "yogurt" would resolve against
  # homebrew-core first if a formula by that name ever lands there.
  depends_on "$REPO_TAP/yogurt"

  def install
    sha = "$SHA"
    models = share/"yogurt/models"
    models.install "$FILE"
    model = models/"$FILE"

    # yogurt reads a "<sha256> <bytes>" sidecar to answer "is this model
    # present?" without re-hashing the file. Write it here: without it
    # yogurt falls back to hashing, and it cannot cache the result because
    # this prefix is not writable at runtime - so every Settings page load
    # would re-hash the whole model.
    (models/"$FILE.sha256").write "#{sha} #{model.size}\n"
  end

  def caveats
    <<~EOS
      yogurt picks this up automatically - it reads models from
      #{HOMEBREW_PREFIX}/share/yogurt/models as well as ~/.yogurt/models.
      Select "$NAME" under Settings -> Transcription -> Local.
    EOS
  end

  test do
    sha = "$SHA"
    model = share/"yogurt/models/$FILE"
    assert_predicate model, :exist?
    assert_equal sha, Digest::SHA256.file(model).hexdigest
    # The sidecar must agree with the file, or yogurt re-hashes every check.
    assert_equal "#{sha} #{model.size}", (share/"yogurt/models/$FILE.sha256").read.strip
  end
end
FORMULA
