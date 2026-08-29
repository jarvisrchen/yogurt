# Stack Research — Yogurt (macOS Local-First Meeting Copilot)

**Domain:** macOS single-binary desktop app + browser UI + system-audio capture + pluggable streaming STT + OpenAI-compatible LLM client
**Researched:** 2026-06-25
**Confidence:** HIGH for core (axum/tokio/whisper-rs/async-openai/rusqlite/keyring/Vite/Tailwind/TipTap); MEDIUM for `screencapturekit` (rapidly evolving, audio loopback is newest surface area); MEDIUM for `deepgram` Rust SDK (community-maintained, pre-1.0).

---

## Verdict

**Ship the locked stack with the versions pinned below.** Every locked choice is the standard 2026 answer for its job. Two flags worth tracking, neither blocking:

1. `screencapturekit` (Rust) is the right path — version 2.x added `Send + Sync` bounds and macOS 13+ system-audio + microphone capture is supported — but the audio-loopback surface is the youngest API in the crate. Keep the **Swift sidecar fallback (PRD §13)** as a documented Phase 2 escape hatch.
2. The official `deepgram` Rust SDK is community-owned and pre-1.0. It works for streaming WebSocket today, but a hand-rolled `tokio-tungstenite` WS client against Deepgram's documented protocol is a 200-line fallback if the SDK trips us up.

No superior alternatives were found for any locked choice. Tauri, Electron, Wry-based wrappers, and BlackHole-style virtual audio drivers were considered and rejected — see "What NOT to Use."

---

## Recommended Stack

### Core — Backend (Rust)

| Technology | Pin | Purpose | Why |
|------------|-----|---------|-----|
| **Rust** | 1.82+ (stable channel) | Language | Required for `cpal` ≥ recent (coreaudio-rs needs ≥1.80); pins us to current edition tooling. |
| **tokio** | `1.51` (LTS until 2027-03) or `1.52` (current stable) | Async runtime | LTS gives the project a year of bug-fix backports without breaking-change churn. `1.51.x` is the LTS line; `1.52.3` is the current stable patch. Pick LTS for a long-lived OSS binary. |
| **axum** | `0.8` | HTTP + WebSocket server | De-facto standard Tokio-stack HTTP framework. 0.8 is the current stable line (`0.8.6`–`0.8.9` as of mid-2026, 191M+ downloads). Pair with `tower` ecosystem for middleware. |
| **tower** + **tower-http** | `0.5` / `0.6` | Middleware (CORS, compression, tracing) | Axum delegates middleware to tower. |
| **hyper** | `1.x` | HTTP layer (transitive via axum 0.8) | No direct dependency — axum pulls it in. |
| **tokio-tungstenite** | `0.24` | WebSocket client for STT providers | Used by the Deepgram adapter (and the fallback if the SDK is wonky). |
| **serde** + **serde_json** | `1.x` | Serialization | Universal Rust JSON. |
| **tracing** + **tracing-subscriber** | `0.1` / `0.3` | Structured logging | Required for any axum/tower app worth debugging. Plays nicely with tower-http's TraceLayer. |
| **anyhow** + **thiserror** | `1.x` | Error handling | `anyhow` for binary/CLI surface, `thiserror` for crate boundaries. |
| **ulid** | `1.x` | Meeting IDs | PRD §9 specifies ULID. |
| **time** or **jiff** | `0.3` / `0.2` | Timestamps | `time` is the safe pick; `jiff` (Burnsushi 2025) is newer and nicer if you can swallow a younger dependency. Stick with `time` for v1. |

### Core — Audio Capture (macOS native)

| Technology | Pin | Purpose | Why |
|------------|-----|---------|-----|
| **screencapturekit** | `2.x` (track latest minor) | System-audio loopback via SCK + mic capture | Safe idiomatic bindings to Apple's ScreenCaptureKit. Supports system audio + microphone capture on macOS 13.0+ with real-time zero-copy frame delivery via IOSurface/Metal. v2.x added `Send + Sync` bounds on output/delegate traits (critical for tokio integration). |
| **cpal** | `0.15` | Fallback mic input (only if SCK mic capture is awkward) | Optional. SCK can do mic, but `cpal` is the boring, battle-tested CoreAudio path. Has a known PR (#894) tracking ScreenCaptureKit loopback support — watch but don't depend on. |
| **hound** or **symphonia** | `3.5` / `0.5` | PCM file write (for opt-in audio retention v1.1) | Only needed once we add per-meeting retain toggle; defer. |

**SCK risk note (re-affirming PRD §13):** the `screencapturekit` crate's audio-loopback surface is its youngest. If Phase 2 spike reveals gaps — partial frames, audio session lifecycle quirks, multi-channel weirdness — the documented fallback is a ~150-line Swift binary invoked over a Unix socket. Architecture change is local to `yogurt-audio`; nothing else moves.

### Core — Speech-to-Text

| Technology | Pin | Purpose | Why |
|------------|-----|---------|-----|
| **whisper-rs** | `0.16` | Local whisper.cpp bindings | Current stable (0.16.0). Exposes `metal` and `coreml` feature flags. Use `metal` baseline; `coreml` can be a Phase 8 follow-up if M1/M3 latency on `small.en` is tight. |
| **whisper.cpp** | bundled via whisper-rs (pins ggml ~b3500+ era) | Inference engine | Static-linked through whisper-rs build script. |
| **deepgram** (Rust SDK) | `0.6.x` (latest community) | Cloud streaming STT default | Community-maintained but actively developed; v1.0 announced as "coming soon" as of mid-2026. WebSocket streaming supported. **Risk:** pre-1.0 churn — keep adapter behind the `Stt` trait so swap-out is a one-file change. |
| `tokio-tungstenite` (already listed) | — | Fallback: hand-rolled Deepgram WS | If `deepgram` crate misbehaves, Deepgram's live API is a documented JSON-over-WS protocol — a custom client is ~200 LOC. |

### Core — LLM Client

| Technology | Pin | Purpose | Why |
|------------|-----|---------|-----|
| **async-openai** | `0.36` (latest stable) | OpenAI + OpenAI-compatible HTTP client | Official-spec-derived crate. Supports custom `base_url` via `OpenAIConfig`, supports SSE streaming, supports `Box<dyn Config>` for per-provider configs. This is the single library that satisfies "one adapter covers ~10 providers" (Minimax, OpenAI, Ollama, LM Studio, OpenRouter, Groq, vLLM, llama.cpp server, Together, Fireworks). |
| **eventsource-stream** | `0.2` | SSE parsing (transitive) | Used by async-openai for chat completion streams. |

### Core — Storage & Secrets

| Technology | Pin | Purpose | Why |
|------------|-----|---------|-----|
| **rusqlite** | `0.40` with `bundled` feature | SQLite | Current rusqlite 0.40.x with bundled libsqlite3-sys 0.38.x ships SQLite 3.53.2 — fully self-contained, no system SQLite dependency. `bundled` is mandatory for the single-binary distribution. |
| **rusqlite_migration** | `1.3` | Schema migrations | Simple, idiomatic, fits PRD §9's migrations folder. |
| **keyring** + **apple-native-keyring-store** | `keyring` 3.x (using `keyring-core` 0.7+) | macOS Keychain | The `keyring` crate has been refactored as of 2026 — the high-level API lives in `keyring-core`, and platform stores like `apple-native-keyring-store` (last updated 2026-04) are the actual credential backends. Use the `keychain` module for command-line apps. Last touched May 2026 — actively maintained. |
| **directories** or **etcetera** | `5.x` / `0.8` | Resolve `~/.yogurt/` | `directories` is the boring standard; `etcetera` is the newer XDG-correct alternative. Either works. |
| **rust-embed** | `8.x` (latest 8.8.0+) | Embed `web/dist` into binary | The standard solution. Works with axum via documented `examples/axum.rs` pattern. Alternative crates exist (`static-serve`, `axum-embed-files`) but offer no advantages for our scale. |

### Core — Frontend

| Technology | Pin | Purpose | Why |
|------------|-----|---------|-----|
| **React** | `19.x` | UI framework | React 19 is stable for client-side production use. **Do not use Server Components** — we're a localhost SPA with no SSR layer. Use plain React 19 + client rendering. |
| **Vite** | `7.x` (current stable) | Dev server + bundler | Vite 7 (June 2025) is the recommended stable. Vite 8 (Rolldown-based, May 2026) is faster but architecturally new — wait for a 8.1+ before betting on it. Vite 7 has security patches backported. |
| **TipTap** | `3.x` (current stable) | Headless rich-text editor | Tiptap 3 graduated in 2025–2026 with the SSR-safe core, mobile touch fixes, and the **official Markdown extension** (bidirectional parse/serialize via `editor.markdown.parse()` / `editor.markdown.serialize()`). This is critical — v1 was missing first-class markdown round-trip. |
| **@tiptap/extension-markdown** | (matches TipTap 3 minor) | Official markdown round-trip | Replaces the community `aguingand/tiptap-markdown` crutch. Built into the v3 line. |
| **Tailwind CSS** | `4.3` (current stable) | Styling | Tailwind v4 GA early 2025, on 4.3 by May 2026, 3.78x faster builds vs v3. Automatic class detection (no `content` array). New project in 2026 has zero reason to start on v3. |
| **TypeScript** | `5.6+` | Frontend language | Standard for React 19 + Vite. |
| **pnpm** | `9.x` | Frontend package manager | PRD §11 calls for `pnpm`. Faster + stricter than npm; less bloat than yarn. |

### Supporting Frontend Libraries

| Library | Pin | Purpose | When to Use |
|---------|-----|---------|-------------|
| **@tiptap/starter-kit** | matches v3 | Base TipTap extensions (paragraph, bold, etc.) | Always — saves wiring 12 extensions individually. |
| **@tanstack/react-query** | `5.x` | Server state, WebSocket cache | Use for the REST endpoints; combine with raw WS for streams. |
| **zustand** | `5.x` | Lightweight client state | Editor UI state, settings panel, transcript dock open/close. Smaller surface than Redux; no provider gymnastics. |
| **lucide-react** | `0.450+` | Icon set | PRD §16.9 explicitly lists Lucide as the candidate. Tree-shakable, matches the editorial-tooly aesthetic. |
| **react-router-dom** | `6.x` (stay on v6, not v7) | Routing for `/welcome`, `/settings`, library | v6 is mature; v7 is a rebrand of Remix and overkill here. |
| **clsx** + **tailwind-merge** | `2.x` / `2.x` | Conditional classnames | Standard Tailwind companions. |
| **ws** (browser native) | n/a | WebSocket transport | Browser-native `WebSocket`; no library needed. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| **cargo-watch** or **bacon** | Rust hot rebuild | `bacon` is newer and prettier; either works. |
| **cargo-nextest** | Test runner | Faster than `cargo test`, especially in CI. |
| **cargo-deny** | License + advisory checks | Useful for MIT compliance audit on transitive deps. |
| **typos** | Spell-check in CI | Cheap; catches embarrassing UI copy errors. |
| **biome** or **eslint + prettier** | Frontend lint/format | Biome is faster and unified; eslint+prettier is what most contributors expect. Pick biome for speed; document it. |
| **cargo-bundle-licenses** | Generate `THIRD_PARTY_LICENSES.md` | Needed before Homebrew submission. |

### Distribution & Packaging

| Tool | Purpose | Notes |
|------|---------|-------|
| **cargo-dist** | Cross-platform release builder | Modern Rust-CLI standard for GitHub Release tarballs + Homebrew formula. Replaces hand-rolled release.yml. |
| **GitHub Actions** | CI matrix | `aarch64-apple-darwin` + `x86_64-apple-darwin` matrix. |
| **lipo** (macOS native) | Universal binary | Combine the two arch tarballs into one `yogurt` universal binary. |
| **Homebrew tap** | `homebrew-yogurt` | Auto-PR via cargo-dist. |

---

## Installation Sketch

### Rust workspace `Cargo.toml` (root)

```toml
[workspace]
members = [
  "crates/yogurt-cli",
  "crates/yogurt-server",
  "crates/yogurt-audio",
  "crates/yogurt-stt",
  "crates/yogurt-llm",
  "crates/yogurt-db",
  "crates/yogurt-notes",
  "crates/yogurt-prompts",
]
resolver = "2"

[workspace.dependencies]
tokio = { version = "1.51", features = ["full"] }            # LTS
axum = { version = "0.8", features = ["ws", "macros"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "compression-gzip"] }
tokio-tungstenite = { version = "0.24", features = ["rustls-tls-native-roots"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
thiserror = "1"
ulid = "1"
time = { version = "0.3", features = ["serde", "macros"] }

# Audio
screencapturekit = "2"                                       # macOS-only
cpal = "0.15"                                                # optional fallback

# STT
whisper-rs = { version = "0.16", features = ["metal"] }
deepgram = "0.6"

# LLM
async-openai = "0.36"

# Storage + secrets
rusqlite = { version = "0.40", features = ["bundled"] }
rusqlite_migration = "1.3"
keyring = "3"
apple-native-keyring-store = "0.1"                           # confirm exact pin at Phase 5
directories = "5"

# Embed assets
rust-embed = { version = "8", features = ["axum"] }
```

### Frontend `web/package.json`

```jsonc
{
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "react-router-dom": "^6.26.0",
    "@tiptap/react": "^3.0.0",
    "@tiptap/starter-kit": "^3.0.0",
    "@tiptap/extension-markdown": "^3.0.0",
    "@tanstack/react-query": "^5.0.0",
    "zustand": "^5.0.0",
    "lucide-react": "^0.450.0",
    "clsx": "^2.1.0",
    "tailwind-merge": "^2.5.0"
  },
  "devDependencies": {
    "vite": "^7.0.0",
    "@vitejs/plugin-react": "^4.3.0",
    "tailwindcss": "^4.3.0",
    "@tailwindcss/vite": "^4.3.0",
    "typescript": "^5.6.0"
  }
}
```

---

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative | Why Not Now |
|-------------|-------------|-------------------------|-------------|
| `axum` 0.8 | `actix-web` 4.x | If you need bleeding-edge throughput | Tokio-stack consistency wins; we'd lose tower middleware. |
| `axum` 0.8 | `rocket` 0.5 | If you want batteries-included DX | Async story is worse; weaker WS integration. |
| `screencapturekit` crate | Swift sidecar binary | If Rust crate's audio loopback API has gaps in Phase 2 spike | Adds a build artifact + IPC; only fall back if needed. |
| `whisper-rs` | `whisperkit` Swift sidecar | If we need ANE acceleration | Pulls in Swift toolchain; whisper-rs `metal` is fast enough on M-series for v1. |
| `async-openai` | hand-rolled `reqwest` + SSE | If we needed a custom protocol | async-openai's `OpenAIConfig::with_api_base()` already handles every OpenAI-compatible provider. |
| `deepgram` crate | hand-rolled `tokio-tungstenite` WS client | If SDK has bugs or stalls before 1.0 | Pre-1.0 churn risk is real; build the adapter behind the `Stt` trait to keep swap cheap. |
| Vite 7 | Vite 8 (Rolldown) | When 8.1+ ships and ecosystem catches up | 10-30x faster builds, but plugin ecosystem is too fresh for a v1 ship. |
| Tiptap 3 | ProseMirror directly | If TipTap mark system can't express black/grey diffing (PRD §13 fallback) | TipTap is a thin layer over ProseMirror; falling through is straightforward and we can do it incrementally. |
| Tailwind v4 | vanilla CSS modules | If we hated utility-first | Design board is utility-friendly; v4's speed is a productivity win during phase 1 design-system buildout. |
| `rust-embed` | `include_dir` or `static-serve` | If we needed compression baked in | rust-embed has the longest production track record; tower-http handles compression at serve time anyway. |
| `rusqlite` + `bundled` | `sqlx` with SQLite | If we wanted compile-time SQL checks | sqlx adds a build-time DB connection requirement and is awkward inside a single static binary. Stick with rusqlite. |
| `keyring` (refactored) | `security-framework` crate directly | If keyring refactor (2026) trips us up | We get cross-platform-correct API for free; only drop down if Keychain features we need (synchronization, ACL prompts) aren't exposed. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| **Tauri** | Wraps a webview + adds an event bus. PRD §2 explicitly defers menu-bar/Tauri to v2. Wrapping now means second build target and an extra IPC layer for zero v1 benefit. | Plain `axum` + browser at `localhost:7878`. |
| **Electron** | Bundles Chromium per app (~150MB). Killer for "single static binary distribution" pitch. Node memory tax. | Same — axum + browser. |
| **Wry / native webview** | Same architectural overhead as Tauri without Tauri's polish. | Same — axum + browser. |
| **BlackHole / virtual audio drivers** | Requires user to install kernel-adjacent software. Granola explicitly avoided this. ScreenCaptureKit is the supported Apple path. | `screencapturekit` crate (with Swift sidecar fallback). |
| **`getUserMedia` in browser** | Browser would prompt for *mic* permission separately from the backend's Screen Recording prompt. Confuses users. PRD §13 already flagged this. | All audio captured by the Rust binary; browser only renders UI. |
| **`sqlx`** | Compile-time DB connection requirement; awkward inside a build that ships as a static binary. | `rusqlite` with `bundled`. |
| **`reqwest-eventsource` for LLM streaming** | Reinventing what async-openai already does correctly. | `async-openai` streaming. |
| **Next.js / SSR React** | Localhost SPA — server-rendering nothing, hydrating nothing. Adds 100KB+ runtime for no win. | Plain React 19 + Vite. |
| **Vite 8** | Just shipped (May 2026) with Rolldown — plugin ecosystem hasn't caught up. | Vite 7 stable until 8.1+. |
| **Tailwind v3** | v4 is GA, faster, simpler — no reason to start a 2026 project on v3. | Tailwind 4.3+. |
| **Tiptap v2** | v3 has official Markdown extension, SSR safety, mobile-touch fixes. | Tiptap 3.x. |
| **`time` crate v0.2 / `chrono`** | `time` 0.3 is the boring stable answer; `chrono` has had RFC churn. | `time` 0.3 (or `jiff` 0.2 if you're feeling adventurous). |
| **community `aguingand/tiptap-markdown`** | Superseded by Tiptap's official markdown extension in v3. | `@tiptap/extension-markdown`. |
| **`yew` / `leptos` / Rust frontend** | Single-developer OSS project — debugging a Rust frontend at 3am the day before launch is a poor use of time. | React 19. |

---

## Stack Patterns by Variant

**Default user (cloud STT, BYO LLM key):**
- `deepgram` Rust SDK over WebSocket
- `async-openai` against whichever provider's `base_url` the user pasted
- Audio routed: `screencapturekit` → tokio broadcast → Deepgram WS adapter

**Privacy user (full local):**
- `whisper-rs` with `metal` feature, `small.en` baseline
- `async-openai` pointed at `http://localhost:11434/v1` (Ollama) or `http://localhost:1234/v1` (LM Studio)
- Same audio path, different STT adapter

**Power user (self-hosted LLM, cloud STT):**
- Mixed: Deepgram for streaming partials, vLLM/llama.cpp-server endpoint for LLM
- Same code, different settings

The trait boundary (`yogurt-stt`) and provider config (`yogurt-llm`) make all three configs the same binary.

---

## Version Compatibility Matrix

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| axum 0.8 | tokio 1.x, tower 0.5, tower-http 0.6 | tower-http 0.5 is also fine; 0.6 has nicer trace defaults. |
| whisper-rs 0.16 | macOS 13+, Metal feature flag | CoreML feature exists but adds Swift/CoreML model conversion step; skip in v1. |
| async-openai 0.36 | reqwest 0.12, tokio 1.x | Pulls in `eventsource-stream` for SSE. |
| screencapturekit 2.x | macOS 13.0+ (audio loopback), macOS 14+ for direct-to-file, macOS 15+ for newer APIs | We only need the 13+ audio surface for v1. |
| rusqlite 0.40 + bundled | SQLite 3.53.2 (vendored via libsqlite3-sys 0.38) | Self-contained build, no system SQLite. |
| Tiptap 3 / @tiptap/extension-markdown | React 18 or 19, ProseMirror 1.x | React 19 works; verify in phase 1 spike. |
| Vite 7 | Node 20.19+ | Drop Node 18 support — already EOL. |
| Tailwind 4.3 | Vite 7, no `tailwind.config.js` required | New CSS-first config; design tokens go in `@theme` block. |

---

## Risk Register (Stack-Specific)

| Risk | Mitigation | Owner |
|------|------------|-------|
| `screencapturekit` v2.x audio API still maturing | Phase 2 spike before building dependent features; Swift sidecar fallback documented in PRD §13 | yogurt-audio crate |
| `deepgram` Rust SDK is community pre-1.0 | Hide behind `Stt` trait; `tokio-tungstenite` hand-roll as fallback (~200 LOC) | yogurt-stt crate |
| `keyring` ecosystem mid-refactor (apple-native-keyring-store split out 2026) | Confirm exact pin combo in Phase 5; spike a write + read against macOS Keychain before integrating into settings UI | yogurt-server crate |
| TipTap mark system may struggle with structural diffing of LLM-rewritten markdown | Phase 4 prototype against three real transcripts (PRD §13); ProseMirror direct fallback if marks can't carry the diff | yogurt-notes + web |
| Tailwind v4 design-token migration (CSS-first config) | Phase 1 design-system phase already budgeted; design tokens from PRD §16 fit v4's `@theme` block naturally | web |
| Universal binary size with `whisper.cpp` + `rust-embed` + bundled SQLite | Expect 30-60MB stripped; acceptable for `brew install`. Static linking is non-negotiable. | release pipeline |

---

## Sources

### Verified (HIGH confidence)
- [axum on crates.io](https://crates.io/crates/axum) — version line, download count
- [tokio releases](https://github.com/tokio-rs/tokio/releases) — LTS schedule (1.47 LTS until 2026-09, 1.51 LTS until 2027-03), 1.52.3 current stable
- [Vite 7 release blog](https://vite.dev/blog/announcing-vite7) — June 2025 release, Node 20.19+ requirement
- [Vite 8 release blog](https://vite.dev/blog/announcing-vite8) — May 2026, Rolldown-based, ecosystem caveat
- [Tailwind CSS v4.0 blog](https://tailwindcss.com/blog/tailwindcss-v4) — GA, 4.3 current stable May 2026
- [Tiptap 3.0 announcement](https://tiptap.dev/tiptap-editor-v3) and [What's new](https://tiptap.dev/docs/resources/whats-new) — v3 stable, SSR fix, mobile touch
- [Tiptap Markdown docs](https://tiptap.dev/docs/editor/markdown) — official bidirectional markdown extension, parse/serialize API
- [async-openai on crates.io](https://crates.io/crates/async-openai) and [docs.rs 0.36](https://docs.rs/crate/async-openai/latest) — current 0.36.0, base_url config pattern, SSE streaming
- [whisper-rs 0.16 features](https://docs.rs/crate/whisper-rs/latest/features) — metal + coreml feature flags
- [rusqlite 0.40 / libsqlite3-sys 0.38.1](https://docs.rs/crate/rusqlite/latest) — bundled SQLite 3.53.2
- [rust-embed 8.8.0 axum example](https://docs.rs/crate/rust-embed/latest/source/examples/axum.rs) — canonical pattern
- [keyring on crates.io](https://crates.io/crates/keyring) and [apple-native-keyring-store](https://crates.io/crates/apple-native-keyring-store) — 2026 refactor: keyring-core 0.7+, platform store split, last updates May/Apr 2026
- [React 19 server-components blog](https://react.dev/blog/2024/12/05/react-19) — stable, but we use plain CSR
- [Granola architecture (PRD Appendix A)](https://github.com/jarvisrchen/yogurt/blob/main/docs/PRD.md) — confirms ScreenCaptureKit + Deepgram + OpenAI/Anthropic pattern

### Verified (MEDIUM confidence)
- [screencapturekit on crates.io](https://crates.io/crates/screencapturekit) and [docs.rs 1.5](https://docs.rs/crate/screencapturekit/1.5.0) — v2.x has Send+Sync bounds, macOS 13+ audio support; audio-loopback API is the youngest surface
- [Deepgram Rust SDK GitHub](https://github.com/deepgram/deepgram-rust-sdk) and [crate](https://crates.io/crates/deepgram) — community-owned, "1.0 coming soon" as of mid-2026
- [CPAL](https://github.com/RustAudio/cpal) — requires Rust 1.80+, CoreAudio backend stable; SCK loopback PR #894 still open

### Reference / context
- [Whisper STT on Apple Silicon 2026 benchmarks](https://www.promptquorum.com/local-llms/apple-silicon-whisper-metal-benchmark) — confirms whisper.cpp Metal at 10-12x real-time on M5 Pro / 5-7x on M3 Max for large-v3
- [WhisperKit v1.0](https://whipscribe.com/tools/whisperkit) — alternative path (ANE optimization), explicitly considered and rejected for v1

---

*Stack research for: macOS local-first meeting copilot (Granola alternative)*
*Researched: 2026-06-25*
*Confidence overall: HIGH on locked choices; MEDIUM on `screencapturekit` audio-loopback maturity and `deepgram` Rust SDK pre-1.0 status — both mitigated by trait boundaries and documented fallbacks.*
