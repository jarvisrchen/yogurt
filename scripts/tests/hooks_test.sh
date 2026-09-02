#!/usr/bin/env bash
# Runs .githooks/commit-msg and .githooks/pre-commit directly against
# sample messages and a temp repo on main - no framework, plain asserts,
# bash 3.2 / BSD tools only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMMIT_MSG_HOOK="$REPO_ROOT/.githooks/commit-msg"
PRE_COMMIT_HOOK="$REPO_ROOT/.githooks/pre-commit"

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

EM_DASH="$(printf '\xe2\x80\x94')"

# ── commit-msg ───────────────────────────────────────────────────────

msg_file() {
  local f="$WORK/msg-$$-$RANDOM"
  cat > "$f"
  printf '%s' "$f"
}

f="$(msg_file <<< "DX-5: a fine commit message")"
status=0
"$COMMIT_MSG_HOOK" "$f" >/dev/null 2>"$WORK/err" || status=$?
t "commit-msg accepts a clean message (exit 0)" [ "$status" -eq 0 ]

f="$(msg_file <<< "DX-5: did a thing

Generated with Fancy Tool")"
status=0
"$COMMIT_MSG_HOOK" "$f" >/dev/null 2>"$WORK/err" || status=$?
t "commit-msg rejects Generated with (exit 1)" [ "$status" -eq 1 ]
t "commit-msg names the rule" grep -qi "generated with" "$WORK/err"

f="$(msg_file <<< "DX-5: did a thing

Co-Authored-By: Claude <noreply@anthropic.com>")"
status=0
"$COMMIT_MSG_HOOK" "$f" >/dev/null 2>"$WORK/err" || status=$?
t "commit-msg rejects a Co-Authored-By naming an agent (exit 1)" [ "$status" -eq 1 ]
t "commit-msg names the rule" grep -qi "co-authored-by" "$WORK/err"

f="$(msg_file <<< "DX-5: did a thing

Co-Authored-By: Jane Doe <jane@example.com>")"
status=0
"$COMMIT_MSG_HOOK" "$f" >/dev/null 2>"$WORK/err" || status=$?
t "commit-msg accepts a Co-Authored-By naming a human (exit 0)" [ "$status" -eq 0 ]

f="$(msg_file <<< "DX-5: a change ${EM_DASH} not a fix")"
status=0
"$COMMIT_MSG_HOOK" "$f" >/dev/null 2>"$WORK/err" || status=$?
t "commit-msg rejects an em dash (exit 1)" [ "$status" -eq 1 ]
t "commit-msg names the offending line" grep -qF "$EM_DASH" "$WORK/err"

# ── pre-commit ───────────────────────────────────────────────────────

REPO="$WORK/repo"
git init -q "$REPO"
git -C "$REPO" symbolic-ref HEAD refs/heads/main
git -C "$REPO" config user.email t@t.example
git -C "$REPO" config user.name t
touch "$REPO/README.md"
git -C "$REPO" add -A
git -C "$REPO" commit -q -m init

status=0
( cd "$REPO" && "$PRE_COMMIT_HOOK" ) >/dev/null 2>"$WORK/err-precommit" || status=$?
t "pre-commit refuses on main (exit 1)" [ "$status" -eq 1 ]
t "pre-commit names the fix" grep -q "just start" "$WORK/err-precommit"

git -C "$REPO" checkout -q -b feat/thing
status=0
( cd "$REPO" && "$PRE_COMMIT_HOOK" ) >/dev/null 2>"$WORK/err-precommit2" || status=$?
t "pre-commit allows a non-main branch (exit 0)" [ "$status" -eq 0 ]

echo "hooks_test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
