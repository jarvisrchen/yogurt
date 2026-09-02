# Releasing yogurt

How a tagged commit becomes `brew install jarvisrchen/yogurt/yogurt` on someone else's Mac.

The pipeline is [`.github/workflows/release.yml`](../.github/workflows/release.yml).
This doc is the operational companion: what the pipeline does, what has to be true before you tag, and the decisions and recovery paths behind it.
The actual checklist to run lives in the [`release` skill](../.claude/skills/release/SKILL.md): `scripts/release.sh preflight <version>` - a read-only check of tag, version, CI status, open doc PRs, and README wording against `origin/main` - then `scripts/release.sh ship <version>`, which runs the rest end to end and resumes on re-run.
A row-per-release record of what happened lives in [`docs/RELEASE-LOG.md`](RELEASE-LOG.md).

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
- [x] **Repo merge settings, applied 2026-09-01:** `gh api -X PATCH repos/jarvisrchen/yogurt -F allow_rebase_merge=false -F allow_merge_commit=false -F delete_branch_on_merge=true`.
  Squash is the only merge button available on a PR.
  Branches are deleted on merge, so never pass `--delete-branch` to `gh pr merge`.
  No branch protection ruleset on `main`, by decision, since there is a sole collaborator.

## When it goes wrong

**Build failed after the tag was pushed.** Delete the tag locally and remotely (`./scripts/release.sh untag <version>`, or by hand: `git tag -d v0.1.0 && git push --delete origin v0.1.0`), fix, re-tag. Safe as long as no Release was published and nobody installed. `untag` refuses (exit 2) once a GitHub Release exists - past that point, fix `Formula/yogurt.rb` by hand in the tap repo instead, per the next paragraph.

**Release published but the formula is wrong.** Fix `Formula/yogurt.rb` directly in the tap repo; it is a plain file and takes effect on the next `brew update`. No need to re-cut the release.

**`brew install` reports a sha256 mismatch.** The tarball was rebuilt after the formula was written. Recompute with `shasum -a 256` against the actual release asset and commit the correction to the tap.

**arm64 fails to link with `___isPlatformVersionAtLeast` undefined.**
whisper.cpp's Metal backend guards newer Metal APIs with `@available`, and because the deployment target is macOS 13 while the guards probe macOS 14+, clang cannot fold them and lowers each into a call to `__isPlatformVersionAtLeast`.
That symbol lives in `libclang_rt.osx.a`, which rustc never puts on the link line because it links with `-nodefaultlibs`.
`crates/yogurt-stt/build.rs` exists solely to fix this.
x86_64 is immune because ggml only builds `ggml-metal` for arm64, so a green x86_64 leg tells you nothing about arm64.
If this resurfaces, check that the build script still resolves a runtime dir - it degrades to a `cargo:warning` rather than failing loudly.

**`strings | comm` looks like a way to check a feature landed in the built binary, and it is not.**
`strings` emits long concatenated lines, so a line differing anywhere reads as new even when the substring you actually care about is present in both binaries.
Use provenance instead: `git tag --contains <commit>` to confirm the commit is only in the tag you just cut, plus a direct `strings <binary> | grep <marker>` for a distinguishing string or symbol.

**`brew untap` and `brew uninstall` refuse once a `yogurt-model-*` formula is installed, because those formulae depend on `yogurt`.**
This is not a leftover to force past; a real local model install blocks it by design.
`brew upgrade jarvisrchen/yogurt/yogurt` (or `brew reinstall jarvisrchen/yogurt/yogurt` when already at that version) is the normal smoke-test path.
A from-scratch `untap`/`uninstall`/install cycle only works on a machine with no model formula installed.

**Re-read `origin/main`'s sha immediately before tagging, not the sha you last checked CI against.**
`main` moves: the v0.3.0 release needed a second merge attempt because another PR landed between the CI check and the merge, so the sha the tag pointed at was stale by the time it was pushed.

**Use `git log <lasttag>..origin/main` to establish what a release actually ships, rather than assuming the previous release already contains everything merged before it.**
v0.7.0 shipped three PRs that had merged after the v0.6.0 tag was cut, which only `git log` against the last tag would have surfaced up front.
