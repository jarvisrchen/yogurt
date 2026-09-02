#!/usr/bin/env bash
# Runnable check for scripts/task.sh's `start` subcommand: name derivation,
# claim detection, and the resume path - against a throwaway bare repo and
# clone, never the real yogurt-worktrees tree.
# No framework - plain assertions, bash 3.2 / BSD tools only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TASK="$SCRIPT_DIR/../task.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

BARE="$WORK/origin.git"
MAIN="$WORK/main-checkout"
git init --bare -q "$BARE"
git init -q "$MAIN"
git -C "$MAIN" symbolic-ref HEAD refs/heads/main
git -C "$MAIN" config user.email t@t.example
git -C "$MAIN" config user.name t
mkdir -p "$MAIN/scripts" "$MAIN/docs"
cp "$SCRIPT_DIR/../ticket.sh" "$MAIN/scripts/ticket.sh"
chmod +x "$MAIN/scripts/ticket.sh"

cat > "$MAIN/docs/TODO.md" <<'EOF'
# TODO

## Ticket IDs

| Prefix | Section |
| --- | --- |
| `MTG` | Meetings |

## Meetings

- [ ] **MTG-10** Test ticket
  <details>
  <summary>Details</summary>

  Body.
  </details>
EOF
cat > "$MAIN/docs/TODO-DONE.md" <<'EOF'
# Done
EOF

git -C "$MAIN" add -A
git -C "$MAIN" commit -q -m init
git -C "$MAIN" remote add origin "$BARE"
git -C "$MAIN" push -q origin main

WROOT="$(cd "$MAIN/.." && pwd -P)/yogurt-worktrees"

run() { YOGURT_MAIN="$MAIN" "$TASK" "$@"; }

fail=0
pass=0

t() {
  local desc="$1"; shift
  if "$@"; then
    pass=$((pass + 1))
  else
    echo "FAIL: $desc"
    fail=$((fail + 1))
  fi
}

# --- no args ---
status=0
run start >/dev/null 2>&1 || status=$?
t "start with no args exits 2" [ "$status" -eq 2 ]

# --- unknown ticket ---
status=0
run start XX-999 --no-bootstrap >/dev/null 2>&1 || status=$?
t "start on an unopened ticket ID exits 2" [ "$status" -eq 2 ]

# --- bad slug ---
status=0
run start Bad_Name --no-bootstrap >/dev/null 2>&1 || status=$?
t "start with an invalid slug exits 2" [ "$status" -eq 2 ]

# --- name derivation: ticket ID + words ---
out="$(run start MTG-10 flash more --no-bootstrap)"
t "start derives mtg-10-flash-more from MTG-10 flash more" \
  [ "$(echo "$out" | tail -1)" = "$WROOT/mtg-10-flash-more" ]
t "start creates the worktree directory" [ -d "$WROOT/mtg-10-flash-more" ]
t "start creates the branch" \
  bash -c 'git -C "$1" show-ref --verify --quiet refs/heads/mtg-10-flash-more' _ "$MAIN"

# --- name derivation: bare lowercase slug ---
out="$(run start release-notes --no-bootstrap)"
t "start accepts a lowercase slug with no ticket" \
  [ "$(echo "$out" | tail -1)" = "$WROOT/release-notes" ]

# --- resume ---
out2="$(run start MTG-10 flash more --no-bootstrap)"
t "resume prints the same path" \
  [ "$(echo "$out2" | tail -1)" = "$WROOT/mtg-10-flash-more" ]

# --- claim: directory without branch ---
mkdir -p "$WROOT/ghost-dir"
status=0
run start ghost-dir --no-bootstrap >/dev/null 2>&1 || status=$?
t "a directory claimed without its branch refuses with exit 1" [ "$status" -eq 1 ]
rm -rf "$WROOT/ghost-dir"

# --- claim: branch without directory ---
git -C "$MAIN" branch ghost-branch >/dev/null
status=0
run start ghost-branch --no-bootstrap >/dev/null 2>&1 || status=$?
t "a branch claimed without its directory refuses with exit 1" [ "$status" -eq 1 ]
git -C "$MAIN" branch -D ghost-branch >/dev/null

echo "task_test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
