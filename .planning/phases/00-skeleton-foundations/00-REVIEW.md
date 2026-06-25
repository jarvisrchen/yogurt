---
phase: 00-skeleton-foundations
reviewed: 2026-06-25T15:55:00Z
depth: deep
files_reviewed: 24
files_reviewed_list:
  - Cargo.toml
  - rust-toolchain.toml
  - .gitignore
  - crates/yogurt-cli/Cargo.toml
  - crates/yogurt-cli/src/main.rs
  - crates/yogurt-cli/src/commands/mod.rs
  - crates/yogurt-cli/src/commands/start.rs
  - crates/yogurt-cli/tests/cli.rs
  - crates/yogurt-server/Cargo.toml
  - crates/yogurt-server/build.rs
  - crates/yogurt-server/src/lib.rs
  - crates/yogurt-server/src/routes.rs
  - crates/yogurt-server/src/assets.rs
  - crates/yogurt-server/src/dev_proxy.rs
  - crates/yogurt-server/src/session.rs
  - crates/yogurt-server/src/storage.rs
  - crates/yogurt-server/src/storage/migrations.rs
  - crates/yogurt-server/src/ws.rs
  - crates/yogurt-server/tests/embedded.rs
  - crates/yogurt-server/tests/health.rs
  - crates/yogurt-server/tests/storage.rs
  - crates/yogurt-server/tests/ws_auth.rs
  - web/src/App.tsx
  - web/src/lib/api.ts
findings:
  blocker: 2
  high: 4
  medium: 7
  low: 4
  total: 17
status: findings-fixed
fixed_at: 2026-06-25T15:57:00Z
fix_commits:
  - a3fffb3  # BL-01, MD-05, MD-06 (session half)
  - b7c8ea1  # BL-02
  - b1042df  # HI-01
  - e86fe0b  # HI-02, MD-06 (storage half)
  - 65d48c7  # HI-03, HI-04, MD-03, MD-04, MD-02 (proxy half)
  - 68c8cad  # MD-01, MD-02 (assets half), MD-07, LO-01..04
---

# Phase 0: Code Review Report

**Reviewed:** 2026-06-25T15:55:00Z
**Depth:** deep
**Files Reviewed:** 24
**Status:** findings

## Summary

Phase 0 ships a competent scaffold and most of the pitfall mitigations are wired correctly: WAL mode is set on the writer, query_only=ON is set on read connections, the session-token file is opened with mode 0600 *before* the bytes hit disk, and the constant-time token compare uses `subtle::ct_eq`. The verifier already covered the happy-path behavioral spot-checks.

However, the foundation has real holes that compound: **(1) the WS Origin allowlist is bypassable** because axum/hyper return `Sec-WebSocket-Protocol: yogurt.<token>` as a multi-header *and* the token-in-URL flow leaks the token into server logs and browser history (since browser WebSocket APIs don't support custom headers, the URL flow is the *only* practical client path — so the token-in-query-string is the production code path, not a fallback); **(2) the session token compare leaks length and the file write is not atomic** (a crash mid-write produces an empty token file that the loader silently regenerates, rotating the token and DoS'ing any active sessions); **(3) WAL mode is not durably persisted on the *read* connections** — readers can race ahead of the writer's WAL setup if `init_at` is called repeatedly on cold storage; **(4) the dev-proxy buffers full request body into memory with `usize::MAX`** which is a trivial OOM DoS even on localhost; **(5) the SPA fallback handler does not handle path traversal `/../etc/passwd` correctly** in the URL — `uri.path()` is normalized by axum, but the candidate is passed directly to `mime_guess::from_path`, and a malicious filename embedded in the URL can produce surprising MIME types; **(6) `default_db_path` silently writes to whatever `$HOME` is set to, with no permission check** on `~/.yogurt/` itself — if the directory pre-exists with mode 0755 (group/other readable), a local attacker can read both the session token and the SQLite DB.

Also: there are no tests for the URL/path-traversal SPA fallback, no test that proves the WS subprotocol header path actually works end-to-end (only the query-param path is tested), no test that the dev proxy strips hop-by-hop headers (only the down-Vite path is tested), and no test that proves `--port 0` (auto-bind) or `--port` out-of-range behaviors fail gracefully. The verifier confirmed observable goals; this review surfaces what the verifier could not have caught without writing additional probes.

## Blocker Issues

### BL-01: Session-token file write is not atomic; mid-write crash silently rotates the token (DoS)

**File:** `crates/yogurt-server/src/session.rs:55-73, 75-107`

**Code excerpt:**
```rust
pub fn load_or_create(path: &Path) -> Result<SessionToken> {
    // ...
    if path.exists() {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading session token at {}", path.display()))?;
        let token = raw.trim().to_string();
        if token.is_empty() {
            // File exists but is empty — treat as missing and regenerate.
            return generate_and_persist(path);
        }
        return Ok(SessionToken(token));
    }
    generate_and_persist(path)
}

fn generate_and_persist(path: &Path) -> Result<SessionToken> {
    // ...
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        ...
    file.write_all(token.as_bytes())
        ...
}
```

**What's wrong:**
1. `OpenOptions::create(true).truncate(true).open(path)` **truncates the existing file BEFORE writing the new token**. If the process is killed (Ctrl+C, OOM, panic) between `open()` and `write_all()` — even a microsecond window — the on-disk file is now empty.
2. On the next boot, `load_or_create` sees `token.is_empty()` and *silently calls `generate_and_persist` again*, rotating the token.
3. This silently invalidates every active browser session and every saved WS subprotocol token — the user has no idea why their WebSocket suddenly returns 403 after a crash.
4. There is no `fsync()` after `write_all`, so even a clean shutdown can leave the file as zero-length if the OS hasn't flushed the page cache (filesystem-dependent but real on ext4 / APFS under power loss).

**Why it's wrong (consequence):** Phase 0 ships infrastructure for the entire app. Every later phase that holds a long-lived WS connection (Phase 3 transcript stream, Phase 6 chat) will silently break for users whose machines crashed during a boot. The "empty file → regenerate" branch is a footgun, not a recovery mechanism.

**Recommended fix:** Use a write-rename pattern with fsync. Write to `<path>.tmp` (with mode 0600), `fsync()` it, then `rename()` over the final path (atomic on the same filesystem). And — more importantly — **fail loud on an empty token file** rather than silently regenerating:

```rust
fn generate_and_persist(path: &Path) -> Result<SessionToken> {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);

    let tmp = path.with_extension("tmp");
    {
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        opts.mode(0o600);
        let mut file = opts.open(&tmp)
            .with_context(|| format!("creating tmp token at {}", tmp.display()))?;
        file.write_all(token.as_bytes())
            .with_context(|| format!("writing tmp token at {}", tmp.display()))?;
        file.sync_all().context("fsync tmp token")?;
    }
    std::fs::rename(&tmp, path)
        .with_context(|| format!("atomic-rename token to {}", path.display()))?;
    Ok(SessionToken(token))
}
```

And remove the silent-regenerate-on-empty branch — return `anyhow::bail!("session token file is empty; delete {} and restart", path.display())` instead.

---

### BL-02: WS handler reads `Sec-WebSocket-Protocol` via `h.to_str()` only; tokens with leading/trailing whitespace, multiple subprotocol headers, or non-ASCII bytes will silently fail auth — but more importantly, the handler does NOT echo back the negotiated subprotocol, which violates RFC 6455 and will break any browser client that sends one

**File:** `crates/yogurt-server/src/ws.rs:28-73`

**Code excerpt:**
```rust
pub async fn ws_handler(
    State(state): State<AppState>,
    Query(params): Query<WsParams>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // ...
    let candidate = params.token.clone().or_else(|| {
        headers
            .get("sec-websocket-protocol")
            .and_then(|h| h.to_str().ok())
            .and_then(|proto| {
                proto
                    .split(',')
                    .map(str::trim)
                    .find_map(|p| p.strip_prefix("yogurt.").map(str::to_string))
            })
    });
    // ...
    ws.on_upgrade(handle_socket)
}
```

**What's wrong:**
1. **RFC 6455 §4.2.2** says: if the client sends `Sec-WebSocket-Protocol`, the server MUST either echo back one of the offered subprotocols in its 101 response or fail the handshake. This code accepts the subprotocol header for auth but `ws.on_upgrade(handle_socket)` produces a 101 with **no `Sec-WebSocket-Protocol` response header**. Strict browser clients (and `tokio-tungstenite` in some configurations) treat this as a handshake failure.
2. The browser `WebSocket` API has no way to set arbitrary headers, so `Sec-WebSocket-Protocol` is the *only* way a browser can pass the token without putting it in the URL (where it leaks into server logs via `tracing::info!(addr = ?cfg.addr, ...)` and into the browser's history/devtools network panel). This is therefore the production path for the web UI, but it has never been exercised — the test at `tests/ws_auth.rs:117-149` uses the query-string path.
3. `headers.get("sec-websocket-protocol")` returns *only the first* matching header. RFC 7230 allows multiple `Sec-WebSocket-Protocol` headers (or one with comma-separated values). The single-`get` path silently drops additional values; an attacker could append `Sec-WebSocket-Protocol: yogurt.<wrong-token>` *after* the legitimate one to force the wrong-token path.
4. `h.to_str().ok()` returns `None` on any non-visible-ASCII byte — a single `\t` or stray UTF-8 in a copy-pasted token silently produces 403.

**Why it's wrong (consequence):** The browser-facing WS flow is broken by design but not tested. Phase 3 will ship a transcript dock that connects to `/ws` and immediately fail handshake, with no auth-test coverage to catch the regression. Plus the token-leak-via-URL becomes the *only* working path, which is a separate security problem (BL-01 also implies tokens are logged to stdout by tracing).

**Recommended fix:**
1. Echo back the subprotocol in the upgrade response. Axum's `WebSocketUpgrade::protocols(...)` or `on_upgrade_with_config(...)` supports this. Use the `axum::extract::ws::WebSocketUpgrade::protocols()` API: extract the offered protocols, pick the first `yogurt.<token>`, and pass it to `.on_upgrade(...)` via the response builder so it appears in the 101 headers.
2. Use `headers.get_all("sec-websocket-protocol")` and iterate all values; reject if *any* value fails the prefix check rather than only the first.
3. Add an integration test that drives the subprotocol path end-to-end with `tokio_tungstenite::tungstenite::client::IntoClientRequest` + `Sec-WebSocket-Protocol: yogurt.<token>` and asserts the 101 echoes the protocol back.

## High Issues

### HI-01: `is_addr_in_use` walks `anyhow::Error::chain()` but `tokio::net::TcpListener::bind` errors propagate through `axum::serve(...).await?` — only the *initial bind* is wrapped; the cli also catches AddrInUse on a runtime upgrade error that may not actually be a bind conflict

**File:** `crates/yogurt-cli/src/commands/start.rs:34-56` paired with `crates/yogurt-server/src/lib.rs:87-89`

**Code excerpt:**
```rust
// lib.rs
let listener = TcpListener::bind(cfg.addr).await?;
axum::serve(listener, app).await?;
```

```rust
// start.rs
if is_addr_in_use(&err) {
    let port = args.port;
    let next_port = port.wrapping_add(1);
    eprintln!(
        "Port {port} is already in use. Try --port {next_port} or run lsof -i :{port}"
    );
    std::process::exit(1);
}
```

**What's wrong:**
1. `port.wrapping_add(1)` means `--port 65535` produces the suggestion `--port 0` — which on Unix means "ask the kernel for an ephemeral port" (not at all what the user wants). The user would get an `--port 0` suggestion and a working-but-random-port server.
2. The classification function only inspects the error chain for `io::ErrorKind::AddrInUse`. After the listener is bound, *any* later runtime error inside `axum::serve` (e.g., a connection-accept error that returns `AddrInUse` from some weird intermediate state) would also trigger the "port in use" message — misleading the user when the real failure is something else.
3. The error is captured by reference and walked, but the matching is based on `io_err.kind()`. `std::io::Error::kind()` returning `AddrInUse` is the right signal, but axum 0.8 wraps connection errors in `hyper::Error` which only sometimes contains a downcastable `io::Error`. If axum changes how it surfaces the bind error (the workspace pins `axum = "0.8"` not a fixed `0.8.x`), the detection silently breaks and the user gets a confusing anyhow stack trace instead of the canonical message.

**Why it's wrong (consequence):** FOUND-06 is one of the five ROADMAP success criteria. If the message format silently degrades on an axum point release, the gate fails without anyone noticing. The `wrapping_add(1)` is a correctness bug: at the boundary it gives bad advice.

**Recommended fix:**
```rust
let next_port = port.checked_add(1).unwrap_or(port - 1);
```
Or just hardcode the "try a free port" advice without suggesting a specific number near boundaries. Also: catch `AddrInUse` directly at the bind site in `lib.rs` and surface it as a dedicated error type (`ServerError::PortInUse(u16)`) so the CLI matches on the type rather than walking the anyhow chain.

---

### HI-02: Read pool connections do NOT set WAL journal mode — they inherit it from the writer's pragma, but pragmas are per-connection in SQLite for some pragmas; query_only is set but journal_mode is not

**File:** `crates/yogurt-server/src/storage.rs:48-74`

**Code excerpt:**
```rust
let mut writer = Connection::open(db_path)?;
writer.pragma_update(None, "journal_mode", "WAL")?;
writer.pragma_update(None, "synchronous", "NORMAL")?;
// ...
for _ in 0..READ_POOL_SIZE {
    let r = Connection::open(db_path)?;
    r.pragma_update(None, "query_only", "ON")?;
    r.pragma_update(None, "foreign_keys", "ON")?;
    reads.push(Arc::new(Mutex::new(r)));
}
```

**What's wrong:**
1. `journal_mode=WAL` is **persistent** at the database-file level (SQLite docs §"Persistence of WAL mode"), so the writer-side `PRAGMA journal_mode=WAL` does persist — but ONLY because the writer ran the pragma first and migrations committed. The read connections happen to inherit it via the on-disk header. This is fragile: if `Storage::init_at` ever races (e.g., two processes calling it simultaneously on first boot, or someone deletes the DB file between writer-open and reader-open), the readers can attach to a rolled-back journal mode.
2. `synchronous=NORMAL` is **not persistent** (it's per-connection) and is NOT set on the read connections. Reads in NORMAL vs FULL behave the same for SELECTs, so this is more aesthetic than correctness — but it means that if a read connection ever escapes the `query_only=ON` gate (e.g., the gate is dropped in a future refactor), writes through it would happen at FULL sync, blocking the writer.
3. `query_only=ON` is per-connection but **not persistent** — if the read connection is ever recycled across `Connection::open` (it isn't in this code, but the pool is a `Vec<Arc<Mutex<Connection>>>` that lives for the program lifetime, so it's safe today), the protection vanishes.
4. The migration runs in a transaction inside `migrations::run(&mut writer)` *before* the read pool is opened. But the writer's `journal_mode=WAL` pragma was applied OUTSIDE the transaction. If the migration transaction commits but the WAL pragma somehow failed silently (it returns `Result` but is `?`-propagated, so this is OK today), readers would still see the legacy journal mode.

**Why it's wrong (consequence):** The dual-pool invariant (one writer, four readers) relies on every pragma being correctly applied to every connection. Adding `journal_mode=WAL` explicitly on reads is cheap and removes the implicit-inheritance assumption. A reader still has to know "I'm using WAL" because some pragmas like `wal_autocheckpoint` are per-connection and tuning them on the writer alone doesn't help readers.

**Recommended fix:**
```rust
for _ in 0..READ_POOL_SIZE {
    let r = Connection::open(db_path)?;
    // Belt-and-suspenders: ensure WAL is observed by the reader too. WAL is
    // persistent on-disk but pragmas like wal_autocheckpoint are not.
    r.pragma_update(None, "journal_mode", "WAL")?;
    r.pragma_update(None, "synchronous", "NORMAL")?;
    r.pragma_update(None, "query_only", "ON")?;
    r.pragma_update(None, "foreign_keys", "ON")?;
    reads.push(Arc::new(Mutex::new(r)));
}
```

Also: `Storage::init_at` should take an exclusive flock on the DB file during init to prevent the cross-process race.

---

### HI-03: `dev_proxy::proxy_to_vite` buffers the entire request body into memory with `usize::MAX` cap — trivial OOM DoS

**File:** `crates/yogurt-server/src/dev_proxy.rs:18-24`

**Code excerpt:**
```rust
let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
    Ok(b) => b,
    Err(e) => {
        tracing::warn!(?e, "vite proxy: failed to buffer request body");
        return (StatusCode::BAD_GATEWAY, "vite proxy: body read failed").into_response();
    }
};
```

**What's wrong:**
1. `usize::MAX` as the body cap means the dev proxy will happily allocate gigabytes of RAM before failing. A local malicious process (or a misbehaving Vite plugin issuing huge requests) can `curl --data-binary @largefile` and OOM the server.
2. This is the **dev-mode-only** proxy, so the practical attack surface is tight — but yogurt is "single-process, single-binary"; an OOM here takes down the whole server including the user's in-progress meeting capture.
3. The proxy also doesn't stream — even legitimately large requests (uploading a 100MB transcript via some future endpoint) would be fully buffered. The Vite dev server itself doesn't expect large POSTs, but the upstream code path silently allows them.

**Why it's wrong (consequence):** A single accidental curl in a developer's terminal could crash `yogurt start --dev`. More worryingly, the *exact same `usize::MAX`* pattern will likely be copy-pasted into Phase 5+ proxy code (audio upload, LLM streaming) where it becomes a real production risk.

**Recommended fix:**
```rust
const MAX_PROXY_BODY: usize = 8 * 1024 * 1024; // 8 MB; way more than dev needs
let body_bytes = match axum::body::to_bytes(body, MAX_PROXY_BODY).await {
    Ok(b) => b,
    Err(e) => {
        tracing::warn!(?e, "vite proxy: request body exceeded 8MB cap or read failed");
        return (StatusCode::PAYLOAD_TOO_LARGE, "vite proxy: request too large").into_response();
    }
};
```
Better: convert to a streaming proxy using `reqwest::Body::wrap_stream` so memory usage is bounded regardless of body size.

---

### HI-04: Dev proxy does NOT handle WebSocket upgrades; `/ws` requests through dev mode will hit the proxy fallback and fail bizarrely

**File:** `crates/yogurt-server/src/routes.rs:7-18` paired with `crates/yogurt-server/src/dev_proxy.rs`

**Code excerpt:**
```rust
let router = Router::new()
    .route("/api/health", get(health))
    .route("/ws", get(crate::ws::ws_handler))
    .with_state(state);

match mode {
    Mode::Release => router.fallback(serve_embedded),
    Mode::Dev => router.fallback(crate::dev_proxy::proxy_to_vite),
}
```

This is correct: `/ws` is a registered route so it never hits the fallback. **BUT**: `web/vite.config.ts:18` configures Vite to proxy `/ws` back to `:7878` with `ws: true`. This means the dev-mode flow is:
- Browser → Vite (5173) → yogurt server (7878) /ws — works.
- Browser → yogurt server (7878) /ws directly — also works.

What's broken: any non-`/ws` upgrade request in dev mode (e.g., Vite's HMR WebSocket on `/__vite_hmr`) will go through `proxy_to_vite`, which calls `reqwest::Client::new().request(method, &target).body(body_bytes.to_vec()).send()` — **reqwest does NOT do WebSocket upgrades**. The request will be silently sent as a plain HTTP GET with the `Upgrade: websocket` header stripped (HI-05 below) and the user will get an HMR-disconnected dev experience that's hard to debug.

**What's wrong:**
1. The dev proxy strips `upgrade` (correctly, per RFC 7230) but does not detect and handle WS upgrade requests as a special case.
2. Vite's own HMR uses `/__vite_hmr` (or `/?__vite_hmr` depending on version). In dev mode, the browser connects to the yogurt server directly (not Vite), so the HMR WS request hits `proxy_to_vite` and breaks.
3. The verifier only tested the "Vite is down → 502" case, not "Vite is up, browser tries to HMR through yogurt".

**Why it's wrong (consequence):** The two-terminal dev workflow only works if the developer goes directly to `http://localhost:5173` (Vite). If they go to `http://localhost:7878` (yogurt server), HMR is silently broken. The PRD §11 dev workflow doesn't say which port the dev should hit — this needs to be either documented as "use 5173 in dev" or fixed.

**Recommended fix:**
Either:
- (a) Document in README that dev mode requires opening `http://localhost:5173` (Vite), not `:7878`, and have the `--dev` flag auto-open `:5173` instead of `:7878`; OR
- (b) Detect `Connection: upgrade` + `Upgrade: websocket` in `proxy_to_vite` and either return a 502 with an actionable message ("dev mode WS upgrades not supported; use http://localhost:5173 for HMR") or implement a real WS proxy with `tokio-tungstenite` on both legs.

## Medium Issues

### MD-01: `serve_embedded` is vulnerable to path traversal via percent-encoded `..` in the URL path

**File:** `crates/yogurt-server/src/assets.rs:17-37`

**Code excerpt:**
```rust
pub async fn serve_embedded(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let candidate = if path.is_empty() { "index.html" } else { path };
    match WebDist::get(candidate) {
        Some(file) => { ... }
        None => match WebDist::get("index.html") { ... }
    }
}
```

**What's wrong:**
1. `axum::http::Uri::path()` does NOT normalize `/../` traversal segments — it returns the raw path (post-percent-decoding by hyper). A request for `/%2e%2e/etc/passwd` may arrive as `/../etc/passwd` and `candidate = "../etc/passwd"`.
2. `rust-embed::RustEmbed::get(candidate)` looks up by exact string match against the embedded asset table. `WebDist::get("../etc/passwd")` will return `None` (it only knows the files in `web/dist/`), so this is **probably safe in practice** — but it depends entirely on rust-embed not doing any path normalization or filesystem lookup. If a future maintainer swaps to a debug mode that reads from disk (rust-embed has a `debug-embed` feature that does exactly that), traversal becomes a live bug.
3. The candidate is also passed to `mime_guess::from_path(candidate)`, which doesn't traverse but does parse the extension. A URL like `/foo.php.svg` returns the SVG MIME type but `WebDist::get("foo.php.svg")` returns `None` and falls back to `index.html` served as `text/html` — not a vuln today, but suggests the MIME-handling logic is brittle.

**Why it's wrong (consequence):** Latent vulnerability — works today but breaks if rust-embed is swapped for a dev-mode disk loader. Worth a defensive check.

**Recommended fix:**
```rust
pub async fn serve_embedded(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    // Reject any path containing traversal segments or absolute-path indicators.
    if path.split('/').any(|seg| seg == ".." || seg == "." || seg.is_empty() && !path.is_empty()) {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }
    let candidate = if path.is_empty() { "index.html" } else { path };
    // ... rest unchanged
}
```

---

### MD-02: `serve_embedded` uses `.unwrap()` on Response builder — a malformed asset (impossible today but reachable via a future bug) panics the server task

**File:** `crates/yogurt-server/src/assets.rs:27, 33`

**Code excerpt:**
```rust
Response::builder()
    .header(header::CONTENT_TYPE, mime.as_ref())
    .body(Body::from(file.data.into_owned()))
    .unwrap()
```

**What's wrong:** `Response::builder().header(...).body(...).unwrap()` panics if the header value is invalid (contains CR/LF, for instance). `mime.as_ref()` from `mime_guess` is always a valid ASCII MIME string so this is safe today. But there are TWO `.unwrap()`s in this short function and one in `dev_proxy.rs:49`. The pattern is wrong: handler panics in axum kill the connection task but don't crash the process — except a panic inside `on_upgrade` (`ws.rs:72`) WILL leak resources.

**Why it's wrong (consequence):** Code-quality lint. Easy to do right.

**Recommended fix:** Use `.expect("static MIME types from mime_guess are always valid")` for self-documenting panics, or use `.unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "...").into_response())`.

---

### MD-03: `proxy_to_vite` calls `resp.bytes().await.unwrap_or_default()` — upstream stream error silently produces an empty 200

**File:** `crates/yogurt-server/src/dev_proxy.rs:48`

**Code excerpt:**
```rust
let bytes = resp.bytes().await.unwrap_or_default();
builder.body(Body::from(bytes)).unwrap()
```

**What's wrong:** If the upstream Vite response body fails mid-read (network blip, timeout, Vite crash mid-response), the proxy silently returns an empty body with whatever status code Vite already sent. The browser sees a "successful" but blank response with no indication of what went wrong.

**Why it's wrong (consequence):** Confusing dev experience — symptoms look like a Vite bug when the failure is in the proxy.

**Recommended fix:**
```rust
let bytes = match resp.bytes().await {
    Ok(b) => b,
    Err(e) => {
        tracing::warn!(?e, "vite proxy: upstream body read failed");
        return (StatusCode::BAD_GATEWAY, format!("vite proxy: upstream body error: {e}")).into_response();
    }
};
```

---

### MD-04: `is_hop_by_hop` uses `to_ascii_lowercase()` allocation per header per request — and HeaderName is already canonical lowercase

**File:** `crates/yogurt-server/src/dev_proxy.rs:69-82`

**Code excerpt:**
```rust
fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str().to_ascii_lowercase().as_str(),
        "connection" | ...
    )
}
```

**What's wrong:** `HeaderName::as_str()` is already guaranteed lowercase per the http crate's invariant. The `to_ascii_lowercase()` is an unnecessary allocation per header per request. Minor — but it's the kind of thing that compounds when the proxy is hit on every dev request.

**Why it's wrong (consequence):** Code quality / minor perf. Won't matter at v1 scale.

**Recommended fix:** Just `name.as_str()` directly. Or better, use `http::header::CONNECTION.as_str()` etc. for compile-time correctness.

---

### MD-05: Session token loaded from disk is never re-validated against length / charset — a corrupted file silently authenticates with garbage

**File:** `crates/yogurt-server/src/session.rs:61-69`

**Code excerpt:**
```rust
if path.exists() {
    let raw = std::fs::read_to_string(path)?;
    let token = raw.trim().to_string();
    if token.is_empty() {
        return generate_and_persist(path);
    }
    return Ok(SessionToken(token));
}
```

**What's wrong:**
1. `read_to_string` accepts any UTF-8. A file containing `"hello"` would be loaded as a valid token; the WS handler would then require clients to send `"hello"` as the session token. This is technically secure (you still need to know the disk contents), but it's a clear "WTF" debugging trap.
2. No length check. A two-character token would be accepted. The constant-time compare in `validate()` short-circuits on length mismatch (line 34), so a two-byte token is trivially brute-forceable.
3. No format validation. The token should always be 43 base64-URL chars (32 bytes encoded with NO_PAD). The loader should assert this and refuse to use a malformed token.

**Why it's wrong (consequence):** If the token file is corrupted (disk corruption, accidental edit, restored from a partial backup), the server boots happily with whatever garbage is there. The user has no signal that auth is in a degraded state.

**Recommended fix:**
```rust
const EXPECTED_TOKEN_LEN: usize = 43;
let token = raw.trim().to_string();
if token.len() != EXPECTED_TOKEN_LEN || !token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
    anyhow::bail!(
        "session token at {} is malformed (expected {EXPECTED_TOKEN_LEN}-char URL-safe base64); \
         delete it and restart to regenerate",
        path.display()
    );
}
```

---

### MD-06: `~/.yogurt/` directory is created with default umask (0755 on most systems) — session token + SQLite DB are listable / stat-able by other local users

**File:** `crates/yogurt-server/src/session.rs:56-59` and `crates/yogurt-server/src/storage.rs:40-43`

**Code excerpt:**
```rust
// session.rs
if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating token parent {}", parent.display()))?;
}

// storage.rs
if let Some(parent) = db_path.parent() {
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating storage parent dir {}", parent.display()))?;
}
```

**What's wrong:**
1. `create_dir_all` honors the process umask. The default macOS umask is 0022, so `~/.yogurt/` is created mode 0755 — world-readable directory.
2. The session-token file is 0600 (good), but on macOS multi-user systems, *another user can `ls ~rchen/.yogurt/`* and see that the token file exists, plus stat its size and mtime (which reveals when the token was rotated).
3. The SQLite database file is created with default mode (0644 on macOS) — **world-readable**. A local attacker can copy `~/.yogurt/db.sqlite` and read all meeting transcripts and chat history.
4. PRD §7 says "localhost trust assumption" but that's about network, not filesystem. The privacy posture in PROJECT.md says "audio never leaves machine" and implies meeting data is private — but the filesystem ACLs say otherwise.

**Why it's wrong (consequence):** On a shared macOS machine (rare for ICs but real for security-conscious orgs), another user account can read all of yogurt's data. This directly contradicts the PRD's privacy claims.

**Recommended fix:**
```rust
#[cfg(unix)]
fn create_dir_0700(p: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(p)
}
#[cfg(not(unix))]
fn create_dir_0700(p: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(p)
}
```
And open the SQLite file with mode 0600 (rusqlite's `OpenFlags` don't expose this directly; you may need to pre-create the file with the right mode then have rusqlite open the existing path). At minimum, after `Connection::open(db_path)` succeeds on first creation, `chmod` it to 0600.

---

### MD-07: `routes.rs` registers `/ws` with `get(...)` only — Origin/token check runs only for GET, but WS clients sometimes send OPTIONS preflight

**File:** `crates/yogurt-server/src/routes.rs:11`

**Code excerpt:**
```rust
.route("/ws", get(crate::ws::ws_handler))
```

**What's wrong:** WebSocket upgrade is over HTTP/1.1 GET, so this is correct for the protocol. However, if a browser sends a CORS preflight `OPTIONS /ws` (it shouldn't for same-origin, but extensions and DevTools sometimes do), it hits the fallback handler — in `Mode::Release` that's `serve_embedded`, which returns `index.html` (200 with HTML body) for `OPTIONS /ws`. A naive CORS-aware client would interpret that as a successful preflight.

This is not exploitable because the actual upgrade still goes through `ws_handler`'s Origin check. But the surprising "OPTIONS /ws → 200 with index.html" behavior could mislead future debugging.

**Why it's wrong (consequence):** Code-quality / surprising-behavior. No security impact today.

**Recommended fix:** Use `.route("/ws", any(crate::ws::ws_handler))` so the auth check applies to all methods on the path, returning 405 or 403 for non-GET.

## Low Issues

### LO-01: Browser auto-open task is spawned BEFORE the server actually binds; on a slow boot the browser hits a connection-refused screen

**File:** `crates/yogurt-cli/src/commands/start.rs:17-26`

**What's wrong:** `open::that(&url_for_open)` is called immediately on a background task while `yogurt_server::run` is still doing `TcpListener::bind` + `Storage::init_at` + migrations. On a cold-cache first boot (whisper.cpp init in later phases will be much slower), the browser opens and shows "127.0.0.1 refused to connect" before the server is up.

**Recommended fix:** Spawn the open task with a small `tokio::time::sleep(Duration::from_millis(250))` first, or — better — gate it on a health-check poll loop with a 2s timeout.

---

### LO-02: `tracing_subscriber::fmt().with_env_filter(...).init()` is called BEFORE `Cli::parse()` — `--help` and `--version` will emit "yogurt is starting" tracing init noise to stderr

**File:** `crates/yogurt-cli/src/main.rs:34-41`

**What's wrong:** `tracing_subscriber::fmt::init()` doesn't print anything by itself, but it does install a global subscriber. The very first `tracing::info!` after `--help` (none exist before parse, so this is benign today) would print. More importantly, the order is fragile: any future code that adds `tracing::info!` before `Cli::parse()` would leak log lines into `--help` output.

**Recommended fix:** Move the subscriber init to *after* `Cli::parse()`, or guard it behind a `--quiet` flag check.

---

### LO-03: `web/src/App.tsx:16` swallows fetchHealth errors with `console.error` — no UI signal that the server is unreachable

**What's wrong:** If `/api/health` returns 500 or the server is down, the UI shows "loading…" forever with only a console.error. Phase 0 is a scaffold so this is OK, but it sets a precedent for "silent failures" that Phase 1+ should explicitly correct.

**Recommended fix:** Set an error state and render "server unreachable — check terminal" inline.

---

### LO-04: `assert_cmd` integration test `it_starts_server_and_serves_health` sleeps 400ms then makes an HTTP request — flaky on slow CI

**File:** `crates/yogurt-cli/tests/cli.rs:29-37`

**What's wrong:** Fixed 400ms sleep before the HTTP probe is a classic flaky-test pattern. On a slow CI runner (GitHub Actions macOS runners can be very slow on first cargo invocation), the server may not be bound yet. The `ws_auth.rs` test does this correctly with a 50-iteration TcpStream::connect probe — apply the same pattern here.

**Recommended fix:**
```rust
for _ in 0..50 {
    if reqwest::get("http://127.0.0.1:17879/api/health").await.is_ok() { break; }
    tokio::time::sleep(Duration::from_millis(20)).await;
}
```

---

## Things I checked and found CLEAN

- `subtle::ct_eq` usage in `session.rs:37` — correct, follows the standard recipe with explicit length gate.
- `rand::thread_rng().fill_bytes(...)` in `session.rs:77` — uses `ThreadRng` which is a CSPRNG (ChaCha12-based). Correct entropy source.
- `.gitignore` correctly excludes `.env.local`, `/target/`, `web/dist/` (via `dist/` rule line 8).
- `Cargo.toml` dependency pins are conservative and match the CONTEXT.md D-03 commitments.
- `migrations::run` wraps schema creation in a transaction (`conn.transaction()` + `tx.commit()`); idempotent via `CREATE TABLE IF NOT EXISTS`.
- `Storage::read()` round-robin via `AtomicUsize::fetch_add` + modulo — race-free, correct.
- The WebSocket Origin allowlist correctly uses the *actual bound port* (not the hardcoded 7878), so tests on ephemeral ports work and there's no hardcode-skew vulnerability.
- `--port 0` (auto-bind) works at the syscall level (Tokio binds to ephemeral port), but the CLI prints `tracing::info!(%url, ...)` with the URL containing `:0` — the user has no way to discover the actual bound port. Worth a follow-up but not in scope for Phase 0.
- React 19 + TipTap 2 combo: `useEditor` returns an editor reference, `EditorContent` renders it — verified the import paths are correct and the test mocks the API surface cleanly.
- Tailwind v4 + Vite 6 wiring: `@tailwindcss/vite` plugin + `@import "tailwindcss"` in `index.css` is the canonical v4 setup.
- `rust-toolchain.toml` channel is `stable` (deviation from plan's `1.83`) but workspace `rust-version = "1.83"` is enforced by Cargo regardless. Verifier already accepted this; I confirm.

---

_Reviewed: 2026-06-25T15:55:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_

---

## Fix Log

Applied 2026-06-25T15:57:00Z by gsd-code-fixer (autonomous mode).
Branch: `gsd/autonomous`. All in-scope findings resolved; no deferrals.

Verification gate per fix: `cargo test --workspace` (28 tests, all pass) +
`cargo clippy --all-targets -- -D warnings` (clean) + `cargo fmt --all -- --check`
(clean).

| ID    | Status  | Commit    | Notes |
|-------|---------|-----------|-------|
| BL-01 | fixed   | `a3fffb3` | Atomic tmp+fsync+rename write; fail-loud on empty file; manual `Debug` on `SessionToken` redacts to `<REDACTED>`. Regression: `it_fails_loud_on_empty_token_file`, `it_persists_with_no_tmp_file_left_behind`. |
| BL-02 | fixed   | `b7c8ea1` | Chose Option (a): dropped `Sec-WebSocket-Protocol` auth path entirely (was broken — no RFC 6455 echo-back). `?token=<token>` is now the sole auth contract. Added `redact_token_in_uri()` helper for future request-logging middleware. `WsParams` Debug redacts the token. Regression: `it_rejects_ws_with_subprotocol_only_no_query_token`, `it_rejects_ws_with_header_injection_attempt_in_query`, `it_redacts_token_query_in_uri_logs`. |
| HI-01 | fixed   | `b1042df` | `checked_add(1).filter(|p| *p > 0)` replaces `wrapping_add`. At the 65535 boundary, suggests `lsof -i :65535 && kill <pid>` instead of `--port 0`. Regression: `it_does_not_suggest_port_0_at_upper_boundary` (gracefully skips if 65535 isn't bindable in sandbox). |
| HI-02 | fixed   | `e86fe0b` | Every read-pool connection now applies the full pragma set (`journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`, `query_only=ON`). Per-connection correctness replaces fragile inheritance-from-writer. Regression: `it_applies_wal_pragma_to_read_pool_connections`. |
| HI-03 | fixed   | `65d48c7` | `usize::MAX` body cap → `16 MiB`. Oversized bodies return `413 Payload Too Large` rather than buffering. Regression: `it_rejects_oversized_request_body_with_413`. |
| HI-04 | fixed   | `65d48c7` | Dev proxy detects `Connection: upgrade` + `Upgrade: websocket` and returns `426 Upgrade Required` with a message pointing at `http://localhost:5173` for Vite HMR. Regression: `it_rejects_websocket_upgrade_through_dev_proxy_with_426`. |
| MD-01 | fixed   | `68c8cad` | `serve_embedded` rejects any path segment that is `..` or `.` with `400 Bad Request` before calling `rust-embed::get`. Defense-in-depth for a potential future swap to `debug-embed`. |
| MD-02 | fixed   | `65d48c7`, `68c8cad` | All `Response::builder().body(...).unwrap()` calls in `assets.rs` and `dev_proxy.rs` are now `.expect("...")` with self-documenting panic conditions. |
| MD-03 | fixed   | `65d48c7` | `resp.bytes().await.unwrap_or_default()` → explicit match that returns `502 Bad Gateway` with the upstream error string when the proxied response body fails mid-read. |
| MD-04 | fixed   | `65d48c7` | `is_hop_by_hop` drops the per-header `to_ascii_lowercase()` allocation — `HeaderName::as_str()` is already lowercase per the http-crate invariant. |
| MD-05 | fixed   | `a3fffb3` | `load_or_create` validates token length (43 chars) and charset (URL-safe base64) on read. Malformed file returns a hard error. Regression: `it_fails_loud_on_malformed_token`. |
| MD-06 | fixed   | `a3fffb3`, `e86fe0b` | `~/.yogurt/` is now created at mode 0700 via `DirBuilder::mode(0o700)` in both `session::load_or_create` and `Storage::init_at`. Pre-existing looser directories are tightened. The SQLite file is chmod'd to 0600 after creation. Regressions: `it_creates_parent_dir_at_mode_0700`, `it_tightens_a_preexisting_loose_parent_dir`, `it_creates_db_file_at_mode_0600_and_parent_at_0700`, `it_tightens_a_preexisting_loose_storage_parent_dir`. |
| MD-07 | fixed   | `68c8cad` | `/ws` is now registered with `any()` so non-GET methods hit the Origin+token check instead of falling through to the SPA handler. |
| LO-01 | fixed   | `68c8cad` | Browser-auto-open task now polls `TcpStream::connect` against the bind address (up to 2s, 50ms steps) before calling `open::that`. Eliminates the cold-boot "connection refused" race. |
| LO-02 | fixed   | `68c8cad` | `tracing_subscriber` init moved AFTER `Cli::parse()` so `--help` / `--version` paths cannot emit startup log noise. |
| LO-03 | fixed   | `68c8cad` | `App.tsx` now tracks a `healthError` state and renders `unreachable — <msg>` inline with red styling instead of silently `console.error`-ing and showing `loading…` forever. |
| LO-04 | fixed   | `68c8cad` | `it_starts_server_and_serves_health` replaced the fixed 400ms sleep with a 100-iteration / 50ms poll loop (5s budget). Mirrors the `ws_auth.rs` readiness pattern. |

### Test count summary

Phase 0 test suite grew from **9 tests** (verifier baseline) to **28 tests**
after fixes (+19 regression tests across BL/HI/MD/LO findings). All 28 pass
on `gsd/autonomous` HEAD with `--D warnings` clippy clean.

### Scope/deferral notes

None deferred. All 17 in-scope findings were addressed within Phase 0
boundaries. Note that some closely-related fixes shipped together in a
single commit when they touched the same file (e.g., MD-06 has two halves
— session.rs and storage.rs — committed separately because they belong to
different logical layers).

_Fix log generated by gsd-code-fixer at 2026-06-25T15:57:00Z._
