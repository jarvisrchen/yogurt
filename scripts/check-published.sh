#!/usr/bin/env bash
# check-published - confirms what's actually live still matches what's
# documented: the latest release tag equals the tap formula version, both
# tarball URLs 200 with shas matching SHA256SUMS, every README
# `yogurt-model-*` line names a formula that exists in the tap, and every
# model mirror URL baked into crates/yogurt-stt resolves.
#
# Read-only, no prompts. Runnable by hand (the real trigger is "I just
# hand-edited the tap formula, did I break it?") and weekly from
# .github/workflows/check-published.yml. This is the one drift a PR-time
# check cannot see - see docs/RELEASING.md's v0.4.0 log entry, where
# v0.3.0's README documented `brew install` commands nothing on the tap
# answered to.
#
# Sources scripts/release.sh for emit_check/json_escape/semver_lt/
# previous_tag/sha256sums_get/formula_version/formula_shas and the
# REPO/TAP_REPO constants, in the style scripts/tests/release_test.sh
# already relies on (release.sh skips `main` when sourced).
#
# Usage: scripts/check-published.sh [--json] [--issue]
#   --json    emit a JSON array of {check, ok, detail} instead of text
#   --issue   on any FAIL, `gh issue create` (skipped if one is already
#             open) - pass only from the scheduled workflow, never by hand
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=release.sh
source "$REPO_ROOT/scripts/release.sh"

usage() {
  cat <<'EOF'
Usage: scripts/check-published.sh [--json] [--issue]

Read-only checks that what's published still matches what's documented:
tap formula version, release assets, README yogurt-model-* lines, and
model mirror URLs. Options:
  --json    emit a JSON array of {check, ok, detail} instead of text
  --issue   on any FAIL, gh issue create (skipped if one is already open)
EOF
}

ISSUE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --json) JSON=1; shift ;;
    --issue) ISSUE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "check-published.sh: unknown argument $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/yogurt-check-published.XXXXXX")"
trap 'rm -rf "$tmpdir"' EXIT

overall_ok=0
TOTAL=0
FAILED=0
FAILS=()   # "FAIL: <detail>" lines, used only for --issue's issue body

# run <name> <true|false> <detail> - thin wrapper over emit_check that
# also tracks pass/fail counts and collects FAIL detail lines. emit_check
# alone can't do this: its own CHECK_JSON array only fills in --json mode,
# and text mode needs a count for the summary line regardless.
run() {
  local name="$1" ok="$2" detail="$3"
  TOTAL=$((TOTAL + 1))
  if [ "$ok" != "true" ]; then
    FAILED=$((FAILED + 1))
    FAILS+=("FAIL: $detail")
    overall_ok=1
  fi
  # emit_check's own exit status mirrors $ok - always false on a FAIL -
  # so it must not be this function's last command: under `set -e`, a
  # bare failing statement (not part of if/&&/||) aborts the whole
  # script on the very first FAIL, before later checks even run.
  emit_check "$name" "$ok" "$detail" || true
}

# url_status <url> - HTTP status code after following redirects, "000" on
# any curl failure (DNS, timeout, ...).
url_status() {
  curl -s -o /dev/null -w '%{http_code}' -L --max-time 20 --head "$1" 2>/dev/null || printf '000'
}

# ---- tag vs tap formula version, tarballs, shas ------------------------

latest_tag="$(previous_tag 999999.999999.999999)"

if [ -z "$latest_tag" ]; then
  run tag_found false "no v* tag found on origin"
else
  version="${latest_tag#v}"

  tap_branch="$(gh api "repos/$TAP_REPO" --jq '.default_branch' 2>/dev/null || true)"
  formula="$tmpdir/yogurt.rb"
  if [ -n "$tap_branch" ] \
      && gh api "repos/$TAP_REPO/contents/Formula/yogurt.rb?ref=$tap_branch" --jq '.content' 2>/dev/null | base64 -d >"$formula" 2>/dev/null \
      && [ -s "$formula" ]; then
    run formula_fetch true "fetched Formula/yogurt.rb from $TAP_REPO@$tap_branch"

    formula_ver="$(formula_version "$formula")"
    if [ -n "$formula_ver" ] && [ "$formula_ver" = "$version" ]; then
      run tag_version_match true "latest tag $latest_tag matches tap formula version $formula_ver"
    else
      run tag_version_match false "latest tag $latest_tag ($version) does not match tap formula version '$formula_ver'"
    fi

    url_arm="$(grep -oE '"https://github\.com/[^"]+\.tar\.gz"' "$formula" | tr -d '"' | sed -n '1p')"
    url_x86="$(grep -oE '"https://github\.com/[^"]+\.tar\.gz"' "$formula" | tr -d '"' | sed -n '2p')"
    sha_arm="$(formula_shas "$formula" | sed -n '1p')"
    sha_x86="$(formula_shas "$formula" | sed -n '2p')"

    if [ -n "$url_arm" ]; then
      code="$(url_status "$url_arm")"
      if [ "$code" = "200" ]; then
        run tarball_url_aarch64 true "$url_arm returned 200"
      else
        run tarball_url_aarch64 false "$url_arm returned $code, want 200"
      fi
    else
      run tarball_url_aarch64 false "no arm64 tarball url found in Formula/yogurt.rb"
    fi
    if [ -n "$url_x86" ]; then
      code="$(url_status "$url_x86")"
      if [ "$code" = "200" ]; then
        run tarball_url_x86_64 true "$url_x86 returned 200"
      else
        run tarball_url_x86_64 false "$url_x86 returned $code, want 200"
      fi
    else
      run tarball_url_x86_64 false "no x86_64 tarball url found in Formula/yogurt.rb"
    fi

    sums="$tmpdir/SHA256SUMS"
    if gh release download "$latest_tag" -R "$REPO" -D "$tmpdir" --clobber -p 'SHA256SUMS' >/dev/null 2>&1 && [ -s "$sums" ]; then
      sums_arm="$(sha256sums_get "$sums" yogurt-aarch64-apple-darwin.tar.gz)"
      sums_x86="$(sha256sums_get "$sums" yogurt-x86_64-apple-darwin.tar.gz)"
      if [ -n "$sha_arm" ] && [ "$sha_arm" = "$sums_arm" ]; then
        run tarball_sha_aarch64 true "formula arm64 sha256 matches SHA256SUMS ($sha_arm)"
      else
        run tarball_sha_aarch64 false "formula arm64 sha256 ('$sha_arm') != SHA256SUMS ('$sums_arm')"
      fi
      if [ -n "$sha_x86" ] && [ "$sha_x86" = "$sums_x86" ]; then
        run tarball_sha_x86_64 true "formula x86_64 sha256 matches SHA256SUMS ($sha_x86)"
      else
        run tarball_sha_x86_64 false "formula x86_64 sha256 ('$sha_x86') != SHA256SUMS ('$sums_x86')"
      fi
    else
      run sha256sums_fetch false "could not download SHA256SUMS for $latest_tag from $REPO"
    fi
  else
    run formula_fetch false "could not fetch Formula/yogurt.rb from $TAP_REPO@${tap_branch:-?}"
  fi
fi

# ---- README yogurt-model-* lines vs tap formulae -----------------------

readme_slugs="$(grep -oE 'yogurt-model-[A-Za-z0-9._-]+' "$REPO_ROOT/README.md" | sort -u)"
while IFS= read -r slug; do
  [ -n "$slug" ] || continue
  if gh api "repos/$TAP_REPO/contents/Formula/$slug.rb?ref=${tap_branch:-}" >/dev/null 2>&1; then
    run "readme_formula_$slug" true "README's $slug names an existing tap formula (Formula/$slug.rb)"
  else
    run "readme_formula_$slug" false "README names $slug but Formula/$slug.rb is missing from $TAP_REPO@${tap_branch:-?}"
  fi
done <<<"$readme_slugs"

# ---- model mirror URLs in crates/yogurt-stt -----------------------------

mirror_urls="$(grep -oE '"https://github\.com/[^"]*/releases/download/[^"]+"' "$REPO_ROOT/crates/yogurt-stt/src/models.rs" | tr -d '"' | sort -u)"
while IFS= read -r url; do
  [ -n "$url" ] || continue
  code="$(url_status "$url")"
  name="$(basename "$url")"
  if [ "$code" = "200" ]; then
    run "model_mirror_$name" true "$url returned 200"
  else
    run "model_mirror_$name" false "$url returned $code, want 200"
  fi
done <<<"$mirror_urls"

# ---- output -------------------------------------------------------------

if [ "$JSON" -eq 1 ]; then
  (IFS=,; printf '[%s]\n' "${CHECK_JSON[*]}")
else
  if [ "$overall_ok" -eq 0 ]; then
    echo "check-published: $TOTAL/$TOTAL ok"
  else
    echo "check-published: $((TOTAL - FAILED))/$TOTAL ok, $FAILED FAIL"
  fi
fi

if [ "$overall_ok" -ne 0 ] && [ "$ISSUE" -eq 1 ]; then
  title="check-published: $FAILED failures on $(date -u +%Y-%m-%d)"
  existing="$(gh issue list --repo "$REPO" --state open --json title \
    --jq '[.[] | select(.title | startswith("check-published:"))] | length' 2>/dev/null || echo 1)"
  if [ "$existing" -eq 0 ]; then
    body="$(printf '%s\n' "${FAILS[@]}")"
    gh issue create --repo "$REPO" --title "$title" --body "$body" >/dev/null
    echo "issue: opened '$title'" >&2
  else
    echo "issue: skipped - an open check-published issue already exists" >&2
  fi
fi

exit "$overall_ok"
