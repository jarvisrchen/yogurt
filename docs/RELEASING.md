# Releasing yogurt

How a tagged commit becomes `brew install jarvisrchen/yogurt/yogurt` on someone else's Mac.

The pipeline is [`.github/workflows/release.yml`](../.github/workflows/release.yml).
This doc is the operational companion: what the pipeline does, what has to be true before you tag, and a log of what actually happened each release.

---

## What triggers a release

Pushing a tag that matches `v*`.
That is the only thing that publishes anything.

Merging a PR into `main` does **not** release.
Neither does pushing commits to `main` directly.
Those run `ci.yml` (fmt, clippy, tests, web build) and stop there.
`release.yml` never looks at branches, only at tags, so `main` can move as much as you like without shipping.

The tag is what selects the code, not the branch.
`git tag v0.1.0` on whatever commit you are on, then `git push origin v0.1.0`, and that commit is what gets built and published.

The one other trigger is `workflow_dispatch`, which is the dry run.
It builds both arches and stops before publishing.

There is a PR in the flow, but it runs the other way and lands at the end.
The pipeline opens it against the **tap** repo, not this one, and by the time it exists the GitHub Release is already public.
Merging it is the last manual step, and it is what makes `brew install` serve the new version.

## The shape of a release

Users never compile anything.
GitHub Actions builds the binary, attaches it to a GitHub Release, and rewrites the Homebrew formula to point at it.
Homebrew just downloads and untars.

```
git push origin v0.1.0        <- the trigger; nothing else starts this
      |
      +-- build      macos-26, matrix aarch64 + x86_64
      |              rust 1.96 + node 22 + pnpm 9.15.4
      |              pnpm build (web/dist must exist -- rust-embed needs it)
      |              cargo build --release -p yogurt, strip, tar.gz, sha256
      |
      +-- release    attaches both tarballs + SHA256SUMS to the GitHub Release
      |
      +-- tap        rewrites Formula/yogurt.rb with the real version + shas,
                     opens a PR on jarvisrchen/homebrew-yogurt
                            |
                     you merge it by hand
                            |
                     brew install works
```

The `tap` job is gated on `if: github.event_name == 'push'`, so a `workflow_dispatch` dry run exercises the build but never touches the tap.

## Decisions worth not relitigating

**The formula ships a prebuilt binary, not source.**
It downloads a tarball and does `bin.install "yogurt"`, with zero `depends_on`.
No Rust, Node, pnpm, or Xcode on the user's machine.
This works because the binary is genuinely self-contained: `web/dist` is embedded via rust-embed, SQLite is bundled by rusqlite, and `MACOSX_DEPLOYMENT_TARGET=13.0` matches the macOS 13+ floor.

**Binaries are unsigned / ad-hoc signed, and that is fine for Homebrew.**
Homebrew and `curl` installs never set the `com.apple.quarantine` xattr, so Gatekeeper never evaluates them.
Notarization only matters for browser-downloaded artifacts, which is why the README's direct-download path documents `xattr -d com.apple.quarantine` and the Homebrew path does not.
Adding codesign + notarize + staple later is self-contained: it slots between `strip` and `tarball` without touching job structure or `needs:` ordering.

**We deliberately do not ship a `.app` bundle, and macOS permission prompts say "Terminal", not "Yogurt".**
`yogurt` is a bare Mach-O with no `Info.plist` and no bundle identifier, so TCC attributes Screen Recording and Microphone requests to the *responsible process* - whichever terminal launched it.
This is intended.
Consequences: the grant is coarse (every CLI run from that terminal inherits it), it is per-terminal-app (switching from Terminal to iTerm re-prompts), and users revoke it under Terminal in System Settings rather than under Yogurt.
Signing the CLI with a Developer ID would not change the prompt; only wrapping it in a `.app` with a `CFBundleIdentifier` would.
Do not add a bundle without deciding to give up the Terminal attribution.

**crates.io publishing is descoped.**
When it returns it goes in as a `publish` job with `needs: [release, tap]`.
Never reorder so publish runs before release or tap.

## Why not bare `brew install yogurt`

That requires homebrew-core, whose bar is roughly 75+ stars / 30+ forks / 30+ watchers on a public repo, plus a formula that **builds from source** - core rejects precompiled-binary formulae outright.
The practical equivalent today is two commands, after which the bare name works forever:

```bash
brew tap jarvisrchen/yogurt
brew install yogurt
```

## One-time prerequisites

These are not per-release, but every one of them has to hold before the first `brew install` can succeed.

- [x] **`jarvisrchen/yogurt` is public.** Release assets on a private repo require an auth token to download, so `brew install` 404s for everyone but the owner.
- [x] **`jarvisrchen/homebrew-yogurt` exists and is public,** with a `Formula/` directory. Done.
- [x] **`HOMEBREW_TAP_TOKEN` is set** as a repo secret on `jarvisrchen/yogurt`, with push + `pull_requests: write` on the tap repo. Check with `gh api repos/jarvisrchen/homebrew-yogurt --jq '.permissions'` using that token.
- [x] **Git history carries no real secrets.** `.env.local` is gitignored; the history scan for `sk-` / `AKIA` / `ghp_` / `gsk_` found only a placeholder in an archived `MANUAL_TESTING.md`.

## Cutting a release

1. **Land everything.** Working tree clean, CI green on `main`.
2. **Check the lockfile.** CI runs `pnpm install --frozen-lockfile`; a `package.json` edit without a matching `pnpm-lock.yaml` fails the build. Verify locally with `pnpm --dir web install --frozen-lockfile`.
3. **Set the version.** `Cargo.toml` `[workspace.package] version` must match the tag you are about to push, minus the `v`.
4. **Dry run.**
   ```bash
   gh workflow run Release -R jarvisrchen/yogurt -f dry-run=true --ref main
   gh run watch <id> -R jarvisrchen/yogurt --exit-status
   ```
   This builds both arches and skips publishing. Do it before tagging - a tag you have to delete and re-push is the messy failure mode.
5. **Tag and push.**
   ```bash
   git tag v0.1.0 && git push origin v0.1.0
   ```
6. **Merge the tap PR.** The pipeline only *opens* it. Until it merges, `brew install` serves the previous formula.
7. **Smoke test from a clean state.**
   ```bash
   brew uninstall yogurt; brew untap jarvisrchen/yogurt
   brew install jarvisrchen/yogurt/yogurt
   yogurt --version   # formula's `test do` asserts this contains "yogurt"
   yogurt start
   ```

## When it goes wrong

**Build failed after the tag was pushed.** Delete the tag locally and remotely (`git tag -d v0.1.0 && git push --delete origin v0.1.0`), fix, re-tag. Safe as long as no Release was published and nobody installed.

**Release published but the formula is wrong.** Fix `Formula/yogurt.rb` directly in the tap repo; it is a plain file and takes effect on the next `brew update`. No need to re-cut the release.

**`brew install` reports a sha256 mismatch.** The tarball was rebuilt after the formula was written. Recompute with `shasum -a 256` against the actual release asset and commit the correction to the tap.

**arm64 fails to link with `___isPlatformVersionAtLeast` undefined.**
whisper.cpp's Metal backend guards newer Metal APIs with `@available`, and because the deployment target is macOS 13 while the guards probe macOS 14+, clang cannot fold them and lowers each into a call to `__isPlatformVersionAtLeast`.
That symbol lives in `libclang_rt.osx.a`, which rustc never puts on the link line because it links with `-nodefaultlibs`.
`crates/yogurt-stt/build.rs` exists solely to fix this.
x86_64 is immune because ggml only builds `ggml-metal` for arm64, so a green x86_64 leg tells you nothing about arm64.
If this resurfaces, check that the build script still resolves a runtime dir - it degrades to a `cargo:warning` rather than failing loudly.

## Release log

| Version | Date | Notes |
| --- | --- | --- |
| v0.1.0 pre-flight | 2026-08-30 | First dry run of the Release workflow ([33338166544](https://github.com/jarvisrchen/yogurt/actions/runs/33338166544)) - the workflow had never executed before. x86_64 built and packaged; **aarch64 failed at link** on an undefined `___isPlatformVersionAtLeast` out of whisper's `ggml-metal-device.m.o`. Fixed by [PR #1](https://github.com/jarvisrchen/yogurt/pull/1) (`crates/yogurt-stt/build.rs`), which had independently hit the same failure via `setup.sh` on a clean machine. Dry run [33338873469](https://github.com/jarvisrchen/yogurt/actions/runs/33338873469) is green on both arches. Repo still private at the time; tap formula still the `0.0.0` placeholder with zeroed shas. |
| v0.1.0 | 2026-08-30 | First real release. All four jobs green ([33339847685](https://github.com/jarvisrchen/yogurt/actions/runs/33339847685)), including `tap`, so the token scopes were right. Formula shas verified by re-downloading the published tarballs (`ec7c4de3...` arm64, `1ecac01c...` x86_64). Tap PR merged, `brew install jarvisrchen/yogurt/yogurt` installs 11.7 MB into the Cellar and `yogurt --version` prints `yogurt 0.1.0`. The installed binary carries `com.apple.provenance` but no `com.apple.quarantine`, confirming Gatekeeper stays out of the Homebrew path. |
| v0.2.0 | 2026-08-30 | Ships the Deepgram key Test button (#2). All four jobs green ([33349475657](https://github.com/jarvisrchen/yogurt/actions/runs/33349475657)) after a clean dry run. Formula shas verified against re-downloaded tarballs (`568d5468...` arm64, `39f4e1ca...` x86_64). `brew install` serves 0.2.0, no `com.apple.quarantine`, and the shipped binary contains both `/api/settings/stt/test` and the Deepgram probe URL, so the feature is in the artifact rather than just the version string. Bumping caught a footgun: `yogurt-cli` pinned `yogurt-server = { version = "0.1.0" }`, which made `cargo update --workspace` fail outright. Removed, since a version on a path dep is only needed for crates.io. |
| v0.3.0 | 2026-09-01 | Ships AUD-4 (models resolve from a Homebrew prefix, #20) and AUD-1 (cumulative live partials, #21). All four jobs green ([33455470834](https://github.com/jarvisrchen/yogurt/actions/runs/33455470834)) after a clean dry run ([33455193807](https://github.com/jarvisrchen/yogurt/actions/runs/33455193807)). Formula shas verified against re-downloaded tarballs (`cac88dcb...` arm64, `9a15bbc5...` x86_64). Smoke tested from a clean state: `brew install` serves 0.3.0, no `com.apple.quarantine`. Verified the actual feature rather than the version string - installed `yogurt-model-tiny-en` from the new `models-v1` mirror and `yogurt doctor` reported `tiny.en (homebrew)` resolving out of `/opt/homebrew/share/yogurt/models`. `main` moved twice mid-release (#25 landed between the CI check and the merge), so the version-bump PR needed a second merge attempt - re-verify `origin/main`'s sha immediately before tagging rather than reusing the one you checked CI against. |
| v0.4.0 | 2026-09-01 | Ships LLM-5 (#24): CLI providers get a timeout budgeted for generation rather than a handshake. All four jobs green ([33460106637](https://github.com/jarvisrchen/yogurt/actions/runs/33460106637)) after a clean dry run ([33459894840](https://github.com/jarvisrchen/yogurt/actions/runs/33459894840)). Formula shas verified against re-downloaded tarballs (`2215bec5...` arm64, `56d9d6d3...` x86_64). Smoke tested from a clean state: `brew install` serves 0.4.0, no `com.apple.quarantine`. First release under the step-4 doc-PR check, which immediately caught #27 (the LLM-5 TODO checkoff) still open - merged before tagging. Also published the four `yogurt-model-*` tap formulae ([tap #4](https://github.com/jarvisrchen/homebrew-yogurt/pull/4)), closing the gap where v0.3.0's README documented `brew install` commands that returned "No available formula" for everyone; `yogurt-model-tiny-en` now installs from the published tap and `yogurt doctor` reports `tiny.en (homebrew)`. Note on verifying a feature is in the artifact: LLM-5 changes a timeout constant and control flow rather than adding user-facing text, so there is no distinguishing string to grep for. `strings | comm` looks like it works and does not - `strings` emits long concatenated lines, so a line differing anywhere reads as new even when the substring exists in both binaries. Provenance is the reliable check: `git tag --contains <commit>` returned only v0.4.0, and `GENERATION_TIMEOUT = 300s` is present in the v0.4.0 tree and absent in v0.3.0. |
| v0.5.0 | 2026-09-01 | Ships the remainder of AUD-4 (#29): `ModelSpec` gains `mirror_url`, and `download()` tries the `models-v1` GitHub mirror before falling back to HuggingFace, so the in-app download button stops depending on HF reachability - the mirror and the tap formulae had already shipped in v0.4.0, but nothing on the download path used them yet. All four jobs green ([33462065181](https://github.com/jarvisrchen/yogurt/actions/runs/33462065181)) after a clean dry run ([33461878955](https://github.com/jarvisrchen/yogurt/actions/runs/33461878955)). Formula shas verified against re-downloaded tarballs (`031f12bc...` arm64, `7cdd3e04...` x86_64). Smoke tested against the existing install rather than a fully clean state - `brew untap`/`uninstall` refused because `yogurt-model-tiny-en` depends on the `yogurt` formula, a real local model install rather than a leftover to force past - `brew install jarvisrchen/yogurt/yogurt` upgraded it in place; `yogurt --version` reports 0.5.0, no `com.apple.quarantine`. Verified the actual feature rather than the version string: `strings` on the installed binary shows all four `releases/download/models-v1/ggml-*.bin` mirror URLs baked in. This release closes out AUD-4 in full; `docs/TODO.md` was already moved to DONE in #29. |
