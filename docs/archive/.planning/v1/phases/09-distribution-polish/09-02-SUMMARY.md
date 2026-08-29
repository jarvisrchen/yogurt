---
phase: 09-distribution-polish
plan: 02
subsystem: release-pipeline
tags: [ci, homebrew, release-workflow]
dependency-graph:
  requires: ["09-01"]
  provides: ["local homebrew tap formula seed", "release.yml tap job"]
  affects: [".github/workflows/release.yml"]
tech-stack:
  added: []
  patterns: ["GitHub Actions heredoc formula generation", "generate_release_notes over hand-maintained CHANGELOG"]
key-files:
  created:
    - scripts/homebrew/yogurt.rb
    - scripts/homebrew/README.md
  modified:
    - .github/workflows/release.yml
decisions:
  - "Homebrew tap bootstrap deferred to local seed files (scripts/homebrew/) instead of creating the remote jarvisrchen/homebrew-yogurt repo this session -- repo is private and gh CLI is authed as the wrong account; orchestrator handles remote bootstrap + secrets later."
  - "cargo publish / crates.io publishing descoped entirely for now -- release.yml has only a one-line comment marking where a future publish job would go (needs: [release, tap])."
  - "CHANGELOG.md not created (user's global rule forbids hand-maintained changelogs) -- release job now uses generate_release_notes: true instead of body_path: CHANGELOG.md."
metrics:
  duration: "~20 minutes"
  completed: "2026-08-28"
---

# Phase 9 Plan 02: Homebrew Tap Seed + Release Tap Job Summary

Seeded a local Homebrew formula placeholder and extended `release.yml` with a `tap` job that will rewrite and PR the formula to the (not-yet-created) `jarvisrchen/homebrew-yogurt` tap repo on real tag pushes; crates.io publishing and CHANGELOG.md were descoped per this session's plan amendments.

## What Was Built

### Task 1 (amended): Local Homebrew tap seed
Instead of creating the remote sibling repo `jarvisrchen/homebrew-yogurt` (blocked -- repo is private, `gh` CLI authed as the wrong account, no remote access this session), created a local seed at `scripts/homebrew/`:

- `scripts/homebrew/yogurt.rb` -- placeholder Homebrew formula. `class Yogurt < Formula`, `version "0.0.0"`, `on_macos` block branching on `Hardware::CPU.arm?` with per-arch GitHub Release tarball URLs and 64-character hex `sha256` placeholders, `install` block (`bin.install "yogurt"`), and a `test do ... end` block asserting `yogurt --version` output contains "yogurt".
- `scripts/homebrew/README.md` -- explains this directory seeds the `jarvisrchen/homebrew-yogurt` tap; documents the one-time bootstrap command (`gh repo create jarvisrchen/homebrew-yogurt --public ...` then copy `Formula/yogurt.rb` into the new repo); explains that after bootstrap, the release workflow's `tap` job keeps the formula updated automatically.

Bootstrapping the actual remote repo and adding the `HOMEBREW_TAP_TOKEN` / `CRATES_IO_TOKEN` secrets is explicitly deferred to the orchestrator, per user instruction. No `gh` commands were run.

### Task 2 (amended): `tap` job in `release.yml`
Added a `tap` job to `.github/workflows/release.yml`, positioned after the existing `release` job (from plan 09-01):

- `needs: release`, `if: github.event_name == 'push'` -- runs only on real tag pushes, never on `workflow_dispatch` dry-runs.
- Downloads the per-arch tarball artifacts (`actions/download-artifact@v4`, `pattern: yogurt-*`, `merge-multiple: true`).
- Computes `arm_sha` / `x86_sha` via `shasum -a 256` on each tarball, and resolves `version` from `GITHUB_REF`.
- Checks out `jarvisrchen/homebrew-yogurt` into `tap/` using `secrets.HOMEBREW_TAP_TOKEN`.
- Rewrites `tap/Formula/yogurt.rb` via heredoc with the resolved version and per-arch SHA256s (verified the generated Ruby is syntactically valid after GitHub Actions expression substitution -- see Verification below).
- Opens a PR against the tap's `main` via `gh pr create`, using `GH_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}`.

**No `publish` job was added** (crates.io publishing deferred per amendment). A one-line comment at the bottom of the workflow marks where it would go (`needs: [release, tap]`, preserving the `build -> release -> tap -> publish` ordering documented in `09-CONTEXT.md` D-03/D-10) so a future plan can add it without re-deriving the ordering constraint.

**Additionally** changed the `release` job's `Create GitHub Release` step from `body_path: CHANGELOG.md` to `generate_release_notes: true` -- CHANGELOG.md was never created (skipped per amendment 4; the user's global rules forbid hand-maintained CHANGELOG files), so the release body now comes from GitHub's auto-generated notes instead.

### Task 3: SKIPPED
CHANGELOG.md creation skipped entirely per amendment. Its only load-bearing consumer (the `release` job's `body_path`) was re-pointed at `generate_release_notes: true` instead, so nothing depends on a CHANGELOG.md file anymore.

### Task 4 (amended): Local verification only
No remote workflow trigger was run (`gh workflow run`, `gh run watch` -- all skipped, no `gh` commands executed). Verified locally instead:

- **YAML validity:** `ruby -ryaml -e "YAML.load_file('.github/workflows/release.yml')"` parses cleanly.
- **Job graph:** `jobs.keys` = `["build", "release", "tap"]`; `release.needs` = `"build"`; `tap.needs` = `"release"`; `tap.if` = `"github.event_name == 'push'"` -- confirms `build -> release -> tap` ordering and the dry-run skip guard.
- **Content checks:** workflow contains `jarvisrchen/homebrew-yogurt` and `HOMEBREW_TAP_TOKEN`; does NOT contain any `run:` line invoking `cargo publish` (only the deferred-work comment mentions it in prose).
- **Formula Ruby syntax:** `ruby -c scripts/homebrew/yogurt.rb` -> `Syntax OK`. Additionally extracted the `tap` job's heredoc-generated formula via the parsed YAML, substituted sample values for the `${{ }}` GitHub Actions expressions, and ran `ruby -c` against the result -> `Syntax OK` (confirms the heredoc's indentation and interpolation produce valid Ruby once GitHub Actions resolves the expressions).

## Deviations from Plan

### Plan Amendments (user-directed, not auto-applied deviations)

1. **Skipped the `user_setup` checkpoint task** (bootstrap tap repo + add repo secrets) -- no `gh` commands run, no remote repo created, no secrets added. Explicitly deferred to the orchestrator for a later session with correct GitHub account access.
2. **Task 1 redirected from remote repo creation to local seed** at `scripts/homebrew/` (`yogurt.rb` + `README.md`) as described above.
3. **Task 2's `publish` job omitted entirely**; only the `tap` job was added, plus a one-line deferred-work comment. `cargo publish` / `CRATES_IO_TOKEN` are not referenced anywhere in the workflow.
4. **Task 3 (CHANGELOG.md) skipped entirely** -- forbidden by the user's global CLAUDE.md rule against hand-maintained CHANGELOG files. `generate_release_notes: true` substituted as the release-notes source.
5. **Task 4's remote dry-run trigger skipped** -- local-only verification performed instead (YAML parse, Ruby syntax checks, manual job-graph review), as detailed above.

### Auto-fixed Issues
None beyond the amendments above -- no bugs found in the pre-existing `release.yml` (build/release jobs from 09-01 were left untouched, as instructed).

## Known Stubs

- `scripts/homebrew/yogurt.rb` sha256 values are placeholders (`0` x64) and `version "0.0.0"` -- intentional; the `tap` job in `release.yml` overwrites these with real values on every real tag push. Will only become real once a tag is cut (plan 09-03) AND the tap repo is bootstrapped (deferred, see above).
- The `tap` job cannot actually run end-to-end yet: it depends on the `jarvisrchen/homebrew-yogurt` repo existing and `HOMEBREW_TAP_TOKEN` being set as a repo secret -- neither exists yet (deferred to the orchestrator). The job is syntactically and structurally correct but untested against the real remote target.

## Threat Flags

| Flag | File | Description |
|------|------|--------------|
| threat_flag: new-external-write-target | `.github/workflows/release.yml` (`tap` job) | New job writes to an external repo (`jarvisrchen/homebrew-yogurt`) using a scoped PAT (`HOMEBREW_TAP_TOKEN`) via `gh pr create` -- not present in plan 09-01's workflow. Token is intentionally repo-scoped per the plan's `user_setup` guidance (Contents + Pull requests write only, single repo). No code in this repo handles the token beyond passing it to `actions/checkout` and `gh`/`git` env vars.

## Self-Check: PASSED

- FOUND: `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/scripts/homebrew/yogurt.rb`
- FOUND: `/Users/rchen/.treehouse/yogurt-c2d339/2/yogurt/scripts/homebrew/README.md`
- FOUND: commit `b0948e3` (chore(release): seed local Homebrew tap formula placeholder)
- FOUND: commit `a263bf5` (ci(release): add Homebrew tap PR job; defer crates.io publish)
- FOUND (git log --oneline --all | grep -q): both hashes present in `git log`.
