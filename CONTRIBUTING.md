# Contributing to yogurt

## Dev environment setup

One-time:

```bash
brew install rust node@22 cmake
git clone https://github.com/jarvisrchen/yogurt.git   # private repo: `gh auth login` first, or use the SSH URL
cd yogurt
./scripts/setup.sh
```

Homebrew is the only prerequisite.
`setup.sh` installs or bootstraps Rust, Node 22, CMake, Corepack, and the pinned pnpm version.
It also validates the exact tool versions before building.
`just dev-bg` also needs tmux, which `setup.sh` does not install: `brew install tmux`.

`setup.sh` is idempotent: it installs the [`just`](https://github.com/casey/just)
task runner via Homebrew, writes a `.env.local` stub for API keys, builds the
web bundle, and does a release build. Pass `--no-just` to skip the `just`
install and use `./scripts/*.sh` directly. Run `just` with no arguments any
time to list every recipe.

## How to run yogurt

There are three ways to run the app.
All of them share the same `~/.yogurt/db.sqlite`, so switching between them never loses data.

| Mode | Command | Binary | Web assets served from | Reads `.env.local` | Rebuilds first |
|------|---------|--------|------------------------|--------------------|----------------|
| Product | `yogurt start` (or `target/release/yogurt start`) | whatever you point at | the bundle embedded at build time | no | never |
| Release loop | `just release` | `target/release/yogurt` | the bundle embedded at build time | no | yes: `pnpm build` + `cargo build --release`, both incremental |
| Dev loop | `just dev` (or `just backend` + `just frontend`) | `target/debug/yogurt --dev` | Vite on `:5173`, hot reload | yes | yes: `cargo build` (debug) |

Which one to pick:

- **Using yogurt** (record a meeting, browse the library): `just release`. Single process, embedded web bundle, `http://localhost:7878`.
- **Editing only Rust**: `just release`. Incremental web + cargo `--release` build, then runs.
- **Editing the React UI**: `just dev`. Vite + Rust backend together in one terminal, Ctrl-C stops both.
- **Editing React and you want separate logs**: `just frontend` in terminal A, `just backend` in terminal B.

Always open the browser at `http://localhost:7878`, never `:5173` directly - the session lives on the backend.

The trap: running a binary directly after editing React shows the old UI with no warning, because the bundle lives inside the binary.
Use `just release` (rebuilds) or `just dev` (proxies to Vite) instead of invoking `target/*/yogurt` by hand.

### `yogurt`, `just`, and the scripts

`yogurt` is the product: one binary with two subcommands, `yogurt start` and `yogurt doctor` (flags in the [README](README.md#command-line)).
That is all a Homebrew user ever runs.

`just` is the contributor task runner.
Every recipe is a thin wrapper over a script in `scripts/` that builds something and/or execs `yogurt start` with the right flags, plus conveniences the bare binary does not have: an incremental rebuild before launch and a prompt when the port is busy.
There is nothing you can do with `just` that you cannot do by hand; `just --list` shows every recipe.

| `just` recipe | Runs |
|---------------|------|
| `just backend [args]` | `./scripts/run-backend.sh [args]` |
| `just frontend` | `./scripts/run-frontend.sh` |
| `just release [args]` | `./scripts/run-release.sh [args]` |
| `just dev` | `just bootstrap`, resolve a free (Vite, backend) port pair, then `run-frontend.sh` in the background, wait until Vite answers (so the backend proxy does not 502 on first request), then `run-backend.sh` in the foreground; Ctrl-C kills both |
| `just bootstrap` | Restores the gitignored files a checkout needs in order to run: `.env.local` (copied from the main checkout), `web/node_modules`, `web/dist`. No-ops once they are present |

`just dev` and `just bootstrap` are the only recipes with logic of their own: the readiness wait and paired shutdown in one, the what-is-missing checks in the other.
Call the scripts directly when you want one process with its own log stream; use `just dev` for the everyday loop.

## Running tests

```bash
just test         # test-rust + test-web
just test-rust     # cargo test --workspace --features yogurt-stt/local-stt --no-fail-fast
just test-web      # pnpm --dir web test + Playwright e2e smoke
just test-hw       # hardware smoke tests - real SCK + mic pipeline, never in `just test` or CI
just lint          # cargo fmt --check + clippy -D warnings + check-docs
just lint-web      # pnpm --dir web typecheck
just fmt           # auto-format Rust
```

Tests fall into three tiers.
Unit and mocked tests (`just test-rust`, `just test-web`) cover pure logic and browser-mocked UI, need no hardware, and run on every PR.
Real-binary smoke tests spawn the actual `yogurt` binary against a temp `HOME` and drive it over `yogurt ctl` (`crates/yogurt-cli/tests/ctl_smoke.rs`); the hardware-free ones run inside `just test-rust` in CI, since they exercise the real binary without touching audio or the screen.
Hardware tests open a real screen-capture and microphone pipeline and need Screen Recording (and Microphone) permission grants; every one is `#[ignore]`d and re-checks `YOGURT_HW_TESTS=1` itself, so they only run locally via `just test-hw`, never under `just test` or CI.

CI runs `just lint` and `just test-rust` in the rust job, and `just lint-web`,
`just test-web` and `just build-web` in the web job, on every PR.
Run the whole set before pushing.

## Workspace layout

Eight crates under `crates/`:

| Crate | Purpose |
|---|---|
| `yogurt-cli` | Binary entrypoint (`yogurt start`, `yogurt doctor`) |
| `yogurt-server` | axum HTTP/WS server, routes, embedded web assets |
| `yogurt-audio` | Mic + system-audio capture (ScreenCaptureKit), TCC permission checks |
| `yogurt-stt` | Pluggable `Stt` trait; Deepgram cloud adapter + whisper.cpp local adapter |
| `yogurt-llm` | OpenAI-compatible LLM client, provider presets |
| `yogurt-db` | SQLite (providers, settings, meetings) + `~/.yogurt/keys.json` key store |
| `yogurt-notes` | Augmented-notes diffing/merge logic |
| `yogurt-prompts` | Bundled enhance/chat prompt templates |

`web/` is the React + Vite + TipTap frontend, embedded into the binary via
`rust-embed` at release-build time.

**Build order matters.** `rust-embed`'s `#[folder = "../../web/dist/"]`
derive in `crates/yogurt-server/src/assets.rs` requires `web/dist/` to
exist at compile time, and `web/dist/` is gitignored.
Build the web bundle (`just build-web`, or `pnpm --dir web build` directly)
before any command that compiles `yogurt-server` -- `cargo build`,
`just build`, `cargo test`, `cargo clippy`.
`just setup` and `just release` handle this ordering for you (`just dev`
does not need the bundle because it proxies to Vite); a bare
`cargo build --release` on a fresh clone will fail or embed a stale bundle
if you skip the web build first. CI builds the web bundle before rustfmt/
clippy/test for the same reason.

## Code style

- `cargo fmt --all` before every commit; CI enforces `--check`.
- `cargo clippy --workspace --features yogurt-stt/local-stt --all-targets -- -D warnings` must be clean.
- Conventional Commits (`feat:`, `fix:`, `docs:`, `refactor:`, ...) for commit messages.
- No em dash (the long dash some editors auto-insert for "--") anywhere --
  use a plain hyphen or double hyphen instead, as this file does throughout.
- In Markdown files, put each full sentence on its own line (semantic
  linefeeds); wrap at the sentence, not at a fixed column.

## Adding a new STT or LLM provider

- **STT engine:** implement the `Stt` trait in `crates/yogurt-stt/src/` (see `deepgram.rs` or `whisper_local.rs` for the shape).
- **LLM provider:** add a preset card to `yogurt_db::providers::PRESETS` in `crates/yogurt-db/src/providers.rs` -- any OpenAI-compatible `base_url` works without new code.

## Dev-only environment variables

All of these are read only when the backend runs with `--dev` (which loads `.env.local`) or by the test suite and run scripts.
Release builds never read `.env.local`.

| Variable | Read by | Effect |
|----------|---------|--------|
| `YOGURT_DEEPGRAM_API_KEY` | bootstrap | Seeds the Deepgram key into `~/.yogurt/keys.json` on first run. |
| `YOGURT_OPENAI_API_KEY`, `YOGURT_OPENROUTER_API_KEY`, `YOGURT_MINIMAX_API_KEY`, `YOGURT_GEMINI_API_KEY`, `YOGURT_DEEPSEEK_API_KEY` | bootstrap | Seed the matching LLM provider preset. One per entry in `ENV_PRESETS` (`crates/yogurt-server/src/bootstrap.rs`). |
| `YOGURT_LLM_BASE_URL`, `YOGURT_LLM_API_KEY`, `YOGURT_LLM_MODEL` | LLM resolver | Override the active LLM provider for this process without touching Settings or the key file. |
| `YOGURT_DEEPGRAM_MODEL` | cloud STT | Deepgram model name (default `nova-3`). Stamped into each meeting's `stt_engine`. |
| `YOGURT_VITE_BASE` | `--dev` proxy | Vite origin to proxy to (default `http://127.0.0.1:5173`). |
| `YOGURT_MEMORY_KEYSTORE=1` | tests | In-memory key store so tests never touch the real `~/.yogurt/keys.json`. Set automatically by `just test`. |
| `YOGURT_PORT_POLICY` | `scripts/run-*.sh` | What to do when the port is busy: `ask` (default), `kill`, `next`, or `fail`. |

The `yogurt start` and `yogurt doctor` flags themselves are documented in the [README](README.md#command-line).

## Troubleshooting the dev loop

**Port already in use.** `just release` and `just dev` prompt before
starting if the target port is busy (`[k] kill it`, `[n] use next port`,
`[a] abort`). Skip the prompt with `YOGURT_PORT_POLICY=kill` or
`YOGURT_PORT_POLICY=next`, or pick a port up front with `just release --port 7900`.

**Resetting to a fresh-install state:**

```bash
just reset-db     # wipes ~/.yogurt/db.sqlite; next launch shows /welcome (~/.yogurt/keys.json stays)
just release
```

## Branching

Work on a branch and open a PR. Do not push to `main`.

The repo is public and releases are cut from `main`, so an unreviewed commit there is something strangers can build.
CI runs on every PR (fmt, clippy at `-D warnings`, Rust tests, web typecheck, web tests, the Playwright E2E smoke, and `scripts/check-docs.sh`); let it gate the merge.

Tracked git hooks back this up locally: `.githooks/pre-commit` refuses a commit on `main`, and `.githooks/commit-msg` refuses agent attribution and em dashes.
`just bootstrap` and `./scripts/setup.sh` both point git at them (`git config core.hooksPath .githooks`), which covers every worktree; bypass a hook with `git commit --no-verify` when a rule misfires.

`just pr "<title>" --body-file <f>` validates the title, body, and (for a code change) an absolute-path handover line, then pushes and opens the PR; `just land [pr]` waits for CI, squash-merges, and removes the worktree and branch.
See `docs/.planning/agent-workflow.md` section 4B for the full design.

```bash
git checkout -b feat/my-change
# ... commit ...
just pr "feat: my-change" --body-file body.md
# once CI is green and it's ready to merge:
just land
```

### Working in a git worktree

`just start <ID> [words]` does the worktree, branch and bootstrap below in one step and prints the handover command; use the manual steps only when you want the mechanics or have no ticket ID.

A worktree gives a branch its own directory, so you can run two branches side by side without stashing:

```bash
git worktree add ../yogurt-worktrees/my-change -b feat/my-change origin/main
cd ../yogurt-worktrees/my-change
just dev
```

A fresh worktree contains only tracked files, and everything needed to *run* is gitignored: `.env.local`, `web/node_modules`, `web/dist` (whose absence fails the build at `#[derive(RustEmbed)] folder ... does not exist`).
`just dev` calls `just bootstrap` first, which restores all three from the main checkout, so there is no separate setup step.
`just dev` prefers `:5173` and `:7878` but moves to the next free pair when they are taken, so a second worktree runs alongside the first - it prints the ports it picked, and the two instances are independent (`/api`, `/ws`, HMR, and the WS origin allowlist all follow the pair).
They do share `~/.yogurt/`, so both see the same meetings and the same keys; only one of them should be recording at a time.
Pin the pair yourself with `YOGURT_VITE_PORT` and `YOGURT_BACKEND_PORT`, or force the old behaviour with `YOGURT_PORT_POLICY=ask`.
If the branch carries a database migration, set `YOGURT_DATA_DIR=~/.yogurt-<slug>` before `just dev` so this worktree gets its own `db.sqlite` instead of upgrading the shared one out from under the main binary.
Keys and models are still read from `~/.yogurt` either way.

Delete the worktree once its PR merges: `git worktree remove ../yogurt-worktrees/my-change`.

## Releasing

Pushing a tag matching `v*` is the only thing that publishes a release.
Merging a PR or pushing to `main` runs CI and ships nothing, so `main` can move freely between releases.

```bash
./scripts/release.sh preflight 0.1.0   # read-only judgment gate
./scripts/release.sh ship 0.1.0        # dry run, bump, tag, watch, verify, finish
```

`ship` bumps `[workspace.package]` in `Cargo.toml` through its own PR, dry-runs the pipeline, tags the
merge commit, and waits for both macOS arches to build - only arm64 compiles whisper's Metal backend, so
a green x86_64 leg alone proves nothing. The tag push opens a PR against
[jarvisrchen/homebrew-yogurt](https://github.com/jarvisrchen/homebrew-yogurt) with the real version and
checksums; `ship` merges it and smoke-tests the local `brew install`. Every step is skip-if-done, so
re-running it after a timeout resumes.

[docs/RELEASING.md](docs/RELEASING.md) is the full runbook: prerequisites, the decisions behind shipping
an unsigned prebuilt binary, failure recovery, and a log of each release.
Agents in this repo run the same procedure as a checklist with the `release` skill
(`.claude/skills/release/SKILL.md`).

## Filing issues

Run `yogurt doctor --json` and paste the output -- it never includes API key
values or note content, only provider names and presence/absence flags.

Found a security vulnerability instead of a regular bug?
See [SECURITY.md](SECURITY.md) -- report it privately, not as a public issue.

## License

By contributing, you agree your contributions are licensed under the project's
[MIT license](LICENSE).
