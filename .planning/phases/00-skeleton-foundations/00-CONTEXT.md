# Phase 0: Skeleton & Foundations - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Cargo workspace builds, `yogurt start` serves a "Hello yogurt" SPA from a single static binary, with the foundational pitfall mitigations (SQLite WAL + dual pool, embedded SPA fallback, localhost-only bind, WS Origin check + session token, port-conflict UX) baked in from day one. No audio capture, no STT, no LLM, no notes editor — this phase ships scaffolding only.

</domain>

<decisions>
## Implementation Decisions

### Workspace & Toolchain
- **D-01:** Cargo workspace at the repo root using resolver "2"; first two members are `crates/yogurt-cli` (binary) and `crates/yogurt-server` (library). Additional crates land in later phases — Phase 0 ships these two only despite the eventual 8-crate target.
- **D-02:** Rust pinned to `1.83` via `rust-toolchain.toml` with `rustfmt` + `clippy` components. The superpowers plan locks 1.83; do not bump.
- **D-03:** Workspace dependencies pinned: `tokio 1.42 (features=full)`, `axum 0.8 (features=macros)`, `tower 0.5`, `tower-http 0.6 (features=fs,trace)`, `clap 4.5 (features=derive)`, `rust-embed 8.5 (features=mime-guess)`, `reqwest 0.12 (default-features=false; features=json,rustls-tls,stream)`, `mime_guess 2`, `assert_cmd 2`, `anyhow 1`, `serde 1`, `serde_json 1`, `tracing 0.1`, `tracing-subscriber 0.3 (features=env-filter)`.
- **D-04:** Release profile uses `lto = "thin"`, `codegen-units = 1`, `strip = true`.
- **D-05:** Binary target name is `yogurt` (set via `[[bin]] name="yogurt" path="src/main.rs"` inside `crates/yogurt-cli`); the package itself is also named `yogurt`.

### CLI Surface
- **D-06:** `yogurt start` is the sole subcommand for Phase 0; it accepts `--port <u16>` (default `7878`), `--no-open` (skip browser auto-open), and `--dev` (route non-API requests to Vite at `:5173`).
- **D-07:** `tracing_subscriber::fmt` with `EnvFilter` defaulting to `yogurt=info,yogurt_server=info` initializes at CLI startup.
- **D-08:** Browser auto-open uses the `open = "5"` crate spawned on a background task so a launch failure cannot block the server from binding.

### Server Architecture
- **D-09:** `yogurt-server` exposes `pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()>` and a `pub enum Mode { Dev, Release }`; the CLI selects the variant from the `--dev` flag.
- **D-10:** Router has exactly one always-on route in Phase 0 (`GET /api/health` returning `{"status":"ok","service":"yogurt-server"}`) plus a mode-dependent fallback handler — embedded asset serve in Release, Vite proxy in Dev. The transitional `GET /` "hello yogurt" handler from Task 0.3 is deleted in Task 0.7 to keep clippy `-D warnings` clean.
- **D-11:** Localhost-only bind: address is hardcoded as `[127.0.0.1, port]` in `commands::start::run`; never `0.0.0.0`. This is the foundation of the localhost trust assumption documented in PRD §7.
- **D-12:** Listener uses `tokio::net::TcpListener::bind(addr)` and `axum::serve(listener, app)`; bind errors must propagate so port-conflict UX (D-19) can format them.

### Embedded SPA Asset Pipeline
- **D-13:** `web/dist` is embedded via `rust-embed` (`#[derive(RustEmbed)] #[folder = "../../web/dist/"] struct WebDist`); fallback handler returns the requested asset or falls through to `index.html` for SPA client-side routes. This implements the "embedded SPA fallback" pitfall mitigation.
- **D-14:** MIME types resolved via `mime_guess::from_path(...).first_or_octet_stream()`; `mime_guess` declared as a direct dependency (not relying on rust-embed's transitive feature) so the import is explicit and survives refactors.
- **D-15:** `web/dist/` is gitignored; the README + Task 0.10 smoke document the required `pnpm --dir web build` step before the embedded test passes on a fresh clone.

### Dev Proxy
- **D-16:** In `Mode::Dev`, non-API requests proxy to `http://127.0.0.1:5173` using `reqwest::Client`. Hop-by-hop headers (`connection`, `keep-alive`, `proxy-authenticate`, `proxy-authorization`, `te`, `trailers`, `transfer-encoding`, `upgrade`, `host`) are stripped on both legs.
- **D-17:** Vite proxies `/api` and `/ws` back to `:7878` (`ws: true` for WebSocket upgrade) so the browser sees a single origin regardless of which side it hits.
- **D-18:** Proxy upstream failure renders `502 Bad Gateway` with the actionable copy `yogurt dev proxy: cannot reach vite at http://127.0.0.1:5173\n\nrun: pnpm --dir web dev` — surfaces the dual-terminal dev workflow on first miss.

### Pitfall Mitigations (Bake-in from Day One)
- **D-19:** Port-conflict UX: the CLI must catch `bind` errors and print `Port 7878 is already in use. Try --port 7879 or run lsof -i :7878` then exit non-zero. The hardcoded message must reference the actual port the user passed.
- **D-20:** WS Origin allowlist: WebSocket handshake handler accepts only `Origin: http://localhost:7878` and `http://127.0.0.1:7878` (matching the bound port); all other Origins (including null) are rejected with `403 Forbidden`. Phase 0 ships the validator + a stub WS endpoint at `/ws`; real WS traffic lands in Phase 3.
- **D-21:** Session token: on first server boot, write a 32-byte URL-safe random token to `~/.yogurt/session-token` with mode `0600`. The WS handler requires this token as either a `?token=` query param or `Sec-WebSocket-Protocol: yogurt.<token>` subprotocol header. Token survives restarts (read if present, generate if missing).
- **D-22:** SQLite WAL + dual pool: on server startup, initialize `~/.yogurt/db.sqlite`, set `PRAGMA journal_mode=WAL`, run schema migrations, and expose two handles: a read pool (`r2d2` or hand-rolled vec of `Connection`s) and a single-writer `Mutex<Connection>`. Both are stored in axum app state via `Arc`.
- **D-23:** v1 schema migration (Phase 0 scope): create `meetings(id TEXT PRIMARY KEY, title TEXT, started_at INTEGER NOT NULL, ended_at INTEGER, notes_md TEXT, enriched_md TEXT, transcript_json TEXT)` and `chat_messages(id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, created_at INTEGER NOT NULL, FOREIGN KEY(meeting_id) REFERENCES meetings(id))`, plus indexes `idx_meetings_started_at ON meetings(started_at DESC)` and `idx_chat_messages_meeting_id ON chat_messages(meeting_id, created_at)`. The `enriched_doc_json TEXT` column is **deferred to Phase 4** per REQUIREMENTS.md split-mapping for STORE-01.

### Frontend Scaffold
- **D-24:** Web stack pinned per superpowers plan: React `^19.0.0` + `react-dom ^19.0.0`, Vite `^6.0.0`, TypeScript `^5.6.0`, Tailwind `^4.0.0` via `@tailwindcss/vite ^4.0.0`, TipTap `^2.10.0` (core/react/starter-kit), Vitest `^2.1.0`, jsdom `^25.0.0`, `@testing-library/react ^16.1.0`. Brand tokens + Tiptap 3 upgrade happen in Phase 1 — Phase 0 ships an unstyled functional scaffold.
- **D-25:** Vite dev server uses `port: 5173`, `strictPort: true` (fail loud on collision rather than auto-incrementing).
- **D-26:** `vite.config.ts` starts with `/// <reference types="vitest/config" />` so `tsc --noEmit` (run during `pnpm build`) recognizes the `test:` block.
- **D-27:** Vitest setup file `web/src/vitest.setup.ts` imports `@testing-library/jest-dom/vitest`; wired into config via `test.setupFiles`.

### Test Conventions
- **D-28:** Rust unit tests inline via `#[cfg(test)] mod tests`; Rust integration tests at `crates/<crate>/tests/<area>.rs` using `assert_cmd` for the CLI and `reqwest` + `#[tokio::test]` for HTTP.
- **D-29:** Frontend tests use Vitest at `web/src/**/*.test.ts(x)`; jsdom environment, globals enabled, `vitest.setup.ts` loads jest-dom matchers.
- **D-30:** Naming: `it_<does_thing>` for Rust, `it("<does thing>", ...)` for Vitest. No E2E / Playwright in Phase 0.

### Licensing & Distribution Scaffold
- **D-31:** `LICENSE` is MIT, `Copyright (c) 2026 Jarvis Chen`. README documents the dual-terminal dev workflow and the future `brew install yogurt && yogurt start` path even though Phase 9 is what actually ships it.

### Claude's Discretion
- Exact `tracing` log formatting beyond the default `EnvFilter`.
- Internal layout of the SQLite pool wrapper (struct shape, naming) provided D-22's contract holds.
- Specific session-token entropy source (any cryptographically-strong RNG is fine).
- Whether the read pool is a hand-rolled `Vec<Connection>` behind a `Semaphore` or a `r2d2` pool — either satisfies D-22.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Authoritative implementation plan
- `docs/superpowers/plans/2026-06-25-yogurt-phase-0-skeleton.md` — Authoritative implementation plan for this phase. Plan tasks below cite this for exact Cargo.toml, Rust source, and TypeScript content. Numbered Task 0.1–0.10 inside this file are the source of truth — GSD plans below chunk those into waves.

### Product requirements
- `docs/PRD.md` §7 — Architecture diagram (single Rust binary, browser at localhost:7878)
- `docs/PRD.md` §8 — Component breakdown (yogurt-cli vs yogurt-server split)
- `docs/PRD.md` §9 — Storage layout (`~/.yogurt/` directory, `db.sqlite` + `notes/`, ULID meeting IDs)
- `docs/PRD.md` §11 — Distribution & dev workflow (single static binary, embedded assets, Homebrew target)
- `docs/PRD.md` §16 — Design tokens (only referenced; not applied until Phase 1)

### Project planning
- `.planning/REQUIREMENTS.md` — Section "Foundation" (FOUND-01 through FOUND-06) and "Local Storage" (STORE-01, STORE-02, STORE-05 in scope for Phase 0; STORE-03/04 + STORE-01 `enriched_doc_json` deferred to Phase 4)
- `.planning/ROADMAP.md` — "### Phase 0: Skeleton & Foundations" success criteria (5 must-be-true gates)
- `.planning/PROJECT.md` — Core value, tech-stack constraints, single-process invariant

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- None yet — this is a greenfield phase. The repo contains only `docs/`, `.planning/`, `yogurt-app-design/` (HTML design board for future Phase 1 reference), and a starter `.gitignore`.

### Established Patterns
- No prior Rust or TypeScript code patterns to follow — Phase 0 establishes the conventions other phases will inherit (test layout per D-28/D-29/D-30, workspace shape per D-01).

### Integration Points
- `.gitignore` exists (160B Vercel-style file already covering `.env`, `.env.local`, `.env*.local`, `.next/`, `dist/`, `build/`, `node_modules/`); Task 0.1 appends only the Rust / pnpm / Lavish additions to avoid duplication. The `.env.local` rule MUST remain effective — Task 0.1 Step 4 verifies via `git check-ignore -v .env.local` because the user's Minimax dev key lives there.
- `docs/PRD.md` exists and is authoritative for architecture & storage layout — do not duplicate its content in code comments; link instead.
- `.planning/` directory exists with ROADMAP / REQUIREMENTS / PROJECT files; these are not modified in this phase.
- `yogurt-app-design/` design board exists but is consumed in Phase 1, not Phase 0.

</code_context>

<specifics>
## Specific Ideas

- The two-terminal dev workflow (`pnpm --dir web dev` + `cargo run -p yogurt -- start --dev`) must work with HMR — editing `web/src/App.tsx` should reflect in the browser via the Vite proxy.
- The release smoke (`./target/release/yogurt start --no-open`) must boot in under 1s and be fully self-contained — no `web/dist` lookup at runtime. This is the single-static-binary promise that gates the entire product pitch.
- TipTap is wired in Phase 0 only to prove the build works (a `<p>Type something — TipTap is working.</p>` editor). The actual aiGrey/transcriptTs marks land in Phase 4 — do not over-design here.
- The hardcoded background paper color (`#FBF7EF`) and ink color (`#211D18`) in `web/src/index.css` are throwaway placeholders. Phase 1 replaces this with proper design tokens; do not import `yogurt-app-design/` assets in Phase 0.
- Port conflict UX copy is exact: `Port 7878 is already in use. Try --port 7879 or run lsof -i :7878`. The example port in the suggestion is `current_port + 1`.

</specifics>

<deferred>
## Deferred Ideas

These came up in the superpowers plan but explicitly belong to later phases:

- **Phase 1:** Tailwind 4 brand tokens (paper / ink / blueberry / strawberry / matcha), Instrument Serif + Hanken Grotesk + JetBrains Mono via `@fontsource/*`, design-system primitives (Button, Pill, Card, RecordingBadge, BrowserChrome), swirl logo as React component, `/style-guide` route. Tiptap upgrades to v3 with `@tiptap/extension-markdown` at the same time.
- **Phase 2:** Audio capture via ScreenCaptureKit, `yogurt-audio` crate, meeting-relative clock, Swift sidecar fallback path.
- **Phase 3:** `SttEngine` trait, Deepgram adapter, live transcript dock UI, real WebSocket traffic over the `/ws` endpoint scaffolded here.
- **Phase 4:** `enriched_doc_json TEXT` column migration (the second half of STORE-01), TipTap aiGrey/transcriptTs marks, server-side AST diff, bundled `enhance.md`, `MarkdownExporter` writing to `~/.yogurt/notes/`.
- **Phase 5:** `LlmClient` trait, settings UI, Keychain integration with eager-load + 5s timeout, `--dev` mode `.env.local` loading.
- **Phase 6:** In-meeting chat pill + chat window + `chat-system.md`.
- **Phase 7:** Library view, `/welcome` onboarding, empty/error states, FTS5 search, copy-markdown / reveal-in-finder.
- **Phase 8:** whisper.cpp local STT behind `local-stt` Cargo feature.
- **Phase 9:** GitHub Actions release matrix, notarization, Homebrew tap PR, `cargo publish`, `yogurt doctor` subcommand, universal binary via `lipo`.
- **Phase 0 superpowers Task 0.10 Step 5 (tag push):** Tagging `v0.0.1-phase-0` is gated on explicit user confirmation per the superpowers plan; the GSD plan below does not auto-tag.

</deferred>

---

*Phase: 00-skeleton-foundations*
*Context gathered: 2026-06-25*
