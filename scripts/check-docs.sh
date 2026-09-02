#!/usr/bin/env bash
# -e is deliberately omitted: several loops below intentionally run a
# `grep` that finds nothing (exit 1) per file, and pipefail would abort
# the script on that instead of letting the loop continue to the next file.
set -uo pipefail

# check-docs.sh - keeps docs honest against the code: documented /api paths,
# `just` recipes, relative links, backticked repo paths, no em dash, and
# size budgets. Run by `just check-docs` (called from `just lint`) and
# .github/workflows/docs.yml. About 15 seconds, no cargo/pnpm needed.
#
# Em dash scope (rule 5) is prose docs and scripts only - Rust and
# TypeScript comments are out of scope.
#
# BSD (macOS) and GNU (ubuntu) grep/sed/awk only: no grep -P, no sed -i ''.

cd "$(git rev-parse --show-toplevel)"

VIOLATIONS=$(mktemp)
trap 'rm -f "$VIOLATIONS"' EXIT
violate() { echo "$1: $2: $3" >>"$VIOLATIONS"; }

# Reference/operational docs: excludes the backlog (TODO.md, TODO-DONE.md)
# and planning docs (docs/.planning/), which describe not-yet-built paths
# and recipes on purpose, and docs/archive/, which is frozen history.
DOC_FILES=$(
  { find docs -name '*.md' -type f
    find .claude/skills -name '*.md' -type f
    printf 'README.md\nCONTRIBUTING.md\nAGENTS.md\n'
  } | grep -v '^docs/archive/' | grep -v '^docs/\.planning/' | grep -vE '^docs/TODO(-DONE)?\.md$'
)

# All tracked markdown outside the archive, for the link and repo-path rules.
ALL_MD=$(git ls-files '*.md' | grep -v '^docs/archive/')

# Blanks out fenced code blocks (```...```), keeping line numbers intact.
# Only used for the link rule: `[text](path)` inside a fence is literal
# example text, not a real link.
defence() { awk '/^```/{f=!f; print ""; next} f{print ""; next} {print}' "$1"; }

# --- Rule 1: documented /api paths must match a real route ---------------

routes=$(awk '
  /\.route\(/ {
    if (match($0, /"[^"]*"/)) { print substr($0, RSTART + 1, RLENGTH - 2); next }
    getline nxt
    if (match(nxt, /"[^"]*"/)) print substr(nxt, RSTART + 1, RLENGTH - 2)
  }
' $(find crates/yogurt-server/src -name '*.rs' -type f))
if [ -z "$routes" ]; then
  echo "check-docs: found zero routes in crates/yogurt-server/src - extraction is broken"
  exit 1
fi
# Each route reduced to its literal prefix up to the first `{param}`.
route_prefixes=$(echo "$routes" | sed -E 's/\{.*$//' | sed -E 's#/$##')

for f in $DOC_FILES; do
  grep -noE '/api/[A-Za-z0-9_/${}:*-]+' "$f" 2>/dev/null | while IFS=: read -r line token; do
    case "$token" in
      *'*'*) continue ;; # wildcard mention ("/api/*", "/api/settings*")
    esac
    prefix=$(echo "$token" | sed -E 's/[$:{].*$//' | sed -E 's#/$##')
    echo "$route_prefixes" | grep -qxF "$prefix" ||
      violate "$f:$line" "api-path" "$token has no matching route (prefix $prefix)"
  done
done

# --- Rule 2: backticked `just <name>` must be a real recipe ---------------

recipes=$(grep -E '^[a-z][a-z0-9-]*(:| )' justfile | grep -oE '^[a-z][a-z0-9-]*')
for f in $DOC_FILES; do
  grep -noE '`just [a-z][a-z0-9-]*' "$f" 2>/dev/null | sed -E 's/`just //' | while IFS=: read -r line name; do
    echo "$recipes" | grep -qxF "$name" ||
      violate "$f:$line" "just-recipe" "just $name is not a recipe in the justfile"
  done
done

# --- Rule 3: relative markdown links must resolve --------------------------

for f in $ALL_MD; do
  dir=$(dirname "$f")
  defence "$f" | grep -noE '\]\([^)]*\)' | while IFS=: read -r line rest; do
    target=${rest#"]("}
    target=${target%")"}
    case "$target" in
      http://* | https://* | mailto:* | '#'* | '') continue ;;
    esac
    target=${target%%#*}
    [ -e "$dir/$target" ] || violate "$f:$line" "link" "$target does not resolve"
  done
done

# --- Rule 4: backticked repo paths must exist -------------------------------

for f in $DOC_FILES; do
  grep -noE '`(docs|scripts|crates|web/src|\.github|\.claude)/[A-Za-z0-9_./-]+\.[A-Za-z0-9]+`' "$f" 2>/dev/null |
    while IFS=: read -r line rest; do
      path=${rest#"\`"}
      path=${path%"\`"}
      case "$path" in *'*'*) continue ;; esac
      [ -e "$path" ] || violate "$f:$line" "repo-path" "$path does not exist"
    done
done

# --- Rule 5: no em dash in prose docs and scripts ---------------------------

EM_DASH="$(printf '\xe2\x80\x94')"
EM_DASH_FILES=$(
  { git ls-files '*.md'; echo justfile; git ls-files scripts; } \
    | grep -v '^docs/archive/' \
    | grep -v '/tests/fixtures/' \
    | grep -v '^crates/yogurt-prompts/templates/'
)
for f in $EM_DASH_FILES; do
  grep -noF "$EM_DASH" "$f" 2>/dev/null | while IFS=: read -r line _; do
    violate "$f:$line" "em-dash" 'use a plain "-" instead of U+2014'
  done
done

# --- Rule 6: size budgets ---------------------------------------------------

agents_size=$(wc -c <AGENTS.md | tr -d ' ')
[ "$agents_size" -lt 12288 ] || violate "AGENTS.md:1" "size" "$agents_size bytes exceeds the 12 KB budget"
todo_size=$(wc -c <docs/TODO.md | tr -d ' ')
[ "$todo_size" -lt 24576 ] || violate "docs/TODO.md:1" "size" "$todo_size bytes exceeds the 24 KB budget"

# --- Report ------------------------------------------------------------------

if [ -s "$VIOLATIONS" ]; then
  cat "$VIOLATIONS"
  exit 1
fi
echo "check-docs: ok"
