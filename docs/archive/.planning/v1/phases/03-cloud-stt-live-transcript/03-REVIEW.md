---
phase: 03-cloud-stt-live-transcript
reviewed: 2026-06-25T18:16:00-07:00
depth: deep
files_reviewed: 16
files_reviewed_list:
  - crates/yogurt-stt/src/lib.rs
  - crates/yogurt-stt/src/deepgram.rs
  - crates/yogurt-stt/tests/deepgram_mock.rs
  - crates/yogurt-stt/Cargo.toml
  - crates/yogurt-server/src/meetings.rs
  - crates/yogurt-server/src/ws.rs
  - crates/yogurt-server/src/routes.rs
  - crates/yogurt-server/src/lib.rs
  - crates/yogurt-server/tests/meeting_rest.rs
  - crates/yogurt-server/tests/meeting_ws.rs
  - crates/yogurt-server/tests/e2e_synthetic_audio.rs
  - web/src/lib/ws.ts
  - web/src/lib/ws.test.ts
  - web/src/components/TranscriptLine.tsx
  - web/src/components/TranscriptDock.tsx
  - web/src/components/TranscriptDock.test.tsx
  - web/src/routes/Meeting.tsx
  - web/src/App.tsx
  - web/src/App.test.tsx
  - web/src/index.css
findings:
  blocker: 5
  warning: 12
  info: 7
  total: 24
status: issues_found
---

# Phase 3: Code Review Report

**Reviewed:** 2026-06-25T18:16-07:00
**Depth:** deep
**Files Reviewed:** 16+ (source) and 5 tests
**Status:** issues_found — multiple blocker-tier defects in WS auth, supervisor lifecycle, Deepgram backpressure, and transcript wire-protocol correctness.

## Summary

Phase 3 is the project's first end-to-end pipeline (mic+system → Deepgram → broadcast → WS → React dock). The structural skeleton works (REST + WS round-trip tests pass), but the code carries **five blocker-tier defects** that will surface as production bugs the moment a real user opens a meeting:

1. The per-meeting WS endpoint `/ws/meetings/{id}` ships **without Origin or session-token auth** — any localhost-reachable page (including SSRF-via-link-preview, third-party browser tabs, image preloads in HTML email rendered locally, etc.) can subscribe to any meeting's transcript. The Phase 0 lockdown that prompted CR-02/BL-02 was undone here. The comment claiming "future hardening pass" parks this risk indefinitely.
2. Deepgram adapter has **no backpressure or reconnect** path. If Deepgram's WS stalls or the per-channel `mpsc<AudioChunk>(64)` fills (~1.3 s of audio at 50 fps), the upstream broadcast pump `await`s on `dest.send(...)`, blocking the audio pump entirely — every subsequent frame for **both channels** sits in the broadcast buffer until lag fires. A transient cloud blip silently kills the transcript for the rest of the meeting because the supervisor returns `Ok(())` with no restart.
3. The Deepgram transcript-wire-protocol parse reads `is_final` and `start` at the **frame top level**, but Deepgram emits both inside the `channel` object (and `is_final` is actually `speech_final` for end-of-utterance vs interim). Every Deepgram-served partial will be treated as final-at-ts-0; in production every interim line will render locked at 00:00:00. The mock test passes only because the mock fixture matches the *wrong* parse, not the real Deepgram wire shape.
4. The supervisor's audio→STT adapter exits as soon as **either** mic or system stream closes (RAII drop), not when both close. If the SCK system stream drops first (common when no app is producing audio), the mic-only path is silently terminated; the meeting appears to lose half its transcript while the user is still speaking.
5. The audio capture thread is `std::thread::spawn` without a `JoinHandle`. On `stop()` the supervisor task is aborted (which drops `_shutdown_tx`), but the OS thread can outlive process-shutdown teardown by ~50 ms each. With multiple back-to-back start/stop calls in one process (REST tests, future UI flicker), threads accumulate and the `AudioStream` `Drop` (which stops SCK + cpal) is non-deterministic.

Beyond those, the React `useTranscriptWs` hook is **missing the reconnect logic** the 03-03 PLAN promised ("3 attempts with exponential backoff"); the `mergeEvent` key strategy in `TranscriptDock` reuses `ts_ms` across partial→final transitions which silently breaks React reconciliation; `redact_token_in_uri` is dead code (only the WS handler ever sees a token query, and the handler doesn't log the URI); and a CSS animation class (`.dock-closed`) is declared in `index.css` and never referenced.

## BLOCKERS

### BL-01 — `/ws/meetings/{id}` has NO auth (Origin or token)

**File:** `crates/yogurt-server/src/ws.rs:164-179`, `crates/yogurt-server/src/routes.rs:48`
**Code:**
```rust
pub async fn ws_meeting_handler(
    ws: WebSocketUpgrade,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_meeting_socket(socket, id, state))
}
```
And the route mount:
```rust
.route("/ws/meetings/{id}", get(ws_meeting_handler))
```

**What's wrong:**
- The Phase 0 `/ws` endpoint enforces (a) Origin allowlist and (b) session-token validation **before** `ws.on_upgrade(...)` (see `ws.rs:58-91`). The Phase 3 sibling at `/ws/meetings/{id}` does **neither**.
- The author left a comment ("future hardening pass — likely Phase 5") that parks this gap, but the route is *live and reachable* in the binary that ships at the end of Phase 3.
- Beyond Origin, the WS handler doesn't even check that `:id` belongs to a meeting the requester is authorized to view — any caller with the meeting UUID can attach.

**Consequence:** Any localhost-reachable page (third-party browser tab, image preload in a locally-rendered HTML email, SSRF-via-link-preview, Slack/Discord embeds following preview-bots that touch `localhost:7878`, malicious npm postinstall fetch) can subscribe to a meeting's live transcript with zero credentials. The transcript broadcasts complete S→C frames with every word the user speaks — this is the most privacy-sensitive surface in the whole product, and it has no auth at all. This directly violates PRD §7 ("audio never leaves machine unless user opts into cloud STT") in spirit: while audio doesn't leak, the *transcript of that audio* leaks to any page that knows the meeting UUID (which is enumerable: UUID v7 is timestamp-prefixed and predictable within a window).

**Recommended fix:**
- Wire `ws_meeting_handler` through the same Origin allowlist + token validation `ws_handler` uses. Lift the auth into a shared helper:
```rust
pub async fn ws_meeting_handler(
    State(state): State<AppState>,
    Query(params): Query<WsParams>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    ws: WebSocketUpgrade,
) -> Response {
    if let Err(resp) = enforce_ws_auth(&state, &headers, &params) {
        return resp;
    }
    ws.on_upgrade(move |socket| handle_meeting_socket(socket, id, state))
}
```
where `enforce_ws_auth` is factored out of the existing `ws_handler`.
- Do this **before** end of Phase 3; do not ship the binary with this gap.

---

### BL-02 — Deepgram adapter has no backpressure handling; one stall kills the whole pipeline

**File:** `crates/yogurt-stt/src/deepgram.rs:55-83`
**Code:**
```rust
loop {
    let chunk = match audio_rx.recv().await { ... };
    let dest = match chunk.channel {
        Channel::Mic => &mic,
        Channel::System => &sys,
    };
    if dest.send(chunk).await.is_err() {  // ← awaits forever if the per-channel mpsc is full
        tracing::warn!("deepgram pump: session task gone, exiting");
        break;
    }
}
```
Combined with the per-channel writer:
```rust
tokio::spawn(async move {
    while let Some(chunk) = rx.recv().await {
        let bytes = i16_slice_to_le_bytes(&chunk.samples);
        if write.send(Message::Binary(bytes)).await.is_err() { ... }  // ← also unbounded await
    }
});
```

**What's wrong:**
- The per-channel `mpsc::channel(64)` holds ~1.3 s of audio at 50 fps. If Deepgram pauses sending acks (their WS is fully half-duplex, but a stalled writer back-pressures via the TCP send buffer) or the WS round-trip rises, the mpsc fills and `dest.send(...)` `.await`s indefinitely.
- While the pump awaits on (say) `mic`, the `audio_rx.recv().await` cannot fire, so **system-channel chunks** also stop flowing. The audio broadcast then fills (256-slot buffer = ~5 s), then `Lagged` errors fire and chunks are dropped.
- There is **zero reconnect logic.** If the Deepgram WS drops (network blip, 401 from invalid key, 429 rate limit), the spawned reader sees `Err(...)` and `return`s. The writer task drains its mpsc until empty then `close()`s. The pump's `dest.send(...)` then returns `Err` and the pump `break`s. `Stt::start` returns `Ok(())` — the supervisor sees no error, the meeting stays "running" in the registry, and the transcript silently stops for the rest of the meeting.
- HTTP-level Deepgram errors (401, 429, malformed Authorization header) surface as `connect_async` `Err` only at startup. After that, an authentication-revoke mid-meeting (admin rotates the key) silently kills the transcript.

**Consequence:** Any of: a transient cloud blip, a Wi-Fi handoff, a Deepgram rate-limit, a 30-second NAT timeout on the user's home router, or a key-rotation event will permanently kill the transcript without surfacing any error to the UI. The `useTranscriptWs` `connected` indicator will still show "● connected" because the *browser*'s WS to the server is fine — only the upstream Deepgram WS died.

**Recommended fix:**
- Use `try_send` on the mpsc and drop chunks (with a `tracing::warn!`) when full, rather than awaiting. Dropping a 20 ms chunk is degraded behavior; blocking the pump is broken behavior.
- Wrap the connect/read loop in a retry-with-backoff (3 attempts as the 03-01 spec implies, then surface a meaningful error). Emit a synthetic `TranscriptEvent` (e.g. `{text: "[stt disconnected, retrying]", channel: Mic, is_final: true}`) so the dock can render the dropout instead of silently freezing.
- Detect HTTP upgrade failures (`tokio_tungstenite::Error::Http(...)`) and map 401/403 → "bad API key", 429 → "rate limited"; bubble those up so the UI can show a clear message.

---

### BL-03 — `parse_deepgram_event` reads `is_final` and `start` at the wrong JSON path; production transcripts will be mis-tagged and timestamped 00:00:00

**File:** `crates/yogurt-stt/src/deepgram.rs:157-175`
**Code:**
```rust
let alt = v.get("channel")?.get("alternatives")?.get(0)?;
let transcript = alt.get("transcript")?.as_str()?.trim();
...
let start_s = v.get("start")?.as_f64().unwrap_or(0.0);
let is_final = v.get("is_final").and_then(|x| x.as_bool()).unwrap_or(false);
```

**What's wrong:**
- Deepgram's `Results` JSON frames have the shape:
  ```json
  {
    "type": "Results",
    "channel_index": [0,1],
    "duration": 1.04,
    "start": 0.0,
    "is_final": false,
    "speech_final": false,
    "channel": { "alternatives": [...] }
  }
  ```
  So `start` and `is_final` *do* exist at the top level — that part is correct. **However**: in Phase 3 the project's `TranscriptEvent.is_final` is documented as "the engine considers this segment locked in" (lib.rs:58) which maps to Deepgram's `speech_final` (true at end-of-utterance), not `is_final` (true on every interim that won't change). The PRD §5.2 "partial→final stabilization" UX behavior expects the *speech-final* semantics: a final replaces the partial in-place. Using `is_final` instead will mark a long single utterance as a sequence of locked finals (because Deepgram emits `is_final: true` after each ~6 s window even mid-sentence), causing the UI to append rather than replace.
- Independently, the parse is **silently lossy on the channel index**. Deepgram in dual-channel mode emits `channel_index: [0, total]` and the adapter passes the wrong `Channel` label to the parser when the same WS is multi-channel. The current code opens one WS per channel so this is OK in isolation, but it's a contract footgun: if anyone reworks to multiplex (cost saving — see the 2× billing concern in `deepgram.rs:9-10`), the channel will mistag.
- The mock fixture (`deepgram_mock.rs:49-55`) hand-rolls a Deepgram-shape JSON that exactly matches the buggy parse. The test asserts on its own fixture, not on Deepgram's actual wire shape. There is **no fixture sourced from real Deepgram output** (e.g. `tests/fixtures/deepgram-results.json` captured from a smoke run).

**Consequence:** Once pointed at the real Deepgram API, every partial that Deepgram considers "won't change" (interim final, the dominant case) will render as a brand-new locked line in the dock instead of replacing the in-progress partial. The dock will fill with duplicate-ish lines, the auto-scroll will jitter, and the user will think "this is broken." The 03-02 SUMMARY's claim that the e2e test "validates the pipeline" is misleading — the mock matches the parse, not the API.

**Recommended fix:**
- Capture 5–10 real Deepgram `Results` frames into a checked-in fixture (`tests/fixtures/deepgram-*.json`) during the Phase 3 manual smoke run.
- Decide which Deepgram field maps to `TranscriptEvent.is_final`:
  - `speech_final: true` — end-of-utterance (recommended; matches the UX intent).
  - `is_final: true` — Deepgram's "I won't revise this transcript window".
- Read both and pick deliberately:
```rust
let is_final = v.get("speech_final").and_then(|x| x.as_bool())
    .or_else(|| v.get("is_final").and_then(|x| x.as_bool()))
    .unwrap_or(false);
```
- Add a unit test against the captured fixture, not against a hand-rolled mock that mirrors the parser.

---

### BL-04 — Audio adapter exits when **either** stream closes; mic-only or system-only continuation breaks

**File:** `crates/yogurt-server/src/meetings.rs:202-247`
**Code:**
```rust
loop {
    tokio::select! {
        res = mic_rx.recv() => match res {
            ...
            Err(broadcast::error::RecvError::Closed) => {
                tracing::info!("audio adapter: mic stream closed");
                break;  // ← kills the WHOLE adapter on mic close
            }
        },
        res = sys_rx.recv() => match res {
            ...
            Err(broadcast::error::RecvError::Closed) => {
                tracing::info!("audio adapter: system stream closed");
                break;  // ← same for system
            }
        },
    }
}
```

**What's wrong:**
- A `select!` arm returning `Closed` `break`s out of the loop, terminating the adapter even though the *other* channel may still have frames.
- In real SCK behavior, the `SCStream` for system audio can stop independently of cpal mic (e.g., user is on a meeting where no other app is producing audio, or the SCK stream got bounced by macOS for permission-related reasons). When `sys_rx` closes, the user's voice through `mic_rx` is silently dropped from that moment forward.
- The supervisor returns; `stt_handle.abort()` kills Deepgram; the dock shows "connected" but no new lines.

**Consequence:** Meetings with one-sided audio (user dictating without anyone joined; no system audio producer) will appear to stop transcribing partway through with no error message.

**Recommended fix:**
- Track `mic_closed` / `sys_closed` booleans; only `break` once **both** are true:
```rust
let mut mic_open = true;
let mut sys_open = true;
while mic_open || sys_open {
    tokio::select! {
        res = mic_rx.recv(), if mic_open => match res {
            Err(Closed) => mic_open = false,
            ...
        },
        res = sys_rx.recv(), if sys_open => match res {
            Err(Closed) => sys_open = false,
            ...
        },
    }
}
```
- Or, simpler: only `break` when the audio_tx send `Err`s (no receivers) — that already detects shutdown.

---

### BL-05 — Audio capture `std::thread::spawn` orphan: thread can outlive the meeting, and `start` cannot ensure the previous AudioStream is fully released before a new `start` opens a new one

**File:** `crates/yogurt-server/src/meetings.rs:146-169`
**Code:**
```rust
std::thread::spawn(move || {
    let stream = match yogurt_audio::start_capture() { ... };
    ...
    let _ = shutdown_rx.blocking_recv();
    drop(stream);
});
```

**What's wrong:**
- `std::thread::spawn` returns a `JoinHandle` that is dropped on the next line. The thread is detached — the `Registry` has no handle on it.
- `Registry::stop` aborts the tokio supervisor (which drops `_shutdown_tx`), but the OS thread then has to wake up `blocking_recv`, then drop `stream` (which per AUDIO-04 takes ~50 ms for SCK stop + drain). There is **no signal back** that the stream actually closed.
- Consequence chains:
  - **Start-Stop-Start in quick succession:** a user clicking Stop then immediately clicking Start (or a flaky UI re-issuing) can have two `AudioStream`s alive simultaneously, each holding SCK and cpal handles. macOS may reject the second SCK init, or grant it but produce duplicate Frames into now-orphan broadcasts.
  - **Process shutdown:** `axum::serve` returns on Ctrl-C; the tokio runtime drops; the std::thread is *detached* — it gets aborted abruptly in the middle of `drop(stream)`, skipping the SCK clean-stop and leaving macOS in a "stream still active" state that requires the next launch to wait for a TCC reset.
  - **Test cleanup:** `server.abort()` in `meeting_ws.rs`/`e2e_synthetic_audio.rs` does not wait for the std::thread to finish; under `cargo nextest` parallel test execution the orphan thread can race the next test's SCK init.
- Additionally, `oneshot::Receiver::blocking_recv` is not part of the public stable surface of older tokio versions; verify the workspace tokio version exposes it. If the runtime is dropped under the thread, this may panic.

**Recommended fix:**
- Hold the `JoinHandle` inside `Meeting` next to `task`:
  ```rust
  pub struct Meeting {
      ...
      pub capture_thread: Mutex<Option<std::thread::JoinHandle<()>>>,
  }
  ```
- In `stop()`: after `task.abort()`, take the JoinHandle and `join()` it (with a timeout) so the AudioStream Drop has finished before `start()` can re-enter.
- Use `tokio::task::spawn_blocking` instead of raw `std::thread::spawn` so the tokio runtime tracks the thread and won't tear it down out from under `drop(stream)` on shutdown.
- Add a state guard so a `start()` racing against a still-cleaning `stop()` waits or returns 409 Conflict.

---

## WARNINGS

### WR-01 — `useTranscriptWs` is missing reconnect logic

**File:** `web/src/lib/ws.ts:82-132`
**What's wrong:** The 03-03 plan specifies "3 attempts with exponential backoff" reconnect on `onclose`/`onerror`. The implementation sets `setConnected(false)` and does nothing else — the WS stays closed permanently until the user navigates away and back. Combined with BL-02, a single transient drop terminates the live transcript for the rest of the meeting.
**Recommended fix:** On `onclose` (when meetingId is still set and the unmount cleanup hasn't run), schedule a reconnect with `setTimeout(..., 500ms * 2^attempt)` up to 3 attempts. Reset the attempt counter on successful `onopen`. Track via a `useRef` to survive renders. Surface a "reconnecting…" state alongside `connected`.

### WR-02 — TranscriptDock list key reuses `ts_ms`; React reconciliation breaks on partial→final swap

**File:** `web/src/components/TranscriptDock.tsx:128-130`
**Code:**
```tsx
events.map((ev, i) => (
  <TranscriptLine key={`${ev.channel}-${ev.ts_ms}-${i}`} ev={ev} />
))
```
**What's wrong:** `ts_ms` comes from Deepgram's `start` field — it's the segment start time, **not unique per event.** When a partial is replaced with a final (mergeEvent at the same index), the key becomes `mic-1000-0` for both renders, which IS the same key — that part is fine. But two channels emitting at the exact same `start` (Deepgram aligns to its own clock for mic, but if mic and system happen to both start at 0.0 s the keys still differ via channel prefix; that's fine). The real defect: appending `-${i}` as a fallback masks bugs — if `mergeEvent` is ever changed and produces duplicates, you'll get duplicate keys but React will only warn. Worse, when a final later than ts=1000 replaces a partial at ts=1000 in `mergeEvent`, React reuses the DOM node correctly but `i` may shift if the array got reordered (it doesn't today but the key doesn't make the contract explicit).
**Recommended fix:** Generate a stable client-side `clientId` for each *new* event (a counter or `Math.random().toString(36)`) in `mergeEvent`; when replacing a partial in-place, **preserve the previous event's clientId** so React keeps the same DOM node and CSS transitions work. Use that id as the key.

### WR-03 — TranscriptDock auto-scroll uses `useEffect`, not `useLayoutEffect`

**File:** `web/src/components/TranscriptDock.tsx:40-44`
**What's wrong:** `useEffect` runs after the browser has painted the new line, then the scroll jumps. The user sees a one-frame flash of the unscrolled state (the new line below the viewport) before the scroll happens. The 03-03 spec implies smooth follow.
**Recommended fix:** Switch to `useLayoutEffect` so the scroll happens before paint. Side note: setting `scrollTop = scrollHeight` is jumpy; consider `scrollIntoView({behavior: "smooth"})` on a sentinel.

### WR-04 — TranscriptDock auto-scroll-pause threshold uses 24px; user "near bottom" detection has no debounce

**File:** `web/src/components/TranscriptDock.tsx:46-50`
**What's wrong:** `handleScroll` fires on every wheel event. A user reading mid-list will scroll up, scroll past 24px (sticky flips false), scroll back to within 24px (sticky flips true), and the next event will yank them to bottom — even if they're actively reading. There's no hysteresis. Also: scroll handlers on momentum-scrolling trackpads fire 60–120×/s; recalculating `scrollHeight` each call is cheap but a debounce would be defensive.
**Recommended fix:** Use asymmetric thresholds (enter-sticky < 8px; exit-sticky > 32px). Optionally only flip sticky-true after a 300 ms idle.

### WR-05 — `redact_token_in_uri` is dead code

**File:** `crates/yogurt-server/src/ws.rs:95-114`
**What's wrong:** No call site exists in the workspace; the WS handler logs warnings without including the URI. Either:
  - Wire it into a tower-http `TraceLayer` `on_request` to actually redact tokens in request logs, OR
  - Delete the function and its supporting comment block.
**Recommended fix:** Add `tower-http::trace::TraceLayer` with a custom `make_span_with` that calls `redact_token_in_uri(request.uri().to_string())`. Otherwise the BL-02-resolution claim that "tokens are REDACTED from tracing logs" is currently false — they're just never logged at all, which is fine but the dead function falsely advertises the protection.

### WR-06 — `Meeting.tsx` fetches `/api/meetings*` without the session token

**File:** `web/src/routes/Meeting.tsx:55, 72, 90`
**Code:**
```tsx
await fetch("/api/meetings", { method: "POST" });
await fetch(`/api/meetings/${meetingId}/start`, { method: "POST" });
```
**What's wrong:** `/api/meetings*` REST routes have **no `require_session_token` middleware** in `routes.rs:35-49` — they're publicly callable on localhost. The `audio_routes` block does require it, but the meetings group does not. So:
  1. Any localhost-reachable page can create/start/stop meetings without the token (CSRF-like via image preload — `fetch` is preflighted but `<form>` POST isn't).
  2. The frontend will work in dev because the routes are unauthenticated, but if/when Phase 5 (or anyone) adds the auth middleware to `/api/meetings*`, the React code will silently start getting 403s.
**Recommended fix:** Add `/api/meetings*` to the `audio_routes` middleware group (rename to `protected_routes`). Update `Meeting.tsx` to include the session token (read from `window.__YOGURT_TOKEN__` injected by the server template, or via a future `/api/session/me` endpoint).

### WR-07 — `stop` is documented as idempotent but returns 400 when the meeting is unknown

**File:** `crates/yogurt-server/src/meetings.rs:257-266`, `routes.rs:95-104`
**Code:**
```rust
pub async fn stop(&self, id: &MeetingId) -> Result<()> {
    let m = self.get(id).await.ok_or_else(|| anyhow!("meeting not found"))?;
    if let Some(t) = m.task.lock().await.take() { t.abort(); }
    Ok(())
}
```
**What's wrong:** Doc-comment says "Idempotent — calling on an already-stopped meeting is a no-op." That's true for an already-stopped *but still-registered* meeting. Calling on a non-existent meeting returns an error → 400. If a page reloads after the meeting got garbage-collected (Phase 7 will do this), the cleanup `stop()` call will surface a misleading 400 error to the user.
**Recommended fix:** Return `Ok(())` (or 200 with `{"status":"already_stopped"}`) when the meeting isn't found. The endpoint is mutation-shaped but semantically a delete-ish operation; 404 is appropriate only if you intend to differentiate, and the current code returns 400 either way (server error vs not-found is muddled).

### WR-08 — `start_meeting` returns 400 for everything — permission errors, missing key, race conditions all collapse to one status code

**File:** `crates/yogurt-server/src/routes.rs:81-90`
**What's wrong:** The handler converts every `Err` from `Registry::start` into HTTP 400 with `{error: <to_string>}`. The errors are heterogeneous:
- Missing API key → user action (set env var).
- TCC permission denied → user action (System Settings → Screen Recording).
- Audio device unavailable → transient retry.
- "Meeting already started" → client logic bug.
- Underlying SCK init failure → transient retry.

The UI cannot distinguish recoverable from non-recoverable, nor can it deep-link the user to System Settings.
**Recommended fix:** Introduce a small error enum (`StartMeetingError::{MissingApiKey, PermissionDenied, AlreadyStarted, AudioOpenFailed}`) and map to distinct HTTP statuses + machine-readable `{error_code, error_message}` shape.

### WR-09 — Concurrency: `Registry::start` holds the `task` mutex across an `await` that crosses the audio-readiness oneshot

**File:** `crates/yogurt-server/src/meetings.rs:118-176`
**What's wrong:** The function does:
```rust
if m.task.lock().await.is_some() { return Err(...); }  // (1) lock acquired, released
...                                                    // (2) NOT holding the lock
let (..., ready_rx) = oneshot::channel();
std::thread::spawn(move || { ... });
let (mut mic_rx, mut sys_rx) = ready_rx.await...?;     // (3) ~ms to ~seconds
...
*m.task.lock().await = Some(task);                     // (4) re-lock + write
```
Between (1) and (4), a second `start()` call for the same meeting will see `task: None`, pass the guard, and **race** the first call. Two audio threads will spin up, two STT sessions will subscribe — but only the second `task` survives in the slot (the first is leaked).
**Recommended fix:** Hold the `task` mutex from the existence check through the slot assignment (a single critical section), or use a `Mutex<Option<JoinHandle>>` `compare_exchange`-style pattern (insert a sentinel "starting" state).

### WR-10 — `WsParams::token` is silently bypassable on the meetings WS

**File:** `crates/yogurt-server/src/ws.rs:173-179`
**What's wrong:** The meeting WS handler doesn't even extract `Query<WsParams>` — it's signature-incomplete relative to its sibling. This is the structural mirror of BL-01; calling out separately because even if BL-01's auth is added, the handler signature needs the Query extractor *and* the headers extractor.
**Recommended fix:** Match `ws_handler`'s signature exactly, then factor the auth check into a helper.

### WR-11 — `i16_slice_to_le_bytes` allocates per chunk; bytemuck would be both faster and lower-risk

**File:** `crates/yogurt-stt/src/deepgram.rs:146-152`
**What's wrong:** Each ~20 ms mic chunk allocates a fresh `Vec<u8>` of 640 bytes, copies the i16s, and hands it to tungstenite which copies again into the WS frame buffer. At 50 fps × 2 channels this is constant allocation churn.
**Recommended fix:** `bytemuck::cast_slice::<i16, u8>(&samples).to_vec()` is one allocation, no element-wise copy. Or hand a slice into a reusable buffer.

(Performance, NOT in scope per review charter — but it's also a correctness consideration: every allocation is a chance to OOM in pathological lag conditions, and the broadcast already has 256 slots × 640 bytes per chunk.)

### WR-12 — `MeetingId = Uuid` but `routes.rs` uses `Path<Uuid>` directly; doc says "UUID v7" but no validation enforces v7

**File:** `crates/yogurt-server/src/meetings.rs:33`, `routes.rs:81, 95`
**What's wrong:** The registry creates IDs as `Uuid::now_v7()`, but the routes accept any UUID variant. A caller can POST to `/api/meetings/00000000-0000-0000-0000-000000000000/start` and get a clean 400 ("meeting not found") — fine — but the documentation/contract claim of "UUID v7" is not enforced at the boundary. The wider concern: registry uses `Uuid`, but the planner's prose mentions ULID (PRD §9). Decide and stick to one.
**Recommended fix:** Pick one and document; if Uuid v7 is the choice, validate the variant in a custom extractor.

---

## INFO

### IN-01 — `.dock-closed` CSS keyframe is declared but never applied

**File:** `web/src/index.css:120-131`
**What's wrong:** The slide-out animation class is defined but no React component ever sets `className="dock-closed"` — the dock is unmounted (`{open && ...}` in `TranscriptDock.tsx:82`) which prevents any exit animation. The closed-state animation is dead code.
**Recommended fix:** Either implement an exit animation (use `framer-motion`, or a `useEffect` deferred unmount) or delete `.dock-closed` + `@keyframes slideOutRight`.

### IN-02 — Duplicate `slideInRight` keyframes (Phase 1 token + Phase 3 hand-rolled)

**File:** `web/src/index.css:90-93` (Phase 1 `slide-in-right` token) vs `web/src/index.css:115-118` (Phase 3 `slideInRight`)
**What's wrong:** The Phase 1 `@theme` block declares `--animate-slide-in-right: slide-in-right 340ms cubic-bezier(0.2, 0.7, 0.2, 1);` with kebab-case keyframe `slide-in-right`. Phase 3 adds a fresh `slideInRight` (camelCase) and a `.dock-open` class that ignores the Phase 1 token entirely. Two functionally identical keyframes with different names.
**Recommended fix:** Use the Phase 1 token: `className="animate-slide-in-right"` on the panel. Delete the Phase 3 keyframes.

### IN-03 — `@tiptap/extension-markdown` is documented as v3 standard but Meeting.tsx uses StarterKit only

**File:** `web/src/routes/Meeting.tsx:46-50`
**What's wrong:** The CLAUDE.md stack pins `@tiptap/extension-markdown` for "bidirectional parse/serialize" but Phase 3 uses only StarterKit. Acceptable since notes are Phase 4 territory, but the Phase 3 editor will not round-trip markdown when user pastes from Granola/Notion. Flag for Phase 4.

### IN-04 — `App.test.tsx` asserts `MockWebSocket.lastInstance toBeNull` after navigation

**File:** `web/src/App.test.tsx:86`
**Code:**
```ts
await waitFor(() => expect(MockWebSocket.lastInstance).toBeNull());
```
**What's wrong:** The Meeting view mounts `<TranscriptDock meetingId={null}/>`, which the hook short-circuits before constructing a WS — so `lastInstance` stays `null` (the value set in `beforeEach`). The assertion is testing nothing useful (it's `null` from the start). It would only catch a regression where the hook started connecting on `meetingId=null`. Fine, but should be commented as such or replaced with a more direct check.

### IN-05 — `e2e_synthetic_audio.rs` bypasses the Stt layer entirely

**File:** `crates/yogurt-server/tests/e2e_synthetic_audio.rs:55-62`
**What's wrong:** The test calls `m.transcript_tx.send(...)` directly. This tests the broadcast → WS serialize → tungstenite path (`server-side lag`, as the test name says), which is fair. But the test name `e2e_synthetic_audio` implies it exercises audio → Stt → WS, which it does not. Rename to `it_measures_broadcast_to_ws_latency` or actually push synthetic audio through a mock Stt impl.

### IN-06 — `meeting_rest.rs` uses hard-coded ports 17890/17891 (collision risk with other devs / CI parallelism)

**File:** `crates/yogurt-server/tests/meeting_rest.rs:41, 71`
**What's wrong:** Hard-coded ephemeral ports are an antipattern under `cargo nextest --threads`. The mock test in `deepgram_mock.rs` uses `127.0.0.1:0` (correct pattern). Two devs on a shared CI runner will collide.
**Recommended fix:** Bind to `127.0.0.1:0`, then read `listener.local_addr().port()`, then pass that into the client URLs. The existing helper would need to bind and pass the listener into `run_with_config` instead of letting `run_with_config` bind.

### IN-07 — `TranscriptLine` hardcodes color hex literals instead of using `--color-ink` / `--color-grey` Tailwind utilities

**File:** `web/src/components/TranscriptLine.tsx:20-21`
**What's wrong:** The Phase 1 design system put these in `@theme`; using inline `#211D18` / `#A89F90` defeats the dark-mode-ready token system documented in PRD §16.2. Same nitpick applies to `TranscriptDock.tsx:5-9`.
**Recommended fix:** Use `text-ink` / `text-grey` Tailwind utilities (the `@theme` block emits these). Reserve inline style only for things Tailwind cannot express.

---

## Cross-cutting observations

- **Test fidelity vs real-world:** Every Phase 3 test mocks something critical: ws.test.ts and TranscriptDock.test.tsx stub `globalThis.WebSocket`; deepgram_mock.rs's hand-rolled mock is parser-shape-aligned not API-shape-aligned; the e2e test bypasses Stt. This is fine for unit tests but creates a confidence gap that only a real Deepgram smoke run can close. The plan acknowledges this (Task 3.10 Step 3 — "manual smoke against real API"), but the smoke run is not gated.
- **Comment-driven security debt:** `ws.rs:166-172` parks the auth gap with prose ("future hardening pass"); `routes.rs:69-71` parks SQLite migration; `meetings.rs:50-51` parks Phase 7 persistence. Each comment is reasonable in isolation; collectively they're load-bearing future-Phase IOUs that no one is tracking centrally. Recommend a `.planning/deferred-items.md` (which exists for Phase 3 — extend it).
- **Phase 0 + Phase 2 regression check:** No Phase 1/2 tests were modified by Phase 3 commits; Phase 2 audio crate is locked as required. The `index.css` Phase 3 additions are additive (new classes, no edits to existing tokens), so Phase 1 visual regression risk is minimal.

---

_Reviewed: 2026-06-25T18:16-07:00_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: deep_
