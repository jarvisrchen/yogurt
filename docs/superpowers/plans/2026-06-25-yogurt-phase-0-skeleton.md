# Yogurt v1 — Phase 0: Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up a runnable `yogurt start` command that boots an axum server on `localhost:7878` and serves a React/Vite/Tailwind/TipTap web app — first in dev mode (Vite hot-reload proxy), then bundled into a single binary via `rust-embed` for release.

**Architecture:** Cargo workspace at the repo root with `yogurt-cli` (binary entry point) and `yogurt-server` (axum HTTP + WS layer) crates. A `web/` directory holds the Vite-built React app. In dev mode the server proxies non-API routes to Vite at `:5173`; in release mode it serves `web/dist` embedded into the binary via `rust-embed`. This phase ships scaffolding only — no audio, no STT, no LLM, no persistence.

**Tech Stack:** Rust 1.83+ · axum 0.8 · tokio 1 · clap 4 · rust-embed 8 · tower / tower-http · React 19 · Vite 6 · TypeScript 5.6 · Tailwind 4 (CDN in dev, build in release) · TipTap 2 · pnpm 9

**Reference:** `docs/PRD.md` §7 (architecture), §8 (component breakdown), §11 (distribution & dev workflow), §16 (design tokens — applied in Phase 1, not here).

**Out of scope (deferred to later phase plans):**
- Audio capture (Phase 2)
- STT, LLM, settings UI, notes editor, library, onboarding (Phases 3–7)
- Full design-system component build-out (Phase 1)
- Tailwind 4 + DaisyUI / brand tokens (Phase 1 — Phase 0 ships unstyled scaffold)

---

## File structure produced by this phase

```
yogurt/
├── Cargo.toml                          # NEW · workspace root
├── .gitignore                          # MODIFY · add target/, node_modules/, etc.
├── README.md                           # NEW · install + dev quickstart
├── rust-toolchain.toml                 # NEW · pin Rust 1.83
├── crates/
│   ├── yogurt-cli/
│   │   ├── Cargo.toml                  # NEW
│   │   ├── src/
│   │   │   ├── main.rs                 # NEW · clap subcommand router
│   │   │   └── commands/
│   │   │       └── start.rs            # NEW · `yogurt start` impl
│   │   └── tests/
│   │       └── cli.rs                  # NEW · integration: `yogurt --help`
│   └── yogurt-server/
│       ├── Cargo.toml                  # NEW
│       ├── build.rs                    # NEW · noop placeholder for embedded assets
│       └── src/
│           ├── lib.rs                  # NEW · pub run(addr, mode) async
│           ├── routes.rs               # NEW · axum router with GET / and GET /api/health
│           ├── assets.rs               # NEW · rust-embed Asset struct + serve_embedded
│           └── dev_proxy.rs            # NEW · proxy non-API routes to Vite at :5173
├── web/
│   ├── package.json                    # NEW
│   ├── pnpm-lock.yaml                  # AUTO-GENERATED
│   ├── tsconfig.json                   # NEW
│   ├── vite.config.ts                  # NEW · proxy /api + /ws to :7878
│   ├── index.html                      # NEW · Vite entrypoint
│   ├── src/
│   │   ├── main.tsx                    # NEW · React root
│   │   ├── App.tsx                     # NEW · "hello yogurt" + TipTap demo
│   │   ├── index.css                   # NEW · minimal global CSS (Phase 1 replaces)
│   │   └── lib/
│   │       └── api.ts                  # NEW · fetch wrapper for /api/health
│   └── dist/                           # AUTO-GENERATED on `pnpm build` (gitignored)
└── docs/
    └── PRD.md                          # ALREADY EXISTS
```

**Why this split:** `yogurt-cli` is the entry point (binary target). `yogurt-server` is a library that the CLI calls into — this separation makes the server testable without spinning up the CLI, and prepares for future testing of the server in isolation.

---

## Test conventions established in this phase

- **Rust unit tests:** `#[cfg(test)] mod tests` inside the source file under test.
- **Rust integration tests:** `crates/<crate>/tests/<name>.rs` — one file per logical area. Use `assert_cmd` for CLI invocation, `reqwest` + `tokio::test` for HTTP.
- **Frontend unit tests:** Vitest (`web/src/**/*.test.ts(x)`) — set up but no tests yet beyond a smoke `App.test.tsx`.
- **No E2E in this phase.** Playwright comes in Phase 7 or later.
- **Test naming:** `it_<does_thing>` for Rust, `it('<does thing>', ...)` for Vitest.

---

## Phase 0 task list

10 tasks. Each task ends with a commit. Approximate sequence: ~6–8 hours of focused work.

---

### Task 0.1 · Initialize Cargo workspace + .gitignore + rust-toolchain

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `rust-toolchain.toml`
- Modify: `.gitignore`

- [ ] **Step 1: Inspect current state.**

Run: `ls -a` and `cat .gitignore`
Expected: `.git/`, `.lavish/`, `docs/`, `yogurt-app-design/`, `.gitignore` (~94B). No `Cargo.toml`, no `target/`.

- [ ] **Step 2: Write `Cargo.toml` (workspace root).**

```toml
[workspace]
resolver = "2"
members = [
    "crates/yogurt-cli",
    "crates/yogurt-server",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Jarvis Chen <3chenr@gmail.com>"]
license = "MIT"
repository = "https://github.com/jarvisrchen/yogurt"
rust-version = "1.83"

[workspace.dependencies]
# async runtime
tokio = { version = "1.42", features = ["full"] }
# HTTP server
axum = { version = "0.8", features = ["macros"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["fs", "trace"] }
# CLI
clap = { version = "4.5", features = ["derive"] }
# logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
# embedding assets
rust-embed = { version = "8.5", features = ["mime-guess"] }
# helpers
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# HTTP client (used by dev_proxy + tests)
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }
# mime guessing for embedded assets
mime_guess = "2"
# testing
assert_cmd = "2"

[profile.release]
lto = "thin"
codegen-units = 1
strip = true
```

- [ ] **Step 3: Write `rust-toolchain.toml`.**

```toml
[toolchain]
channel = "1.83"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 4: Expand `.gitignore`.**

First check what's already there — the Vercel-style `.gitignore` shipped at repo creation already covers `.env`, `.env.local`, `.env*.local`, `.next/`, `dist/`, `build/`, `node_modules/`. Don't duplicate; append only what's missing:

```gitignore

# Rust
/target/
**/*.rs.bk

# pnpm + frontend (node_modules already there)
.pnpm-store/

# Lavish artifacts
.lavish/
```

> **⚠ Verify env protection.** After appending, run `git check-ignore -v .env.local` — must report a match (e.g. `.gitignore:5:.env*.local`). The user's Minimax API key lives in `.env.local`; if this check fails, fix the gitignore before any commit.

- [ ] **Step 5: Verify workspace parses (will fail — no crates yet).**

Run: `cargo metadata --no-deps`
Expected: error about missing `crates/yogurt-cli/Cargo.toml`. **This is fine** — it confirms the workspace is reading our config.

- [ ] **Step 6: Commit.**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore
git commit -m "chore: init cargo workspace + rust 1.83 toolchain"
```

---

### Task 0.2 · `yogurt-cli` crate skeleton with `--help`

**Files:**
- Create: `crates/yogurt-cli/Cargo.toml`
- Create: `crates/yogurt-cli/src/main.rs`
- Create: `crates/yogurt-cli/src/commands/mod.rs`
- Create: `crates/yogurt-cli/src/commands/start.rs`
- Create: `crates/yogurt-cli/tests/cli.rs`

- [ ] **Step 1: Write `crates/yogurt-cli/Cargo.toml`.**

```toml
[package]
name = "yogurt"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Local-first meeting copilot — Granola's UX, your machine."

[[bin]]
name = "yogurt"
path = "src/main.rs"

[dependencies]
tokio = { workspace = true }
clap = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
anyhow = { workspace = true }

[dev-dependencies]
assert_cmd = { workspace = true }
```

- [ ] **Step 2: Write the failing CLI integration test.**

Create `crates/yogurt-cli/tests/cli.rs`:

```rust
use assert_cmd::Command;

#[test]
fn it_prints_help() {
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.arg("--help");
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(stdout.contains("yogurt"), "help should mention the binary name");
    assert!(stdout.contains("start"), "help should mention the `start` subcommand");
}
```

- [ ] **Step 3: Run the test — expect compile failure (no main.rs yet).**

Run: `cargo test -p yogurt --test cli`
Expected: `error[E0463]: can't find crate for 'main'` or similar — the binary doesn't exist yet.

- [ ] **Step 4: Write `crates/yogurt-cli/src/commands/mod.rs`.**

```rust
pub mod start;
```

- [ ] **Step 5: Write `crates/yogurt-cli/src/commands/start.rs` (stub).**

```rust
use anyhow::Result;

pub async fn run() -> Result<()> {
    println!("yogurt start: not yet wired (task 0.4)");
    Ok(())
}
```

- [ ] **Step 6: Write `crates/yogurt-cli/src/main.rs`.**

```rust
mod commands;

use clap::{Parser, Subcommand};

/// yogurt — local-first meeting copilot.
#[derive(Parser, Debug)]
#[command(name = "yogurt", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Launch the local server and open the browser.
    Start,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yogurt=info,yogurt_server=info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Cmd::Start => commands::start::run().await,
    }
}
```

- [ ] **Step 7: Run the test — expect PASS.**

Run: `cargo test -p yogurt --test cli`
Expected: `test it_prints_help ... ok`

- [ ] **Step 8: Manually verify.**

Run: `cargo run -p yogurt -- --help`
Expected: prints `yogurt — local-first meeting copilot.` and lists `start` as a subcommand.

Run: `cargo run -p yogurt -- start`
Expected: prints `yogurt start: not yet wired (task 0.4)` and exits cleanly.

- [ ] **Step 9: Commit.**

```bash
git add crates/yogurt-cli/
git commit -m "feat(cli): add yogurt binary with start subcommand stub"
```

---

### Task 0.3 · `yogurt-server` crate with axum hello-world

**Files:**
- Create: `crates/yogurt-server/Cargo.toml`
- Create: `crates/yogurt-server/build.rs`
- Create: `crates/yogurt-server/src/lib.rs`
- Create: `crates/yogurt-server/src/routes.rs`

- [ ] **Step 1: Write `crates/yogurt-server/Cargo.toml`.**

```toml
[package]
name = "yogurt-server"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
tokio = { workspace = true }
axum = { workspace = true }
tower = { workspace = true }
tower-http = { workspace = true }
tracing = { workspace = true }
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
rust-embed = { workspace = true }
reqwest = { workspace = true }

[dev-dependencies]
tokio = { workspace = true }
reqwest = { workspace = true }
```

- [ ] **Step 2: Write `crates/yogurt-server/build.rs` (placeholder; real embedding wired in Task 0.7).**

```rust
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
}
```

- [ ] **Step 3: Write the failing health-check test.**

Create `crates/yogurt-server/tests/health.rs`:

```rust
use std::time::Duration;

#[tokio::test]
async fn it_responds_to_health() {
    let addr = "127.0.0.1:17878".parse().unwrap();
    let mode = yogurt_server::Mode::Release;

    // Spawn the server.
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, mode).await
    });

    // Give it a moment to bind.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17878/api/health")
        .await
        .expect("server reachable")
        .json::<serde_json::Value>()
        .await
        .expect("valid JSON");

    assert_eq!(body["status"], "ok");
    handle.abort();
}
```

- [ ] **Step 4: Run the test — expect compile failure (lib doesn't exist).**

Run: `cargo test -p yogurt-server --test health`
Expected: `error[E0432]: unresolved import 'yogurt_server'`

- [ ] **Step 5: Write `crates/yogurt-server/src/routes.rs`.**

```rust
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
}

async fn index() -> &'static str {
    "hello yogurt — phase 0 scaffold (web UI coming in task 0.5)"
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}
```

- [ ] **Step 6: Write `crates/yogurt-server/src/lib.rs`.**

```rust
mod routes;

use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Server runtime mode.
///
/// In `Dev`, non-API requests proxy to a Vite dev server on :5173 (task 0.8).
/// In `Release`, non-API requests serve embedded `web/dist` assets (task 0.7).
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Dev,
    Release,
}

pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    let app = routes::router();
    tracing::info!(?addr, ?mode, "yogurt-server starting");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 7: Run the health test — expect PASS.**

Run: `cargo test -p yogurt-server --test health`
Expected: `test it_responds_to_health ... ok`

- [ ] **Step 8: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): add axum scaffold with health endpoint"
```

---

### Task 0.4 · Wire `yogurt start` → axum server, with browser open

**Files:**
- Modify: `crates/yogurt-cli/Cargo.toml` (add yogurt-server + open dependency)
- Modify: `crates/yogurt-cli/src/commands/start.rs`

- [ ] **Step 1: Add runtime + dev dependencies to `crates/yogurt-cli/Cargo.toml`.**

Append to `[dependencies]`:

```toml
yogurt-server = { path = "../yogurt-server" }
open = "5"
```

(Keep `open` as a single-crate dep — no workspace-level declaration needed for one consumer.)

Append to `[dev-dependencies]` (these are required by the integration test added in Step 2 — make sure they're declared before writing the test):

```toml
reqwest = { workspace = true }
# `tokio` is already in [dependencies] with the `"full"` feature, which covers
# `tokio::process` and `tokio::time`. Listing it again under [dev-dependencies]
# is unnecessary — `cargo test` sees the runtime dep transitively.
```

- [ ] **Step 2: Write the failing CLI integration test for `start`.**

Append to `crates/yogurt-cli/tests/cli.rs`:

```rust
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn it_starts_server_and_serves_health() {
    // Spawn `yogurt start` in the background.
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args(["start", "--port", "17879", "--no-open"])
        .spawn()
        .expect("spawn yogurt");

    tokio::time::sleep(Duration::from_millis(400)).await;

    let body = reqwest::get("http://127.0.0.1:17879/api/health")
        .await
        .expect("server reachable")
        .text()
        .await
        .unwrap();
    assert!(body.contains("\"status\":\"ok\""));

    child.kill().await.ok();
}
```

(Add to `[dev-dependencies]`: `tokio = { workspace = true }`, `reqwest = { workspace = true }`.)

- [ ] **Step 3: Run — expect compile failure (no `--port`, no `--no-open`).**

Run: `cargo test -p yogurt --test cli it_starts_server_and_serves_health`
Expected: argument parsing error.

- [ ] **Step 4: Update `crates/yogurt-cli/src/main.rs` to accept Start flags.**

Replace the `Cmd` enum and the matching arm with:

```rust
#[derive(Subcommand, Debug)]
enum Cmd {
    /// Launch the local server and open the browser.
    Start(StartArgs),
}

#[derive(clap::Args, Debug)]
struct StartArgs {
    /// TCP port to bind.
    #[arg(long, default_value_t = 7878)]
    port: u16,
    /// Do not auto-open the browser on start.
    #[arg(long)]
    no_open: bool,
    /// Run in dev mode (proxies non-API routes to Vite on :5173).
    #[arg(long)]
    dev: bool,
}

// ... in main():
match cli.command {
    Cmd::Start(args) => commands::start::run(args).await,
}
```

You will also need to pass `StartArgs` through — make `commands::start::run` accept `crate::StartArgs` (or re-define a local mirror struct to avoid the cross-module visibility wrinkle; either is fine, the local mirror is cleaner):

Make `commands::start::run` look like:

```rust
use anyhow::Result;
use std::net::SocketAddr;
use yogurt_server::Mode;

pub struct StartArgs {
    pub port: u16,
    pub no_open: bool,
    pub dev: bool,
}

pub async fn run(args: StartArgs) -> Result<()> {
    let addr: SocketAddr = ([127, 0, 0, 1], args.port).into();
    let mode = if args.dev { Mode::Dev } else { Mode::Release };
    let url = format!("http://127.0.0.1:{}", args.port);

    if !args.no_open {
        // Open in a background task so a failure to spawn the browser doesn't
        // block the server from starting.
        let url_for_open = url.clone();
        tokio::spawn(async move {
            if let Err(e) = open::that(&url_for_open) {
                tracing::warn!(?e, "failed to open browser");
            }
        });
    }

    tracing::info!(%url, "yogurt is starting");
    yogurt_server::run(addr, mode).await
}
```

And in `main.rs`, convert the clap-defined `StartArgs` to the command-module mirror before calling `run`:

```rust
Cmd::Start(args) => commands::start::run(commands::start::StartArgs {
    port: args.port,
    no_open: args.no_open,
    dev: args.dev,
}).await,
```

- [ ] **Step 5: Run the test again — expect PASS.**

Run: `cargo test -p yogurt --test cli it_starts_server_and_serves_health -- --nocapture`
Expected: `test it_starts_server_and_serves_health ... ok`

- [ ] **Step 6: Manual smoke.**

Run: `cargo run -p yogurt -- start --no-open`
Expected: server logs "yogurt is starting" then "yogurt-server starting"; `curl localhost:7878/api/health` returns `{"status":"ok","service":"yogurt-server"}`. Hit Ctrl-C to stop.

- [ ] **Step 7: Commit.**

```bash
git add Cargo.toml crates/yogurt-cli/
git commit -m "feat(cli): wire yogurt start to axum server with --port/--no-open/--dev flags"
```

---

### Task 0.5 · Vite + React + TypeScript + Tailwind scaffold

**Files:**
- Create: `web/package.json`
- Create: `web/tsconfig.json`
- Create: `web/vite.config.ts`
- Create: `web/index.html`
- Create: `web/src/main.tsx`
- Create: `web/src/App.tsx`
- Create: `web/src/index.css`
- Create: `web/src/lib/api.ts`

- [ ] **Step 1: Confirm pnpm is installed.**

Run: `pnpm --version`
Expected: `9.x`. If missing: `brew install pnpm`.

- [ ] **Step 2: Write `web/package.json`.**

```json
{
  "name": "yogurt-web",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "dependencies": {
    "@tiptap/core": "^2.10.0",
    "@tiptap/react": "^2.10.0",
    "@tiptap/starter-kit": "^2.10.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@tailwindcss/vite": "^4.0.0",
    "@testing-library/react": "^16.1.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.0",
    "jsdom": "^25.0.0",
    "tailwindcss": "^4.0.0",
    "typescript": "^5.6.0",
    "vite": "^6.0.0",
    "vitest": "^2.1.0"
  }
}
```

- [ ] **Step 3: Write `web/tsconfig.json`.**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "esModuleInterop": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "resolveJsonModule": true,
    "allowImportingTsExtensions": false,
    "noEmit": true,
    "types": ["vitest/globals"]
  },
  "include": ["src", "vite.config.ts"]
}
```

- [ ] **Step 4: Write `web/vite.config.ts`.**

The first line is a triple-slash reference — without it, TypeScript won't recognize the `test:` block and `tsc --noEmit` (run during `pnpm build`) will fail.

```ts
/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/api": "http://localhost:7878",
      "/ws":  { target: "ws://localhost:7878", ws: true },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
  },
});
```

- [ ] **Step 5: Write `web/index.html`.**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>yogurt</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: Write `web/src/index.css`.**

```css
@import "tailwindcss";

:root { color-scheme: light; }
body { margin: 0; font-family: system-ui, -apple-system, "Segoe UI", sans-serif; background: #FBF7EF; color: #211D18; }
```

(Brand palette is hardcoded here as a placeholder — Phase 1 replaces this with proper design tokens.)

- [ ] **Step 7: Write `web/src/lib/api.ts`.**

```ts
export interface HealthResponse {
  status: string;
  service: string;
}

export async function fetchHealth(): Promise<HealthResponse> {
  const res = await fetch("/api/health");
  if (!res.ok) throw new Error(`health check failed: ${res.status}`);
  return res.json() as Promise<HealthResponse>;
}
```

- [ ] **Step 8: Write `web/src/App.tsx` (Tailwind + TipTap smoke).**

```tsx
import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect, useState } from "react";
import { fetchHealth, type HealthResponse } from "./lib/api";

export function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const editor = useEditor({
    extensions: [StarterKit],
    content: "<p>Type something — TipTap is working.</p>",
  });

  useEffect(() => {
    fetchHealth().then(setHealth).catch((e) => console.error(e));
  }, []);

  return (
    <main className="max-w-2xl mx-auto p-10 space-y-6">
      <header className="space-y-1">
        <h1 className="text-3xl font-bold tracking-tight">yogurt</h1>
        <p className="text-sm text-neutral-500">
          phase 0 scaffold · server says: <code className="bg-neutral-100 px-2 py-0.5 rounded">{health ? `${health.service} ${health.status}` : "loading…"}</code>
        </p>
      </header>
      <section className="border border-neutral-300 rounded-lg p-4 bg-white">
        <EditorContent editor={editor} />
      </section>
    </main>
  );
}
```

- [ ] **Step 9: Write `web/src/main.tsx`.**

```tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
```

- [ ] **Step 10: Install + smoke.**

Run: `pnpm --dir web install`
Expected: lockfile written, ~300 packages installed.

Run: `pnpm --dir web dev`
Expected: Vite serves at `http://localhost:5173`. Open it — should see "yogurt" headline, the health line says `loading…` (since the Rust server isn't running), and the TipTap editor is editable.

Stop Vite (Ctrl-C).

- [ ] **Step 11: Commit.**

```bash
git add web/
git commit -m "feat(web): scaffold React + Vite + Tailwind 4 + TipTap"
```

---

### Task 0.6 · Vitest smoke test for the React app

**Files:**
- Create: `web/src/App.test.tsx`

- [ ] **Step 1: Write the smoke test.**

```tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { App } from "./App";

vi.mock("./lib/api", () => ({
  fetchHealth: vi.fn().mockResolvedValue({ status: "ok", service: "yogurt-server" }),
}));

describe("App", () => {
  it("renders the yogurt headline", async () => {
    render(<App />);
    // Use findBy to wait for the React 19 effect cycle to settle before asserting.
    expect(await screen.findByRole("heading", { name: /yogurt/i })).toBeInTheDocument();
  });

  it("shows the health response once fetched", async () => {
    render(<App />);
    await waitFor(() => {
      expect(screen.getByText(/yogurt-server ok/)).toBeInTheDocument();
    });
  });
});
```

- [ ] **Step 2: Add `@testing-library/jest-dom` for the matcher.**

Run: `pnpm --dir web add -D @testing-library/jest-dom`

Create `web/src/vitest.setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

Update `web/vite.config.ts` test block:

```ts
test: {
  environment: "jsdom",
  globals: true,
  setupFiles: ["./src/vitest.setup.ts"],
},
```

- [ ] **Step 3: Run.**

Run: `pnpm --dir web test`
Expected: `2 passed`.

- [ ] **Step 4: Commit.**

```bash
git add web/src/App.test.tsx web/src/vitest.setup.ts web/vite.config.ts web/package.json web/pnpm-lock.yaml
git commit -m "test(web): add Vitest smoke for App + health fetch"
```

---

### Task 0.7 · Embed `web/dist` into the binary via `rust-embed`

**Files:**
- Modify: `crates/yogurt-server/Cargo.toml` (add `mime_guess` and `rust-embed`)
- Create: `crates/yogurt-server/src/assets.rs`
- Modify: `crates/yogurt-server/src/routes.rs` (mount asset fallback in release mode)
- Modify: `crates/yogurt-server/src/lib.rs` (branch on mode, delete the orphaned `/` route)

> **⚠ Note:** `web/dist/` is in `.gitignore`. After any fresh clone you must re-run `pnpm --dir web build` before the embedded test (`it_serves_embedded_index_in_release_mode`) will pass — otherwise `rust-embed` finds an empty directory and serves a confusing "asset not found".

- [ ] **Step 1: Add `mime_guess` to `crates/yogurt-server/Cargo.toml`.**

Append to `[dependencies]`:

```toml
mime_guess = { workspace = true }
```

(Already added to workspace deps in Task 0.1.)

- [ ] **Step 2: Build the frontend once so `web/dist` exists.**

Run: `pnpm --dir web build`
Expected: `web/dist/index.html` + hashed asset files. `du -sh web/dist` ≈ 300-500KB.

- [ ] **Step 3: Write `crates/yogurt-server/src/assets.rs`.**

```rust
use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct WebDist;

/// Axum fallback handler that serves the embedded SPA.
/// On unknown paths it returns `index.html` so client-side routing works.
pub async fn serve_embedded(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let candidate = if path.is_empty() { "index.html" } else { path };

    match WebDist::get(candidate) {
        Some(file) => {
            let mime = mime_guess::from_path(candidate).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(file.data.into_owned()))
                .unwrap()
        }
        None => {
            // SPA fallback to index.html for client-side routes.
            match WebDist::get("index.html") {
                Some(idx) => Response::builder()
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(idx.data.into_owned()))
                    .unwrap(),
                None => (StatusCode::NOT_FOUND, "asset not found").into_response(),
            }
        }
    }
}
```

(Add `mime_guess = "2"` to the workspace deps and reference in `yogurt-server` `Cargo.toml`. `rust-embed`'s `mime-guess` feature already pulls it transitively, but importing the crate directly is cleaner.)

- [ ] **Step 4: Modify `crates/yogurt-server/src/routes.rs`.**

> **⚠ Note:** This step deletes the `GET /` route and the `index` handler function from Task 0.3 — the asset fallback handles `/` in release mode and the dev proxy handles it in dev mode. Make sure to remove the `async fn index()` function entirely; leaving it as dead code will break Task 0.10's clippy `-D warnings` check.

```rust
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

use crate::assets::serve_embedded;
use crate::Mode;

pub fn router(mode: Mode) -> Router {
    let mut router = Router::new()
        .route("/api/health", get(health));

    router = match mode {
        Mode::Release => router.fallback(serve_embedded),
        Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
    };

    router
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "yogurt-server" }))
}
```

(Note we delete the `/` route — the asset fallback handles it now.)

- [ ] **Step 5: Update `crates/yogurt-server/src/lib.rs`.**

```rust
mod assets;
mod dev_proxy;
mod routes;

use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Dev,
    Release,
}

pub async fn run(addr: SocketAddr, mode: Mode) -> Result<()> {
    let app = routes::router(mode);
    tracing::info!(?addr, ?mode, "yogurt-server starting");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 6: Add a stub `dev_proxy.rs` (real impl in Task 0.8).**

Create `crates/yogurt-server/src/dev_proxy.rs`:

```rust
use axum::http::{StatusCode, Uri};
use axum::response::IntoResponse;

pub async fn proxy_to_vite(_uri: Uri) -> impl IntoResponse {
    (StatusCode::SERVICE_UNAVAILABLE, "dev proxy not yet implemented (task 0.8)")
}
```

- [ ] **Step 7: Write the failing release-mode embedded test.**

Create `crates/yogurt-server/tests/embedded.rs`:

```rust
use std::time::Duration;

#[tokio::test]
async fn it_serves_embedded_index_in_release_mode() {
    let addr = "127.0.0.1:17880".parse().unwrap();
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, yogurt_server::Mode::Release).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let body = reqwest::get("http://127.0.0.1:17880/")
        .await.unwrap()
        .text().await.unwrap();

    assert!(body.contains("yogurt"), "embedded index should mention yogurt");
    handle.abort();
}
```

- [ ] **Step 8: Run — expect PASS.**

Run: `cargo test -p yogurt-server`
Expected: `it_responds_to_health ... ok` AND `it_serves_embedded_index_in_release_mode ... ok`.

- [ ] **Step 9: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): embed web/dist via rust-embed for release mode"
```

---

### Task 0.8 · Dev-mode proxy to Vite at `:5173`

**Files:**
- Modify: `crates/yogurt-server/src/dev_proxy.rs`
- Modify: `crates/yogurt-server/Cargo.toml` (add reqwest already in deps; ensure stream feature on)

- [ ] **Step 1: Replace the stub `crates/yogurt-server/src/dev_proxy.rs` with a real proxy.**

```rust
use axum::{
    body::Body,
    http::{header, HeaderMap, HeaderName, Method, Request, StatusCode, Uri},
    response::{IntoResponse, Response},
};

const VITE_BASE: &str = "http://127.0.0.1:5173";

pub async fn proxy_to_vite(
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let path_and_query = uri.path_and_query().map(|x| x.as_str()).unwrap_or("/");
    let target = format!("{VITE_BASE}{path_and_query}");

    // Convert axum body to bytes (sufficient for dev; no large uploads here).
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(?e, "proxy: failed to buffer request body");
            return (StatusCode::BAD_GATEWAY, "vite proxy: body read failed").into_response();
        }
    };

    let client = reqwest::Client::new();
    let mut req = client.request(method, &target).body(body_bytes.to_vec());

    // Forward most headers, but skip hop-by-hop ones.
    for (name, value) in headers.iter() {
        if is_hop_by_hop(name) { continue; }
        if let Ok(v) = value.to_str() {
            req = req.header(name.as_str(), v);
        }
    }

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let mut builder = Response::builder().status(status);
            for (name, value) in resp.headers() {
                if is_hop_by_hop(name) { continue; }
                builder = builder.header(name.as_str(), value.as_bytes());
            }
            let bytes = resp.bytes().await.unwrap_or_default();
            builder.body(Body::from(bytes)).unwrap()
        }
        Err(e) => {
            tracing::warn!(target = %target, ?e, "vite proxy: upstream error — is `pnpm --dir web dev` running?");
            (
                StatusCode::BAD_GATEWAY,
                [(header::CONTENT_TYPE, "text/plain")],
                format!("yogurt dev proxy: cannot reach vite at {VITE_BASE}\n\nrun: pnpm --dir web dev"),
            )
                .into_response()
        }
    }
}

fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "connection" | "keep-alive" | "proxy-authenticate" | "proxy-authorization"
            | "te" | "trailers" | "transfer-encoding" | "upgrade" | "host"
    )
}
```

- [ ] **Step 2: Write the dev-mode test.**

Append to `crates/yogurt-server/tests/embedded.rs`:

```rust
#[tokio::test]
async fn it_returns_bad_gateway_in_dev_mode_when_vite_is_down() {
    let addr = "127.0.0.1:17881".parse().unwrap();
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, yogurt_server::Mode::Dev).await
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    let resp = reqwest::get("http://127.0.0.1:17881/").await.unwrap();
    assert_eq!(resp.status(), 502, "no vite running → 502");

    handle.abort();
}
```

- [ ] **Step 3: Run.**

Run: `cargo test -p yogurt-server`
Expected: all 3 tests pass.

- [ ] **Step 4: Manual two-terminal smoke.**

Terminal 1: `pnpm --dir web dev`
Terminal 2: `cargo run -p yogurt -- start --dev --no-open`

Open `http://localhost:7878` — should see the React app (proxied from Vite). Edit `web/src/App.tsx`, save — HMR should refresh the browser.

`curl localhost:7878/api/health` should still return `{"status":"ok"}` (axum handles the route directly).

- [ ] **Step 5: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): proxy non-API routes to Vite in dev mode"
```

---

### Task 0.9 · LICENSE + README

**Files:**
- Create: `LICENSE`
- Create: `README.md`

- [ ] **Step 0: Write `LICENSE` (MIT, per PRD §15).**

Copy the standard MIT text. Replace `<year>` with `2026` and `<copyright holders>` with `Jarvis Chen`:

```
MIT License

Copyright (c) 2026 Jarvis Chen

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 1: Write `README.md`.**

```markdown
# yogurt

> Local-first, open-source meeting copilot. Granola's UX, your machine.

**Status:** Phase 0 (scaffold). See [docs/PRD.md](docs/PRD.md) for v1 plan.

## Install (eventually)

```bash
brew install yogurt           # not yet — first release in phase 9
yogurt start                  # opens http://localhost:7878 in your browser
```

## Run from source today

```bash
# one-time setup
brew install rust pnpm
git clone https://github.com/jarvisrchen/yogurt.git
cd yogurt
pnpm --dir web install

# optional: seed your API keys (gitignored; auto-loaded once dotenvy lands in Phase 5)
cat > .env.local <<'EOF'
YOGURT_MINIMAX_API_KEY=sk-...
# YOGURT_DEEPGRAM_API_KEY=...   # optional, for cloud STT in Phase 3
EOF

# dev — two terminals
pnpm --dir web dev                                # terminal 1: frontend HMR on :5173
cargo run -p yogurt -- start --dev                # terminal 2: backend on :7878

# release build (single binary with embedded assets)
pnpm --dir web build
cargo run -p yogurt --release -- start --no-open
```

## Architecture (short)

Single Rust binary. axum HTTP + WS server. React + Vite + TipTap UI. Native macOS audio via ScreenCaptureKit (Phase 2). Pluggable STT — cloud (Deepgram) default, local (whisper.cpp) optional (Phase 3 / Phase 8). Bring-your-own OpenAI-compatible LLM key (Phase 5).

See [docs/PRD.md](docs/PRD.md) §7 for the full architecture diagram.

## License

MIT. See [LICENSE](LICENSE).
```

- [ ] **Step 2: Commit.**

```bash
git add LICENSE README.md
git commit -m "docs: add LICENSE (MIT) + README quickstart"
```

---

### Task 0.10 · End-to-end smoke + push

**Files:** none — verification only.

- [ ] **Step 1: Fresh-clone smoke from a sibling directory.**

```bash
cd /tmp
rm -rf yogurt-smoke
git clone /Users/rchen/Documents/code/yogurt yogurt-smoke
cd yogurt-smoke
pnpm --dir web install
pnpm --dir web build
cargo build --release
./target/release/yogurt start --no-open &
sleep 1
curl -s localhost:7878/api/health
curl -s localhost:7878/ | head -c 200
kill %1
cd - && rm -rf /tmp/yogurt-smoke
```

Expected:
- Health curl returns `{"status":"ok","service":"yogurt-server"}`
- Root curl returns HTML containing `<div id="root">` (embedded `web/dist/index.html`).

- [ ] **Step 2: Format + lint.**

Run: `cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: clean.

Run: `pnpm --dir web build`
Expected: tsc + vite both succeed.

- [ ] **Step 3: Push to origin.**

```bash
git push origin main
```

- [ ] **Step 4: Verify on GitHub.**

Open <https://github.com/jarvisrchen/yogurt> in browser — README should render, file tree should match the structure above.

- [ ] **Step 5: Tag the phase milestone — only with explicit user confirmation.**

Pushing a tag is a public, semi-permanent action. Before running the commands below, confirm with the user that they want a `v0.0.1-phase-0` tag published.

```bash
git tag -a v0.0.1-phase-0 -m "Phase 0 complete: skeleton + scaffold runnable"
git push origin v0.0.1-phase-0
```

---

## Phase 0 acceptance criteria

All four must be true:

1. `cargo test --workspace` passes.
2. `pnpm --dir web test` passes.
3. **Dev flow:** in two terminals — `pnpm --dir web dev` + `cargo run -p yogurt -- start --dev` — visiting `http://localhost:7878` shows the React app served from Vite, and editing `App.tsx` hot-reloads in the browser.
4. **Release flow:** `pnpm --dir web build && cargo build --release && ./target/release/yogurt start --no-open` boots in <1s, serves `web/dist` from the embedded binary, and the binary is fully self-contained (no `web/dist` lookup at runtime).

## What this phase does NOT do

Explicitly out of scope (next plans cover these):
- Audio capture (Phase 2 plan)
- Live STT integration (Phase 3 plan)
- The black-you / grey-AI TipTap marks (Phase 4 plan — TipTap is wired here just to prove the build works)
- Settings / LLM client (Phase 5 plan)
- Brand palette + Instrument Serif + design tokens (Phase 1 plan — Phase 0 styles are throwaway)
- SQLite, markdown files, WebSocket layer

## Next plan

After Phase 0 lands, write `docs/superpowers/plans/<date>-yogurt-phase-1-design-system.md` covering:
- Tailwind 4 with the §16 brand tokens (paper / ink / blueberry / strawberry / matcha)
- Instrument Serif + Hanken Grotesk + JetBrains Mono via `@fontsource/*`
- Core component primitives (Button, Pill, Card, RecordingBadge, BrowserChrome)
- The swirl logo as a React component
- A `/style-guide` route that renders §16 to validate tokens are wired correctly

Subsequent phase plans follow the PRD §12 roadmap.
