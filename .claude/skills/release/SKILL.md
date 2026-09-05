---
name: release
description: Cut a yogurt release end to end - verify prerequisites, dry-run the pipeline, push the version tag, merge the Homebrew tap PR, and smoke-test brew install. Use when asked to release, cut a version, ship a v0.x.y, publish to Homebrew, or update the tap formula.
---

# Cut a yogurt release

Reference: [docs/RELEASING.md](../../../docs/RELEASING.md) - decisions, prerequisites, and failure-recovery paths.

Release when a merged PR is worth shipping, not on a schedule.

1. **Preflight.**
   ```bash
   ./scripts/release.sh preflight <version>
   ```
   A `FAIL:` line is a real blocker.
   The one judgment call: an open PR touching `docs/` or `README.md` - merge it first only if it describes something a user can do the moment this tag installs.
   v0.3.0 shipped `brew install` instructions for a formula that did not exist yet.

2. **Ship.**
   ```bash
   ./scripts/release.sh ship <version>
   ```
   Dry-runs the pipeline, bumps `Cargo.toml` in a throwaway PR, merges it, tags the merge commit, watches the build, then runs `verify` and `finish`.
   Skip-if-done throughout - on a timeout, call it again; it resumes.
   `-n` previews the plan; `--allow-open-docs` and `--no-smoke` cover the exceptions.

3. **Log it.** Paste the printed `docs/RELEASE-LOG.md` row, add a bullet for anything the release taught us, open the log PR (docs-only).

## When it goes wrong

`./scripts/release.sh untag <version>` deletes a bad tag - refuses once a GitHub Release exists, since past that point the fix is `Formula/yogurt.rb` by hand in the tap repo.
Full recovery paths are in [docs/RELEASING.md](../../../docs/RELEASING.md).

`strings | comm` looks like proof a feature landed in the binary and is not - long lines differ everywhere, so use `git tag --contains` plus a direct `strings | grep` instead.
