---
name: release
description: Cut a yogurt release end to end - verify prerequisites, dry-run the pipeline, push the version tag, merge the Homebrew tap PR, and smoke-test brew install. Use when asked to release, cut a version, ship a v0.x.y, publish to Homebrew, or update the tap formula.
---

# Cut a yogurt release

Reference: [docs/RELEASING.md](../../../docs/RELEASING.md) explains why the pipeline is shaped this way, the decisions behind shipping an unsigned prebuilt binary, and the failure-recovery paths.
This skill is the procedure. Read the runbook when something deviates from it.

Work through the checklist in order.
Do not skip the dry run, and do not push a tag on a red `main`.

## The one thing to know first

**Pushing a tag matching `v*` is the only trigger.**
Merging a PR or pushing to `main` runs CI and publishes nothing.
The tag selects the commit that gets built, so `main` can move freely without shipping.

## Checklist

1. **Working tree clean, CI green on `main`.**
   Changes reach `main` by PR, never a direct push, so this should already be true.
   ```bash
   git status --porcelain                              # must be empty
   gh run list -R jarvisrchen/yogurt -w CI -L 1 --json headSha,conclusion
   ```
   The green run's `headSha` must be the commit you are about to tag.

2. **Version matches the tag you intend to push.**
   ```bash
   grep -n '^version' Cargo.toml                       # [workspace.package]
   ```
   `Cargo.toml` carries the tag minus the leading `v`.
   Bump it with `cargo update --workspace` afterwards so `Cargo.lock` moves too; `cargo metadata --no-deps` does NOT write the lock, and a stale lock otherwise rides into the tagged tree.
   If that command fails with `failed to select a version for the requirement <crate>`, an intra-workspace path dep has a hand-pinned `version =` that needs removing or bumping.
   A mismatch is silent and ugly: the formula installs a binary whose `yogurt --version` disagrees with the version Homebrew thinks it installed, and the formula's `test do` block can still pass because it only greps for the string `yogurt`.
   Bump and commit this before tagging, never after.

3. **Lockfile in sync**, because CI runs `--frozen-lockfile`.
   ```bash
   pnpm --dir web install --frozen-lockfile
   ```

4. **README does not contradict the release.**
   Check the `**Status:**` line and the Homebrew heading for stale "coming soon" wording.
   Land any fix before the tag so the released tree describes itself accurately.

   **Merge the outstanding doc PRs first.** The tag freezes the tree, so anything still in review ships as whatever `main` said at tag time.
   ```bash
   gh pr list -R jarvisrchen/yogurt --json number,title,files \
     --jq '.[]|select(.files|any(.path|startswith("docs/") or . == "README.md"))|"#\(.number) \(.title)"'
   ```
   The failure this prevents is one-directional and worse than a stale doc: a feature PR whose README describes a thing that is not published yet. v0.3.0 shipped `brew install jarvisrchen/yogurt/yogurt-model-*` instructions while those formulae existed only in an untracked local file, so the released README told every user to run a command that returned "No available formula".
   Ask of each doc change: does this describe something a user can do the moment they install this tag? If not, either publish the dependency first or hold the doc.

   The release log row (step 11) is the one exception - it records run IDs and verified shas that do not exist until after the tag, so it can only land afterwards.

5. **Dry run.** Never tag first.
   ```bash
   gh workflow run Release -R jarvisrchen/yogurt -f dry-run=true --ref main
   gh run watch <id> -R jarvisrchen/yogurt --exit-status
   ```
   Builds both arches and stops before publishing.
   A green `x86_64` leg alone proves nothing: only arm64 compiles whisper's Metal backend, so arch-specific link failures show up there and nowhere else.
   The `tap` job is gated on `event_name == 'push'` and stays skipped here, so a dry run never validates the tap token.

6. **Tag and push.**
   ```bash
   git tag v0.1.0 && git push origin v0.1.0
   ```

7. **Watch it through.** Four jobs: `build` twice, `release`, `tap`.
   ```bash
   gh run watch <id> -R jarvisrchen/yogurt --exit-status
   ```

8. **Verify the formula shas against the real assets.**
   Do not trust the workflow's own `SHA256SUMS`; re-download and hash.
   ```bash
   gh release download v0.1.0 -R jarvisrchen/yogurt --clobber -p "*.tar.gz"
   shasum -a 256 yogurt-*.tar.gz
   gh pr diff <n> -R jarvisrchen/homebrew-yogurt
   ```

9. **Merge the tap PR.** Until this merges, `brew install` serves the previous formula.
   ```bash
   gh pr merge <n> -R jarvisrchen/homebrew-yogurt --squash --delete-branch
   ```

10. **Smoke test from a clean state.**
    ```bash
    brew untap jarvisrchen/yogurt; brew uninstall yogurt
    brew install jarvisrchen/yogurt/yogurt
    yogurt --version
    xattr -l "$(which yogurt)"      # com.apple.provenance ok, com.apple.quarantine is not
    ```

11. **Log it.** Add a row to the release log in `docs/RELEASING.md` and tick any prerequisite that changed.

## When it goes wrong

The GitHub Release publishes *before* the `tap` job runs, so a tap failure leaves a public release with a stale formula.
Fix `Formula/yogurt.rb` by hand in the tap repo rather than re-cutting the release.

For a build failure after the tag is already pushed, delete the tag locally and remotely, fix, re-tag.
Safe only while no Release was published and nobody has installed.

Full recovery guidance is in [docs/RELEASING.md](../../../docs/RELEASING.md).
