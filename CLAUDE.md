# yogurt - agent guide

Local-first meeting copilot for macOS.
Captures mic + system audio without a meeting bot, transcribes live, and fuses sparse user notes with the transcript into "augmented notes".
Single Rust binary serving a React SPA at `localhost:7878`.

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before making structural changes - it maps the system as built and records key decisions and rejected alternatives.

## Hard constraints (never violate)

- Audio never leaves the machine unless the user opted into cloud STT - and then only audio, never notes. Captured audio is deleted after transcription.
- API keys go to the macOS Keychain via the `keyring` crate - never plaintext files.
- One process: no subprocesses, no IPC, no sidecars. The binary embeds `web/dist` (rust-embed) and bundles SQLite (rusqlite `bundled`).
- Zero telemetry of any kind.
- macOS 13+ only (ScreenCaptureKit).
- MIT licensed; keep dependencies MIT-compatible.

## Commands

```
just setup        # one-time: toolchains + pnpm install
just dev          # backend (cargo run) + frontend (vite) together
just build        # pnpm web build, then cargo build --release
just test         # cargo test --workspace --features yogurt-stt/local-stt + web vitest
just lint         # clippy -D warnings + fmt --check + web typecheck
```

The frontend must be built (`pnpm --dir web build`) before any Rust build that compiles `yogurt-server` - `rust-embed` requires `web/dist` to exist.
All app data lives under `~/.yogurt/` (db.sqlite, notes/, models/, session-token).

## Conventions

- Rust: rustfmt + clippy clean at `-D warnings`; `anyhow` at binary surface, `thiserror` at crate boundaries.
- Frontend: React 19 + Vite + Tailwind 4 (tokens in `web/src/index.css` `@theme`, PRD §16 Blueberry) + zustand + TanStack Query.
- Never use an em dash in prose; use a plain "-".
- Do not hand-edit CHANGELOG files; release notes are generated.
- Tests accompany non-trivial logic; E2E behavior is verified against the real binary, not just unit tests.
