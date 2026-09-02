#!/usr/bin/env bash
# Query and update docs/TODO.md and docs/TODO-DONE.md without a full read
# of either file. See docs/TODO.md's "Ticket IDs" section for the format
# this script parses: `- [ ] **PREFIX-N** title` headers, each followed by
# an optionally-indented body, ending at the next `- [` or `#` line (never
# at `</details>` - some bodies carry more content after it).
#
# BSD awk / BSD sed only (no gawk, no GNU-only flags) - this has to run
# on a stock macOS 13+ toolchain. Bash 3.2 compatible where practical.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCS_DIR="${TICKET_DOCS_DIR:-$SCRIPT_DIR/../docs}"
TODO_FILE="$DOCS_DIR/TODO.md"
DONE_FILE="$DOCS_DIR/TODO-DONE.md"

usage() {
  cat >&2 <<'EOF'
usage: ticket.sh                                list open tickets
       ticket.sh <ID>                            print one ticket block
       ticket.sh next <PREFIX>                   print the next free ID
       ticket.sh done <ID> --note-file <path>    close a ticket
       ticket.sh --check                         validate docs/TODO*.md
EOF
}

die() {
  echo "error: $1" >&2
  [ -n "${2:-}" ] && echo "help: $2" >&2
  exit "${3:-1}"
}

# Print "ID<TAB>title" for every open ("- [ ]") ticket header in FILE, in
# file order. Skips fenced code (the "Referencing attachments" example).
list_open() {
  awk '
    /^```/ { infence = !infence; next }
    infence { next }
    /^- \[ \] \*\*/ {
      rest = $0
      sub(/^- \[ \] \*\*/, "", rest)
      idend = index(rest, "**")
      if (idend == 0) next
      id = substr(rest, 1, idend - 1)
      title = substr(rest, idend + 2)
      sub(/^ /, "", title)
      print id "\t" title
    }
  ' "$1"
}

# Print "ID<TAB>lineno<TAB>state" for every ticket header (open or closed)
# in FILE, plus a "<TAB>lineno<TAB>noid" row for a "- [" line with no ID -
# used by --check.
all_headers() {
  awk '
    /^```/ { infence = !infence; next }
    infence { next }
    /^- \[[ xX]\] / {
      state = (substr($0, 4, 1) == " ") ? "open" : "closed"
      rest = $0
      sub(/^- \[[ xX]\] /, "", rest)
      if (rest !~ /^\*\*/) { print "\t" NR "\tnoid"; next }
      sub(/^\*\*/, "", rest)
      idend = index(rest, "**")
      if (idend == 0) { print "\t" NR "\tnoid"; next }
      id = substr(rest, 1, idend - 1)
      title = substr(rest, idend + 2)
      sub(/^ /, "", title)
      print id "\t" NR "\t" state (title == "" ? "\tnotitle" : "")
    }
  ' "$1"
}

# Print the start/end (1-indexed, inclusive) line numbers of ID's block in
# FILE, as "start<TAB>end", or nothing if ID is not there. The range
# includes the block's own trailing blank line, so deleting it leaves
# exactly one blank line between the surrounding entries.
find_block_range() {
  awk -v target="$2" '
    /^```/ { infence = !infence; next }
    infence { next }
    /^- \[[ xX]\] \*\*/ || /^#/ {
      if (start && !end) { end = NR - 1; exit }
      if (!start && index($0, "**" target "**") > 0) start = NR
      next
    }
    END {
      if (start && !end) end = NR
      if (start) print start "\t" end
    }
  ' "$1"
}

# Print ID's block (header line through its trailing blank line) from
# FILE. Exit 1 if not present.
extract_block() {
  local range start end
  range="$(find_block_range "$1" "$2")"
  [ -z "$range" ] && return 1
  start="${range%%	*}"
  end="${range##*	}"
  sed -n "${start},${end}p" "$1"
}

max_number() {
  local prefix="$1"
  { all_headers "$TODO_FILE"; all_headers "$DONE_FILE"; } |
    awk -F'\t' -v p="$prefix" '
      $1 ~ ("^" p "-[0-9]+$") {
        n = substr($1, length(p) + 2) + 0
        if (n > max) max = n
      }
      END { print max + 0 }
    '
}

cmd_list() {
  list_open "$TODO_FILE"
}

cmd_show() {
  local id="$1"
  if extract_block "$TODO_FILE" "$id"; then return 0; fi
  if extract_block "$DONE_FILE" "$id"; then return 0; fi
  die "no ticket $id" "just ticket"
}

cmd_next() {
  local prefix="${1:-}"
  [ -z "$prefix" ] && { usage; exit 2; }
  grep -q "^| \`$prefix\` |" "$TODO_FILE" ||
    { usage; echo "error: unknown prefix $prefix (not in docs/TODO.md's Ticket IDs table)" >&2; exit 2; }
  echo "${prefix}-$(($(max_number "$prefix") + 1))"
}

cmd_done() {
  local id="" note_file=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --note-file) note_file="${2:-}"; shift 2 ;;
      *) [ -z "$id" ] && { id="$1"; shift; } || { usage; exit 2; } ;;
    esac
  done
  [ -z "$id" ] || [ -z "$note_file" ] && { usage; exit 2; }

  for f in "$TODO_FILE" "$DONE_FILE"; do
    grep -q '^<<<<<<<\|^=======$\|^>>>>>>>' "$f" &&
      die "$f has unresolved conflict markers"
  done

  if extract_block "$DONE_FILE" "$id" >/dev/null; then
    die "$id is already in $(basename "$DONE_FILE")"
  fi
  local range start end
  range="$(find_block_range "$TODO_FILE" "$id")"
  [ -z "$range" ] && die "$id is not open in $(basename "$TODO_FILE")" "just ticket $id"
  start="${range%%	*}"
  end="${range##*	}"

  [ -f "$note_file" ] || die "note file not found: $note_file"
  [ -s "$note_file" ] || die "note file is empty: $note_file"

  local block_tmp
  block_tmp="$(mktemp)"
  sed -n "${start},${end}p" "$TODO_FILE" > "$block_tmp"

  local closed_block
  closed_block="$(awk -v notefile="$note_file" '
    BEGIN {
      nnote = 0
      while ((getline line < notefile) > 0) note[++nnote] = line
      close(notefile)
    }
    { n++; lines[n] = $0 }
    END {
      while (n > 0 && lines[n] ~ /^[ \t]*$/) n--
      sub(/^- \[ \] /, "- [x] ", lines[1])
      details = 0
      for (i = 1; i <= n; i++) if (lines[i] ~ /^[ \t]*<\/details>[ \t]*$/) details = i
      if (details > 0) {
        for (i = 1; i < details; i++) print lines[i]
        print ""
        for (j = 1; j <= nnote; j++) print (note[j] == "" ? "" : "  " note[j])
        for (i = details; i <= n; i++) print lines[i]
      } else {
        for (i = 1; i <= n; i++) print lines[i]
        print ""
        for (j = 1; j <= nnote; j++) print (note[j] == "" ? "" : "  " note[j])
      }
    }
  ' "$block_tmp")"

  rm -f "$block_tmp"
  sed -i '' "${start},${end}d" "$TODO_FILE"
  printf '\n%s\n' "$closed_block" >> "$DONE_FILE"

  echo "ticket: closed $id, moved to $(basename "$DONE_FILE") - review with: git diff"
}

cmd_check() {
  local fail=0

  [ -f "$TODO_FILE" ] || die "missing $TODO_FILE"
  [ -f "$DONE_FILE" ] || die "missing $DONE_FILE"

  local ids
  ids="$( { all_headers "$TODO_FILE"; all_headers "$DONE_FILE"; } | cut -f1)"

  # every "- [" line carries an ID and a title
  local bad
  bad="$( { all_headers "$TODO_FILE"; all_headers "$DONE_FILE"; } | awk -F'\t' '$3 == "noid" || $3 == "notitle"' )"
  if [ -n "$bad" ]; then
    echo "$bad" | while IFS=$'\t' read -r _ line reason; do
      echo "ticket: line $line has no valid ID/title ($reason)"
    done
    fail=1
  fi

  # IDs unique across both files
  local dupes
  dupes="$(echo "$ids" | grep -v '^$' | sort | uniq -d)"
  if [ -n "$dupes" ]; then
    while read -r d; do echo "ticket: duplicate ID $d"; done <<< "$dupes"
    fail=1
  fi

  # every prefix seen is in the Ticket IDs table
  local prefixes seen_prefix
  prefixes="$(echo "$ids" | grep -v '^$' | sed -E 's/-[0-9]+$//' | sort -u)"
  while read -r p; do
    [ -z "$p" ] && continue
    grep -q "^| \`$p\` |" "$TODO_FILE" || { echo "ticket: prefix $p has no row in docs/TODO.md's Ticket IDs table"; fail=1; }
  done <<< "$prefixes"

  # next equals max+1 for every prefix (sanity check on the computation itself)
  while read -r p; do
    [ -z "$p" ] && continue
    local m computed
    m="$(max_number "$p")"
    computed="$(cmd_next "$p" 2>/dev/null || true)"
    [ "$computed" = "${p}-$((m + 1))" ] || { echo "ticket: next $p computation is inconsistent"; fail=1; }
  done <<< "$prefixes"

  if [ "$fail" -ne 0 ]; then
    exit 1
  fi
  echo "ticket: ok"
}

main() {
  case "${1:-}" in
    "") cmd_list ;;
    --check) cmd_check ;;
    next) shift; cmd_next "$@" ;;
    done) shift; cmd_done "$@" ;;
    -h|--help) usage ;;
    *) cmd_show "$1" ;;
  esac
}

main "$@"
