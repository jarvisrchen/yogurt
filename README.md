# yogurt

> Local-first, open-source meeting copilot. Granola's UX, your machine.

**Status:** v1.0 milestone code-complete (Phases 0–8 shipped, Phase 9 distribution remaining). See [docs/PRD.md](docs/PRD.md) for the v1 plan.

Yogurt captures your microphone and Mac system audio without joining the call
as a bot, transcribes live, and produces "augmented notes" — sparse markdown
bullets fused in-place with what was actually said. A single Rust binary, a
browser UI at `localhost:7878`, MIT licensed.

## Install (eventually)

```bash
brew install yogurt
yogurt start
```

> Not yet — first release lands in Phase 9. Until then, run from source.

## Run from source today

### Quickstart (two commands)

```bash
git clone https://github.com/jarvisrchen/yogurt
cd yogurt
./scripts/setup.sh   # one-time: prereqs + just task runner + web bundle + release build
just release         # every time: starts yogurt at http://localhost:7878
```

`setup.sh` is idempotent and stops on the first failure with an actionable
hint. It auto-installs the [`just`](https://github.com/casey/just) task
runner via Homebrew (`--no-just` to opt out and use `./scripts/*.sh`
directly). Re-run it any time you want to refresh deps or rebuild the
release binary.

Type `just` with no args to list every recipe:

```text
$ just
Available recipes:
    release *args     # Start the release binary at http://localhost:7878
    dev               # Backend + Vite together in one terminal, Ctrl-C stops both
    backend *args     # Backend only (assumes Vite running elsewhere)
    frontend          # Vite dev server only (:5173)
    setup             # One-time prereq + build (idempotent)
    setup-quick       # Re-run setup without the slow cargo build
    build             # cargo build --release
    build-web         # pnpm --dir web build
    test              # Full cargo + web test suite
    test-rust         # Just cargo tests
    test-web          # Just web tests
    lint              # cargo fmt --check + clippy -D warnings
    fmt               # Auto-format Rust
    clean             # Remove all build artifacts
    clean-incremental # Drop incremental compile cache (~3 GB)
    reset-db          # Wipe ~/.yogurt/yogurt.db — next launch shows /welcome
```

### API keys

Edit `.env.local` (the setup script writes a stub for you, `chmod 600`):

```text
YOGURT_DEEPGRAM_API_KEY=dg_...
YOGURT_OPENAI_API_KEY=sk-...
# YOGURT_MINIMAX_API_KEY=...
# YOGURT_OPENROUTER_API_KEY=...
```

`.env.local` is loaded **only** by `just dev` (or any invocation with
`--dev`), per the SET-11 privacy invariant — release builds never read it.
On first `just dev` boot the seeder copies any present LLM keys into
SQLite + macOS Keychain; after that they show in Settings → Model in any
mode.

If a provider row already exists with no key (e.g. you clicked a preset
chip earlier), the seeder currently skips backfilling — paste the key
into the row's "Paste new key…" field in Settings instead. Tracked as a
known Phase 5 gap.

### Two run modes

| Mode | Command | When to use |
|------|---------|-------------|
| **Release** | `just release` | Validation, acceptance testing, daily driving. Single binary with embedded web bundle. What brew users get. |
| **Dev (HMR)** | `just dev` | UI development. Backend + Vite together in one terminal; Ctrl-C cleans both. Loads `.env.local`. |

For separate terminals: `just frontend` (Vite on :5173) + `just backend` (backend on :7878 in `--dev` mode).

### Port already in use

Every run recipe prompts before starting if the target port is busy:

```text
! port 7878 (release) is busy — held by PID 12345
   [k] kill it       [n] use next port       [a] abort
```

Skip the prompt with an env var, or pick a specific port up-front:

```bash
YOGURT_PORT_POLICY=kill just release      # kill the holder silently
YOGURT_PORT_POLICY=next just release      # pick the next free port silently
just release --port 7900                  # use a specific port
```

## Architecture (short)

Single Rust process owns audio capture, streaming STT, LLM enrichment, web
serving, and SQLite. The browser at `localhost:7878` is the only UI surface;
the binary is the only thing on disk. See [docs/PRD.md §7](docs/PRD.md) for
the architecture diagram and §8 for the component breakdown.

Local state lives under `~/.yogurt/`:

- `yogurt.db` — meetings + chat history (WAL mode, single-writer + read pool)
- `session-token` — random per-install token gating WebSocket + most REST endpoints (mode `0600`)
- `models/` — downloaded whisper.cpp models (Phase 8, opt-in local STT)

Audio never leaves your machine unless you opt into a cloud STT provider, and
even then only the audio stream goes out — never the notes.

## CLI

```text
yogurt start [--port 7878] [--no-open] [--dev]
```

- `--port` — TCP port to bind (default `7878`, localhost only)
- `--no-open` — do not auto-open the browser
- `--dev` — load `.env.local` + proxy non-API routes to the Vite dev server on `:5173`

If the port is already in use, the CLI prints a one-line hint and exits
non-zero:

```text
Port 7878 is already in use. Try --port 7879 or run lsof -i :7878
```

## Privacy posture

- No telemetry, no phone-home, not even opt-in crash reporting in v1.
- API keys live in macOS Keychain via the `keyring` crate (Phase 5+).
- Audio is deleted from disk after transcription unless you explicitly retain
  it (Phase 1.1+).

## License

MIT. See [LICENSE](LICENSE).
