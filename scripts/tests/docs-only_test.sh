#!/usr/bin/env bash
# Tests scripts/lib/docs-only.sh - no framework, plain asserts, BSD bash.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=../lib/docs-only.sh
source "$REPO_ROOT/scripts/lib/docs-only.sh"

pass=0
fail=0

check() {
  local desc="$1" got="$2" want="$3"
  if [ "$got" = "$want" ]; then
    pass=$((pass + 1))
  else
    fail=$((fail + 1))
    printf 'FAIL: %s (got %s want %s)\n' "$desc" "$got" "$want" >&2
  fi
}

is() { if is_docs_only "$@"; then echo 0; else echo 1; fi; }

# --- DOCS_ONLY_PATTERNS must still match ci.yml's paths-ignore anchor ---
ci_patterns=()
while IFS= read -r line; do
  ci_patterns+=("$line")
done < <(awk '
  /paths-ignore: &docs-only/ { grab = 1; next }
  grab && /^      - / {
    line = $0
    sub(/^      - /, "", line)
    gsub(/"/, "", line)
    print line
    next
  }
  grab { exit }
' "$REPO_ROOT/.github/workflows/ci.yml")

same=1
if [ "${#ci_patterns[@]}" -ne "${#DOCS_ONLY_PATTERNS[@]}" ]; then
  same=0
else
  for i in "${!ci_patterns[@]}"; do
    [ "${ci_patterns[$i]}" = "${DOCS_ONLY_PATTERNS[$i]}" ] || same=0
  done
fi
check "DOCS_ONLY_PATTERNS matches ci.yml paths-ignore" "$same" "1"

# --- is_docs_only ---------------------------------------------------
check "docs/RELEASING.md is docs-only"            "$(is docs/RELEASING.md)"                       0
check "README.md is docs-only"                    "$(is README.md)"                               0
check "LICENSE is docs-only"                      "$(is LICENSE)"                                  0
check "docs/deep/nested/file.txt is docs-only"     "$(is docs/deep/nested/file.txt)"                0
check "crates/.../main.rs is NOT docs-only"        "$(is crates/yogurt-server/src/main.rs)"         1
check "mixed set is NOT docs-only"                 "$(is docs/RELEASING.md crates/x/src/main.rs)"   1
check "multiple docs paths is docs-only"           "$(is README.md docs/RELEASING.md)"              0
check "no paths is NOT docs-only"                  "$(is)"                                          1

# --- changed_paths_between -------------------------------------------
# Any two real commits in this repo's history - just needs to run and
# return the paths git itself reports, no framework needed to fake that up.
parent="$(git -C "$REPO_ROOT" rev-parse HEAD~1)"
head="$(git -C "$REPO_ROOT" rev-parse HEAD)"
want="$(git -C "$REPO_ROOT" diff --name-only "$parent" "$head")"
got="$(cd "$REPO_ROOT" && changed_paths_between "$parent" "$head")"
check "changed_paths_between matches git diff --name-only" "$got" "$want"

echo "docs-only_test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
