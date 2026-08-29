# Contributing to yogurt

## Dev environment setup

One-time:

```bash
brew install rust node pnpm cmake
git clone https://github.com/jarvisrchen/yogurt.git   # private repo: `gh auth login` first, or use the SSH URL
cd yogurt
./scripts/setup.sh
```

All four brew formulae are required and none pull the others in: `cmake`
builds whisper.cpp for local STT, and `pnpm` ships as a standalone binary
that does *not* install Node. `setup.sh` re-checks every one of them and
stops with the exact install command if something is missing.

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
Every recipe is a thin wrapper over a script in `scripts/` that builds something and/or execs `yogurt start` with the right flags, plus conveniences the bare binary does not have: an incremental rebuild before launch, a prompt when the port is busy, and optional dev codesigning so Keychain grants survive rebuilds.
There is nothing you can do with `just` that you cannot do by hand; `just --list` shows every recipe.

| `just` recipe | Runs |
|---------------|------|
| `just backend [args]` | `./scripts/run-backend.sh [args]` |
| `just frontend` | `./scripts/run-frontend.sh` |
| `just release [args]` | `./scripts/run-release.sh [args]` |
| `just dev` | `run-frontend.sh` in the background, wait until Vite answers on `:5173` (so the backend proxy does not 502 on first request), then `run-backend.sh` in the foreground; Ctrl-C kills both |

`just dev` is the only recipe with logic of its own: the readiness wait and the paired shutdown.
Call the scripts directly when you want one process with its own log stream; use `just dev` for the everyday loop.

## Running tests

```bash
just test         # cargo test --workspace --features yogurt-stt/local-stt + pnpm --dir web test
just test-rust     # cargo only
just test-web      # pnpm only
just test-e2e      # Playwright smoke against a browser-mocked backend
just lint          # cargo fmt --check + clippy -D warnings
just fmt           # auto-format Rust
```

CI runs `just test` and `just lint` equivalents on every PR. Run both before
pushing.

## Workspace layout

Eight crates under `crates/`:

| Crate | Purpose |
|---|---|
| `yogurt-cli` | Binary entrypoint (`yogurt start`, `yogurt doctor`) |
| `yogurt-server` | axum HTTP/WS server, routes, embedded web assets |
| `yogurt-audio` | Mic + system-audio capture (ScreenCaptureKit), TCC permission checks |
| `yogurt-stt` | Pluggable `Stt` trait; Deepgram cloud adapter + whisper.cpp local adapter |
| `yogurt-llm` | OpenAI-compatible LLM client, provider presets |
| `yogurt-db` | SQLite (providers, settings, meetings) + macOS Keychain wrapper |
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
| `YOGURT_DEEPGRAM_API_KEY` | bootstrap | Seeds the Deepgram key into the Keychain on first run. |
| `YOGURT_OPENAI_API_KEY`, `YOGURT_OPENROUTER_API_KEY`, `YOGURT_MINIMAX_API_KEY` | bootstrap | Seed the matching LLM provider preset. |
| `YOGURT_LLM_BASE_URL`, `YOGURT_LLM_API_KEY`, `YOGURT_LLM_MODEL` | LLM resolver | Override the active LLM provider for this process without touching Settings or the Keychain. Handy when a rebuilt unsigned binary is waiting on a Keychain prompt. |
| `YOGURT_DEEPGRAM_MODEL` | cloud STT | Deepgram model name (default `nova-3`). Stamped into each meeting's `stt_engine`. |
| `YOGURT_VITE_BASE` | `--dev` proxy | Vite origin to proxy to (default `http://127.0.0.1:5173`). |
| `YOGURT_MEMORY_KEYSTORE=1` | tests | In-memory key store so tests never touch the real Keychain. Set automatically by `just test`. |
| `YOGURT_PORT_POLICY` | `scripts/run-*.sh` | What to do when the port is busy: `ask` (default), `kill`, `next`, or `fail`. |

The `yogurt start` and `yogurt doctor` flags themselves are documented in the [README](README.md#command-line).

## Troubleshooting the dev loop

**Keychain prompts on every rebuild.** macOS grants Keychain access per binary
identity, not per app name. An unsigned debug build gets a new identity on
every compile, so a plain `cargo build` invalidates any earlier "Always
Allow" click -- this is why unsigned dev workflows can feel like popup spam.
Test suites never touch the real Keychain (`YOGURT_MEMORY_KEYSTORE=1`, set by
`just test` and CI), and the LLM resolver checks `YOGURT_LLM_*` env vars from
`.env.local` before Keychain, so a dev loop that never pastes a key in Settings
never prompts.
For a dev build where you do paste keys in Settings, make the grant permanent
with a one-time self-signed code-signing identity:

1. Open Keychain Access -> Keychain Access menu -> Certificate Assistant -> Create a Certificate.
2. Name: `yogurt-dev`. Identity Type: Self Signed Root. Certificate Type: Code Signing. Create.
3. Verify: `security find-identity -v -p codesigning | grep yogurt-dev` prints one line.
   If it does not, double-click the cert in Keychain Access, expand Trust, and set Code Signing to Always Trust.
4. Run `just dev` / `just backend` / `just release` as usual. The scripts detect the identity, `codesign` the binary after every build, and print "signed with yogurt-dev identity".

Click "Always Allow" on the next Keychain prompt and it sticks across rebuilds, because macOS now sees the same signed app every time.
Shipped release builds are signed with a real Developer ID, so end users never hit this.

**Port already in use.** `just release` and `just dev` prompt before
starting if the target port is busy (`[k] kill it`, `[n] use next port`,
`[a] abort`). Skip the prompt with `YOGURT_PORT_POLICY=kill` or
`YOGURT_PORT_POLICY=next`, or pick a port up front with `just release --port 7900`.

**Resetting to a fresh-install state:**

```bash
just reset-db     # wipes ~/.yogurt/db.sqlite; next launch shows /welcome (Keychain entries stay)
just release
```

## Filing issues

Run `yogurt doctor --json` and paste the output -- it never includes API key
values or note content, only provider names and presence/absence flags.

Found a security vulnerability instead of a regular bug?
See [SECURITY.md](SECURITY.md) -- report it privately, not as a public issue.

## License

By contributing, you agree your contributions are licensed under the project's
[MIT license](LICENSE).
