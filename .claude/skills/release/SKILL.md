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

1. **Run preflight and act on its judgment items.**
   ```bash
   ./scripts/release.sh preflight <version>
   ```
   Read-only: fetches `origin/main` and checks that `gh`/`jq` are present and authenticated, that the tag does not already exist on origin, that `Cargo.toml`'s version on `origin/main` is below `<version>`, that CI is green for `origin/main`'s sha, and prints the release's scope (`git log <lasttag>..origin/main`).
   A `FAIL:` line is a real blocker - fix it before continuing.
   Two items print as judgment, not failures, unless `--strict` is passed:
   - **Open PRs touching `docs/` or `README.md`.** The tag freezes the tree, so anything still in review ships as whatever `main` said at tag time.
     Ask of each: does this describe something a user can do the moment they install this tag?
     v0.3.0 shipped `brew install jarvisrchen/yogurt/yogurt-model-*` instructions while those formulae existed only in an untracked local file, so the released README told every user to run a command that returned "No available formula".
     Merge first only if the answer is yes; otherwise hold the doc or publish the dependency first.
   - **README wording** (`Status:`, "coming soon", "not yet"). Land a fix before the tag if it now contradicts the release.

   If `Cargo.toml`'s version needs bumping, do it now and commit before tagging, never after: `cargo update --workspace` afterwards so `Cargo.lock` moves too (`cargo metadata --no-deps` does NOT write the lock).
   If that command fails with `failed to select a version for the requirement <crate>`, an intra-workspace path dep has a hand-pinned `version =` that needs removing or bumping.
   A version mismatch used to be silent and ugly: the formula could install a binary whose `yogurt --version` disagreed with the version Homebrew thought it installed, and the old `test do` block still passed because it only checked for the substring `yogurt`.
   `release.yml` now asserts the exact `assert_equal "yogurt #{version}", ...` output, so a mismatch fails the tap formula's own test - but the `tap` job never runs on a dry run, so this assertion is unexercised until the next real release; note in that release's log row whether it held.

2. **Dry run.** Never tag first.
   ```bash
   gh workflow run Release -R jarvisrchen/yogurt -f dry-run=true --ref main
   gh run watch <id> -R jarvisrchen/yogurt --exit-status
   ```
   Builds both arches and stops before publishing.
   A green `x86_64` leg alone proves nothing: only arm64 compiles whisper's Metal backend, so arch-specific link failures show up there and nowhere else.
   The `tap` job is gated on `event_name == 'push'` and stays skipped here, so a dry run never validates the tap token.

3. **Tag and push.**
   ```bash
   git tag v0.1.0 && git push origin v0.1.0
   ```

4. **Watch it through.** Four jobs: `build` twice, `release`, `tap`.
   ```bash
   gh run watch <id> -R jarvisrchen/yogurt --exit-status
   ```

5. **Finish.**
   ```bash
   ./scripts/release.sh finish <version>
   ```
   Re-downloads and hashes both tarballs against `SHA256SUMS`, checks the tap formula's shas and version, merges the tap PR, upgrades the local brew install (`brew reinstall` when already at that version), runs `brew test`, checks for no `com.apple.quarantine`, and prints a pre-filled `docs/RELEASE-LOG.md` row with a `NARRATIVE:` slot.
   Every step is skip-if-done, so re-running it resumes rather than repeating work.
   `finish <version> -n` prints the plan without running anything; `--no-smoke` skips the brew steps.
   Paste the printed row into `docs/RELEASE-LOG.md`, write the `NARRATIVE:` sentence, and open the log PR.

## When it goes wrong

The GitHub Release publishes *before* the `tap` job runs, so a tap failure leaves a public release with a stale formula.
Fix `Formula/yogurt.rb` by hand in the tap repo rather than re-cutting the release.

For a build failure after the tag is already pushed, `./scripts/release.sh untag <version>` deletes the local and remote tag - it refuses (exit 2) once a GitHub Release exists, since at that point the fix is `Formula/yogurt.rb` by hand, not a re-tag.
Safe only while no Release was published and nobody has installed.

Full recovery guidance is in [docs/RELEASING.md](../../../docs/RELEASING.md).
