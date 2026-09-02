# Homebrew tap seed

This directory seeds the `jarvisrchen/homebrew-yogurt` tap repo.
It is not consumed by Homebrew directly from here -- `brew` reads from the
sibling tap repo, not from this repo.
`yogurt.rb` is the placeholder formula that gets copied into the tap repo's
`Formula/` directory as a one-time bootstrap.

## One-time bootstrap (not yet done -- deferred until GitHub remote access is available)

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

## After bootstrap

Once the tap repo exists and `HOMEBREW_TAP_TOKEN` is set as a repo secret on
`jarvisrchen/yogurt` (fine-grained PAT scoped to `homebrew-yogurt` with
Contents *and* Pull requests set to read/write -- see step 4 of
`scripts/release-checklist.md`), the release workflow's `tap` job
(`.github/workflows/release.yml`) keeps `Formula/yogurt.rb` up to date
automatically: on every real (non-dry-run) tag push it rewrites the version
and per-arch SHA256s and opens a PR against the tap repo's `main`. No manual
edits to the tap repo should be needed after this point.
