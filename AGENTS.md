# yogurt - agent guide

yogurt is a local-first meeting copilot for macOS: it captures mic + system audio without a meeting bot, transcribes live, and fuses sparse user notes with the transcript into "augmented notes".
One Rust binary serves a React SPA at `localhost:7878`.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before making structural changes - it maps the system as built and its key decisions.

## Hard constraints (never violate)

- Audio never leaves the machine unless the user opted into cloud STT (then only audio, never notes); captured audio is deleted after transcription.
- API keys live only in `~/.yogurt/keys.json` (mode 0600, `FileKeyStore`) - never in SQLite, a response, or a log.
- One process: no subprocesses, no IPC, no sidecars - except the locked-down `yogurt-llm::CliClient`, an LLM provider only when the user opts in via Settings; see [docs/ARCHITECTURE.md §7.6](docs/ARCHITECTURE.md#76-the-cli-provider-exception-llm-4) for the lockdown and the no-fallback rule.
- Zero telemetry of any kind.
- macOS 13+ only (ScreenCaptureKit).
- MIT licensed; keep dependencies MIT-compatible.

## The task lifecycle

```
just start <ID> [words]                  # worktree + branch, bootstrapped
just dev-bg                              # server in a background tmux window
# edit; verify with `yogurt ctl ...` and `just test`
just ticket done <ID> --note-file <f>    # check off the ticket
just pr "<ID>: <title>" --body-file <f>  # validate, push, open the PR
just land                                # wait for CI, squash-merge, clean up
```

Releases: `scripts/release.sh preflight <version>`, then the `release` skill's checklist through tag and `scripts/release.sh finish <version>`.
Every command documents its own flags via `--help`/usage and the justfile.

## Repo layout

- `docs/FEATURES.md` maps every route to its feature.
- `docs/DEBUGGING-TRANSCRIPTS.md` covers inspecting a live transcript; `docs/MODEL-EVAL.md` covers A/B-ing STT engines and LLMs with `scripts/eval/`.
- `docs/RELEASING.md` is the release runbook; `docs/RELEASE-LOG.md` the row-per-release log.
- `.claude/skills/release/SKILL.md`, `.claude/skills/yogurt-control/SKILL.md`: those procedures as checklists.
- `docs/TODO.md` is the open backlog, `docs/TODO-DONE.md` the closed one; both count toward ID allocation, never archive `TODO-DONE.md`.
- `docs/.planning/` is active GSD planning.
- A stale doc, plan, or Lavish surface archives into the mirrored `docs/archive/` tree, never deleted; `docs/.lavish/` holds ARCHITECTURE.md's HTML companions, tracked like everything under `docs/`.

## Conventions

- Rust: rustfmt + clippy at `-D warnings`; `anyhow` at the binary surface, `thiserror` at crate boundaries.
- Frontend: React 19 + Vite + Tailwind 4 (`web/src/index.css` `@theme`) + zustand + TanStack Query.
- No em dash in prose, a plain "-" instead; one full sentence per line in Markdown.
- Ticket ID in commit subjects and PR titles; no agent attribution in either - `.githooks/commit-msg` and `just pr` enforce it.
- Squash is the only merge method; GitHub's appended `(#N)` is the only back-link from a commit on `main` to its PR.
- Work in a worktree via `just start`, never the shared main checkout - a build there can splice a binary from two branches with no warning (CONTRIBUTING.md has why).
- Tests accompany non-trivial logic; E2E behavior is verified against the real binary (`just test`), not just unit tests.
- A task ends with an absolute-path Manual test section in the PR body (`just pr` enforces it); `just land` cleans up the worktree and branch once CI is green.

A ticket whose diff stays under `web/` or `docs/` may run as a cloud session: a fresh sandbox using the five lines of `ci.yml`'s web job as the environment, Playwright screenshots attached for the pixel-perfect rule since the suite mocks the backend.
Docs-only PRs skip CI.
Rust stays local; the free `macos-26` runner is the cloud verifier (`gh run watch` after push, ~2 minutes warm).
No self-hosted Mac runner, no Linux port, no simulated meeting window.
