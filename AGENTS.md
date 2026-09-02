# yogurt - agent guide

Local-first meeting copilot for macOS.
Captures mic + system audio without a meeting bot, transcribes live, and fuses sparse user notes with the transcript into "augmented notes".
Single Rust binary serving a React SPA at `localhost:7878`.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before making structural changes - it maps the system as built and records key decisions and rejected alternatives.

## Hard constraints (never violate)

- Audio never leaves the machine unless the user opted into cloud STT - and then only audio, never notes. Captured audio is deleted after transcription.
- API keys live only in `~/.yogurt/keys.json` (mode 0600, `FileKeyStore`) - never in SQLite, never in a response body, never logged.
- One process: no subprocesses, no IPC, no sidecars. The binary embeds `web/dist` (rust-embed) and bundles SQLite (rusqlite `bundled`). Single scoped exception (LLM-4, LLM-7): `yogurt-llm::CliClient` spawns a locally-installed agent CLI (`claude -p`, `cursor-agent -p`, `opencode run`) as an LLM provider, but only when the user has explicitly picked it in Settings - the "Claude Code (local CLI)" / "Cursor Agent (local CLI)" / "OpenCode (local CLI)" provider presets. Locked down (`--restricted --strict-mcp-config --disable-slash-commands` for `claude`, `--sandbox enabled` for `cursor-agent`, `--pure` plus `OPENCODE_PERMISSION={"*":"deny"}` for `opencode`, an isolated scratch cwd for all three) since untrusted meeting-transcript text reaches the CLI as plain prompt text. There is no automatic fallback from an unreachable HTTP provider to a CLI - an earlier draft did that and was reverted, since silently rerouting a meeting's real content to a different backend on a network hiccup is a behavior change a user should opt into, not one that happens to them. Do not generalize this into a pattern for other subprocesses without revisiting this constraint again.
- Zero telemetry of any kind.
- macOS 13+ only (ScreenCaptureKit).
- MIT licensed; keep dependencies MIT-compatible.

## Commands

```
just setup        # one-time: toolchains + pnpm install
just dev          # backend (cargo run) + frontend (vite) together
just build        # cargo build --release (build web first, see below)
just test         # cargo test --workspace --features yogurt-stt/local-stt + web vitest
just lint         # fmt --check + clippy -D warnings
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
  Closed tickets live in `docs/TODO-DONE.md` instead; ID allocation counts it too, so do not move it to `docs/archive/`.
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
- Work in a worktree under `../yogurt-worktrees/`, always. `git worktree add ../yogurt-worktrees/<slug> -b <branch> origin/main` is the first command of a task, not the fallback for when you need a branch. Several sessions share the main checkout, so it is the one place where your mistakes land on someone else. A fresh worktree carries nothing that is gitignored, which is exactly the set of files needed to run: no `node_modules`, no `web/dist` (so `cargo build` fails at `#[derive(RustEmbed)] folder ... does not exist`), and no `.env.local` (so `just dev` starts Vite, then aborts with `.env.local not found`). `just bootstrap` restores all three from the main checkout, and `just dev` depends on it, so a new worktree needs no setup step of its own.

- Treat the shared checkout as read-only unless you own it. Do not change its branch, do not `reset --hard` it, and do not run a release build in it. Running something out of it counts as owning it: a session with a server or binary running from that tree has a claim on it that `git status` will not show you, so ask before you build there.
- A build in progress is a write in progress, and it is invisible. `git status` is clean, there is no lock file, and nothing tells you a `cargo build` is running in that tree. Change the branch under one and cargo finishes the run against the new source: some crates compile from the old tree, the rest from the new, and you get a binary spliced from two source trees with exit code 0 and no warning.
- The failure this prevents: `target/release/yogurt` is a single file with no record of which branch produced it. Build in the shared tree while another session is verifying a fix, and their next run silently exercises your code instead of theirs. It reads as a regression in their feature, and nothing in the output says otherwise. A spliced binary is worse than a stale one, because it can pass the test it was built to verify while quietly failing a guarantee the same change was supposed to keep.
- If you do end up working in the shared checkout anyway, re-run `git branch --show-current` immediately before every commit - not just before `git checkout -b`, since the branch can move under you mid-task - and stage explicit paths rather than `git add -A` so you cannot sweep up another session's work.
- Tests accompany non-trivial logic; E2E behavior is verified against the real binary, not just unit tests.
- A task is not done at "PR open, CI green". End it by handing over the manual test: the one copy-pasteable command that runs *that worktree*, followed by the two or three clicks that exercise the change and what should happen. Automated tests say the fix works; this is what lets Richard see it work. Use the absolute path (`cd ~/Documents/code/yogurt-worktrees/<slug> && just dev`), not a path relative to wherever the agent happened to be (`../yogurt-worktrees/<slug>`) - a relative path silently resolves to the wrong place, or nowhere, once pasted from a terminal that isn't sitting in the original working directory.
- `just dev` bootstraps the worktree itself (see above) and moves off :5173 / :7878 when they are busy, so a second worktree can run alongside the first without stopping it - it prints the pair it picked, and the handover command should name the port it will land on only if you know the default is taken.
- Once the PR merges, remove the worktree (`git worktree remove ../yogurt-worktrees/<slug>`) and delete its merged branch as the final step of the task - this is automatic on merge, not gated on Richard having run the manual test first. If he wants to inspect or run the shipped code afterward, it's already on `main`; there is no need to keep the worktree around for that.
