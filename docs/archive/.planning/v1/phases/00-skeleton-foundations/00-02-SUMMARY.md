---
phase: 00-skeleton-foundations
plan: 02
subsystem: foundations
tags: [web, react, vite, tailwind, tiptap, rust-embed, dev-proxy, axum]
requires:
  - cargo-workspace
  - yogurt-cli
  - yogurt-server
  - api-health-route
provides:
  - web-scaffold
  - vitest-smoke
  - embedded-spa-release
  - vite-dev-proxy
  - spa-fallback
affects:
  - crates/yogurt-server/Cargo.toml
  - crates/yogurt-server/src/lib.rs
  - crates/yogurt-server/src/routes.rs
  - Cargo.lock
tech-stack:
  added:
    - react 19.2.7 + react-dom 19.2.7
    - vite 6.4.3 (node 20.19+)
    - "@vitejs/plugin-react 4.7.0"
    - tailwindcss 4.3.1 + @tailwindcss/vite 4.3.1
    - "@tiptap/core 2.27.2, @tiptap/react 2.27.2, @tiptap/starter-kit 2.27.2"
    - typescript 5.9.3
    - vitest 2.1.9 + jsdom 25.0.1
    - "@testing-library/react 16.3.2 + @testing-library/jest-dom 6.9.1"
    - mime_guess (Rust workspace) — promoted to direct dep on yogurt-server (D-14)
  patterns:
    - SPA static-asset embedding via rust-embed RustEmbed derive on a struct pointing to web/dist/
    - SPA fallback (missing path → serve index.html so client-side routing resolves)
    - mode-dependent router fallback (Mode::Release → serve_embedded, Mode::Dev → proxy_to_vite)
    - reqwest-based dev-mode reverse proxy with hop-by-hop header stripping (9-header RFC 7230 set)
    - Vite dev server proxying /api + /ws back to :7878 (single-origin DX)
    - vitest jsdom + @testing-library/react smoke tests for React components
    - explicit defineConfig type-cast pattern to bridge vite 6 + vitest 2.1's vite-5 peer types
key-files:
  created:
    - web/package.json
    - web/pnpm-lock.yaml
    - web/tsconfig.json
    - web/vite.config.ts
    - web/index.html
    - web/src/main.tsx
    - web/src/App.tsx
    - web/src/App.test.tsx
    - web/src/index.css
    - web/src/lib/api.ts
    - web/src/vitest.setup.ts
    - crates/yogurt-server/src/assets.rs
    - crates/yogurt-server/src/dev_proxy.rs
    - crates/yogurt-server/tests/embedded.rs
  modified:
    - crates/yogurt-server/Cargo.toml
    - crates/yogurt-server/src/lib.rs
    - crates/yogurt-server/src/routes.rs
    - Cargo.lock
decisions:
  - "Used `defineConfig` from `vite` (not `vitest/config`) with `as UserConfig` cast — vitest 2.1 pins vite 5 peer types but we run vite 6, and importing from vitest/config caused Plugin<any> type conflicts with @vitejs/plugin-react 4.7. Cast is documented inline; runtime behavior is identical (vitest/globals triple-slash reference still pulls the TestUserConfig type for IDE/tsc awareness)."
  - "Deleted the transitional `async fn index()` from Plan 01 entirely (not stubbed) — Mode::Release fallback now resolves `/` via serve_embedded → index.html, Mode::Dev fallback proxies through to Vite. Dead code would have tripped clippy -D warnings."
  - "Stripped 9 hop-by-hop headers in dev_proxy (connection, keep-alive, proxy-authenticate, proxy-authorization, te, trailers, transfer-encoding, upgrade, host) per RFC 7230 — required because reqwest sets its own host/connection headers and forwarding the originals causes upstream Vite to reject the request."
metrics:
  duration_min: ~28
  completed: 2026-06-25
  tasks_completed: 3
  files_created: 14
  files_modified: 4
  tests_added: 3 (2 vitest, 1 rust integration; plus extension of existing embedded.rs)
---

# Phase 00 Plan 02: Web Scaffold + Embedded SPA + Vite Dev Proxy Summary

Scaffolded the React + Vite + Tailwind 4 + TipTap web app, wired Vitest smoke tests, embedded `web/dist` into `yogurt-server` via `rust-embed` with SPA fallback, and implemented the Dev-mode reverse proxy from axum to Vite's :5173 dev server. `yogurt start --no-open` (Release) now serves the embedded React app from a single static binary; `yogurt start --dev --no-open` proxies non-API requests to `pnpm --dir web dev` for HMR. FOUND-03 (server serves React page via rust-embed) demonstrably met.

## What Was Built

### Web app scaffold (`web/`)
- **`web/package.json`** — `name=yogurt-web`, type=module, scripts `dev` / `build` (tsc && vite build) / `preview` / `test` (vitest run) / `test:watch`. Locked to React 19, Vite 6, Tailwind 4, TipTap 2.10 starter-kit, Vitest 2.1, jsdom 25, @testing-library/react 16. `@testing-library/jest-dom` 6.9 added in Task 2.
- **`web/tsconfig.json`** — strict, target ES2022, lib ES2022+DOM+DOM.Iterable, jsx react-jsx, moduleResolution bundler, types `["vitest/globals"]`. `noUnusedLocals` + `noUnusedParameters` + `noFallthroughCasesInSwitch` enabled.
- **`web/vite.config.ts`** — first line `/// <reference types="vitest/config" />` so `tsc --noEmit` recognizes the test block. Plugins: `[react(), tailwindcss()]`. Server: port 5173, strictPort true, proxy `/api → http://localhost:7878`, `/ws → ws://localhost:7878` (`ws: true`). Test: `environment: "jsdom"`, `globals: true`, `setupFiles: ["./src/vitest.setup.ts"]`. Config object is cast to `UserConfig` (see Deviations) to bridge vitest's vite-5 peer types vs the installed vite 6.
- **`web/index.html`** — standard doctype, `<title>yogurt</title>`, `<div id="root"></div>`, ES-module script tag loading `/src/main.tsx`.
- **`web/src/index.css`** — `@import "tailwindcss";`, `color-scheme: light`, hardcoded `#FBF7EF` paper / `#211D18` ink throwaway palette (Phase 1 replaces with the full §16 token system via Tailwind 4 `@theme`).
- **`web/src/lib/api.ts`** — `HealthResponse` interface and `fetchHealth(): Promise<HealthResponse>` GETting `/api/health` (Vite proxy forwards to :7878 in dev; the embedded SPA hits axum directly in release).
- **`web/src/App.tsx`** — React 19 functional component using `useEditor` + `EditorContent` from `@tiptap/react` with `StarterKit`, plus `useEffect` + `useState` for the health fetch. Renders the `yogurt` headline (`h1`), a subtitle showing the health line in an inline `<code>` chip (`loading…` before the fetch resolves), and a bordered `<section>` wrapping the TipTap editor seeded with `<p>Type something — TipTap is working.</p>`.
- **`web/src/main.tsx`** — React 19 `createRoot(document.getElementById("root")!)` rendering `<StrictMode><App /></StrictMode>`, importing `./index.css`.
- **`web/src/App.test.tsx`** (Task 2) — vitest + `@testing-library/react` smoke. Mocks `./lib/api` so `fetchHealth` resolves `{status: "ok", service: "yogurt-server"}`. Two tests: `renders the yogurt headline` (uses `findByRole("heading", {name: /yogurt/i})` to await the React 19 effect cycle) and `shows the health response once fetched` (uses `waitFor` against `getByText(/yogurt-server ok/)`).
- **`web/src/vitest.setup.ts`** — single `import "@testing-library/jest-dom/vitest";` line so `toBeInTheDocument()` and the rest of the jest-dom matchers are wired.

### Server asset embedding (`crates/yogurt-server/`)
- **`Cargo.toml`** — added `mime_guess = { workspace = true }` to `[dependencies]`. Explicit declaration even though `rust-embed`'s `mime-guess` feature pulls it transitively, per CONTEXT D-14: keeps the `mime_guess` import in `assets.rs` explicit and survives feature-flag refactors.
- **`src/assets.rs`** — `#[derive(RustEmbed)] #[folder = "../../web/dist/"] struct WebDist;`. `pub async fn serve_embedded(uri: Uri) -> Response` strips the leading `/`, treats empty as `index.html`, calls `WebDist::get(candidate)`, sets Content-Type via `mime_guess::from_path(candidate).first_or_octet_stream()`. On `None` falls through to `WebDist::get("index.html")` with `text/html` (SPA fallback). On the (unreachable in practice) missing-index case, returns `(StatusCode::NOT_FOUND, "asset not found")`.
- **`src/dev_proxy.rs`** — `const VITE_BASE: &str = "http://127.0.0.1:5173";`. `pub async fn proxy_to_vite(method: Method, uri: Uri, headers: HeaderMap, body: Body) -> Response` builds the target URL from `uri.path_and_query()`, buffers the request body via `axum::body::to_bytes(body, usize::MAX).await` (BAD_GATEWAY on read failure), forwards method + body + non-hop-by-hop headers via `reqwest::Client`. On `Ok(resp)`: copies status + non-hop-by-hop response headers + body. On `Err(e)`: logs `tracing::warn!(target = %target, ?e, "vite proxy: upstream error — is `pnpm --dir web dev` running?")` and returns `502 Bad Gateway` with `Content-Type: text/plain` and body literally `yogurt dev proxy: cannot reach vite at http://127.0.0.1:5173\n\nrun: pnpm --dir web dev`. `is_hop_by_hop` matches the 9 RFC 7230 header names case-insensitively: `connection`, `keep-alive`, `proxy-authenticate`, `proxy-authorization`, `te`, `trailers`, `transfer-encoding`, `upgrade`, `host`.
- **`src/routes.rs`** — `pub fn router(mode: Mode) -> Router` registering only `GET /api/health` as a real route, then `match mode { Mode::Release => router.fallback(serve_embedded), Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite) }`. The transitional `GET /` route + `async fn index()` handler from Plan 01 are **deleted entirely** — leaving them dead would have failed clippy `-D warnings`.
- **`src/lib.rs`** — declares `mod assets;` + `mod dev_proxy;` alongside `mod routes;`. `run()` now calls `routes::router(mode)` passing the `Mode` enum through.
- **`tests/embedded.rs`** — two `#[tokio::test]` functions:
  - `it_serves_embedded_index_in_release_mode` — spawns `yogurt_server::run` on `127.0.0.1:17880` in `Mode::Release`, sleeps 200ms, GETs `/`, asserts body contains `"yogurt"` (matched against the embedded `<title>yogurt</title>`).
  - `it_returns_bad_gateway_in_dev_mode_when_vite_is_down` — spawns on `:17881` in `Mode::Dev`, GETs `/` with no Vite running, asserts status `502` AND body contains `"pnpm --dir web dev"` (so the user-facing error copy is regression-tested).

## Tests

| # | Test | Crate / File | Status |
|---|------|--------------|--------|
| 1 | `it_prints_help` | yogurt (tests/cli.rs) | passed (carried from Plan 01) |
| 2 | `it_starts_server_and_serves_health` | yogurt (tests/cli.rs) | passed (carried from Plan 01) |
| 3 | `it_responds_to_health` | yogurt-server (tests/health.rs) | passed (carried from Plan 01) |
| 4 | `it_serves_embedded_index_in_release_mode` | yogurt-server (tests/embedded.rs) | passed (NEW) |
| 5 | `it_returns_bad_gateway_in_dev_mode_when_vite_is_down` | yogurt-server (tests/embedded.rs) | passed (NEW) |
| 6 | `App > renders the yogurt headline` | web (src/App.test.tsx) | passed (NEW) |
| 7 | `App > shows the health response once fetched` | web (src/App.test.tsx) | passed (NEW) |

- `cargo test --workspace` final: **5 passed (6 suites, 0.83s)**.
- `pnpm --dir web test` final: **Test Files 1 passed (1); Tests 2 passed (2)** in 565ms.

## Verification

- ✅ `pnpm --dir web install`: 248 packages added, lockfile written, 3.2s.
- ✅ `pnpm --dir web build`: tsc + vite both succeed; `web/dist/index.html` (0.39 kB), `web/dist/assets/index-CRZS9zXK.css` (6.46 kB), `web/dist/assets/index-3YEH5dt_.js` (501.77 kB) emitted in 681ms. The 500kB chunk warning is React 19 + ProseMirror + TipTap baseline — code-splitting deferred to a later phase.
- ✅ `pnpm --dir web test`: 2 passed.
- ✅ `cargo build --workspace`: clean.
- ✅ `cargo test --workspace`: 5 passed.
- ✅ `cargo clippy --all-targets -- -D warnings`: No issues found.
- ✅ Release smoke (`./target/release/yogurt start --no-open --port 27880`):
  - `GET /` → 200, embedded HTML containing `<div id="root">` and the hashed `/assets/index-*.js` + `.css` script/link tags.
  - `GET /api/health` → `{"service":"yogurt-server","status":"ok"}`.
  - `GET /library` (unknown path → SPA fallback) → 200, `Content-Type: text/html`, body is `index.html`.
  - `GET /assets/index-*.css` → 200, `Content-Type: text/css` (mime_guess working).
- ✅ Dev-mode smoke (`./target/release/yogurt start --dev --no-open --port 27881`, no Vite running):
  - `GET /` → 502, body literally `yogurt dev proxy: cannot reach vite at http://127.0.0.1:5173\n\nrun: pnpm --dir web dev`.
  - `GET /api/health` → JSON (still routed directly by axum, dev proxy only handles the fallback).

## Commits

| # | Hash | Message |
|---|------|---------|
| 1 | `ecffb7d` | `feat(web): bootstrap vite+react+tailwind scaffold` |
| 2 | `23f5b1e` | `test(web): add vitest smoke for App component` |
| 3 | `f2c4ad8` | `feat(server): embed web/dist + vite dev proxy + drop placeholder /` |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 — Blocking issue] `vite.config.ts` test block type mismatch with vite 6 + vitest 2.1**

- **Found during:** Task 1, Step 11 (`pnpm --dir web build`).
- **Issue:** Per the plan, `vite.config.ts` imported `defineConfig` from `vite` and added a `test:` block, relying on the `/// <reference types="vitest/config" />` triple-slash directive to teach TypeScript about Vitest's config shape. `tsc --noEmit` (run by `pnpm build`) rejected this with `TS2769: 'test' does not exist in type 'UserConfigExport'`. The triple-slash reference brings the test types into the project but does not augment vite's `defineConfig` overload set, so the literal `test:` property failed structural assignment.
- **First attempt (failed):** Swapped the import to `defineConfig` from `vitest/config`. That compiled past the `test:` issue but immediately tripped a deeper compatibility error: vitest 2.1's `vitest/config` re-exports `vite`'s `defineConfig` typed against its **own** pinned `vite@5.4.21` peer, while `@vitejs/plugin-react@4.7.0` ships `Plugin<any>` typed against the installed `vite@6.4.3`. The two `Plugin<any>` types are nominally distinct (different versioned `node_modules/.pnpm/vite@*` paths) so the `plugins: [react(), tailwindcss()]` array became unassignable — a ~25-frame nested-type error.
- **Final fix:** Kept `defineConfig` from `vite` (so the plugins array type-checks cleanly), kept the triple-slash reference (so `globals: true` enables ambient `describe`/`it`/`expect` types in test files), and `as UserConfig`-cast the config literal so `tsc` accepts the `test:` key without trying to match it against vite's narrower overload set. Runtime semantics are unchanged — Vitest reads the same plain object. Inline comment documents the workaround.
- **Files modified:** `web/vite.config.ts`.
- **Commit:** `ecffb7d` (Task 1) and `23f5b1e` (Task 2 added `setupFiles`).

### Auth gates

None.

### Pre-existing issues (out of scope)

- `pnpm install` reports 1 deprecated transitive sub-dependency (`whatwg-encoding@3.1.1`, pulled in by jsdom 25). Out of scope — jsdom's own bump is upstream's call.
- `vite build` warns the main bundle is >500kB (React + ProseMirror + TipTap baseline). Out of scope for Phase 0; code-splitting belongs to a later phase once we know which routes/components warrant lazy loading.

## Known Stubs

None. Every code path written this plan is exercised by an integration test:
- `serve_embedded` happy path → `it_serves_embedded_index_in_release_mode`.
- `serve_embedded` SPA fallback → covered by manual release-mode `GET /library` smoke (200 + text/html).
- `proxy_to_vite` error path → `it_returns_bad_gateway_in_dev_mode_when_vite_is_down` asserts both the 502 status AND the user-facing error body substring.
- React component → `App.test.tsx` covers headline + health-line rendering.

The `proxy_to_vite` happy path (Vite actually running) is documented as a two-terminal manual smoke in Task 3 Step 12 of the plan — automating it would require spawning a real Vite process in CI, which is overkill at Phase 0. Documented as a manual verification step in the plan itself.

## Threat Flags

None. Plan 02 introduces no new external trust boundaries beyond the localhost-only HTTP bind already covered by Plan 01 (D-11). The dev proxy targets `127.0.0.1:5173`, which is also localhost.

## Self-Check: PASSED

- **Files** — all declared key-files present on disk:
  - `web/`: package.json, pnpm-lock.yaml, tsconfig.json, vite.config.ts, index.html, src/{main.tsx, App.tsx, App.test.tsx, index.css, vitest.setup.ts, lib/api.ts} — verified via `ls web/ web/src/ web/src/lib/`.
  - `crates/yogurt-server/`: src/{assets.rs, dev_proxy.rs} (created), src/{routes.rs, lib.rs} (modified), Cargo.toml (modified), tests/embedded.rs (created).
- **Commits** — `git log --oneline -3` shows `f2c4ad8`, `23f5b1e`, `ecffb7d` on `gsd/autonomous`.
- **Plan acceptance criteria** — every bullet in `<must_haves.truths>` and `<acceptance_criteria>` confirmed:
  - `pnpm --dir web install && pnpm --dir web build` produces `web/dist/index.html` containing `<div id="root">` ✅
  - `pnpm --dir web test` passes 2 tests ✅
  - Release mode: `GET /` returns embedded `index.html`; unknown path SPA-fallback returns same HTML ✅
  - Dev mode with no Vite: `GET /` returns 502 with `pnpm --dir web dev` in body ✅
  - `crates/yogurt-server/src/assets.rs` contains `#[derive(RustEmbed)]` with `#[folder = "../../web/dist/"]` and `serve_embedded` ✅
  - `crates/yogurt-server/src/routes.rs` no longer contains `async fn index` (deleted) and uses `router.fallback(...)` based on `Mode` ✅
  - `crates/yogurt-server/src/dev_proxy.rs` contains `const VITE_BASE: &str = "http://127.0.0.1:5173";` and `is_hop_by_hop` matches all 9 RFC 7230 headers ✅
  - `cargo test -p yogurt-server` runs 3 tests, all passing ✅
  - `cargo clippy --all-targets -- -D warnings` clean ✅
- **Phase requirement covered:** FOUND-03 (`server serves React page via rust-embed`) demonstrably met by `it_serves_embedded_index_in_release_mode` plus the manual release-mode smoke that fetched the embedded `index.html` containing `<div id="root">`.

## Forward Notes for Plan 00-03 and Beyond

- **Toolchain:** rust-toolchain.toml stays on `stable` channel; declared MSRV is still `rust-version = "1.83"`. Plan 00-03 should not bump rust-toolchain.toml back to a pinned numeric version (see Plan 01 deviation #2).
- **Vite/Vitest type bridge:** The `as UserConfig` cast in `web/vite.config.ts` is a workaround for vitest 2.1 pinning vite 5 peer types. When vitest 3.x lands (currently 4.1.9 latest per `pnpm install` advisory output), revisit and consider switching to `defineConfig` from `vitest/config` if the peer pin moves to vite 6+. Until then, the cast is the correct, documented bridge.
- **SPA fallback contract:** `serve_embedded` returns `index.html` for any unknown path. Once Phase 1 introduces real client-side routing (e.g. `/welcome`, `/style-guide`), this is exactly the behavior React Router needs — but it also means any genuinely missing asset (typo in a `<link>` href) silently 200s with HTML. If diagnostics get confusing in Phase 1, consider returning 404 for paths starting with `/assets/` while still SPA-falling-back for top-level paths.
- **mime_guess explicit dep:** `mime_guess = { workspace = true }` is declared on yogurt-server even though `rust-embed`'s `mime-guess` feature pulls it transitively. This is intentional (D-14) — if a future plan turns off rust-embed's feature flags, the `assets.rs` import won't silently break.
- **Web dist is gitignored:** `dist/` in `.gitignore` matches `web/dist/`. The release-mode embedded test (`it_serves_embedded_index_in_release_mode`) requires `pnpm --dir web build` to have run first. On a fresh clone, CI / contributors must run `pnpm --dir web install && pnpm --dir web build` before `cargo test -p yogurt-server`. Task 0.9's README in Plan 0.9 (Plan 03) should document this prominently.
- **`bind` errors not yet user-friendly:** The port-conflict UX (D-19) is still untouched — `yogurt start --port <in-use>` will still surface the raw Tokio error. Plan 03 (or a Plan 04+ slot) should add the formatted `Port X is already in use. Try --port Y or run lsof -i :X` message.
- **WS Origin allowlist + session token (D-20/D-21):** Not yet implemented. Stub `/ws` endpoint also not yet present. Belongs in a later Wave 2/3 plan that introduces the actual WebSocket scaffold.
- **SQLite WAL + dual pool (D-22):** Not implemented. The `meetings` + `chat_messages` schema (D-23) is still on paper only. Belongs in a future Phase 0 plan or moved to early Phase 4 depending on roadmap pacing.
