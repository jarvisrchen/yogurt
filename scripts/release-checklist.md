# Release checklist -- cutting v0.1.0 (and future releases)

This is the manual release-day runbook.
The repo is currently private and no tag has been cut yet.
Steps 1-4 are one-time bootstrap; steps 5-10 repeat for every release.

Publish hygiene rule (load-bearing, do not reorder): GitHub Release
artifacts must be live BEFORE the Homebrew tap PR opens and BEFORE
`cargo publish` runs.
Reversing the order lets Homebrew's mirror infrastructure cache a 404 that
outlives the fix.
The `needs:` ordering in `.github/workflows/release.yml` (`build -> release -> tap`)
already enforces this -- do not touch it without preserving the order.

## One-time bootstrap

1. **Flip `jarvisrchen/yogurt` to public.**
   GitHub repo settings -> Danger Zone -> Change visibility -> Public.

2. **Authenticate the GitHub CLI as `jarvisrchen`.**
   ```bash
   gh auth login
   ```

3. **Create the Homebrew tap repo from the seed in `scripts/homebrew/`.**
   ```bash
   gh repo create jarvisrchen/homebrew-yogurt --public \
     --description "Homebrew tap for yogurt -- local-first meeting copilot." \
     --license=MIT

   cd /tmp && git clone https://github.com/jarvisrchen/homebrew-yogurt.git
   mkdir -p homebrew-yogurt/Formula
   cp /path/to/yogurt/scripts/homebrew/yogurt.rb homebrew-yogurt/Formula/yogurt.rb
   cd homebrew-yogurt
   git add Formula/yogurt.rb
   git commit -m "init: placeholder formula (release workflow auto-bumps on tag)"
   git push origin main
   ```

4. **Create a fine-grained PAT scoped to `homebrew-yogurt`, add it as `HOMEBREW_TAP_TOKEN`.**
   Fine-grained PATs can only be created in the browser -- there is no `gh`
   or REST path for it. Everything after this step is CLI-doable.
   - <https://github.com/settings/personal-access-tokens/new>
   - Resource owner: `jarvisrchen`. Repository access: only `jarvisrchen/homebrew-yogurt`.
   - Permissions: Contents -> Read and write, **and** Pull requests -> Read and write.
     Both are required: Contents alone pushes the `bump-<version>` branch but
     `gh pr create` then 403s. (Metadata -> Read-only is added automatically.)
   - Add the token as a repo secret on `jarvisrchen/yogurt`:
     ```bash
     gh secret set HOMEBREW_TAP_TOKEN -R jarvisrchen/yogurt   # paste at the prompt
     gh secret list -R jarvisrchen/yogurt                     # confirm it is there
     ```
   - Smoke-test the token before spending a real tag -- this runs the same two
     operations the `tap` job does, in the same order:
     ```bash
     export GH_TOKEN=github_pat_...
     git clone https://x-access-token:$GH_TOKEN@github.com/jarvisrchen/homebrew-yogurt.git /tmp/tap-check
     cd /tmp/tap-check && git checkout -b token-smoke && git commit --allow-empty -m smoke
     git push origin token-smoke
     gh pr create --title smoke --body smoke --base main --head token-smoke
     gh pr close token-smoke -R jarvisrchen/homebrew-yogurt --delete-branch
     cd - && rm -rf /tmp/tap-check && unset GH_TOKEN
     ```

## Every release

5. **Push `main`.**
   ```bash
   git push origin main
   ```

6. **Optional: dry-run the release workflow** (free once the repo is public).
   ```bash
   gh workflow run release.yml -f dry-run=true
   gh run watch
   ```
   Verify the `build` and `release` jobs succeed and the dry-run summary
   lists both tarballs + `SHA256SUMS`. The `tap` job only runs on a real tag
   push, so it is skipped in a dry-run -- that's expected.

7. **Tag and push the real release.**
   ```bash
   git tag -a v0.1.0 -m "yogurt v0.1.0 -- first public release"
   git push origin v0.1.0
   ```

8. **Watch the release workflow.**
   ```bash
   gh run watch
   ```
   `build` (x2 targets), `release`, and `tap` must all go green. `cargo publish`
   is not wired up yet (see Deferred below) so there is no `publish` job.

9. **Smoke-test the install on a clean machine.**
   ```bash
   brew install jarvisrchen/yogurt/yogurt
   yogurt doctor
   yogurt start
   ```
   Also smoke-test the direct-download channel:
   ```bash
   curl -L https://github.com/jarvisrchen/yogurt/releases/download/v0.1.0/yogurt-aarch64-apple-darwin.tar.gz | tar xz
   xattr -d com.apple.quarantine ./yogurt   # unsigned binary, browser download only
   ./yogurt --version
   ```

10. **Deferred items -- not part of this release:**
    - **Apple notarization.** The release pipeline ships unsigned/ad-hoc-signed
      binaries (see the NOTE at the top of `.github/workflows/release.yml`).
      Homebrew/curl/cargo installs never set the `com.apple.quarantine`
      xattr, so Gatekeeper never evaluates these binaries -- notarization only
      matters for binaries downloaded directly through a browser. Adding
      codesign + notarytool + stapler steps back later is self-contained:
      insert them between "strip" and "tarball" in the `build` job without
      touching job structure or `needs:` ordering.
    - **`cargo publish`.** Requires publishing the workspace's library crates
      to crates.io in dependency order first (`yogurt-db`, `yogurt-audio`,
      `yogurt-stt`, `yogurt-llm`, `yogurt-notes`, `yogurt-prompts`, then
      `yogurt-server`, then `yogurt-cli` as `yogurt`), plus a
      `CARGO_REGISTRY_TOKEN` repo secret. When re-added, the `publish` job
      goes last with `needs: [release, tap]` per the publish hygiene rule
      above -- never before `release` or `tap`.
