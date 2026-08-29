# yogurt

<!-- TODO: hero screenshot -->

> Local-first, open-source meeting copilot for macOS. Granola's UX, your machine.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/jarvisrchen/yogurt/actions/workflows/ci.yml/badge.svg)](https://github.com/jarvisrchen/yogurt/actions/workflows/ci.yml)

Yogurt captures your microphone and Mac system audio without joining the call
as a bot, transcribes live, and produces "augmented notes" -- sparse markdown
bullets fused in-place with what was actually said.
A single Rust binary, a browser UI at `localhost:7878`, MIT licensed.
Bring your own OpenAI-compatible LLM (Minimax, OpenAI, OpenRouter, local
Ollama, whatever you already pay for).

**Status:** pre-1.0.
A `v0.1.0` release (Homebrew tap + GitHub Releases tarballs) is coming.
Until then, build from source (see below).

## Install

### Homebrew (once v0.1.0 ships)

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

### Direct download (GitHub Releases)

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

## First meeting

1. `yogurt start` -- opens `http://localhost:7878` in your default browser.
2. Grant Screen Recording and Microphone access when macOS prompts for them.
3. Start recording in a meeting.
4. Type sparse notes while you talk -- a few words per point is enough.
5. Stop the meeting.

Your notes get enhanced in place: grey, AI-added text fused into what you
wrote, deep-linked back to the transcript.

Something not working? Run `yogurt doctor` for a rust/macOS/permissions/
providers/STT/models dump, or `yogurt doctor --json` for a machine-readable
version that's safe to paste into a bug report.

## How it works

One Rust process runs the audio capture (ScreenCaptureKit), live STT
(Deepgram cloud or local whisper.cpp), the LLM enhancement pass, an axum
HTTP/WS server, and SQLite storage.
No subprocesses, no IPC, no sidecar binaries, no Electron/Tauri webview.
See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full design,
sequence diagrams, and trust boundaries.

## Privacy posture

- Audio never leaves your machine unless you opt into a cloud STT provider,
  and even then only the audio stream goes out, never the notes.
- No telemetry, no phone-home, not even opt-in crash reporting.
- API keys live in the macOS Keychain, never in plaintext config.
- Audio is deleted from disk after transcription.
- All data lives locally at `~/.yogurt/`.

See [SECURITY.md](SECURITY.md) for the full threat model and how to report
vulnerabilities.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for dev environment setup and the
`just` task runner.

## License

MIT.
See [LICENSE](LICENSE).
