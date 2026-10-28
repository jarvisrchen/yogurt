#!/usr/bin/env bash
# Runnable check for scripts/ticket.sh's parsing/editing logic, against a
# throwaway pair of docs/TODO.md-shaped files (never the real ones).
# No framework - plain assertions, bash 3.2 / BSD tools only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TICKET="$SCRIPT_DIR/../ticket.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
DOCS="$WORK/docs"
mkdir -p "$DOCS"

cat > "$DOCS/TODO.md" <<'EOF'
# TODO

## Ticket IDs

| Prefix | Section |
| --- | --- |
| `AAA` | Alpha |
| `BBB` | Beta |

## Referencing attachments

Example entry, must not be parsed as a real ticket:

```
- [ ] **AAA-99** fenced example, not real
```

## Alpha

- [ ] **AAA-1** First alpha ticket
  <details>
  <summary>Details</summary>

  Some body text.
  </details>

- [ ] **AAA-2** Ticket with no details block

## Beta

- [ ] **BBB-1** Beta ticket
  <details>
  <summary>Details</summary>

  Beta body.
  </details>
EOF

cat > "$DOCS/TODO-DONE.md" <<'EOF'
# Done

- [x] **AAA-0** Already done ticket
  <details>
  <summary>Details</summary>

  Done body.
  </details>

  Landed in #1 (2026-01-01). Resolution text after the closing tag.
EOF

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

# A fake gh on PATH stands in for the open-PR lookup so no test touches the network.
BIN="$WORK/bin"
mkdir -p "$BIN"
GH_OUT="$WORK/gh-out"
: > "$GH_OUT"
printf '#!/bin/sh\n[ -f "%s.fail" ] && exit 1\ncat "%s"\n' "$GH_OUT" "$GH_OUT" > "$BIN/gh"
chmod +x "$BIN/gh"
export PATH="$BIN:$PATH"

run() { TICKET_DOCS_DIR="$DOCS" "$TICKET" "$@"; }

# --- list ---
list_out="$(run)"
t "list has exactly 3 open tickets" [ "$(echo "$list_out" | wc -l | tr -d ' ')" = 3 ]
t "list includes AAA-1" [ -n "$(echo "$list_out" | grep '^AAA-1	')" ]
t "list includes AAA-2" [ -n "$(echo "$list_out" | grep '^AAA-2	')" ]
t "list includes BBB-1" [ -n "$(echo "$list_out" | grep '^BBB-1	')" ]
t "list excludes the fenced example AAA-99" [ -z "$(echo "$list_out" | grep 'AAA-99')" ]

# --- show ---
show_aaa2="$(run AAA-2)"
t "show AAA-2 prints exactly its one-line block" [ "$show_aaa2" = "- [ ] **AAA-2** Ticket with no details block" ]

show_aaa0="$(run AAA-0)"
t "show AAA-0 (closed, DONE file) includes resolution text after </details>" \
  bash -c 'echo "$1" | grep -q "Resolution text after the closing tag"' _ "$show_aaa0"

t "show unknown ID fails" bash -c '! TICKET_DOCS_DIR="$1" "$2" AAA-99 >/dev/null 2>&1' _ "$DOCS" "$TICKET"
err_out="$(run AAA-99 2>&1 >/dev/null || true)"
t "show unknown ID prints error+help" bash -c 'echo "$1" | grep -q "no ticket AAA-99" && echo "$1" | grep -q "just ticket"' _ "$err_out"

# --- next ---
t "next AAA is AAA-3 (max across both files, fenced example excluded)" [ "$(run next AAA)" = "AAA-3" ]
t "next BBB is BBB-2" [ "$(run next BBB)" = "BBB-2" ]
printf 'AAA-7: claimed by an open PR\nbbb-4-claimed-branch\nAAAA-50: other prefix\n' > "$GH_OUT"
t "next AAA skips an ID in an open PR title" [ "$(run next AAA)" = "AAA-8" ]
t "next BBB skips an ID in an open PR branch" [ "$(run next BBB)" = "BBB-5" ]
touch "$GH_OUT.fail"
t "next falls back to the docs when gh fails" [ "$(run next AAA 2>/dev/null)" = "AAA-3" ]
gh_err="$(run next AAA 2>&1 >/dev/null)"
t "next warns when gh fails" bash -c 'echo "$1" | grep -q "could not read open PRs"' _ "$gh_err"
rm -f "$GH_OUT.fail"
: > "$GH_OUT"
t "next with unknown prefix exits 2" bash -c '
  TICKET_DOCS_DIR="$1" "$2" next ZZZ >/dev/null 2>&1
  [ $? -eq 2 ]
' _ "$DOCS" "$TICKET"

# --- check (clean state) ---
t "--check passes on well-formed fixtures" run --check

# --- done ---
note1="$WORK/note1.txt"
printf 'Landed in #100 (2026-01-01). Shipped the alpha behavior, verified by hand.\n' > "$note1"
run done AAA-1 --note-file "$note1" >/dev/null

t "done removes AAA-1 from TODO.md" bash -c '! grep -q "AAA-1" "$1"' _ "$DOCS/TODO.md"
t "done appends AAA-1 to TODO-DONE.md as closed" bash -c 'grep -q "\[x\] \*\*AAA-1\*\*" "$1"' _ "$DOCS/TODO-DONE.md"
t "done inserts the note before </details>" bash -c '
  awk "/\[x\] \*\*AAA-1\*\*/{f=1} f&&/Shipped the alpha behavior/{n++} f&&/<\/details>/{d++; if(n&&!seen){seen=1}} END{exit !(n&&d)}" "$1"
' _ "$DOCS/TODO-DONE.md"

note2="$WORK/note2.txt"
printf 'Landed in #101 (2026-01-01). No details block to begin with.\n' > "$note2"
run done AAA-2 --note-file "$note2" >/dev/null
t "done on a ticket with no details block still appends the note" bash -c '
  grep -A2 "\[x\] \*\*AAA-2\*\*" "$1" | grep -q "No details block to begin with"
' _ "$DOCS/TODO-DONE.md"

t "second done of the same ID fails (idempotency)" bash -c '
  ! TICKET_DOCS_DIR="$1" "$2" done AAA-1 --note-file "$3" >/dev/null 2>&1
' _ "$DOCS" "$TICKET" "$note1"
idempotent_err="$(TICKET_DOCS_DIR="$DOCS" "$TICKET" done AAA-1 --note-file "$note1" 2>&1 >/dev/null || true)"
t "idempotent failure names TODO-DONE.md" bash -c 'echo "$1" | grep -q "already in TODO-DONE.md"' _ "$idempotent_err"

t "done on an unknown ID fails" bash -c '
  ! TICKET_DOCS_DIR="$1" "$2" done AAA-999 --note-file "$3" >/dev/null 2>&1
' _ "$DOCS" "$TICKET" "$note1"

t "done with a missing note file fails" bash -c '
  ! TICKET_DOCS_DIR="$1" "$2" done BBB-1 --note-file "$1/no-such-file.txt" >/dev/null 2>&1
' _ "$DOCS" "$TICKET"

empty_note="$WORK/empty.txt"
: > "$empty_note"
t "done with an empty note file fails" bash -c '
  ! TICKET_DOCS_DIR="$1" "$2" done BBB-1 --note-file "$3" >/dev/null 2>&1
' _ "$DOCS" "$TICKET" "$empty_note"

t "--check still passes after two closeouts" run --check

# --- --check catches breakage ---
cp "$DOCS/TODO.md" "$WORK/TODO.bak"
printf -- '- [ ] **BBB-1** duplicate of an already-closed style ID\n' >> "$DOCS/TODO.md"
t "--check fails on a duplicate ID" bash -c '! TICKET_DOCS_DIR="$1" "$2" --check >/dev/null 2>&1' _ "$DOCS" "$TICKET"
cp "$WORK/TODO.bak" "$DOCS/TODO.md"

# --- done on a section's last entry, right under another entry ---
DOCS2="$WORK/docs2"
mkdir -p "$DOCS2"
printf '# TODO\n\n## Alpha\n\n- [ ] **AAA-1** first\n- [ ] **AAA-2** last in the section\n\n## Beta\n\n- [ ] **BBB-1** beta\n' > "$DOCS2/TODO.md"
printf '# Done\n' > "$DOCS2/TODO-DONE.md"
TICKET_DOCS_DIR="$DOCS2" "$TICKET" done AAA-2 --note-file "$note1" >/dev/null
t "done on a section's last entry keeps the blank line before the next heading" \
  [ "$(cat "$DOCS2/TODO.md")" = "$(printf '# TODO\n\n## Alpha\n\n- [ ] **AAA-1** first\n\n## Beta\n\n- [ ] **BBB-1** beta')" ]

echo "ticket_test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
