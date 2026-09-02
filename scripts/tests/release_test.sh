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

# --- highest_tag_below - pure, a synthetic list, no git at all ----------

tags_list=$'v0.6.0\nv0.7.0\nv0.10.0\nmodels-v1\n'
check "highest_tag_below 0.7.0 picks v0.6.0" "$(printf '%s' "$tags_list" | highest_tag_below 0.7.0)" "v0.6.0"
check "highest_tag_below 0.10.0 is semver, not lexical" "$(printf '%s' "$tags_list" | highest_tag_below 0.10.0)" "v0.7.0"
check "highest_tag_below 0.6.0 has no predecessor in the list" "$(printf '%s' "$tags_list" | highest_tag_below 0.6.0)" ""
check "highest_tag_below ignores non-release tags" "$(printf 'models-v1\n' | highest_tag_below 9.9.9)" ""

# --- previous_tag / ships_since - a throwaway fixture repo, never the
# real repo or the network: "origin" is a local bare repo, so this works
# the same in a full clone or CI's fetch-depth-1 shallow one. -----------

FIXTURE="$WORK/fixture"
BARE="$WORK/origin.git"
git init -q --bare "$BARE"
git init -q -b main "$FIXTURE"
git -C "$FIXTURE" remote add origin "$BARE"
commit() { git -C "$FIXTURE" -c user.email=t@t -c user.name=t commit -q --allow-empty -m "$1"; }
commit "first"
git -C "$FIXTURE" tag v0.6.0
commit "second (#10)"
commit "third (#11)"
git -C "$FIXTURE" tag v0.7.0
commit "fourth"
git -C "$FIXTURE" tag v0.10.0
git -C "$FIXTURE" tag models-v1
git -C "$FIXTURE" push -q origin main --tags

check "previous_tag(0.7.0) via ls-remote" "$(cd "$FIXTURE" && previous_tag 0.7.0)" "v0.6.0"
check "previous_tag(0.10.0) is semver-ordered" "$(cd "$FIXTURE" && previous_tag 0.10.0)" "v0.7.0"
check "previous_tag(0.6.0) has no predecessor" "$(cd "$FIXTURE" && previous_tag 0.6.0)" ""

want_ships="$(git -C "$FIXTURE" log v0.6.0..v0.7.0 --oneline | awk '{printf "%s%s", (NR>1?"; ":""), $0}')"
check "ships_since matches git log --oneline, joined" "$(cd "$FIXTURE" && ships_since v0.6.0 0.7.0)" "$want_ships"

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

# untag -n - reuses the fixture repo above, adding one more tag; still no
# network, since "origin" is that same local bare repo.
git -C "$FIXTURE" tag v9.9.9
git -C "$FIXTURE" push -q origin v9.9.9

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

# --- ship: marker round trip --------------------------------------------

marker="$(ship_marker deadbeefcafe)"
check "ship_marker format" "$marker" "<!-- yogurt-release: P=deadbeefcafe -->"
check "ship_marker_sha round trip" "$(ship_marker_sha "$marker")" "deadbeefcafe"
check "ship_marker_sha round trip, marker embedded in a larger PR body" \
  "$(ship_marker_sha "Ships since v0.7.0: abc first.

$marker
")" "deadbeefcafe"
check "ship_marker_sha: no marker present" "$(ship_marker_sha "no marker here")" ""

# --- ship: ship_tag_status - pure, no git at all ------------------------

check "ship_tag_status: no tag yet" "$(ship_tag_status "" abc123)" "missing"
check "ship_tag_status: tag already at target" "$(ship_tag_status abc123 abc123)" "same"
check "ship_tag_status: tag points elsewhere" "$(ship_tag_status abc123 def456)" "conflict"

# --- ship: ship_main_not_moved - a throwaway local repo, real content so
# is_docs_only's file-pattern judgment is actually exercised (unlike the
# FIXTURE repo above, whose commits are --allow-empty). --------------------

DOCS_FIXTURE="$WORK/docs_fixture"
git init -q -b main "$DOCS_FIXTURE"
git -C "$DOCS_FIXTURE" -c user.email=t@t -c user.name=t commit -q --allow-empty -m base
base_sha="$(git -C "$DOCS_FIXTURE" rev-parse HEAD)"

echo "notes" >"$DOCS_FIXTURE/docs_note.md"
git -C "$DOCS_FIXTURE" add docs_note.md
git -C "$DOCS_FIXTURE" -c user.email=t@t -c user.name=t commit -q -m "docs change"
docs_sha="$(git -C "$DOCS_FIXTURE" rev-parse HEAD)"

echo 'fn main() {}' >"$DOCS_FIXTURE/src.rs"
git -C "$DOCS_FIXTURE" add src.rs
git -C "$DOCS_FIXTURE" -c user.email=t@t -c user.name=t commit -q -m "code change"
code_sha="$(git -C "$DOCS_FIXTURE" rev-parse HEAD)"

same_result=yes
(cd "$DOCS_FIXTURE" && ship_main_not_moved "$base_sha" "$base_sha") || same_result=no
check "ship_main_not_moved: parent(S) == P passes" "$same_result" "yes"

docs_result=yes
(cd "$DOCS_FIXTURE" && ship_main_not_moved "$base_sha" "$docs_sha") || docs_result=no
check "ship_main_not_moved: a docs-only commit between passes" "$docs_result" "yes"

code_result=yes
(cd "$DOCS_FIXTURE" && ship_main_not_moved "$base_sha" "$code_sha") || code_result=no
check "ship_main_not_moved: a code commit between fails" "$code_result" "no"

# --- ship -n: full plan, in order, against a clean fixture --------------
# A dedicated fixture (not FIXTURE above, whose tags/commits are shaped for
# the finish/untag tests): a Cargo.toml + README.md, one commit tagged
# v0.9.0, one commit after it, so preflight's checks all pass for target
# 1.0.0. gh is re-stubbed here to answer only what preflight + the plan
# printer call - `-R jarvisrchen/yogurt` throughout, unlike the `--repo
# jarvisrchen/homebrew-yogurt` stub above.

SHIP_FIXTURE="$WORK/ship_fixture"
SHIP_BARE="$WORK/ship_origin.git"
git init -q --bare "$SHIP_BARE"
git init -q -b main "$SHIP_FIXTURE"
git -C "$SHIP_FIXTURE" remote add origin "$SHIP_BARE"
cat >"$SHIP_FIXTURE/Cargo.toml" <<'EOF'
[workspace.package]
version = "0.9.0"
EOF
echo "# yogurt" >"$SHIP_FIXTURE/README.md"
git -C "$SHIP_FIXTURE" add Cargo.toml README.md
git -C "$SHIP_FIXTURE" -c user.email=t@t -c user.name=t commit -q -m first
git -C "$SHIP_FIXTURE" tag v0.9.0
echo "more" >>"$SHIP_FIXTURE/README.md"
git -C "$SHIP_FIXTURE" add README.md
git -C "$SHIP_FIXTURE" -c user.email=t@t -c user.name=t commit -q -m "second (#99)"
git -C "$SHIP_FIXTURE" push -q origin main --tags

cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
# stub gh - a clean-slate ship 1.0.0 -n: gh auth ok, CI green on any sha,
# no open doc PRs, no existing dry run, no existing bump PR.
args="$*"
case "$args" in
  "auth status") exit 0 ;;
  *"run list -R jarvisrchen/yogurt -w CI "*)
    echo '[{"status":"completed","conclusion":"success"}]' ;;
  *"pr list -R jarvisrchen/yogurt --state open"*) echo '[]' ;;
  *"run list -R jarvisrchen/yogurt -w Release "*) : ;; # --jq already applied: empty means no green dry run
  *"pr list -R jarvisrchen/yogurt --head release/v"*) : ;; # --jq already applied: empty means no bump PR
  *)
    echo "unstubbed gh call: $args" >&2
    exit 1
    ;;
esac
EOF
chmod +x "$BIN/gh"

plan_out="$(cd "$SHIP_FIXTURE" && cmd_ship 1.0.0 -n 2>&1)"
contains "ship -n: plan header" "$plan_out" "PLAN for ship v1.0.0:"
contains "ship -n: dry-run dispatch" "$plan_out" "gh workflow run release.yml -R jarvisrchen/yogurt -f dry-run=true --ref main"
contains "ship -n: bump worktree" "$plan_out" "git worktree add --detach"
contains "ship -n: bump commit" "$plan_out" 'git commit -m "chore: bump version to 1.0.0"'
contains "ship -n: bump PR open" "$plan_out" "gh pr create -R jarvisrchen/yogurt --base main --head release/v1.0.0"
contains "ship -n: bump PR checks" "$plan_out" "gh pr checks <N> -R jarvisrchen/yogurt --watch --fail-fast"
contains "ship -n: bump PR merge" "$plan_out" 'gh pr merge <N> -R jarvisrchen/yogurt --squash --subject "chore: bump version to 1.0.0 (#N)"'
contains "ship -n: tag creation" "$plan_out" "gh api repos/jarvisrchen/yogurt/git/refs -f ref=refs/tags/v1.0.0 -f sha=<merge sha>"
contains "ship -n: finish" "$plan_out" "scripts/release.sh finish 1.0.0"

before() {
  local desc="$1" haystack="$2" first="$3" second="$4"
  case "$haystack" in
    *"$first"*"$second"*) pass=$((pass + 1)) ;;
    *)
      fail=$((fail + 1))
      printf 'FAIL: %s (expected to find %s before %s)\n' "$desc" "$first" "$second" >&2
      ;;
  esac
}

before "ship -n order: dry-run before bump worktree" "$plan_out" \
  "gh workflow run release.yml" "git worktree add --detach"
before "ship -n order: bump worktree before PR open" "$plan_out" \
  "git worktree add --detach" "gh pr create -R jarvisrchen/yogurt"
before "ship -n order: PR open before checks" "$plan_out" \
  "gh pr create -R jarvisrchen/yogurt" "gh pr checks <N>"
before "ship -n order: checks before merge" "$plan_out" \
  "gh pr checks <N>" "gh pr merge <N>"
before "ship -n order: merge before tag" "$plan_out" \
  "gh pr merge <N>" "gh api repos/jarvisrchen/yogurt/git/refs"
before "ship -n order: tag before finish" "$plan_out" \
  "gh api repos/jarvisrchen/yogurt/git/refs" "scripts/release.sh finish 1.0.0"

# --- ship -n: preflight blocks before any plan is printed - the tag
# already exists for 0.7.0 in the real repo's history is what the live
# verification run exercises; here, reuse SHIP_FIXTURE with target
# "0.9.0", which check_tag_available rejects (v0.9.0 already tagged). -----

preflight_block_status=0
preflight_block_out="$(cd "$SHIP_FIXTURE" && cmd_ship 0.9.0 -n 2>&1)" || preflight_block_status=$?
check "ship -n: preflight failure stops before the plan (exit 1)" "$preflight_block_status" "1"
contains "ship -n: preflight failure message" "$preflight_block_out" "FAIL: preflight did not pass for v0.9.0"
case "$preflight_block_out" in
  *"PLAN for ship"*)
    fail=$((fail + 1))
    printf 'FAIL: ship -n: preflight failure must not reach the plan printer\n' >&2
    ;;
  *) pass=$((pass + 1)) ;;
esac

echo "release_test: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
