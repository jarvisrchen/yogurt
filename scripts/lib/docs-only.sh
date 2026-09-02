#!/usr/bin/env bash
# docs-only — shared "is this change docs-only" detection.
#
# Sourced (not executed) by scripts/release.sh and, later, scripts/ship.sh
# (DX-5) — both need the same answer to "did CI skip this commit because it
# only touched docs?". No side effects: sourcing this file must not run
# anything.
#
# DOCS_ONLY_PATTERNS mirrors .github/workflows/ci.yml's
# `paths-ignore: &docs-only` anchor verbatim. Keep them in sync —
# scripts/tests/docs-only_test.sh extracts that anchor at test time and
# fails if the two lists disagree.
DOCS_ONLY_PATTERNS=(
  '**/*.md'
  'docs/**'
  'LICENSE'
)

# Translate one paths-ignore-style glob into a bash `case` pattern. `case`
# matching (unlike filename globbing) already treats `*` as matching `/`,
# so `**/*.md` and `*.md` are equivalent there — only the `**/` prefix and
# `/**` suffix need stripping.
_docs_only_case_pattern() {
  local glob="$1"
  case "$glob" in
    '**/'*) glob="*${glob#\*\*\/}" ;;
    *'/**') glob="${glob%/**}/*" ;;
  esac
  printf '%s' "$glob"
}

# is_docs_only <path>... — true (0) only when every given path matches one
# of DOCS_ONLY_PATTERNS. Called with zero paths, it is vacuously true.
is_docs_only() {
  local path glob pattern matched
  for path in "$@"; do
    matched=0
    for glob in "${DOCS_ONLY_PATTERNS[@]}"; do
      pattern="$(_docs_only_case_pattern "$glob")"
      case "$path" in
        $pattern) matched=1; break ;;
      esac
    done
    [ "$matched" -eq 1 ] || return 1
  done
  return 0
}

# changed_paths_between <shaA> <shaB> — paths touched between two commits.
changed_paths_between() {
  git diff --name-only "$1" "$2"
}
