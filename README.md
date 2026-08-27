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
    test-e2e          # Playwright E2E smoke against a browser-mocked backend
    lint              # cargo fmt --check + clippy -D warnings
    fmt               # Auto-format Rust
    clean             # Remove all build artifacts
    clean-incremental # Drop incremental compile cache (~3 GB)
    reset-db          # Wipe ~/.yogurt/db.sqlite - next launch shows /welcome
    refresh-model-hashes *args # Download whisper models, print SHA256s
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

If a provider row already exists with no key (e.g. you clicked a preset chip earlier), the seeder backfills the missing Keychain entry from the env var on the next `just dev` boot.
Rows that already have a key are left untouched.

### Which command do I run?

Pick by what you're trying to do, not by what mode the project is in.
Yogurt has one single-binary release path and a two-process dev path —
both share the same `~/.yogurt/db.sqlite`, so switching between them
never loses data.

**Just using yogurt** (record a meeting, take notes, browse your library):

```bash
just release
```

One process, embedded web bundle, opens `http://localhost:7878`. Same
build path a brew user gets. Paste API keys via the Settings UI on
first run. **This is the default — pick this 90% of the time.**

**Editing the React UI and want hot reload:**

```bash
just dev
```

Runs Vite (frontend) and the Rust backend together in one terminal.
Edit any file under `web/src/**` and the browser updates without a
restart. Reads `.env.local` and seeds providers into the Keychain on
first boot. Ctrl-C stops both processes cleanly. Use this when you're
working on a component or a page.

**Editing the React UI and you want separate logs in two terminals:**

```bash
# terminal A
just frontend     # Vite dev server on :5173

# terminal B
just backend      # Rust backend on :7878 in --dev mode, proxies to Vite
```

Same end result as `just dev` but you can scroll backend logs and
frontend logs independently. Useful when you're chasing a backend log
line through a UI interaction. **You always open the browser at
`http://localhost:7878`, never `:5173`** — the auth session lives on
the backend, so going direct to Vite leaves you with a blank-canvas
SPA that can't reach the API.

**Editing the Rust backend without touching the UI:**

```bash
just release      # rebuilds incrementally (~5 s if nothing changed) then runs
```

The release recipe always runs a fresh build before starting. No need
to flip to dev mode unless you also want UI HMR. Iteration loop is
`Ctrl-C` → edit Rust → `just release`.

**Running the full test suite before committing:**

```bash
just test         # cargo test --workspace + pnpm --dir web test
just lint         # cargo fmt --check + clippy -D warnings
```

**Resetting to a fresh-install state to retest onboarding:**

```bash
just reset-db     # wipes ~/.yogurt/db.sqlite; next launch shows /welcome
just release      # boot it again
```

#### Quick reference

| I want to… | Run | Notes |
|---|---|---|
| Use yogurt to record a meeting | `just release` | Single process. Paste keys in Settings UI. |
| Edit a React component with HMR | `just dev` | One terminal, both processes. |
| Edit React + scroll logs separately | `just frontend` + `just backend` | Two terminals. Browser still at :7878. |
| Edit only Rust code | `just release` | Same recipe — just rebuilds + reruns. |
| Run all tests + lint | `just test && just lint` | What CI runs. |
| Start fresh to retest onboarding | `just reset-db && just release` | Keychain entries survive. |
| List every recipe with description | `just` | No-arg invocation. |

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

- `db.sqlite` - meetings, chat history, providers, and settings (WAL mode, single-writer + read pool)
- `session-token` — random per-install token gating WebSocket + most REST endpoints (mode `0600`)
- `models/` — downloaded whisper.cpp models (Phase 8, opt-in local STT)

Audio never leaves your machine unless you opt into a cloud STT provider, and
even then only the audio stream goes out — never the notes.

## Keychain prompts (macOS)

API keys are stored in the macOS Keychain, and macOS grants Keychain access per binary identity, not per app name.
An unsigned debug build gets a new identity on every compile, so a plain `cargo build` invalidates any earlier "Always Allow" click.
That is why unsigned dev workflows can feel like popup spam.

What yogurt does about it:

- The test suites never touch the real Keychain.
  Every server-booting integration test and the CLI tests set `YOGURT_MEMORY_KEYSTORE=1`, which swaps in an in-memory store (the `just test` recipes and CI set it too).
- Dev runs with `.env.local` do not need the Keychain at all.
  Keys seeded from env or pasted into Settings are served from an in-process cache for the lifetime of the process, so the Keychain is only read on a fresh boot with no env seeding.
- For dev builds where you paste keys in Settings, you can make grants permanent with a one-time step: open Keychain Access, then Certificate Assistant > Create a Certificate, name it `yogurt-dev`, set Certificate Type to Code Signing, and create it.
  The run scripts detect the identity and sign the binary after every build, so macOS sees the same app across rebuilds and one "Always Allow" click sticks forever.

Shipped release builds get real notarized signing in the distribution pipeline, so end users see at most one prompt.

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
