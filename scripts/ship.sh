#!/usr/bin/env bash
# ship - `pr` validates and opens a PR for the current branch; `land` waits
# for CI, squash-merges it, and cleans up the worktree/branch. Design:
# docs/.planning/agent-workflow.md section 4B (B1, B2).
#
# BSD awk / BSD sed only (no gawk, no GNU-only flags) - stock macOS 13+
# toolchain, same constraint as scripts/task.sh.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/docs-only.sh
source "$REPO_ROOT/scripts/lib/docs-only.sh"

# ── shared helpers ──────────────────────────────────────────────────

usage() {
  cat >&2 <<'EOF'
usage: ship.sh pr <title> --body-file <f> [--draft] [--dry-run]
       ship.sh land [pr] [--dry-run]
EOF
}

die() {
  echo "error: $1" >&2
  [ -n "${2:-}" ] && echo "fix: $2" >&2
  exit "${3:-1}"
}

# Set by cmd_land, read by the EXIT trap that cleans it up - has to be
# global, not a cmd_land local: a trap set inside a function still fires
# after that function (and its locals) have gone out of scope.
LAND_BODY_FILE=""
trap 'rm -f "$LAND_BODY_FILE"' EXIT

# The main checkout, from any worktree - same trick scripts/task.sh uses.
# YOGURT_MAIN overrides it, for tests that run against a throwaway repo.
resolve_main() {
  local raw
  if [ -n "${YOGURT_MAIN:-}" ]; then
    raw="$YOGURT_MAIN"
  else
    raw="$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")"
  fi
  (cd "$raw" && pwd -P)
}

# Ticket ID from a title of the form "XX-123: ..." - empty if the title
# uses the conventional-commit shape instead.
extract_ticket_id() {
  local title="$1"
  if [[ "$title" =~ ^([A-Z]{2,4}-[0-9]+):\  ]]; then
    printf '%s' "${BASH_REMATCH[1]}"
  fi
}

# Strips fenced code blocks (```...```) and inline backtick spans from a
# file, leaving prose only - used before scanning for an em dash so a code
# example that legitimately contains one doesn't trip the rule.
strip_code_spans() {
  awk '
    /^```/ { infence = !infence; print ""; next }
    infence { print ""; next }
    { print }
  ' "$1" | sed -E 's/`[^`]*`//g'
}

# heading_level "### foo" -> 3
heading_level() {
  local s="$1" n=0
  while [ "${s:n:1}" = "#" ]; do n=$((n + 1)); done
  printf '%s' "$n"
}

# Removes the section starting at a heading matching "^#+ *Manual test"
# through the next heading of the same or higher level, or EOF.
strip_manual_test_section() {
  awk '
    function level(s,   n) { n = 0; while (substr(s, n + 1, 1) == "#") n++; return n }
    {
      is_heading = ($0 ~ /^#+([ \t]|$)/)
      if (skip && is_heading) {
        if (level($0) <= skip_level) skip = 0
      }
      if (!skip && is_heading && $0 ~ /^#+ *Manual test/) {
        skip = 1
        skip_level = level($0)
        next
      }
      if (skip) next
      print
    }
  ' "$1"
}

# The inverse of strip_manual_test_section: only the Manual test section
# (heading included), for land's final re-print.
extract_manual_test_section() {
  awk '
    function level(s,   n) { n = 0; while (substr(s, n + 1, 1) == "#") n++; return n }
    {
      is_heading = ($0 ~ /^#+([ \t]|$)/)
      if (grab && is_heading) {
        if (level($0) <= grab_level) grab = 0
      }
      if (!grab && is_heading && $0 ~ /^#+ *Manual test/) {
        grab = 1
        grab_level = level($0)
      }
      if (grab) print
    }
  ' "$1"
}

# Rewrites a "cd <worktree>/yogurt-worktrees/<slug> && ..." handover line
# to point at the main checkout instead.
rewrite_handover_for_main() {
  local main="$1"
  sed -E "s#cd (/Users/[^ ]+|~)/[^ ]*yogurt-worktrees/[^ ]+ (&&|\;)#cd $main \\2#"
}

# ── shared checks (pr and land) ──────────────────────────────────────

# Refuses unless title carries an ID that is checked off in
# docs/TODO-DONE.md at $2 (a git ref, e.g. HEAD or a branch name).
check_ticket_done() {
  local title="$1" ref="$2" id
  id="$(extract_ticket_id "$title")"
  [ -z "$id" ] && return 0
  git show "$ref:docs/TODO-DONE.md" 2>/dev/null | grep -q "^- \[x\] \*\*$id\*\*" ||
    die "$id is not checked off in docs/TODO-DONE.md on $ref" \
        "just ticket done $id --note-file <path>" 1
}

# ── pr ────────────────────────────────────────────────────────────

check_worktree_branch() {
  local gitdir commondir branch
  gitdir="$(git rev-parse --path-format=absolute --git-dir)"
  commondir="$(git rev-parse --path-format=absolute --git-common-dir)"
  [ "$gitdir" != "$commondir" ] ||
    die "run this from a linked worktree, not the main checkout" "just start <ID> [words]" 1
  branch="$(git rev-parse --abbrev-ref HEAD)"
  [ "$branch" != "main" ] ||
    die "on branch main" "just start <ID> [words]" 1
  printf '%s' "$branch"
}

check_title() {
  local title="$1"
  if [[ "$title" =~ ^[A-Z]{2,4}-[0-9]+:\  ]] ||
     [[ "$title" =~ ^(docs|chore|ci|fix|feat|test|build)(\(.+\))?:\  ]]; then
    return 0
  fi
  die "title '$title' has no ticket ID or conventional prefix" \
      "use 'XX-123: ...' or 'docs|chore|ci|fix|feat|test|build(scope): ...'" 1
}

check_body_attribution_and_em_dash() {
  local body_file="$1" stripped line em_dash
  # Checked against the body with code spans/fences blanked out - a PR
  # about this very script needs to say "Generated with" in a code span
  # without tripping the rule it is describing.
  stripped="$(strip_code_spans "$body_file")"
  line="$(printf '%s\n' "$stripped" | grep -ni 'generated with' | head -1 || true)"
  [ -n "$line" ] && die "PR body contains \"Generated with\" outside a code span: $line" "remove the agent attribution" 1
  line="$(printf '%s\n' "$stripped" | grep -nEi 'co-authored-by:.*(claude|anthropic|codex|cursor|opencode|copilot)' | head -1 || true)"
  [ -n "$line" ] && die "PR body contains a Co-Authored-By trailer naming an agent outside a code span: $line" "remove it" 1
  em_dash="$(printf '\xe2\x80\x94')"
  line="$(printf '%s\n' "$stripped" | grep -nF "$em_dash" | head -1 || true)"
  [ -n "$line" ] && die "PR body contains an em dash outside a code span: $line" "use a plain '-' instead" 1
  return 0
}

check_handover() {
  local body_file="$1" merge_base changed p touches_code=false
  merge_base="$(git merge-base origin/main HEAD)"
  changed="$(git diff --name-only "$merge_base" HEAD)"
  while IFS= read -r p; do
    [ -z "$p" ] && continue
    case "$p" in
      crates/*|web/src/*|justfile|scripts/*) touches_code=true; break ;;
    esac
  done <<< "$changed"
  $touches_code || return 0
  grep -Eq 'cd (/Users/[^ ]+|~)/.*yogurt-worktrees/[^ ]+ (&&|;) just ' "$body_file" ||
    die "code change (crates/, web/src, justfile or scripts/) but no absolute-path handover line in the PR body" \
        "add a line like: cd /Users/you/Documents/code/yogurt-worktrees/<slug> && just <recipe>" 1
}

cmd_pr() {
  local title="" body_file="" draft=false dry_run=false
  while [ $# -gt 0 ]; do
    case "$1" in
      --body-file) body_file="${2:-}"; shift 2 ;;
      --draft) draft=true; shift ;;
      --dry-run) dry_run=true; shift ;;
      -h|--help) usage; exit 0 ;;
      -*) die "ship.sh pr: unknown flag $1" "" 2 ;;
      *) [ -z "$title" ] && { title="$1"; shift; } || die "ship.sh pr: unexpected argument $1" "" 2 ;;
    esac
  done
  [ -z "$title" ] && { usage; exit 2; }
  [ -z "$body_file" ] && { usage; exit 2; }
  [ -f "$body_file" ] || die "body file not found: $body_file" "" 1

  local branch
  branch="$(check_worktree_branch)"
  check_title "$title"
  check_body_attribution_and_em_dash "$body_file"
  check_handover "$body_file"
  check_ticket_done "$title" "HEAD"

  local create_args=(gh pr create --title "$title" --body-file "$body_file")
  $draft && create_args+=(--draft)

  if $dry_run; then
    echo "dry-run: all checks passed"
    echo "would push: git push -u origin $branch"
    echo "would create: ${create_args[*]}"
    exit 0
  fi

  git push -u origin "$branch"
  local url
  url="$("${create_args[@]}")"
  echo "$url"
  echo "next: just land"
}

# ── land ──────────────────────────────────────────────────────────

resolve_pr() {
  local pr_arg="$1" json err
  if [ -n "$pr_arg" ]; then
    if ! json="$(gh pr view "$pr_arg" --json number,headRefName,title,body,state 2>&1)"; then
      die "could not resolve PR $pr_arg" "$json" 1
    fi
  else
    if ! json="$(gh pr view --json number,headRefName,title,body,state 2>&1)"; then
      die "no PR for the current branch" "$json - open one with just pr" 1
    fi
  fi
  printf '%s' "$json"
}

find_worktree_for_branch() {
  local main="$1" branch="$2"
  git -C "$main" worktree list --porcelain | awk -v want="refs/heads/$branch" '
    /^worktree / { path = substr($0, 10); next }
    /^branch /   { if ($2 == want) { print path; exit } }
  '
}

land_preflight() {
  local main="$1" worktree_dir="$2" branch="$3" title="$4"

  if [ -n "$worktree_dir" ] && [ -d "$worktree_dir" ]; then
    local dirty
    dirty="$(git -C "$worktree_dir" status --porcelain 2>/dev/null || true)"
    [ -z "$dirty" ] ||
      die "worktree $worktree_dir has uncommitted changes" "commit or stash them, then run just land again" 1
  fi

  local local_sha origin_sha
  local_sha="$(git -C "$main" rev-parse "refs/heads/$branch" 2>/dev/null)" ||
    die "branch $branch not found in $main" "git -C $main fetch origin $branch:$branch" 1
  origin_sha="$(git -C "$main" rev-parse "origin/$branch" 2>/dev/null)" ||
    die "origin/$branch not found" "push your branch first: just pr ..." 1
  [ "$local_sha" = "$origin_sha" ] ||
    die "HEAD not pushed: $branch ($local_sha) differs from origin/$branch ($origin_sha)" \
        "push your latest commits" 1

  check_ticket_done "$title" "refs/heads/$branch"
}

# Prints "ci skipped (docs-only)", or waits (real run) / announces (dry
# run) CI on <pr_num>, capped around 5 minutes. Exits 1 on red or timeout.
land_ci_check() {
  local main="$1" branch="$2" pr_num="$3" dry_run="$4"
  local merge_base changed p all_docs=1 any=0
  merge_base="$(git -C "$main" merge-base origin/main "origin/$branch")"
  changed="$(git -C "$main" diff --name-only "$merge_base" "origin/$branch")"
  while IFS= read -r p; do
    [ -z "$p" ] && continue
    any=1
    is_docs_only "$p" || { all_docs=0; break; }
  done <<< "$changed"
  if [ "$any" -eq 1 ] && [ "$all_docs" -eq 1 ]; then
    echo "ci skipped (docs-only)"
    return 0
  fi

  if $dry_run; then
    echo "would wait for CI: gh pr checks $pr_num --watch --fail-fast (cap ~5m)"
    return 0
  fi

  if command -v timeout >/dev/null 2>&1; then
    if timeout 300 gh pr checks "$pr_num" --watch --fail-fast; then
      echo "ci: green"
      return 0
    fi
    local status=$?
    if [ "$status" -eq 124 ]; then
      echo "ci: still running after 5m - run just land again to resume" >&2
      exit 1
    fi
    land_ci_report_failure "$pr_num"
    exit 1
  fi

  # No GNU timeout on stock macOS: poll gh pr checks --json ourselves.
  local waited=0 checks fail_count pending_count
  while [ "$waited" -lt 300 ]; do
    checks="$(gh pr checks "$pr_num" --json name,bucket 2>/dev/null || echo '[]')"
    fail_count="$(printf '%s' "$checks" | grep -c '"bucket":"fail"' || true)"
    if [ "$fail_count" -gt 0 ]; then
      land_ci_report_failure "$pr_num"
      exit 1
    fi
    pending_count="$(printf '%s' "$checks" | grep -c '"bucket":"pending"' || true)"
    if [ "$checks" != "[]" ] && [ "$pending_count" -eq 0 ]; then
      echo "ci: green"
      return 0
    fi
    sleep 10
    waited=$((waited + 10))
  done
  echo "ci: still running after 5m - run just land again to resume" >&2
  exit 1
}

land_ci_report_failure() {
  local pr_num="$1" failing
  failing="$(gh pr checks "$pr_num" --json name,bucket --jq '.[] | select(.bucket=="fail") | .name' 2>/dev/null || true)"
  echo "ci failed:" >&2
  printf '%s\n' "$failing" >&2
  gh run view --log-failed >&2 2>&1 || true
}

land_cleanup() {
  local main="$1" worktree_dir="$2" branch="$3" dry_run="$4"

  if [ -n "$worktree_dir" ] && [ -d "$worktree_dir" ]; then
    local dirty
    dirty="$(git -C "$worktree_dir" status --porcelain 2>/dev/null || true)"
    if [ -n "$dirty" ]; then
      die "worktree $worktree_dir is dirty, refusing to remove" \
          "$(printf '%s' "$dirty" | tr '\n' ' ')" 1
    fi
    if $dry_run; then
      echo "would run: git worktree remove $worktree_dir"
    else
      git worktree remove "$worktree_dir"
      echo "removed worktree $worktree_dir"
    fi
  elif [ -n "$worktree_dir" ]; then
    echo "worktree $worktree_dir already removed"
  else
    echo "no worktree found for branch $branch - skipping worktree removal"
  fi

  if git show-ref --verify --quiet "refs/heads/$branch"; then
    if $dry_run; then
      echo "would run: git branch -D $branch"
    else
      git branch -D "$branch"
      echo "deleted local branch $branch"
    fi
  else
    echo "local branch $branch already deleted"
  fi

  if $dry_run; then
    echo "would run: git fetch origin --prune; git push origin --delete $branch (if it still exists); git fetch origin --prune"
    return 0
  fi

  git fetch origin --prune --quiet
  if git ls-remote --exit-code --heads origin "$branch" >/dev/null 2>&1; then
    git push origin --delete "$branch"
    echo "deleted origin branch $branch"
  else
    echo "origin branch $branch already deleted"
  fi
  git fetch origin --prune --quiet
}

cmd_land() {
  local pr_arg="" dry_run=false
  while [ $# -gt 0 ]; do
    case "$1" in
      --dry-run) dry_run=true; shift ;;
      -h|--help) usage; exit 0 ;;
      -*) die "ship.sh land: unknown flag $1" "" 2 ;;
      *) [ -z "$pr_arg" ] && { pr_arg="$1"; shift; } || die "ship.sh land: unexpected argument $1" "" 2 ;;
    esac
  done

  local json
  json="$(resolve_pr "$pr_arg")"

  local pr_num branch pr_title pr_state
  pr_num="$(printf '%s' "$json" | jq -r '.number')"
  branch="$(printf '%s' "$json" | jq -r '.headRefName')"
  pr_title="$(printf '%s' "$json" | jq -r '.title')"
  pr_state="$(printf '%s' "$json" | jq -r '.state')"
  LAND_BODY_FILE="$(mktemp)"
  local pr_body_file="$LAND_BODY_FILE"
  printf '%s' "$json" | jq -r '.body' > "$pr_body_file"

  local main worktree_dir
  main="$(resolve_main)"
  worktree_dir="$(find_worktree_for_branch "$main" "$branch")"

  echo "land: PR #$pr_num ($branch), state=$pr_state"

  if [ "$pr_state" = "MERGED" ]; then
    echo "already merged - skipping to cleanup"
  else
    land_preflight "$main" "$worktree_dir" "$branch" "$pr_title"
    land_ci_check "$main" "$branch" "$pr_num" "$dry_run"

    local sha squash_body
    sha="$(git -C "$main" rev-parse "origin/$branch")"
    squash_body="$(strip_manual_test_section "$pr_body_file")"
    if $dry_run; then
      echo "would run: gh pr merge $pr_num --squash --match-head-commit $sha --subject \"$pr_title (#$pr_num)\" --body <PR body minus Manual test>"
    else
      gh pr merge "$pr_num" --squash --match-head-commit "$sha" \
        --subject "$pr_title (#$pr_num)" --body "$squash_body"
      echo "merged #$pr_num"
    fi
  fi

  cd "$main"
  land_cleanup "$main" "$worktree_dir" "$branch" "$dry_run"

  if $dry_run; then
    return 0
  fi

  echo
  extract_manual_test_section "$pr_body_file" | rewrite_handover_for_main "$main"
  echo "cwd is gone: cd $main"
}

# ── dispatch ─────────────────────────────────────────────────────────

main() {
  if [ $# -eq 0 ]; then
    usage
    exit 2
  fi
  local cmd="$1"
  shift
  case "$cmd" in
    pr) cmd_pr "$@" ;;
    land) cmd_land "$@" ;;
    -h|--help) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
}

# Skip when sourced, matching scripts/release.sh, so sourcing never runs a
# command or exits.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
