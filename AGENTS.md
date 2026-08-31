# yogurt - agent guide

Local-first meeting copilot for macOS.
Captures mic + system audio without a meeting bot, transcribes live, and fuses sparse user notes with the transcript into "augmented notes".
Single Rust binary serving a React SPA at `localhost:7878`.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before making structural changes - it maps the system as built and records key decisions and rejected alternatives.

## Hard constraints (never violate)

- Audio never leaves the machine unless the user opted into cloud STT - and then only audio, never notes. Captured audio is deleted after transcription.
- API keys live only in `~/.yogurt/keys.json` (mode 0600, `FileKeyStore`) - never in SQLite, never in a response body, never logged.
- One process: no subprocesses, no IPC, no sidecars. The binary embeds `web/dist` (rust-embed) and bundles SQLite (rusqlite `bundled`).
- Zero telemetry of any kind.
- macOS 13+ only (ScreenCaptureKit).
- MIT licensed; keep dependencies MIT-compatible.

## Commands

```
just setup        # one-time: toolchains + pnpm install
just dev          # backend (cargo run) + frontend (vite) together
just build        # pnpm web build, then cargo build --release
just test         # cargo test --workspace --features yogurt-stt/local-stt + web vitest
just lint         # clippy -D warnings + fmt --check + web typecheck
```

The frontend must be built (`pnpm --dir web build`) before any Rust build that compiles `yogurt-server` - `rust-embed` requires `web/dist` to exist.
All app data lives under `~/.yogurt/` (db.sqlite, notes/, models/, session-token).

## Repo layout

- `docs/ARCHITECTURE.md` is the mechanism doc; `docs/.lavish/` holds its interactive HTML companions - create new Lavish review surfaces there, not at the repo root.
- `docs/DEBUGGING-TRANSCRIPTS.md` covers inspecting a live transcript: tailing `transcript_json`, reading raw WS frames, and the known UI-vs-DB divergences.
- `docs/MODEL-EVAL.md` covers A/B-ing STT engines and LLMs: `scripts/eval/` plays a fixed scripted conversation into a recording and grades two resulting summaries with headless Claude.
- `docs/RELEASING.md` is the release runbook: what the tagged-push pipeline does, the one-time prerequisites, and a log of each release.
- `.claude/skills/release/SKILL.md` is the same process as an executable checklist; invoke the `release` skill rather than improvising a release.
- `docs/TODO.md` is the backlog. Every item has a ticket ID (`UI-1`, `MTG-3`, `AUD-2`, ...); reference it in commits and PR titles, and follow the allocation rule at the top of that file when adding one.
- `docs/.planning/` is where active GSD planning for the next milestone goes.
- When a doc, plan, or Lavish surface is no longer relevant, move it into the mirrored `docs/archive/` tree (`archive/.lavish/`, `archive/.planning/v1/`, `archive/PRD.md`, ...) - archive, never delete.
- Everything under `docs/` is tracked in git, including `.lavish/`.

## Conventions

- Rust: rustfmt + clippy clean at `-D warnings`; `anyhow` at binary surface, `thiserror` at crate boundaries.
- Frontend: React 19 + Vite + Tailwind 4 (tokens in `web/src/index.css` `@theme`, PRD §16 Blueberry) + zustand + TanStack Query.
- Never use an em dash in prose; use a plain "-".
- `main` is protected by convention: branch, then open a PR. Never commit or push directly to `main`. The repo is public and `v0.1.0` ships from it, so an unreviewed commit on `main` is a published mistake rather than a local one.
- Do not hand-edit CHANGELOG files; release notes are generated.
- No agent attribution in git history or on GitHub: no "Generated with Claude Code" footer, no session link, no `Co-Authored-By` for an agent, in commit messages or PR bodies.
- Squash and merge PRs. GitHub appends `(#N)` to the squashed commit subject, which is the only thing that back-links a commit on `main` to its PR. Rebase-and-merge replays your original commits verbatim and leaves no PR reference, so `main` loses the trail (see `1656270`, merged from #6).
- Several sessions share this checkout, so the branch under you can change mid-task. Re-run `git branch --show-current` immediately before every commit, not just before `git checkout -b`, and stage explicit paths rather than `git add -A` so you cannot sweep up another session's work.
- Do not move the shared checkout onto a different branch while another session is working in it. If you need a branch of your own, add a worktree under `../yogurt-worktrees/` instead and leave the shared tree on `main`.
- Tests accompany non-trivial logic; E2E behavior is verified against the real binary, not just unit tests.
