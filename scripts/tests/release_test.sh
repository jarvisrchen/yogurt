#!/usr/bin/env bash
# Tests the pure parts of scripts/release.sh: SHA256SUMS/formula parsing,
# log-row rendering, and the -n plan output of finish/untag. No framework -
# plain asserts, bash 3.2 / BSD tools only, style of docs-only_test.sh.
#
# gh and brew are stubbed via a PATH shim so the plan-output tests need no
# network. Everything else sources release.sh directly (it skips `main`
# when sourced) to call its functions in isolation.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
RELEASE="$SCRIPT_DIR/../release.sh"

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

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# shellcheck source=../release.sh
source "$RELEASE"

# --- sha256sums_get -----------------------------------------------------

SUMS="$WORK/SHA256SUMS"
cat >"$SUMS" <<'EOF'
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  yogurt-aarch64-apple-darwin.tar.gz
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  yogurt-x86_64-apple-darwin.tar.gz
EOF
check "sha256sums_get arm64" "$(sha256sums_get "$SUMS" yogurt-aarch64-apple-darwin.tar.gz)" "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
check "sha256sums_get x86_64" "$(sha256sums_get "$SUMS" yogurt-x86_64-apple-darwin.tar.gz)" "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
check "sha256sums_get missing filename" "$(sha256sums_get "$SUMS" nope.tar.gz)" ""

# check_eq against real computed hashes of fixture "tarballs" - this is
# the actual comparison verify does, just against files that are not
# really tarballs.
printf 'arm content' >"$WORK/arm.bin"
printf 'x86 content' >"$WORK/x86.bin"
arm_hash="$(shasum -a 256 "$WORK/arm.bin" | awk '{print $1}')"
x86_hash="$(shasum -a 256 "$WORK/x86.bin" | awk '{print $1}')"
cat >"$SUMS" <<EOF
$arm_hash  yogurt-aarch64-apple-darwin.tar.gz
$x86_hash  yogurt-x86_64-apple-darwin.tar.gz
EOF
matched="$(shasum -a 256 "$WORK/arm.bin" | awk '{print $1}')"
check "computed hash matches SHA256SUMS entry" "$matched" "$(sha256sums_get "$SUMS" yogurt-aarch64-apple-darwin.tar.gz)"
check_eq_out="$(check_eq sha256_aarch64 "arm64 matches" "$matched" "$(sha256sums_get "$SUMS" yogurt-aarch64-apple-darwin.tar.gz)")"
contains "check_eq PASS wording" "$check_eq_out" "PASS: arm64 matches"
check_eq_bad="$(check_eq sha256_aarch64 "arm64 matches" "deadbeef" "$(sha256sums_get "$SUMS" yogurt-aarch64-apple-darwin.tar.gz)" || true)"
contains "check_eq FAIL wording" "$check_eq_bad" "FAIL: arm64 matches"

# --- formula_version / formula_shas -------------------------------------

FORMULA="$WORK/yogurt.rb"
cat >"$FORMULA" <<'EOF'
class Yogurt < Formula
  desc "Local-first meeting copilot -- Granola's UX, your machine."
  homepage "https://github.com/jarvisrchen/yogurt"
  version "0.7.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jarvisrchen/yogurt/releases/download/v0.7.0/yogurt-aarch64-apple-darwin.tar.gz"
      sha256 "1111111111111111111111111111111111111111111111111111111111111111"
    else
      url "https://github.com/jarvisrchen/yogurt/releases/download/v0.7.0/yogurt-x86_64-apple-darwin.tar.gz"
      sha256 "2222222222222222222222222222222222222222222222222222222222222222"
    end
  end

  def install
    bin.install "yogurt"
  end

  test do
    assert_equal "yogurt #{version}", shell_output("#{bin}/yogurt --version").strip
  end
end
EOF
check "formula_version" "$(formula_version "$FORMULA")" "0.7.0"
formula_arm="$(formula_shas "$FORMULA" | sed -n '1p')"
formula_x86="$(formula_shas "$FORMULA" | sed -n '2p')"
check "formula_shas arm64 (first)" "$formula_arm" "1111111111111111111111111111111111111111111111111111111111111111"
check "formula_shas x86_64 (second)" "$formula_x86" "2222222222222222222222222222222222222222222222222222222222222222"

# --- previous_tag / ships_since - against this repo's real tag history --

check "previous_tag(0.7.0)" "$(previous_tag 0.7.0)" "v0.6.0"
check "previous_tag(0.1.0) has no predecessor" "$(previous_tag 0.1.0)" ""
want_ships="$(git -C "$REPO_ROOT" log v0.6.0..v0.7.0 --oneline | awk '{printf "%s%s", (NR>1?"; ":""), $0}')"
check "ships_since matches git log --oneline, joined" "$(cd "$REPO_ROOT" && ships_since v0.6.0 0.7.0)" "$want_ships"

# --- render_log_row - fixed inputs, no network -------------------------

row="$(render_log_row 0.7.0 2026-09-02 33572225259 33571768998 879289fcbc56 399f2ecaa5a4 v0.6.0 "abc123 first; def456 second")"
contains "row has version" "$row" "| v0.7.0 |"
contains "row has date" "$row" "| 2026-09-02 |"
contains "row has NARRATIVE slot" "$row" "NARRATIVE:"
contains "row has push run link" "$row" "[33572225259](https://github.com/jarvisrchen/yogurt/actions/runs/33572225259)"
contains "row has dry run link" "$row" "[33571768998](https://github.com/jarvisrchen/yogurt/actions/runs/33571768998)"
contains "row has arm sha prefix" "$row" '`879289fc...`'
contains "row has x86 sha prefix" "$row" '`399f2eca...`'
contains "row has ships list" "$row" "Ships since v0.6.0: abc123 first; def456 second"

# --- -n plans - PATH-shimmed gh/brew, no network ------------------------

BIN="$WORK/bin"
mkdir -p "$BIN"
cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
# stub gh - answers only the calls print_finish_plan/cmd_untag make.
args="$*"
case "$args" in
  *"pr list --repo jarvisrchen/homebrew-yogurt --head bump-0.7.0"*) echo "8 MERGED" ;;
  *"pr list --repo jarvisrchen/homebrew-yogurt --head bump-0.8.0"*) echo "9 OPEN" ;;
  *"release view v0.7.0"*) exit 0 ;;
  *"release view v9.9.9"*) exit 1 ;;
  *)
    echo "unstubbed gh call: $args" >&2
    exit 1
    ;;
esac
EOF
chmod +x "$BIN/gh"

cat >"$BIN/brew" <<'EOF'
#!/usr/bin/env bash
case "$*" in
  "list --versions jarvisrchen/yogurt/yogurt") echo "jarvisrchen/yogurt/yogurt 0.7.0" ;;
  *) exit 1 ;;
esac
EOF
chmod +x "$BIN/brew"

export PATH="$BIN:$PATH"

plan="$(print_finish_plan 0.7.0 0)"
contains "finish plan: already-merged PR is not re-merged" "$plan" "tap PR #8 already MERGED, nothing to merge"
contains "finish plan: reinstall when already at target version" "$plan" "brew reinstall jarvisrchen/yogurt/yogurt"

plan_open="$(print_finish_plan 0.8.0 0)"
contains "finish plan: open PR gets a merge command" "$plan_open" "gh pr merge 9 --repo jarvisrchen/homebrew-yogurt --squash --delete-branch"
contains "finish plan: upgrade when not at target version" "$plan_open" "brew upgrade jarvisrchen/yogurt/yogurt"

plan_no_smoke="$(print_finish_plan 0.7.0 1)"
contains "finish plan: --no-smoke skips brew steps" "$plan_no_smoke" "brew upgrade/reinstall, brew test and the quarantine check are skipped"
case "$plan_no_smoke" in
  *"brew reinstall jarvisrchen/yogurt/yogurt"*|*"brew upgrade jarvisrchen/yogurt/yogurt"*)
    fail=$((fail + 1))
    printf 'FAIL: finish plan: --no-smoke must not print a brew upgrade/reinstall command\n' >&2
    ;;
  *) pass=$((pass + 1)) ;;
esac

# untag -n against a throwaway fixture repo with a real (local + "remote")
# tag, so the plan reflects an actual git ls-remote/rev-parse - no network,
# since "origin" is a local bare repo.
FIXTURE="$WORK/fixture"
BARE="$WORK/origin.git"
git init -q --bare "$BARE"
git init -q -b main "$FIXTURE"
git -C "$FIXTURE" remote add origin "$BARE"
git -C "$FIXTURE" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
git -C "$FIXTURE" tag v9.9.9
git -C "$FIXTURE" push -q origin main --tags

untag_plan="$(cd "$FIXTURE" && cmd_untag 9.9.9 -n)"
contains "untag plan: deletes remote tag" "$untag_plan" "git push origin :refs/tags/v9.9.9"
contains "untag plan: deletes local tag" "$untag_plan" "git tag -d v9.9.9"

git -C "$FIXTURE" tag -d v9.9.9 >/dev/null
untag_plan_local_only="$(cd "$FIXTURE" && cmd_untag 9.9.9 -n)"
contains "untag plan: local-only deletion, no remote command" "$untag_plan_local_only" "git push origin :refs/tags/v9.9.9"
case "$untag_plan_local_only" in
  *"git tag -d"*)
    fail=$((fail + 1))
    printf 'FAIL: untag plan: local tag already gone, should not print git tag -d\n' >&2
    ;;
  *) pass=$((pass + 1)) ;;
esac

untag_plan_nothing="$(cd "$FIXTURE" && cmd_untag 1.2.3 -n)"
contains "untag plan: nothing to do when no tag exists" "$untag_plan_nothing" "nothing to do"

untag_refused=0
(cd "$FIXTURE" && cmd_untag 0.7.0 -n) >/dev/null 2>&1 || untag_refused=$?
check "untag refuses (exit 2) when a Release exists, even with -n" "$untag_refused" "2"

echo "release_test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
