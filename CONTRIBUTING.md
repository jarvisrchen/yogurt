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

## The dev loop

Yogurt has one single-binary release path and a two-process dev path. Both
share the same `~/.yogurt/db.sqlite`, so switching between them never loses
data.

**Just using yogurt** (record a meeting, browse the library):

```bash
just release      # single process, embedded web bundle, http://localhost:7878
```

**Editing the React UI with hot reload:**

```bash
just dev          # Vite + Rust backend together, Ctrl-C stops both
```

**Editing React with separate logs:**

```bash
just frontend     # Vite dev server on :5173, terminal A
just backend      # Rust backend in --dev mode, terminal B
```

Always open the browser at `http://localhost:7878`, never `:5173` directly --
the session lives on the backend.

**Editing only Rust:**

```bash
just release      # rebuilds incrementally, then runs
```

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
`just setup` and `just release` handle this ordering for you; a bare
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

## Troubleshooting the dev loop

**Keychain prompts on every rebuild.** macOS grants Keychain access per binary
identity, not per app name. An unsigned debug build gets a new identity on
every compile, so a plain `cargo build` invalidates any earlier "Always
Allow" click -- this is why unsigned dev workflows can feel like popup spam.
Test suites never touch the real Keychain (`YOGURT_MEMORY_KEYSTORE=1`, set by
`just test` and CI). For a dev build where you paste keys in Settings, you
can make grants permanent: open Keychain Access -> Certificate Assistant ->
Create a Certificate, name it `yogurt-dev`, set Certificate Type to Code
Signing. The run scripts detect the identity and sign the binary after every
build, so macOS sees the same app across rebuilds.

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
