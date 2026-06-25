# Phase 9: Distribution Polish - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

GitHub Actions release workflow on `v*` tag push produces notarized per-arch tarballs (universal binary optional), opens a Homebrew tap PR with updated SHA256s, runs `cargo publish`, and ships a `yogurt doctor` subcommand for TCC reset / model re-download / port diagnostics. The end-to-end install path `brew install yogurt && yogurt start` works for a non-technical user on a fresh Mac.

This is the final phase (depends on Phase 8). Phases 0-8 built all product features; Phase 9 packages, signs, and ships them.

</domain>

<decisions>
## Implementation Decisions

### Release Pipeline

- **D-01:** GitHub Actions release workflow triggers on push of any tag matching `v*` (plus `workflow_dispatch` with a `dry-run: true` input for testing).
- **D-02:** Matrix build for `aarch64-apple-darwin` + `x86_64-apple-darwin` produces per-arch tarballs. **Universal binary via `lipo` is OPTIONAL in v1** — per-arch tarballs are the v1 ship; the Homebrew formula branches on `Hardware::CPU.arm?`.
- **D-03:** Strict job ordering enforced via `needs:` — `build → release → tap → publish`. Never push a Homebrew formula PR or run `cargo publish` before the GitHub Release artifacts are live (avoids 404-cached `brew install` failures).
- **D-04:** Hand-rolled workflow (not `cargo-dist`). Smaller surface, strict ordering control, no 0.x tool dependency on the release path. Decision recorded in `CHANGELOG.md` v0.1.0 "Release engineering" subsection.

### Signing & Notarization

- **D-05:** Each release notarized via `notarytool` + `staple` using a stable Apple Developer ID.
- **D-06:** Bundle ID pinned to `ai.yogurt.app` (required for stable TCC permission grants — `tccutil reset ScreenCapture ai.yogurt.app` from `yogurt doctor` only works against a fixed bundle ID).
- **D-07:** First-launch on macOS must pass `spctl -a -vv` ("accepted") — no Gatekeeper "damaged" error, no right-click → Open workaround required.

### Distribution Channels

- **D-08:** Three install channels, all served from the same tagged release: `brew install jarvisrchen/yogurt/yogurt`, `cargo install yogurt`, direct tarball download from the GitHub Release.
- **D-09:** Homebrew tap lives in sibling repo `jarvisrchen/homebrew-yogurt` (bootstrap repo + placeholder formula must exist before first release). Release workflow auto-bumps `Formula/yogurt.rb` version + per-arch SHA256s and opens a PR — never commits directly to the tap `main`.
- **D-10:** `cargo publish -p yogurt` runs LAST (after release + tap PR), is idempotent (skips if the version is already on crates.io).

### `yogurt doctor` Subcommand

- **D-11:** `yogurt doctor` lives under existing `yogurt-cli` crate from Phase 0; adds `commands/doctor.rs` module. Subcommand reports: Rust version, macOS version, Screen Recording permission status, db path, configured provider names (NEVER keys), active STT, local whisper.cpp models on disk.
- **D-12:** `--json` flag emits machine-readable diagnostic for bug reports. Output never includes API key values or note content — presence/absence flags only.
- **D-13:** `yogurt doctor` includes a TCC reset path (`tccutil reset ScreenCapture ai.yogurt.app`), model re-download capability for whisper.cpp models in `~/.yogurt/models`, and port-conflict diagnostics (whether `7878` is in use).
- **D-14:** `--version` is wired by clap from `Cargo.toml` version field (no code change needed — verify only).

### v1 Ship Gate

- **D-15:** **DIST-09 is the phase gate, not a checkbox.** Acceptance = a non-technical user on a fresh Mac running `brew install yogurt && yogurt start` successfully records a first meeting end-to-end (Screen Recording grant, paste an LLM key, record + transcribe + augmented notes appear). This is a `checkpoint:human-verify` task — not auto-verifiable.
- **D-16:** Apple Developer ID enrollment is a `user_setup` step — Claude cannot do this. Same for creating the `jarvisrchen/homebrew-yogurt` repo (gh cli can technically run it, but it's a one-time bootstrap that needs human ownership) and adding `CRATES_IO_TOKEN` + `HOMEBREW_TAP_TOKEN` repo secrets.

### Claude's Discretion

- Exact YAML structure of the workflow files (within the strict-ordering constraint).
- `yogurt doctor` human-readable output formatting.
- README structure and screenshot selection.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Source-of-truth implementation plan
- `docs/superpowers/plans/2026-06-25-yogurt-phase-9-polish-and-distribution.md` — Full task-by-task implementation plan (13 tasks). PLAN.md files in this phase derive from this document.

### Product requirements
- `docs/PRD.md` §11 — Distribution & dev workflow (install channels, dev loop, contributor onboarding)
- `docs/PRD.md` §14 — Success criteria (the four end-to-end gates Phase 9 must satisfy)
- `docs/PRD.md` §15 — Anti-telemetry stance, MIT license, repo location

### Requirements
- `.planning/REQUIREMENTS.md` "Distribution" section — DIST-01 through DIST-10
- `.planning/ROADMAP.md` "### Phase 9: Distribution Polish" — Goal + Success Criteria

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `yogurt-cli` crate (Phase 0): existing binary entrypoint — Phase 9 extends it with the `doctor` subcommand under `commands/doctor.rs`. `--version` is already wired by clap from `Cargo.toml`.
- `yogurt-server::settings` (Phase 5): extended with `update_check_enabled: bool` default `false` for the optional self-update toggle.
- `yogurt-db` settings load (Phase 5): existing pattern reused for the update-check toggle persistence.
- `~/.yogurt/notes/*.md` markdown export (Phase 7): the safety net for the crash-recovery path — notes always live on disk independent of the DB.

### Established Patterns
- Workspace structure: `crates/yogurt-{cli,server,audio,stt,llm,db,notes,prompts}` already established in Phase 0 — Phase 9 adds files within these crates, no restructuring.
- Settings persistence: `~/.yogurt/config.toml` + Keychain for secrets (Phase 5 convention) — `yogurt doctor` reads the TOML directly to list provider *names* only.
- Bundle ID `ai.yogurt.app`: must thread through Cargo.toml metadata, notarization config, and the TCC reset command in `yogurt doctor`.

### Integration Points
- `.github/workflows/release.yml` (new) — orchestrates the build → notarize → release → tap → publish pipeline.
- `crates/yogurt-cli/src/commands/doctor.rs` (new) — adds a third subcommand alongside `start` and any others from Phase 0.
- Sibling repo `jarvisrchen/homebrew-yogurt` (external) — formula auto-bumped by the release workflow on every tag.

</code_context>

<specifics>
## Specific Ideas

- **DIST-09 acceptance is a non-technical user on a fresh Mac installing via `brew install yogurt && yogurt start` and successfully recording a first meeting end-to-end.** This is the phase gate, not a checkbox. The human-verify task in plan 09-03 must be performed on an actual fresh Mac (or at minimum a clean shell with no cached binaries) following the smoke-test checklist in the superpowers plan Task 9.13 Step 5.
- **Publish hygiene rule is load-bearing:** GitHub Release artifacts must be live BEFORE the formula PR opens and BEFORE `cargo publish` runs. If reversed, `brew install` 404s get cached by Homebrew's mirror infrastructure and the bad state outlives the fix. The workflow `needs:` ordering enforces this — keep it that way.
- README install snippet must show all three channels (Homebrew, cargo, direct download) and clearly indicate the recommended path for non-developers.

</specifics>

<deferred>
## Deferred Ideas

- **Universal binary via `lipo`** — optional in v1; per-arch tarballs are the v1 ship. Can be added if `cargo-dist` migration happens later or if a single `brew install` artifact becomes desirable. Per-arch is simpler, smaller per-download, and aligns with the Homebrew formula's `Hardware::CPU.arm?` branch.
- **Auto-update install mechanism** → v2. The optional self-update check (OFF by default) only *reports* a new version; users `brew upgrade yogurt` or `cargo install yogurt --force` themselves.
- **Linux / Windows builds** — macOS only per PRD §5.8.
- **Crash reporting service (Sentry, etc.)** — anti-goal per PRD §15. Crash markers stay local at `~/.yogurt/last_crash.json`.
- **Telemetry of any kind** — anti-goal per PRD §15.
- **Logic-bug fixes from prior phases** — if Phase 9 testing reveals a real bug from an earlier phase, file an issue and fix under that phase's milestone.
- **Code signing without notarization** — either we have a stable Developer ID and ship a notarized binary, or we don't ship v1. Notarization deferral was the v1.1 plan in the superpowers doc; this CONTEXT pulls it INTO v1 per ROADMAP §Phase 9 Success Criterion 1.

</deferred>

---

*Phase: 09-distribution-polish*
*Context gathered: 2026-06-25*
