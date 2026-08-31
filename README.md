# yogurt

<!-- TODO: hero screenshot -->

> Local-first, open-source meeting copilot for macOS. Granola's UX, your machine.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/jarvisrchen/yogurt/actions/workflows/ci.yml/badge.svg)](https://github.com/jarvisrchen/yogurt/actions/workflows/ci.yml)

Yogurt captures your microphone and Mac system audio without joining the call
as a bot, transcribes live, and produces "augmented notes": sparse markdown
bullets fused in place with what was actually said.
A single Rust binary, a browser UI at `localhost:7878`, MIT licensed.
Bring your own OpenAI-compatible LLM (Minimax, OpenAI, OpenRouter, local
Ollama, whatever you already pay for).

**Status:** pre-1.0.
Released as a Homebrew tap and as GitHub Releases tarballs; see [docs/RELEASING.md](docs/RELEASING.md) for the release log.
Building from source still works and is documented below.

## Install

### Homebrew

```bash
brew install jarvisrchen/yogurt/yogurt
yogurt start
```

Tapping first lets you use the short name from then on, including for
`brew upgrade` and `brew info`:

```bash
brew tap jarvisrchen/yogurt
brew install yogurt
```

### From source

Prereqs: macOS 13+ and Homebrew.
`setup.sh` installs Rust, Node 22, CMake, and the pinned pnpm version through Homebrew and Corepack.

```bash
git clone https://github.com/jarvisrchen/yogurt.git
cd yogurt
./scripts/setup.sh          # checks prereqs, builds the web bundle + release binary
./target/release/yogurt start
```

`setup.sh` is idempotent and the recommended path. To do it by hand instead:

```bash
pnpm --dir web install && pnpm --dir web build
cargo build --release
```

(Or `cargo install --path crates/yogurt-cli` to put `yogurt` on your `$PATH`.)

If you have [`just`](https://github.com/casey/just) installed (`setup.sh` installs it), `just release` is the same as `./target/release/yogurt start` plus an incremental rebuild of the web bundle and binary first, and a prompt if port 7878 is busy.
`just` is only a contributor convenience; the shipped product is the `yogurt` binary alone.
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

## Updating

Stop the running server first (Ctrl-C in the terminal running `yogurt start`); a
replaced binary does not take effect until the process restarts.

Your data lives in `~/.yogurt/` and is never touched by an update.
Migrations run automatically on the next `yogurt start`.

**Homebrew**

```bash
brew update && brew upgrade yogurt   # short name works once you've tapped
yogurt --version
```

Without the tap, the fully-qualified name works too:
`brew upgrade jarvisrchen/yogurt/yogurt`.
If `brew upgrade` says you're already on the latest but
[Releases](https://github.com/jarvisrchen/yogurt/releases) shows something
newer, the tap formula hasn't been merged yet; there is nothing to do but wait.

**From source**

```bash
git pull
./scripts/setup.sh          # rebuilds the web bundle and the release binary
./target/release/yogurt start
```

Skipping the web build leaves the old UI embedded in the new binary, so use
`setup.sh` (or `just release`) rather than `cargo build` alone.

**Direct download**

Re-run the `curl` from [Direct download](#direct-download-github-releases) over
the old binary, then clear quarantine again.
The flag is set per download, so it comes back on every update.

## First meeting

1. `yogurt start` opens `http://localhost:7878` in your default browser.
2. Grant Screen Recording and Microphone access when macOS prompts for them.
3. Start recording in a meeting.
4. Type sparse notes while you talk. A few words per point is enough.
5. Stop the meeting.

Your notes get enhanced in place: grey, AI-added text fused into what you
wrote, deep-linked back to the transcript.

Something not working? Run `yogurt doctor` for a rust/macOS/permissions/
providers/STT/models dump, or `yogurt doctor --json` for a machine-readable
version that's safe to paste into a bug report.

## Transcription models

Transcription runs one of two ways, switchable under **Settings -> Transcription**:

- **Cloud (default).** Deepgram, needs an API key and sends audio to their API.
- **Local.** whisper.cpp on your machine. No key, no network, nothing leaves the laptop.

Local needs a model file. Pick one under **Settings -> Transcription -> Local** and yogurt downloads it to `~/.yogurt/models/`, verifying the SHA256 before use.

| Model | Size | Intel Macs | Notes |
| --- | --- | --- | --- |
| `tiny.en` | 75 MB | fine | Fastest, roughest. Good for a quick check that local works. |
| `small.en` | 487 MB | fine | **Default.** The best size-to-quality tradeoff for meetings. |
| `medium.en` | 1.5 GB | slow | Wants Apple Silicon. |
| `large-v3-turbo` | 1.6 GB | slow | Wants Apple Silicon. Near `large-v3` quality, much faster. |
| `large-v3` | 3.0 GB | slow | Wants Apple Silicon. Slowest, best quality. |

The `.en` models are English-only; `large-v3` and `large-v3-turbo` are multilingual.
The three larger models lean on arm64 Metal kernels, so on an Intel Mac the picker tags them `slow`.
You can still pick one, it just may not keep up with live speech.

Models live in `~/.yogurt/models/` and are yours to manage:

```bash
yogurt doctor                            # lists which models are present, and where
yogurt doctor --redownload-model small.en   # drop the local copy and re-fetch it
```

You can also drop a `ggml-*.bin` file into `~/.yogurt/models/` yourself.
yogurt identifies models by hash, not by where they came from, so a file copied in by any means is picked up as long as it matches.

### Workaround: installing models with Homebrew

The download button fetches from `huggingface.co`. If that is blocked on your network, or you would rather not sit through a multi-gigabyte download in a browser tab, the models are also mirrored on this repo's releases and installable through Homebrew - which fetches from `github.com`, already proven reachable, since that is where `brew` got yogurt itself:

```bash
brew install jarvisrchen/yogurt/yogurt-model-tiny-en          # 75 MB
brew install jarvisrchen/yogurt/yogurt-model-small-en         # 487 MB, the default
brew install jarvisrchen/yogurt/yogurt-model-medium-en        # 1.5 GB
brew install jarvisrchen/yogurt/yogurt-model-large-v3-turbo   # 1.6 GB
```

yogurt reads these automatically, from `$(brew --prefix)/share/yogurt/models` as well as `~/.yogurt/models`.
The model shows up in the picker tagged `brew`, with no delete button: remove it with `brew uninstall` instead, so Homebrew stays consistent.
`yogurt doctor` shows which copy is in use.

The mirrored bytes are identical to HuggingFace's, verified against the same pinned SHA256.

`large-v3` is the one model with no Homebrew option: at 3.0 GB it is over GitHub's 2 GB limit for a release asset, so it can only come from HuggingFace.
Reach for `large-v3-turbo` instead, which is close in quality and considerably faster.

## Command line

`yogurt --help` and `yogurt <command> --help` are the source of truth; this is the same information in one place.
(`just release` / `just dev` are contributor wrappers around these commands, see [CONTRIBUTING.md](CONTRIBUTING.md#just-vs-yogurt).)

### `yogurt start`

Launches the local server and opens the browser.

| Flag | Default | What it does |
|------|---------|--------------|
| `--port <PORT>` | `7878` | TCP port to bind. Always binds `127.0.0.1` only, never a LAN interface. |
| `--no-open` | off | Do not auto-open `http://localhost:<port>` in your default browser once the server is listening. |
| `--dev` | off | Developer mode. Proxies every non-`/api` request to the Vite dev server on `:5173` instead of serving the web bundle embedded in the binary, loads `.env.local` from the current directory, and allows WebSocket upgrades from the `:5173` origin for hot reload. Never use this for normal use; see [CONTRIBUTING.md](CONTRIBUTING.md). |

Without `--dev` the UI you see is whatever was compiled into the binary at build time.

### `yogurt doctor`

Prints a diagnostic dump (Rust and macOS versions, Screen Recording and Microphone permission state, configured providers, downloaded STT models) plus repair actions.

| Flag | What it does |
|------|--------------|
| `--json` | Emit the same diagnostics as JSON, safe to paste into a bug report. |
| `--check-port` | Report whether port 7878 is free or in use, and suggest a `--port` value if it is busy. |
| `--reset-screen-recording` | Reset the Screen Recording TCC grant for `ai.yogurt.app` so macOS prompts again on next start. |
| `--redownload-model <MODEL>` | Delete the local copy of a whisper.cpp model (for example `small.en`) so the next start re-downloads it. |

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
- API keys live in `~/.yogurt/keys.json` (mode 0600), never in the database or in any response body.
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
