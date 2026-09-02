#!/usr/bin/env bash
# Runnable check for scripts/ship.sh's `pr` and `land` subcommands, against
# a throwaway bare repo + main checkout + worktrees, and a stub `gh` on
# PATH that records its argv and prints canned JSON. No framework - plain
# assertions, bash 3.2 / BSD tools only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHIP="$SCRIPT_DIR/../ship.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass=0
fail=0
t() {
  local desc="$1"; shift
  if "$@"; then
    pass=$((pass + 1))
  else
    echo "FAIL: $desc"
    fail=$((fail + 1))
  fi
}

# ── stub gh ───────────────────────────────────────────────────────────

BIN="$WORK/bin"
mkdir -p "$BIN"
cat > "$BIN/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
LOG="${GH_STUB_LOG:?GH_STUB_LOG not set}"
printf '%s\n' "$*" >> "$LOG"
case "${1:-}/${2:-}" in
  pr/view)
    cat "${GH_STUB_PR_VIEW_FILE:?GH_STUB_PR_VIEW_FILE not set}"
    ;;
  pr/create)
    echo "https://github.com/example/example/pull/${GH_STUB_PR_NUMBER:-1}"
    ;;
  pr/checks)
    if printf '%s\n' "$*" | grep -q -- '--json'; then
      cat "${GH_STUB_PR_CHECKS_JSON:-/dev/null}"
    else
      exit "${GH_STUB_PR_CHECKS_EXIT:-0}"
    fi
    ;;
  pr/merge)
    echo "merged"
    ;;
  run/view)
    echo "gh-stub: run view log"
    ;;
  *)
    echo "gh-stub: unhandled: $*" >&2
    exit 1
    ;;
esac
STUB
chmod +x "$BIN/gh"

# ── origin + main checkout ──────────────────────────────────────────

BARE="$WORK/origin.git"
MAIN="$WORK/main-checkout"
WROOT="$WORK/yogurt-worktrees"
git init --bare -q "$BARE"
git init -q "$MAIN"
git -C "$MAIN" symbolic-ref HEAD refs/heads/main
git -C "$MAIN" config user.email t@t.example
git -C "$MAIN" config user.name t

mkdir -p "$MAIN/docs"
cat > "$MAIN/docs/TODO.md" <<'EOF'
# TODO

## Ticket IDs

| Prefix | Section |
| --- | --- |
| `MTG` | Meetings |
EOF
cat > "$MAIN/docs/TODO-DONE.md" <<'EOF'
# Done

- [x] **MTG-99** Ship the dummy crate
EOF
git -C "$MAIN" add -A
git -C "$MAIN" commit -q -m init
git -C "$MAIN" remote add origin "$BARE"
git -C "$MAIN" push -q origin main

# add_worktree <slug> - creates a worktree + branch off origin/main.
add_worktree() {
  git -C "$MAIN" worktree add -q "$WROOT/$1" -b "$1" origin/main
}

# ── body fixtures ────────────────────────────────────────────────────

EM_DASH="$(printf '\xe2\x80\x94')"

BODY_GOOD="$WORK/body-good.md"
cat > "$BODY_GOOD" <<EOF
## What
Docs update, nothing else.

## Verification
Ran check-docs.
EOF

BODY_GENERATED="$WORK/body-generated.md"
cat > "$BODY_GENERATED" <<EOF
## What
Docs update.

Generated with Fancy Tool.
EOF

BODY_COAUTHORED="$WORK/body-coauthored.md"
cat > "$BODY_COAUTHORED" <<EOF
## What
Docs update.

Co-Authored-By: Claude <noreply@anthropic.com>
EOF

BODY_EMDASH="$WORK/body-emdash.md"
cat > "$BODY_EMDASH" <<EOF
## What
This is a change ${EM_DASH} not a fix.
EOF

BODY_EMDASH_CODE="$WORK/body-emdash-code.md"
cat > "$BODY_EMDASH_CODE" <<EOF
## What
See \`a${EM_DASH}b\` for details.

\`\`\`
another ${EM_DASH} inside a fence
\`\`\`
EOF

BODY_WITH_HANDOVER="$WORK/body-with-handover.md"
cat > "$BODY_WITH_HANDOVER" <<'EOF'
## What
Ship the dummy crate.

## Manual test
cd /Users/test/Documents/code/yogurt-worktrees/code-change && just dev
1. Check the thing.
EOF

BODY_LAND="$WORK/body-land.md"
cat > "$BODY_LAND" <<'EOF'
## What
Doc update for the land test.

## Verification
Ran check-docs.

## Manual test
cd /Users/test/Documents/code/yogurt-worktrees/land-docs-merge && just dev
1. Confirm the docs render.

## Caveats
None.
EOF

# ── pr: worktree fixtures ────────────────────────────────────────────

add_worktree conv-docs
echo "notes" > "$WROOT/conv-docs/docs/NOTES.md"
git -C "$WROOT/conv-docs" add -A
git -C "$WROOT/conv-docs" commit -q -m "docs: notes"
git -C "$WROOT/conv-docs" push -q -u origin conv-docs

add_worktree code-change
mkdir -p "$WROOT/code-change/crates/dummy/src"
echo "// dummy" > "$WROOT/code-change/crates/dummy/src/lib.rs"
git -C "$WROOT/code-change" add -A
git -C "$WROOT/code-change" commit -q -m "feat: dummy crate"
git -C "$WROOT/code-change" push -q -u origin code-change

add_worktree ticket-not-done
echo "notes" > "$WROOT/ticket-not-done/docs/OTHER.md"
git -C "$WROOT/ticket-not-done" add -A
git -C "$WROOT/ticket-not-done" commit -q -m "docs: other"
git -C "$WROOT/ticket-not-done" push -q -u origin ticket-not-done

# run_pr <worktree> <title> <body> [args...] - runs `ship.sh pr` with a
# clean env (no GH_STUB_* set: `pr` never calls gh under --dry-run).
run_pr() {
  local wt="$1" title="$2" body="$3"; shift 3
  ( cd "$WROOT/$wt" && "$SHIP" pr "$title" --body-file "$body" --dry-run "$@" )
}

status=0
run_pr conv-docs "not a valid title" "$BODY_GOOD" >/dev/null 2>"$WORK/err" || status=$?
t "bad title is refused (exit 1)" [ "$status" -eq 1 ]
t "bad title message names the fix" grep -q "ticket ID or conventional prefix" "$WORK/err"

status=0
run_pr conv-docs "docs: update notes" "$BODY_GOOD" >"$WORK/out" 2>&1 || status=$?
t "conventional title, docs-only, no handover needed: accepted (exit 0)" [ "$status" -eq 0 ]
t "dry-run prints the plan" grep -q "would create" "$WORK/out"

status=0
run_pr code-change "MTG-99: ship the dummy crate" "$BODY_WITH_HANDOVER" >"$WORK/out" 2>"$WORK/err" || status=$?
t "ticket-ID title + handover present + ticket done: accepted (exit 0)" [ "$status" -eq 0 ]

status=0
run_pr conv-docs "docs: update notes" "$BODY_GENERATED" >/dev/null 2>"$WORK/err" || status=$?
t "\"Generated with\" in body is refused (exit 1)" [ "$status" -eq 1 ]
t "refusal names Generated with" grep -qi "generated with" "$WORK/err"

status=0
run_pr conv-docs "docs: update notes" "$BODY_COAUTHORED" >/dev/null 2>"$WORK/err" || status=$?
t "Co-Authored-By trailer naming an agent is refused (exit 1)" [ "$status" -eq 1 ]
t "refusal names Co-Authored-By" grep -qi "co-authored-by" "$WORK/err"

status=0
run_pr conv-docs "docs: update notes" "$BODY_EMDASH" >/dev/null 2>"$WORK/err" || status=$?
t "em dash in prose is refused (exit 1)" [ "$status" -eq 1 ]
t "refusal names em dash" grep -qi "em dash" "$WORK/err"

status=0
run_pr conv-docs "docs: update notes" "$BODY_EMDASH_CODE" >/dev/null 2>"$WORK/err" || status=$?
t "em dash inside a code span/fence is accepted (exit 0)" [ "$status" -eq 0 ]

status=0
run_pr code-change "MTG-99: ship the dummy crate" "$BODY_GOOD" >/dev/null 2>"$WORK/err" || status=$?
t "code change without a handover line is refused (exit 1)" [ "$status" -eq 1 ]
t "refusal names the handover line" grep -qi "handover" "$WORK/err"

status=0
run_pr ticket-not-done "MTG-1: not actually done" "$BODY_GOOD" >/dev/null 2>"$WORK/err" || status=$?
t "ticket not checked off in TODO-DONE is refused (exit 1)" [ "$status" -eq 1 ]
t "refusal names TODO-DONE" grep -qi "todo-done" "$WORK/err"

# ── land: docs-only CI skip + manual-test stripping ─────────────────

add_worktree land-docs-merge
echo "notes" > "$WROOT/land-docs-merge/docs/LAND.md"
git -C "$WROOT/land-docs-merge" add -A
git -C "$WROOT/land-docs-merge" commit -q -m "docs: land test"
git -C "$WROOT/land-docs-merge" push -q -u origin land-docs-merge

VIEW_LAND="$WORK/view-land.json"
jq -n --arg body "$(cat "$BODY_LAND")" \
  '{number: 42, headRefName: "land-docs-merge", title: "docs: land test", body: $body, state: "OPEN"}' \
  > "$VIEW_LAND"

LOG_LAND="$WORK/log-land"
: > "$LOG_LAND"
status=0
out="$(cd "$WROOT/land-docs-merge" && PATH="$BIN:$PATH" GH_STUB_LOG="$LOG_LAND" GH_STUB_PR_VIEW_FILE="$VIEW_LAND" YOGURT_MAIN="$MAIN" "$SHIP" land 2>&1)" || status=$?
t "land on a docs-only PR succeeds (exit 0)" [ "$status" -eq 0 ]
t "land prints ci skipped (docs-only)" bash -c 'printf "%s" "$1" | grep -q "ci skipped (docs-only)"' _ "$out"
t "land never calls gh pr checks for a docs-only PR" bash -c '! grep -q "^pr checks" "$1"' _ "$LOG_LAND"
# The merge call's --body arg embeds real newlines, so it spans several
# physical lines in the log file - check the whole file, not one grep line.
t "land calls gh pr merge" grep -q '^pr merge' "$LOG_LAND"
t "merge body excludes the Manual test section" bash -c '! grep -q "Manual test" "$1"' _ "$LOG_LAND"
t "merge body keeps the sections around it" grep -q "Caveats" "$LOG_LAND"
t "land removes the worktree" [ ! -d "$WROOT/land-docs-merge" ]
t "land deletes the local branch" bash -c '! git -C "$1" show-ref --verify --quiet refs/heads/land-docs-merge' _ "$MAIN"

# ── land: resume (already MERGED skips to cleanup) ──────────────────

add_worktree land-resume-clean
echo "notes" > "$WROOT/land-resume-clean/docs/RESUME.md"
git -C "$WROOT/land-resume-clean" add -A
git -C "$WROOT/land-resume-clean" commit -q -m "docs: resume test"
git -C "$WROOT/land-resume-clean" push -q -u origin land-resume-clean

VIEW_MERGED_CLEAN="$WORK/view-merged-clean.json"
jq -n '{number: 43, headRefName: "land-resume-clean", title: "docs: resume test", body: "## What\nx\n", state: "MERGED"}' \
  > "$VIEW_MERGED_CLEAN"

LOG_RESUME="$WORK/log-resume"
: > "$LOG_RESUME"
status=0
out="$(cd "$WROOT/land-resume-clean" && PATH="$BIN:$PATH" GH_STUB_LOG="$LOG_RESUME" GH_STUB_PR_VIEW_FILE="$VIEW_MERGED_CLEAN" YOGURT_MAIN="$MAIN" "$SHIP" land 2>&1)" || status=$?
t "land on an already-merged PR succeeds (exit 0)" [ "$status" -eq 0 ]
t "land on an already-merged PR skips to cleanup" bash -c 'printf "%s" "$1" | grep -q "already merged"' _ "$out"
t "land never re-merges an already-merged PR" bash -c '! grep -q "^pr merge" "$1"' _ "$LOG_RESUME"
t "resume removes the worktree" [ ! -d "$WROOT/land-resume-clean" ]
t "resume deletes the local branch" bash -c '! git -C "$1" show-ref --verify --quiet refs/heads/land-resume-clean' _ "$MAIN"

# ── land: refuses cleanup on a dirty worktree ────────────────────────

add_worktree land-resume-dirty
echo "notes" > "$WROOT/land-resume-dirty/docs/DIRTY.md"
git -C "$WROOT/land-resume-dirty" add -A
git -C "$WROOT/land-resume-dirty" commit -q -m "docs: dirty resume test"
git -C "$WROOT/land-resume-dirty" push -q -u origin land-resume-dirty
echo "uncommitted" > "$WROOT/land-resume-dirty/scratch.txt"

VIEW_MERGED_DIRTY="$WORK/view-merged-dirty.json"
jq -n '{number: 44, headRefName: "land-resume-dirty", title: "docs: dirty resume test", body: "## What\nx\n", state: "MERGED"}' \
  > "$VIEW_MERGED_DIRTY"

LOG_DIRTY="$WORK/log-dirty"
: > "$LOG_DIRTY"
status=0
( cd "$WROOT/land-resume-dirty" && PATH="$BIN:$PATH" GH_STUB_LOG="$LOG_DIRTY" GH_STUB_PR_VIEW_FILE="$VIEW_MERGED_DIRTY" YOGURT_MAIN="$MAIN" "$SHIP" land ) >/dev/null 2>"$WORK/err-dirty" || status=$?
t "land refuses to clean up a dirty worktree (exit 1)" [ "$status" -eq 1 ]
t "refusal mentions the dirty worktree" grep -qi "dirty" "$WORK/err-dirty"
t "dirty worktree is not removed" [ -d "$WROOT/land-resume-dirty" ]
t "branch survives a refused cleanup" bash -c 'git -C "$1" show-ref --verify --quiet refs/heads/land-resume-dirty' _ "$MAIN"

echo "ship_test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
