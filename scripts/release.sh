#!/usr/bin/env bash
# release - subcommands that back the release skill (.claude/skills/release/SKILL.md).
#
# Bash plus `gh` and `jq`, in the style of scripts/publish-model-mirror.sh.
# Flags only, no prompts. `-n` prints the plan on anything that mutates.
# `--json` emits a per-check array where the subcommand has checks.
#
# `preflight`, `verify`, `finish` and `untag` exist. `ship` lands in a
# follow-up PR - see docs/TODO.md DX-7 and docs/.planning/agent-workflow.md
# section 4C.
#
# Usage errors exit 2. A failed check exits 1.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/docs-only.sh
source "$REPO_ROOT/scripts/lib/docs-only.sh"

REPO="jarvisrchen/yogurt"
TAP_REPO="jarvisrchen/homebrew-yogurt"

usage() {
  cat <<'EOF'
Usage: scripts/release.sh <command> [options]

Commands:
  preflight <version>
      Read-only checks that origin/main is ready to ship as v<version>.
      Makes no changes. Options:
        -n         no-op (preflight never mutates; accepted for symmetry)
        --json     emit a JSON array of {check, ok, detail} instead of text
        --strict   fail (rather than list) if any open PR touches docs/ or README.md

  verify <version>
      Read-only checks that the published GitHub Release and tap formula
      for v<version> are internally consistent. Makes no changes. Options:
        --json     emit a JSON array of {check, ok, detail} instead of text

  finish <version> [--no-smoke] [-n]
      Runs verify, merges the tap PR, upgrades the local brew install and
      prints a pre-filled docs/RELEASE-LOG.md row. Skip-if-done: safe to
      re-run. Options:
        --no-smoke   skip the brew upgrade/test/quarantine steps
        -n           print the plan (every mutating command) and exit 0

  untag <version> [-n]
      Deletes the local and remote v<version> tag. Refuses (exit 2) if a
      GitHub Release v<version> already exists - fix the tap formula by
      hand instead (see docs/RELEASING.md "When it goes wrong"). Options:
        -n           print the plan and exit 0
EOF
}

# ---- output helpers --------------------------------------------------

JSON=0
CHECK_JSON=()    # --json: one already-escaped object per check

json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\t'/\\t}"
  printf '%s' "$s"
}

# emit_check <name> <true|false> <detail> [pass_label] - prints
# "<pass_label>: "/"FAIL: " in text mode (pass_label defaults to "ok",
# preflight's wording; verify passes "PASS"), records a JSON object in
# --json mode. Returns the check's own ok/fail status so callers can
# `check_x || OVERALL_OK=1`.
emit_check() {
  local name="$1" ok="$2" detail="$3" label="${4:-ok}"
  if [ "$JSON" -eq 1 ]; then
    CHECK_JSON+=("{\"check\":\"$name\",\"ok\":$ok,\"detail\":\"$(json_escape "$detail")\"}")
  elif [ "$ok" = "true" ]; then
    printf '%s: %s\n' "$label" "$detail"
  else
    printf 'FAIL: %s\n' "$detail"
  fi
  [ "$ok" = "true" ]
}

# ---- semver -----------------------------------------------------------

# semver_lt a b - true when a < b, comparing X.Y.Z numerically.
semver_lt() {
  local IFS=.
  local -a a=($1) b=($2)
  local i x y
  for i in 0 1 2; do
    x="${a[$i]:-0}"
    y="${b[$i]:-0}"
    [ "$x" -lt "$y" ] && return 0
    [ "$x" -gt "$y" ] && return 1
  done
  return 1
}

# ---- preflight ----------------------------------------------------

# resolve_ci_status <sha> - walks first-parent ancestors of <sha> (up to
# 30) looking for a CI run, skipping over commits CI legitimately skipped
# because they were docs-only. Prints "STATUS|sha|detail" on stdout.
resolve_ci_status() {
  local cur="$1" i=0 runs count status conclusion parent changed all_docs_only changed_line
  while [ "$i" -lt 30 ]; do
    runs="$(gh run list -R "$REPO" -w CI -c "$cur" --json status,conclusion -L 5 2>/dev/null || echo '[]')"
    count="$(jq 'length' <<<"$runs")"
    if [ "$count" -gt 0 ]; then
      status="$(jq -r '.[0].status' <<<"$runs")"
      conclusion="$(jq -r '.[0].conclusion' <<<"$runs")"
      if [ "$status" != "completed" ]; then
        echo "PENDING|$cur|CI run for $cur is $status, not yet completed"
      elif [ "$conclusion" = "success" ]; then
        echo "GREEN|$cur|CI green for $cur"
      else
        echo "RED|$cur|CI run for $cur concluded $conclusion"
      fi
      return 0
    fi
    parent="$(git rev-parse "${cur}^1" 2>/dev/null)" || {
      echo "NOTFOUND|$cur|no CI run and no parent commit to walk back to"
      return 0
    }
    changed="$(changed_paths_between "$parent" "$cur")"
    # array-free: read one path at a time so a path with a space in it
    # never gets word-split into two arguments.
    all_docs_only=1
    while IFS= read -r changed_line; do
      [ -n "$changed_line" ] || continue
      is_docs_only "$changed_line" || { all_docs_only=0; break; }
    done <<<"$changed"
    if [ "$all_docs_only" -eq 0 ]; then
      echo "NOTFOUND|$cur|no CI run for $cur and it is not docs-only"
      return 0
    fi
    cur="$parent"
    i=$((i + 1))
  done
  echo "NOTFOUND|$cur|no CI run found within 30 commits"
}

check_tools() {
  if ! command -v gh >/dev/null 2>&1; then
    emit_check tools false "gh CLI not found"
  elif ! command -v jq >/dev/null 2>&1; then
    emit_check tools false "jq not found"
  elif ! gh auth status >/dev/null 2>&1; then
    emit_check tools false "gh is not authenticated - run 'gh auth login'"
  else
    emit_check tools true "gh and jq present, gh auth ok"
  fi
}

check_tag_available() {
  local version="$1" tag
  tag="v$version"
  if git ls-remote --tags origin "refs/tags/$tag" | grep -q .; then
    emit_check tag_available false "$tag already exists on origin"
  else
    emit_check tag_available true "$tag does not exist on origin"
  fi
}

check_version_below_target() {
  local version="$1" cargo_version
  cargo_version="$(git show origin/main:Cargo.toml | awk '
    /^\[workspace\.package\]/ { inblock = 1; next }
    /^\[/ { inblock = 0 }
    inblock && /^version = / { gsub(/"/, ""); print $3; exit }
  ')"
  if [ -z "$cargo_version" ]; then
    emit_check version_below_target false "could not read [workspace.package] version from origin/main Cargo.toml"
  elif semver_lt "$cargo_version" "$version"; then
    emit_check version_below_target true "origin/main Cargo.toml version $cargo_version is below target $version"
  else
    emit_check version_below_target false "origin/main Cargo.toml version $cargo_version is not below target $version"
  fi
}

check_ci_green() {
  local head_sha="$1"
  if ! command -v gh >/dev/null 2>&1 || ! command -v jq >/dev/null 2>&1; then
    emit_check ci_green false "gh/jq not available, cannot check CI status"
    return
  fi
  local result status sha detail
  result="$(resolve_ci_status "$head_sha")"
  status="${result%%|*}"
  local rest="${result#*|}"
  sha="${rest%%|*}"
  detail="${rest#*|}"
  if [ "$status" = "GREEN" ]; then
    if [ "$sha" = "$head_sha" ]; then
      emit_check ci_green true "$detail"
    else
      emit_check ci_green true "$detail (origin/main HEAD $head_sha is docs-only; walked back to $sha)"
    fi
  elif [ "$status" = "NOTFOUND" ]; then
    emit_check ci_green false "CI status not found; check manually ($detail)"
  else
    emit_check ci_green false "$detail"
  fi
}

check_open_doc_prs() {
  local strict="$1"
  if ! command -v gh >/dev/null 2>&1 || ! command -v jq >/dev/null 2>&1; then
    emit_check open_doc_prs false "gh/jq not available, cannot list open PRs"
    return
  fi
  local prs n list
  prs="$(gh pr list -R "$REPO" --state open --json number,title,files \
    --jq '[.[] | select(.files | any(.path | startswith("docs/") or . == "README.md"))]')"
  n="$(jq 'length' <<<"$prs")"
  if [ "$n" -eq 0 ]; then
    emit_check open_doc_prs true "no open PRs touching docs/ or README.md"
    return
  fi
  list="$(jq -r '.[] | "#\(.number) \(.title)"' <<<"$prs")"
  if [ "$strict" -eq 1 ]; then
    emit_check open_doc_prs false "open PRs touching docs/ or README.md (--strict): $list"
  else
    emit_check open_doc_prs true "open PRs touching docs/ or README.md - judgment: merge first only if it describes something installable at this tag: $list"
  fi
}

check_readme_wording() {
  local matches
  matches="$(git show origin/main:README.md | grep -nE 'Status:|coming soon|not yet' || true)"
  if [ -z "$matches" ]; then
    emit_check readme_wording true "no Status:/coming-soon/not-yet wording in origin/main README.md"
  else
    emit_check readme_wording true "origin/main README.md lines to review (judgment): $matches"
  fi
}

check_release_scope() {
  local last_tag="" ref tag ver
  while IFS= read -r ref; do
    tag="${ref#refs/tags/}"
    tag="${tag%^\{\}}"
    case "$tag" in v*) ;; *) continue ;; esac
    ver="${tag#v}"
    if [ -z "$last_tag" ] || semver_lt "${last_tag#v}" "$ver"; then
      last_tag="$tag"
    fi
  done < <(git ls-remote --tags origin | awk '{print $2}')

  if [ -z "$last_tag" ]; then
    emit_check release_scope false "no existing v* tag found on origin"
    return
  fi
  local range_log
  range_log="$(git log "${last_tag}..origin/main" --oneline)"
  if [ -z "$range_log" ]; then
    emit_check release_scope false "nothing to release: git log ${last_tag}..origin/main is empty"
  else
    emit_check release_scope true "$(printf 'since %s:\n%s' "$last_tag" "$range_log")"
  fi
}

cmd_preflight() {
  local version="" strict=0
  while [ $# -gt 0 ]; do
    case "$1" in
      -n) shift ;; # preflight never mutates; accepted as a no-op
      --json) JSON=1; shift ;;
      --strict) strict=1; shift ;;
      -h|--help) usage; exit 0 ;;
      -*)
        echo "release.sh preflight: unknown flag $1" >&2
        usage >&2
        exit 2
        ;;
      *)
        if [ -n "$version" ]; then
          echo "release.sh preflight: unexpected argument $1" >&2
          exit 2
        fi
        version="$1"
        shift
        ;;
    esac
  done
  if [ -z "$version" ]; then
    echo "release.sh preflight: missing <version>" >&2
    usage >&2
    exit 2
  fi

  git fetch origin --quiet
  local head_sha
  head_sha="$(git rev-parse origin/main)"

  local overall_ok=0
  check_tools || overall_ok=1
  check_tag_available "$version" || overall_ok=1
  check_version_below_target "$version" || overall_ok=1
  check_ci_green "$head_sha" || overall_ok=1
  check_open_doc_prs "$strict" || overall_ok=1
  check_readme_wording || true
  check_release_scope || overall_ok=1

  if [ "$JSON" -eq 1 ]; then
    (IFS=,; printf '[%s]\n' "${CHECK_JSON[*]}")
  else
    echo "next: follow the release skill from the dry-run step"
  fi

  exit "$overall_ok"
}

# ---- verify -----------------------------------------------------------

# sha256sums_get <sums_file> <filename> - the hash listed for <filename>
# in a `shasum -a 256`-style sums file ("<hash>  <filename>" per line).
sha256sums_get() {
  awk -v want="$2" '$2==want{print $1; exit}' "$1"
}

# formula_version <formula_file> - the value of the formula's
# `version "..."` line.
formula_version() {
  awk -F'"' '/^  version "/{print $2; exit}' "$1"
}

# formula_shas <formula_file> - the formula's two `sha256 "..."` values,
# one per line, in file order: arm64 first, x86_64 second. release.yml's
# heredoc always writes the `Hardware::CPU.arm?` branch before the `else`
# branch, so file order is a reliable stand-in for parsing the
# `on_macos do` block.
formula_shas() {
  grep -oE '"[0-9a-f]{64}"' "$1" | tr -d '"'
}

# check_eq <name> <detail> <got> <want> - PASS (as "PASS: ...") only when
# got is non-empty and equals want; a blank got usually means upstream
# parsing failed. Used only by verify - preflight's checks are their own
# true/false judgments, not equality comparisons.
check_eq() {
  local name="$1" detail="$2" got="$3" want="$4"
  if [ -n "$got" ] && [ "$got" = "$want" ]; then
    emit_check "$name" true "$detail ($got)" PASS
  else
    emit_check "$name" false "$detail: got '$got', want '$want'"
  fi
}

# host_tarball - the release tarball name for the machine running this
# script; empty for an architecture the release pipeline does not build.
host_tarball() {
  case "$(uname -m)" in
    arm64) printf 'yogurt-aarch64-apple-darwin.tar.gz' ;;
    x86_64) printf 'yogurt-x86_64-apple-darwin.tar.gz' ;;
    *) printf '' ;;
  esac
}

# tap_formula_ref <version> - the tap-repo ref to read Formula/yogurt.rb
# from: the bump PR's branch if it still exists, else the tap's default
# branch. `finish` deletes the branch on merge, so verify falls back for
# any already-finished release (that is how it can still check v0.7.0).
tap_formula_ref() {
  local branch="bump-$1"
  if gh api "repos/$TAP_REPO/branches/$branch" >/dev/null 2>&1; then
    printf '%s' "$branch"
  else
    gh api "repos/$TAP_REPO" --jq '.default_branch'
  fi
}

VERIFY_ARM_SHA=""
VERIFY_X86_SHA=""
tmpdir=""    # set by cmd_verify; global so its EXIT trap can still see it

# cmd_verify <version> [--json] - read-only; returns 0 when every check
# passes. On success, sets VERIFY_ARM_SHA/VERIFY_X86_SHA so `finish` can
# reuse the already-downloaded hashes in its log row.
cmd_verify() {
  local version=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --json) JSON=1; shift ;;
      -h|--help) usage; return 0 ;;
      -*)
        echo "release.sh verify: unknown flag $1" >&2
        usage >&2
        return 2
        ;;
      *)
        if [ -n "$version" ]; then
          echo "release.sh verify: unexpected argument $1" >&2
          return 2
        fi
        version="$1"
        shift
        ;;
    esac
  done
  if [ -z "$version" ]; then
    echo "release.sh verify: missing <version>" >&2
    usage >&2
    return 2
  fi

  local tag="v$version" overall_ok=0
  # tmpdir is intentionally not `local`: the EXIT trap below fires at
  # actual process exit, by which point cmd_verify's own local scope is
  # long gone, and `set -u` would call a local out-of-scope var unbound.
  tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/yogurt-release-verify.XXXXXX")"
  trap 'rm -rf "$tmpdir"' EXIT

  # `gh release download` never sets com.apple.quarantine (that xattr is
  # a browser download's doing), so untarring the binary below is safe -
  # it never triggers a Gatekeeper prompt.
  if ! gh release download "$tag" -R "$REPO" -D "$tmpdir" --clobber \
      -p 'yogurt-aarch64-apple-darwin.tar.gz' \
      -p 'yogurt-x86_64-apple-darwin.tar.gz' \
      -p 'SHA256SUMS' >/dev/null 2>&1; then
    emit_check release_assets false "could not download release assets for $tag from $REPO - does the GitHub Release exist?" || overall_ok=1
  else
    local sums="$tmpdir/SHA256SUMS"
    local arm_tar="$tmpdir/yogurt-aarch64-apple-darwin.tar.gz"
    local x86_tar="$tmpdir/yogurt-x86_64-apple-darwin.tar.gz"
    local arm_local x86_local arm_sums x86_sums
    arm_local="$(shasum -a 256 "$arm_tar" | awk '{print $1}')"
    x86_local="$(shasum -a 256 "$x86_tar" | awk '{print $1}')"
    arm_sums="$(sha256sums_get "$sums" "yogurt-aarch64-apple-darwin.tar.gz")"
    x86_sums="$(sha256sums_get "$sums" "yogurt-x86_64-apple-darwin.tar.gz")"
    VERIFY_ARM_SHA="$arm_local"
    VERIFY_X86_SHA="$x86_local"

    check_eq sha256_aarch64 "arm64 tarball sha256 matches SHA256SUMS" "$arm_local" "$arm_sums" || overall_ok=1
    check_eq sha256_x86_64 "x86_64 tarball sha256 matches SHA256SUMS" "$x86_local" "$x86_sums" || overall_ok=1

    local ref formula="$tmpdir/yogurt.rb"
    ref="$(tap_formula_ref "$version")"
    if gh api "repos/$TAP_REPO/contents/Formula/yogurt.rb?ref=$ref" --jq '.content' 2>/dev/null | base64 -d >"$formula" 2>/dev/null && [ -s "$formula" ]; then
      emit_check formula_fetch true "fetched Formula/yogurt.rb from $TAP_REPO@$ref" PASS
      local formula_arm formula_x86
      formula_arm="$(formula_shas "$formula" | sed -n '1p')"
      formula_x86="$(formula_shas "$formula" | sed -n '2p')"
      check_eq formula_sha_aarch64 "formula arm64 sha256 matches SHA256SUMS" "$formula_arm" "$arm_sums" || overall_ok=1
      check_eq formula_sha_x86_64 "formula x86_64 sha256 matches SHA256SUMS" "$formula_x86" "$x86_sums" || overall_ok=1
      check_eq formula_version "formula version matches" "$(formula_version "$formula")" "$version" || overall_ok=1
    else
      emit_check formula_fetch false "could not fetch Formula/yogurt.rb from $TAP_REPO@$ref" || overall_ok=1
    fi

    local host_tar
    host_tar="$(host_tarball)"
    if [ -z "$host_tar" ]; then
      emit_check binary_version false "unsupported host arch $(uname -m), cannot untar and run yogurt --version" || overall_ok=1
    else
      (cd "$tmpdir" && tar -xzf "$host_tar")
      local printed
      printed="$("$tmpdir/yogurt" --version 2>&1 || true)"
      check_eq binary_version "extracted $host_tar's yogurt --version" "$printed" "yogurt $version" || overall_ok=1
    fi
  fi

  if [ "$JSON" -eq 1 ]; then
    (IFS=,; printf '[%s]\n' "${CHECK_JSON[*]}")
  fi
  return "$overall_ok"
}

# ---- finish -------------------------------------------------------

# find_tap_pr <version> - "<number> <state>" for the bump-<version> PR on
# the tap repo, or empty if none exists.
find_tap_pr() {
  gh pr list --repo "$TAP_REPO" --head "bump-$1" --state all \
    --json number,state --jq '.[0] | "\(.number) \(.state)"' 2>/dev/null
}

# brew_installed_version - the version brew reports for the yogurt
# formula, or empty if not installed.
brew_installed_version() {
  brew list --versions jarvisrchen/yogurt/yogurt 2>/dev/null | awk '{print $2}'
}

# highest_tag_below <version> - reads candidate tag names (one per line,
# e.g. "v0.6.0") on stdin and prints the highest v* one strictly below
# <version>, or empty if none. Pure - no git calls - so it is testable
# with a synthetic list; ordering is semver (v0.10.0 > v0.7.0), not
# lexical (where the string "v0.10.0" sorts below "v0.7.0").
highest_tag_below() {
  local version="$1" best="" tag ver
  while IFS= read -r tag; do
    case "$tag" in v*) ;; *) continue ;; esac
    ver="${tag#v}"
    [ "$ver" = "$version" ] && continue
    semver_lt "$ver" "$version" || continue
    if [ -z "$best" ] || semver_lt "${best#v}" "$ver"; then
      best="$tag"
    fi
  done
  printf '%s' "$best"
}

# previous_tag <version> - the highest existing v* tag strictly below
# <version>, or empty if none. Reads `git ls-remote --tags origin`, never
# local tags: preflight's check_tag_available established that local tags
# can lie (a stale one after a deleted-and-repushed release), and CI's
# fetch-depth-1 checkout has no local tags to read at all.
previous_tag() {
  git ls-remote --tags origin \
    | awk '{print $2}' | sed -e 's#^refs/tags/##' -e 's/\^{}$//' \
    | highest_tag_below "$1"
}

# render_log_row <version> <date> <push_run> <dry_run> <arm_sha> <x86_sha>
# <prev_tag> <ships> - the docs/RELEASE-LOG.md row `finish` prints, with a
# literal NARRATIVE: slot for the human sentence.
render_log_row() {
  local version="$1" date="$2" push_run="$3" dry_run="$4" arm_sha="$5" x86_sha="$6" prev_tag="$7" ships="$8"
  local run_url="https://github.com/$REPO/actions/runs"
  printf '| v%s | %s | NARRATIVE: <one sentence - what this release ships and why>. All four jobs green ([%s](%s/%s)) after a clean dry run ([%s](%s/%s)). Formula shas verified against re-downloaded tarballs (`%.8s...` arm64, `%.8s...` x86_64). Ships since %s: %s |\n' \
    "$version" "$date" "$push_run" "$run_url" "$push_run" "$dry_run" "$run_url" "$dry_run" "$arm_sha" "$x86_sha" "${prev_tag:-(none)}" "$ships"
}

# ships_since <prev_tag> <version> - "hash subject; hash subject; ..." for
# every commit between <prev_tag> (or the start of history, if empty) and
# v<version>.
ships_since() {
  local prev_tag="$1" version="$2" range
  if [ -n "$prev_tag" ]; then
    range="${prev_tag}..v${version}"
  else
    range="v${version}"
  fi
  git log "$range" --oneline | awk '{printf "%s%s", (NR>1?"; ":""), $0}'
}

# print_finish_plan <version> <no_smoke> - the mutating commands `finish`
# would run, without running them.
print_finish_plan() {
  local version="$1" no_smoke="$2" pr tap_pr_num tap_pr_state installed
  pr="$(find_tap_pr "$version")"
  tap_pr_num="${pr%% *}"
  tap_pr_state="${pr#* }"

  echo "PLAN for finish v$version:"
  if [ -z "$pr" ]; then
    echo "  (no tap PR found for branch bump-$version on $TAP_REPO - finish would fail here)"
  elif [ "$tap_pr_state" = "OPEN" ]; then
    echo "  gh pr merge $tap_pr_num --repo $TAP_REPO --squash --delete-branch"
  else
    echo "  (tap PR #$tap_pr_num already $tap_pr_state, nothing to merge)"
  fi
  echo '  git -C "$(brew --repo jarvisrchen/yogurt)" pull --ff-only'
  if [ "$no_smoke" -eq 1 ]; then
    echo "  (--no-smoke: brew upgrade/reinstall, brew test and the quarantine check are skipped)"
  else
    installed="$(brew_installed_version)"
    if [ "$installed" = "$version" ]; then
      echo "  brew reinstall jarvisrchen/yogurt/yogurt"
    else
      echo "  brew upgrade jarvisrchen/yogurt/yogurt"
    fi
    echo "  brew test jarvisrchen/yogurt/yogurt"
  fi
}

# cmd_finish <version> [--no-smoke] [-n] - runs verify, merges the tap
# PR, upgrades the local brew install, and prints the pre-filled log row.
# Every step is skip-if-done, so a re-run after a partial failure resumes.
cmd_finish() {
  local version="" no_smoke=0 dry=0
  while [ $# -gt 0 ]; do
    case "$1" in
      --no-smoke) no_smoke=1; shift ;;
      -n) dry=1; shift ;;
      -h|--help) usage; return 0 ;;
      -*)
        echo "release.sh finish: unknown flag $1" >&2
        usage >&2
        return 2
        ;;
      *)
        if [ -n "$version" ]; then
          echo "release.sh finish: unexpected argument $1" >&2
          return 2
        fi
        version="$1"
        shift
        ;;
    esac
  done
  if [ -z "$version" ]; then
    echo "release.sh finish: missing <version>" >&2
    usage >&2
    return 2
  fi

  if ! cmd_verify "$version"; then
    echo "FAIL: verify did not pass for v$version - fix before finishing" >&2
    return 1
  fi

  if [ "$dry" -eq 1 ]; then
    print_finish_plan "$version" "$no_smoke"
    return 0
  fi

  local pr tap_pr_num tap_pr_state
  pr="$(find_tap_pr "$version")"
  if [ -z "$pr" ]; then
    echo "FAIL: no tap PR found for branch bump-$version on $TAP_REPO" >&2
    return 1
  fi
  tap_pr_num="${pr%% *}"
  tap_pr_state="${pr#* }"
  if [ "$tap_pr_state" = "OPEN" ]; then
    gh pr merge "$tap_pr_num" --repo "$TAP_REPO" --squash --delete-branch
    echo "ok: merged tap PR #$tap_pr_num"
  else
    echo "ok: tap PR #$tap_pr_num already $tap_pr_state"
  fi

  if [ "$no_smoke" -eq 0 ]; then
    local tap_dir
    tap_dir="$(brew --repo jarvisrchen/yogurt)"
    git -C "$tap_dir" pull --ff-only

    local installed
    installed="$(brew_installed_version)"
    if [ "$installed" = "$version" ]; then
      brew reinstall jarvisrchen/yogurt/yogurt
    else
      brew upgrade jarvisrchen/yogurt/yogurt
    fi

    local brew_bin got
    brew_bin="$(brew --prefix)/bin/yogurt"
    got="$("$brew_bin" --version)"
    if [ "$got" != "yogurt $version" ]; then
      echo "FAIL: $brew_bin --version printed '$got', want 'yogurt $version'" >&2
      return 1
    fi
    echo "PASS: $brew_bin --version prints yogurt $version"

    brew test jarvisrchen/yogurt/yogurt
    echo "ok: brew test jarvisrchen/yogurt/yogurt passed"

    if xattr -p com.apple.quarantine "$brew_bin" >/dev/null 2>&1; then
      echo "FAIL: $brew_bin carries com.apple.quarantine" >&2
      return 1
    fi
    echo "PASS: $brew_bin has no com.apple.quarantine"
  fi

  local tag_sha parent_sha push_run dry_run prev_tag ships today
  tag_sha="$(gh api "repos/$REPO/commits/v$version" --jq '.sha')"
  parent_sha="$(gh api "repos/$REPO/commits/$tag_sha" --jq '.parents[0].sha')"
  push_run="$(gh run list -R "$REPO" -w Release -c "$tag_sha" --json databaseId,event --jq '[.[] | select(.event=="push")][0].databaseId')"
  dry_run="$(gh run list -R "$REPO" -w Release -c "$parent_sha" --json databaseId,event --jq '[.[] | select(.event=="workflow_dispatch")][0].databaseId')"
  prev_tag="$(previous_tag "$version")"
  ships="$(ships_since "$prev_tag" "$version")"
  today="$(date +%Y-%m-%d)"

  echo
  echo "docs/RELEASE-LOG.md row:"
  render_log_row "$version" "$today" "${push_run:-?}" "${dry_run:-?}" "$VERIFY_ARM_SHA" "$VERIFY_X86_SHA" "$prev_tag" "$ships"
}

# ---- untag ----------------------------------------------------------

# cmd_untag <version> [-n] - deletes the local and remote v<version> tag.
# Refuses (exit 2) while a GitHub Release v<version> exists.
cmd_untag() {
  local version="" dry=0
  while [ $# -gt 0 ]; do
    case "$1" in
      -n) dry=1; shift ;;
      -h|--help) usage; return 0 ;;
      -*)
        echo "release.sh untag: unknown flag $1" >&2
        usage >&2
        return 2
        ;;
      *)
        if [ -n "$version" ]; then
          echo "release.sh untag: unexpected argument $1" >&2
          return 2
        fi
        version="$1"
        shift
        ;;
    esac
  done
  if [ -z "$version" ]; then
    echo "release.sh untag: missing <version>" >&2
    usage >&2
    return 2
  fi

  local tag="v$version"
  if gh release view "$tag" -R "$REPO" >/dev/null 2>&1; then
    echo "release.sh untag: GitHub Release $tag already exists on $REPO" >&2
    echo 'fix Formula/yogurt.rb by hand in the tap repo instead - see docs/RELEASING.md "When it goes wrong"' >&2
    return 2
  fi

  local remote_exists=0 local_exists=0
  git ls-remote --tags origin "refs/tags/$tag" | grep -q . && remote_exists=1
  git rev-parse -q --verify "refs/tags/$tag" >/dev/null 2>&1 && local_exists=1

  if [ "$dry" -eq 1 ]; then
    echo "PLAN for untag $tag:"
    [ "$remote_exists" -eq 1 ] && echo "  git push origin :refs/tags/$tag"
    [ "$local_exists" -eq 1 ] && echo "  git tag -d $tag"
    if [ "$remote_exists" -eq 0 ] && [ "$local_exists" -eq 0 ]; then
      echo "  (nothing to do: $tag does not exist locally or on origin)"
    fi
    return 0
  fi

  if [ "$remote_exists" -eq 1 ]; then
    git push origin ":refs/tags/$tag"
    echo "ok: deleted remote tag $tag"
  else
    echo "ok: remote tag $tag does not exist, nothing to delete"
  fi
  if [ "$local_exists" -eq 1 ]; then
    git tag -d "$tag"
    echo "ok: deleted local tag $tag"
  else
    echo "ok: local tag $tag does not exist, nothing to delete"
  fi
}

main() {
  if [ $# -eq 0 ]; then
    usage >&2
    exit 2
  fi
  local cmd="$1"
  shift
  case "$cmd" in
    preflight) cmd_preflight "$@" ;;
    verify) cmd_verify "$@" ;;
    finish) cmd_finish "$@" ;;
    untag) cmd_untag "$@" ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "release.sh: unknown command '$cmd'" >&2
      usage >&2
      exit 2
      ;;
  esac
}

# Skip when sourced (scripts/tests/release_test.sh does this to reach the
# pure functions directly) so sourcing never runs a command or exits.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
