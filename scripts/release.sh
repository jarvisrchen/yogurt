#!/usr/bin/env bash
# release — subcommands that back the release skill (.claude/skills/release/SKILL.md).
#
# Bash plus `gh` and `jq`, in the style of scripts/publish-model-mirror.sh.
# Flags only, no prompts. `-n` prints the plan on anything that mutates.
# `--json` emits a per-check array where the subcommand has checks.
#
# Only `preflight` exists so far (DX-7 PR 1 of 3). `verify`, `finish`,
# `untag` and `ship` land in follow-up PRs — see docs/TODO.md DX-7 and
# docs/.planning/agent-workflow.md section 4C.
#
# Usage errors exit 2. A failed check exits 1.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/docs-only.sh
source "$REPO_ROOT/scripts/lib/docs-only.sh"

REPO="jarvisrchen/yogurt"

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

More commands (verify, finish, untag, ship) land in follow-up PRs.
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

# emit_check <name> <true|false> <detail> — prints "ok: "/"FAIL: " in text
# mode, records a JSON object in --json mode. Returns the check's own
# ok/fail status so callers can `check_x || OVERALL_OK=1`.
emit_check() {
  local name="$1" ok="$2" detail="$3"
  if [ "$JSON" -eq 1 ]; then
    CHECK_JSON+=("{\"check\":\"$name\",\"ok\":$ok,\"detail\":\"$(json_escape "$detail")\"}")
  elif [ "$ok" = "true" ]; then
    printf 'ok: %s\n' "$detail"
  else
    printf 'FAIL: %s\n' "$detail"
  fi
  [ "$ok" = "true" ]
}

# ---- semver -----------------------------------------------------------

# semver_lt a b — true when a < b, comparing X.Y.Z numerically.
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

# resolve_ci_status <sha> — walks first-parent ancestors of <sha> (up to
# 30) looking for a CI run, skipping over commits CI legitimately skipped
# because they were docs-only. Prints "STATUS|sha|detail" on stdout.
resolve_ci_status() {
  local cur="$1" i=0 runs count status conclusion parent changed
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
    if ! is_docs_only $changed; then
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

main() {
  if [ $# -eq 0 ]; then
    usage >&2
    exit 2
  fi
  local cmd="$1"
  shift
  case "$cmd" in
    preflight) cmd_preflight "$@" ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "release.sh: unknown command '$cmd'" >&2
      usage >&2
      exit 2
      ;;
  esac
}

main "$@"
