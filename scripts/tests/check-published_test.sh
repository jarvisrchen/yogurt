#!/usr/bin/env bash
# Tests scripts/check-published.sh end to end: git, gh and curl are
# PATH-shimmed stubs in a fixture bin dir, so this is hermetic even
# though the script itself is not (style of release_test.sh's -n plan
# tests, which stub gh/brew the same way).
#
# check-published.sh reads the *real* README.md and
# crates/yogurt-stt/src/models.rs (it has no repo-under-test to point
# at), so the README-formula-missing and mirror-404 scenarios below pick
# a real slug / real mirror URL out of those files rather than inventing
# fixture ones.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECK_PUBLISHED="$REPO_ROOT/scripts/check-published.sh"

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

contains() {
  local desc="$1" haystack="$2" needle="$3"
  case "$haystack" in
    *"$needle"*) pass=$((pass + 1)) ;;
    *)
      fail=$((fail + 1))
      printf 'FAIL: %s (expected to contain %s)\n---\n%s\n---\n' "$desc" "$needle" "$haystack" >&2
      ;;
  esac
}

not_contains() {
  local desc="$1" haystack="$2" needle="$3"
  case "$haystack" in
    *"$needle"*)
      fail=$((fail + 1))
      printf 'FAIL: %s (expected NOT to contain %s)\n---\n%s\n---\n' "$desc" "$needle" "$haystack" >&2
      ;;
    *) pass=$((pass + 1)) ;;
  esac
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

REAL_SLUG="yogurt-model-medium-en"          # a real README line, made to look missing from the tap
REAL_MIRROR_BASENAME="ggml-medium.en.bin"   # a real mirror URL, made to 404

# ---- fixture bin: git, gh, curl -----------------------------------------

BIN="$WORK/bin"
mkdir -p "$BIN"

cat >"$BIN/git" <<'EOF'
#!/usr/bin/env bash
# stub git - only answers `ls-remote --tags origin`, the one git call
# check-published.sh makes (via release.sh's previous_tag).
case "$*" in
  "ls-remote --tags origin")
    IFS=' ' read -r -a tags <<<"${STUB_TAGS:-v0.6.0 v0.7.0}"
    for t in "${tags[@]}"; do
      printf 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\trefs/tags/%s\n' "$t"
    done
    ;;
  *)
    echo "unstubbed git call: $*" >&2
    exit 1
    ;;
esac
EOF
chmod +x "$BIN/git"

cat >"$BIN/gh" <<GHEOF
#!/usr/bin/env bash
# stub gh - answers the tap/formula/release/issue calls
# check-published.sh makes. Behavior is tuned per test via env vars
# (STUB_*), all read fresh on every invocation.
args="\$*"

case "\$args" in
  "api repos/jarvisrchen/homebrew-yogurt --jq .default_branch")
    echo "\${STUB_TAP_BRANCH:-main}"
    ;;

  *"contents/Formula/yogurt.rb?ref="*"--jq .content"*)
    ver="\${STUB_FORMULA_VERSION:-0.7.0}"
    arm_sha="\${STUB_FORMULA_ARM_SHA:-1111111111111111111111111111111111111111111111111111111111111111}"
    x86_sha="\${STUB_FORMULA_X86_SHA:-2222222222222222222222222222222222222222222222222222222222222222}"
    formula=\$(cat <<RB
class Yogurt < Formula
  version "\$ver"
  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jarvisrchen/yogurt/releases/download/v\$ver/yogurt-aarch64-apple-darwin.tar.gz"
      sha256 "\$arm_sha"
    else
      url "https://github.com/jarvisrchen/yogurt/releases/download/v\$ver/yogurt-x86_64-apple-darwin.tar.gz"
      sha256 "\$x86_sha"
    end
  end
end
RB
    )
    printf '%s' "\$formula" | base64
    ;;

  *"contents/Formula/${REAL_SLUG}.rb?ref="*)
    # existence check for the one slug this test controls
    if [ "\${STUB_MISSING_FORMULA:-0}" = "1" ]; then
      echo "not found" >&2
      exit 1
    fi
    echo '{}'
    ;;

  *"contents/Formula/"*".rb?ref="*)
    # every other model formula: always present
    echo '{}'
    ;;

  "release download "*)
    dest=""
    prev=""
    for a in \$args; do
      if [ "\$prev" = "-D" ]; then dest="\$a"; fi
      prev="\$a"
    done
    arm_sha="\${STUB_SUMS_ARM_SHA:-1111111111111111111111111111111111111111111111111111111111111111}"
    x86_sha="\${STUB_SUMS_X86_SHA:-2222222222222222222222222222222222222222222222222222222222222222}"
    printf '%s  yogurt-aarch64-apple-darwin.tar.gz\n' "\$arm_sha" >"\$dest/SHA256SUMS"
    printf '%s  yogurt-x86_64-apple-darwin.tar.gz\n' "\$x86_sha" >>"\$dest/SHA256SUMS"
    ;;

  "issue list --repo jarvisrchen/yogurt --state open --json title"*)
    echo "\${STUB_ISSUE_EXISTING:-0}"
    ;;

  "issue create --repo jarvisrchen/yogurt --title "*)
    : >"\${STUB_ISSUE_MARKER:-/dev/null}"
    ;;

  *)
    echo "unstubbed gh call: \$args" >&2
    exit 1
    ;;
esac
GHEOF
chmod +x "$BIN/gh"

cat >"$BIN/curl" <<CURLEOF
#!/usr/bin/env bash
# stub curl - answers the --head status checks check-published.sh makes.
# Everything is 200 unless the URL matches STUB_404_URL_SUBSTR.
url="\${@: -1}"
case "\$url" in
  *"\${STUB_404_URL_SUBSTR:-__none__}"*) printf '404' ;;
  *) printf '200' ;;
esac
CURLEOF
chmod +x "$BIN/curl"

run_cp() {
  # run_cp [extra args...] - runs check-published.sh with the fixture
  # bin dir first on PATH (so gh/jq/base64/etc from the real PATH still
  # resolve for anything not stubbed).
  PATH="$BIN:$PATH" "$CHECK_PUBLISHED" "$@"
}

# ---- 1. version match, everything else default-clean --------------------

out="$(STUB_FORMULA_VERSION=0.7.0 run_cp 2>&1)" && rc=0 || rc=$?
check "happy path exits 0" "$rc" "0"
contains "happy path: tag_version_match ok" "$out" "ok: latest tag v0.7.0 matches tap formula version 0.7.0"
contains "happy path: tarball url ok" "$out" "ok: https://github.com/jarvisrchen/yogurt/releases/download/v0.7.0/yogurt-aarch64-apple-darwin.tar.gz returned 200"
contains "happy path: tarball sha ok" "$out" "ok: formula arm64 sha256 matches SHA256SUMS"
contains "happy path: readme formula ok" "$out" "ok: README's $REAL_SLUG names an existing tap formula"
contains "happy path: mirror url ok" "$out" "ok: https://github.com/jarvisrchen/yogurt/releases/download/models-v1/$REAL_MIRROR_BASENAME returned 200"
contains "happy path: summary line" "$out" "check-published: "

# ---- 2. version mismatch -------------------------------------------------

out="$(STUB_FORMULA_VERSION=0.6.0 run_cp 2>&1)" && rc=0 || rc=$?
check "version mismatch exits 1" "$rc" "1"
contains "version mismatch: FAIL line" "$out" "FAIL: latest tag v0.7.0 (0.7.0) does not match tap formula version '0.6.0'"

# ---- 3. 404 tarball -------------------------------------------------------

out="$(STUB_FORMULA_VERSION=0.7.0 STUB_404_URL_SUBSTR="aarch64-apple-darwin.tar.gz" run_cp 2>&1)" && rc=0 || rc=$?
check "404 tarball exits 1" "$rc" "1"
contains "404 tarball: FAIL line" "$out" "FAIL: https://github.com/jarvisrchen/yogurt/releases/download/v0.7.0/yogurt-aarch64-apple-darwin.tar.gz returned 404, want 200"
contains "404 tarball: other checks still run" "$out" "ok: formula arm64 sha256 matches SHA256SUMS"

# ---- 4. sha mismatch --------------------------------------------------

out="$(STUB_FORMULA_VERSION=0.7.0 STUB_FORMULA_ARM_SHA="9999999999999999999999999999999999999999999999999999999999999999" run_cp 2>&1)" && rc=0 || rc=$?
check "sha mismatch exits 1" "$rc" "1"
contains "sha mismatch: FAIL line" "$out" "FAIL: formula arm64 sha256 ('9999999999999999999999999999999999999999999999999999999999999999') != SHA256SUMS"

# ---- 5. README formula missing from tap ----------------------------------

out="$(STUB_FORMULA_VERSION=0.7.0 STUB_MISSING_FORMULA=1 run_cp 2>&1)" && rc=0 || rc=$?
check "missing README formula exits 1" "$rc" "1"
contains "missing README formula: FAIL line" "$out" "FAIL: README names $REAL_SLUG but Formula/$REAL_SLUG.rb is missing from"
contains "missing README formula: other slugs still ok" "$out" "ok: README's yogurt-model-tiny-en names an existing tap formula"

# ---- 6. mirror URL 404 -----------------------------------------------

out="$(STUB_FORMULA_VERSION=0.7.0 STUB_404_URL_SUBSTR="$REAL_MIRROR_BASENAME" run_cp 2>&1)" && rc=0 || rc=$?
check "mirror 404 exits 1" "$rc" "1"
contains "mirror 404: FAIL line" "$out" "FAIL: https://github.com/jarvisrchen/yogurt/releases/download/models-v1/$REAL_MIRROR_BASENAME returned 404, want 200"

# ---- 7. --json shape ------------------------------------------------------

json_out="$(STUB_FORMULA_VERSION=0.7.0 run_cp --json 2>&1)"
if command -v jq >/dev/null 2>&1; then
  echo "$json_out" | jq -e 'type == "array" and length > 0 and all(.[]; has("check") and has("ok") and has("detail"))' >/dev/null \
    && pass=$((pass + 1)) \
    || { fail=$((fail + 1)); printf 'FAIL: --json output is not a well-formed array of {check,ok,detail}\n---\n%s\n---\n' "$json_out" >&2; }
else
  printf 'SKIP: --json shape test (no jq on PATH)\n' >&2
fi
not_contains "--json mode prints no text lines" "$json_out" "check-published: "

# ---- 8. --issue is exercised only here, and only opens once -------------

MARKER="$WORK/issue-created"

rm -f "$MARKER"
STUB_FORMULA_VERSION=0.6.0 STUB_ISSUE_EXISTING=0 STUB_ISSUE_MARKER="$MARKER" run_cp >/dev/null 2>&1 || true
check "no --issue flag: gh issue list/create never called (no marker)" "$([ -f "$MARKER" ] && echo yes || echo no)" "no"

rm -f "$MARKER"
out="$(STUB_FORMULA_VERSION=0.6.0 STUB_ISSUE_EXISTING=0 STUB_ISSUE_MARKER="$MARKER" run_cp --issue 2>&1)" || true
check "--issue with a FAIL and none open: issue created" "$([ -f "$MARKER" ] && echo yes || echo no)" "yes"
contains "--issue: opened message" "$out" "issue: opened 'check-published: "
rm -f "$MARKER"

out="$(STUB_FORMULA_VERSION=0.6.0 STUB_ISSUE_EXISTING=1 STUB_ISSUE_MARKER="$MARKER" run_cp --issue 2>&1)" || true
check "--issue with a FAIL but one already open: not re-created" "$([ -f "$MARKER" ] && echo yes || echo no)" "no"
contains "--issue: skipped message" "$out" "issue: skipped - an open check-published issue already exists"

out="$(STUB_FORMULA_VERSION=0.7.0 STUB_ISSUE_EXISTING=0 STUB_ISSUE_MARKER="$MARKER" run_cp --issue 2>&1)" || true
check "--issue with everything passing: no issue created" "$([ -f "$MARKER" ] && echo yes || echo no)" "no"

echo "check-published_test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
