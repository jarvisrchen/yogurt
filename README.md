# yogurt

> Local-first, open-source meeting copilot for macOS. Granola's UX, your machine.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/jarvisrchen/yogurt/actions/workflows/ci.yml/badge.svg)](https://github.com/jarvisrchen/yogurt/actions/workflows/ci.yml)

Once `v0.1.0` ships: `brew install jarvisrchen/yogurt/yogurt && yogurt start`.

**Status:** v1.0 code-complete (Phases 0-8 shipped, Phase 9 distribution polish in progress).
Homebrew, direct-download, and GitHub Releases go live with the `v0.1.0` tag.
Until then, run from source (see below).
See [docs/archive/PRD.md](docs/archive/PRD.md) for the v1 plan and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how the system works.

Yogurt captures your microphone and Mac system audio without joining the call
as a bot, transcribes live, and produces "augmented notes" -- sparse markdown
bullets fused in-place with what was actually said. A single Rust binary, a
browser UI at `localhost:7878`, MIT licensed. Bring your own OpenAI-compatible
LLM (Minimax, OpenAI, OpenRouter, local Ollama, whatever you already pay for).

## Install

### Homebrew (recommended, once v0.1.0 ships)

```bash
brew install jarvisrchen/yogurt/yogurt
yogurt start
```

### From source

```bash
git clone https://github.com/jarvisrchen/yogurt.git
cd yogurt
pnpm --dir web install && pnpm --dir web build
cargo build --release
./target/release/yogurt start
```

(Or `cargo install --path crates/yogurt-cli` to put `yogurt` on your `$PATH`.)
See [CONTRIBUTING.md](CONTRIBUTING.md) for the full dev workflow, including
hot-reload and the `just` task runner.

### Direct download

```bash
curl -L https://github.com/jarvisrchen/yogurt/releases/latest/download/yogurt-aarch64-apple-darwin.tar.gz | tar xz
# Intel Macs: yogurt-x86_64-apple-darwin.tar.gz
./yogurt start
```

Browser-downloaded binaries carry a quarantine flag until notarization ships.
If Gatekeeper refuses to run the binary, clear it once with:

```bash
xattr -d com.apple.quarantine ./yogurt
```

(or right-click the binary in Finder and choose Open).

## First run

1. `yogurt start` -- opens `http://localhost:7878` in your default browser.
2. Click "New meeting" -- macOS prompts for Screen Recording permission; grant it.
3. Settings -> Model -- paste an API key for any OpenAI-compatible provider (or point at a local Ollama/LM Studio endpoint).
4. Record. Type sparse bullets while you talk; augmented notes (grey, AI-added, deep-linked to the transcript) appear within seconds of ending the meeting.

## Architecture

```
Browser (localhost:7878) <--HTTP+WS--> yogurt (single Rust binary)
                                          |-- axum HTTP/WS server
                                          |-- audio capture (ScreenCaptureKit)
                                          |-- STT engine (Deepgram cloud | whisper.cpp local)
                                          |-- LLM client (any OpenAI-compatible endpoint)
                                          `-- SQLite + markdown export (~/.yogurt/)
                                                |
                                                v (only if cloud STT/LLM selected)
                                          HTTPS out to your chosen provider
```

One process. No subprocesses, no IPC, no sidecar binaries, no Electron/Tauri
webview. Local state lives under `~/.yogurt/`:

- `db.sqlite` -- meetings, chat history, providers, and settings (WAL mode)
- `session-token` -- random per-install token gating WebSocket + most REST endpoints (mode `0600`)
- `models/` -- downloaded whisper.cpp models (opt-in local STT)
- `notes/*.md` -- markdown export of every meeting, independent of the DB

See [docs/archive/PRD.md §7](docs/archive/PRD.md) for the full architecture diagram and §8 for the crate breakdown.

## Diagnostics

```bash
yogurt doctor                 # rust/macOS/permissions/providers/STT/models dump
yogurt doctor --json          # same, machine-readable -- safe to paste into a bug report
yogurt --version
```

Repair flags:

- `--reset-screen-recording` -- resets the Screen Recording TCC grant so macOS re-prompts
- `--check-port` -- reports whether `:7878` is already in use
- `--redownload-model <name>` -- deletes a local whisper.cpp model so it re-downloads on next use

`yogurt doctor --json` never includes API key values or note content, only
provider names and presence/absence flags -- safe to paste into a GitHub issue.

## Privacy posture

- Audio never leaves your machine unless you opt into a cloud STT provider, and even then only the audio stream goes out, never the notes.
- No telemetry, no phone-home, not even opt-in crash reporting.
- API keys live in the macOS Keychain, never in plaintext config.
- Audio is deleted from disk after transcription.
- All data lives locally at `~/.yogurt/`.

### Threat model

Yogurt trusts anyone with local access to `localhost:7878` on this Mac to
read your notes and transcripts -- same trust boundary as the Granola
desktop app, or any other localhost-bound dev tool. It is not a multi-user
or network-exposed service; do not port-forward it.

## License

MIT. See [LICENSE](LICENSE).
