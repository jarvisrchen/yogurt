# yogurt

> Local-first, open-source meeting copilot. Granola's UX, your machine.

**Status:** Phase 0 (scaffold). See [docs/PRD.md](docs/PRD.md) for v1 plan.

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

One-time setup:

```bash
brew install rust pnpm
git clone https://github.com/jarvisrchen/yogurt
cd yogurt
pnpm --dir web install
```

(Optional) Seed a dev API key for Phase 5+:

```bash
cat > .env.local <<'EOF'
YOGURT_MINIMAX_API_KEY=sk-...
EOF
```

The `.env.local` convention is gated to Phase 5 — it has no effect in Phase 0.

Two-terminal dev workflow (HMR):

```bash
# terminal 1 — Vite dev server with HMR
pnpm --dir web dev

# terminal 2 — Rust server with /api + /ws + dev proxy to Vite
cargo run -p yogurt -- start --dev
```

Open http://localhost:7878 in your browser. Changes to `web/src/**` hot-reload
through the Vite proxy.

Single-binary release build:

```bash
pnpm --dir web build
cargo run -p yogurt --release -- start --no-open
```

The release binary embeds `web/dist` via `rust-embed` — no Vite required at
runtime.

## Architecture (short)

Single Rust process owns audio capture, streaming STT, LLM enrichment, web
serving, and SQLite. The browser at `localhost:7878` is the only UI surface;
the binary is the only thing on disk. See [docs/PRD.md §7](docs/PRD.md) for
the architecture diagram and §8 for the component breakdown.

Local state lives under `~/.yogurt/`:

- `db.sqlite` — meetings + chat history (WAL mode, single-writer + read pool)
- `session-token` — random per-install token gating the WebSocket endpoint
  (mode `0600`)
- `notes/` — exported markdown (Phase 4+)

Audio never leaves your machine unless you opt into a cloud STT provider, and
even then only the audio stream goes out — never the notes.

## CLI

```text
yogurt start [--port 7878] [--no-open] [--dev]
```

- `--port` — TCP port to bind (default `7878`, localhost only)
- `--no-open` — do not auto-open the browser
- `--dev` — proxy non-API routes to the Vite dev server on `:5173`

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
