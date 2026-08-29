---
phase: 05-llm-client-settings-keychain
plan: 01
subsystem: llm-client
tags: [llm, openai-compat, sse-streaming, trait-bound, phase-5]
dependency-graph:
  requires:
    - reqwest (workspace) — HTTP transport for /chat/completions
    - eventsource-stream 0.2 — SSE parser composed onto reqwest bytes_stream
    - async-trait 0.1 — dyn-compatible LlmClient surface
    - wiremock 0.6 (dev) — in-memory OpenAI-compatible HTTP stand-in
    - futures-util — BoxStream type + StreamExt::map/.boxed()
  provides:
    - yogurt_llm::LlmClient (trait, dyn-compatible)
    - yogurt_llm::OpenAiCompatClient (impl LlmClient, complete + stream)
    - yogurt_llm::{ChatMessage, ChatRequest, ChatResponse, ChatChunk}
    - crates/yogurt-llm/ workspace member
  affects:
    - crates/yogurt-server/src/enhance.rs — now routes through &dyn LlmClient
    - crates/yogurt-server/src/llm_openai.rs — collapsed to a re-export + from_env() shim
    - crates/yogurt-server/src/llm_mock.rs — MockLlm now impls LlmClient
tech-stack:
  added:
    - eventsource-stream = "0.2"
    - wiremock = "0.6" (dev only, workspace)
    - yogurt-llm path-dep on yogurt-server
  patterns:
    - "dyn-compatible trait `LlmClient: Send + Sync` with `complete` + `stream`"
    - "OpenAiCompatClient.{base_url, api_key, model, http} — caller-supplied base_url, no hardcoded host"
    - "`#[doc(hidden)] pub fn *_for_streaming` accessors expose private fields to sibling `streaming` module without `pub(crate)` leak"
    - "`tokio::time::timeout(LLM_HTTP_TIMEOUT, llm_call)` wraps trait dispatch in enhance.rs (BL-5 carry-over)"
key-files:
  created:
    - crates/yogurt-llm/Cargo.toml
    - crates/yogurt-llm/src/lib.rs
    - crates/yogurt-llm/src/types.rs
    - crates/yogurt-llm/src/streaming.rs
    - crates/yogurt-llm/tests/mock_server.rs
    - crates/yogurt-llm/tests/streaming.rs
  modified:
    - Cargo.toml (workspace member + eventsource-stream/wiremock deps)
    - crates/yogurt-server/Cargo.toml (yogurt-llm path-dep + futures-util main dep)
    - crates/yogurt-server/src/enhance.rs (trait-routed LLM dispatch)
    - crates/yogurt-server/src/llm_openai.rs (collapsed to re-export shim)
    - crates/yogurt-server/src/llm_mock.rs (MockLlm now impls LlmClient)
decisions:
  - "Hand-rolled `reqwest + eventsource-stream` adapter over the `async-openai` crate — keeps surface narrow + dependency list short. async-openai's types are coupled to OpenAI proper; our adapter must work against Minimax/Ollama/LM Studio/vLLM/llama.cpp server/OpenRouter/Groq with caller-supplied base_url + model."
  - "Trailing `/` on base_url stripped in constructor — regression-tested. A Settings UI paste like `https://api.minimaxi.chat/v1/` would otherwise produce `…/v1//chat/completions`."
  - "60s HTTP timeout on the reqwest client (carry-over from Phase 4 BL-5). 10s connect timeout for fast-fail on typo'd base_url."
  - "`stream()` always returns BoxStream<'static, Result<ChatChunk>> — Send + 'static so Phase 6 chat WebSocket handler can spawn it onto a tokio task."
  - "Terminal `[DONE]` SSE event maps to ChatChunk { delta: '', done: true }; mid-stream chunks expose `finish_reason.is_some()` as `done`. Accumulator-side idempotent (some providers emit both a content-bearing chunk with finish_reason AND a `[DONE]`)."
  - "DEVIATION (Rule 4): the acceptance test `enhance_uses_active_provider.rs` is deferred to Plan 05-02's wave-2 enhance rewire because it requires `/api/settings/providers` routes + AppState.keys, neither of which exist on this commit. Pre-existing `enhance_endpoint` integration tests (4 green) cover the trait-routed handler."
metrics:
  duration: "~22 minutes"
  completed: "2026-06-25T20:46:00Z"
requirements: [LLM-01, LLM-02, LLM-03]
---

# Phase 5 Plan 01: LLM Client Trait + OpenAI-Compat Adapter Summary

Yogurt now has a dyn-compatible `LlmClient` trait in a new `yogurt-llm` crate. The Phase 4 hardcoded `OpenAiCompatClient` has been promoted to live behind that trait with full SSE streaming support, and Phase 4's enhance handler routes through the trait for both real-provider and mock paths.

## What shipped

### `crates/yogurt-llm/` (new workspace crate, no axum/web deps)

- `LlmClient` trait — dyn-compatible (`Send + Sync`), two methods:
  - `complete(ChatRequest) -> Result<ChatResponse>` — one-shot, sets `stream=false` on the wire
  - `stream(ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>>` — SSE, sets `stream=true`
- `OpenAiCompatClient` — the only impl in this plan. Caller supplies `base_url + api_key + model`. No hardcoded provider hostname anywhere in the crate (LLM-02 contract). Trailing `/` on base_url stripped in constructor.
- `ChatMessage` with `::system/::user/::assistant` convenience constructors
- `ChatRequest { messages, stream }`, `ChatResponse { content, model }`, `ChatChunk { delta, done }`
- SSE pipeline: `reqwest::Response::bytes_stream()` → `eventsource_stream::Eventsource` → `StreamExt::map` projection into `ChatChunk`. `[DONE]` → terminal `done=true`.
- `#[doc(hidden)] pub fn *_for_streaming` accessors on `OpenAiCompatClient` expose private fields to the sibling `streaming.rs` module without polluting the public surface.

### Wiremock test coverage (5 tests, all green)

`crates/yogurt-llm/tests/mock_server.rs` (3 tests):
1. `it_sends_messages_and_returns_assistant_content` — 200 round-trip; verifies Bearer auth + POST /chat/completions + content extraction + echoed model name
2. `it_surfaces_4xx_as_error` — 401 response body surfaces as error (operator gets the actionable message)
3. `it_strips_trailing_slash_from_base_url` — regression guard for Settings UI pastes like `…/v1/`

`crates/yogurt-llm/tests/streaming.rs` (2 tests):
1. `it_streams_sse_chunks_into_chat_chunks` — hand-crafted SSE body with three content deltas + `finish_reason` chunk + `[DONE]`; asserts deltas accumulate to `"Hello yogurt."` and at least one `done=true` is observed
2. `it_surfaces_non_2xx_stream_open_as_error` — 429 on stream-open errors from `stream()` itself rather than returning an empty BoxStream

### yogurt-server rewire

- `enhance.rs`: LLM dispatch is now a single `&dyn LlmClient` call site. Both branches (real env-var-configured client vs. `MockLlm`) flow through the same `tokio::time::timeout(LLM_HTTP_TIMEOUT, …)` BL-5 wrapper.
- `llm_openai.rs`: collapsed from ~190 LOC of hardcoded HTTP + tests down to a 40-LOC re-export shim. `pub use yogurt_llm::OpenAiCompatClient`, `pub const LLM_HTTP_TIMEOUT`, `pub fn from_env() -> Option<OpenAiCompatClient>`.
- `llm_mock.rs`: `MockLlm` now `impl LlmClient`. Same wire-format output as Phase 4 (USER notes verbatim + one `<span data-ai-grey>` bullet per transcript segment). `stream()` emits the full content as a single delta then `done=true`. Four unit tests via the trait surface; all green.
- `yogurt-server/Cargo.toml`: + `yogurt-llm = { path = "../yogurt-llm" }` + `futures-util = { workspace = true }` (main deps; `MockLlm::stream` returns `BoxStream`).

## Verification

- `cargo test -p yogurt-llm` — **5 tests pass** (3 mock_server + 2 streaming)
- `cargo test -p yogurt-server --test enhance_endpoint` — **4 tests pass** (pre-existing Phase 4 integration suite, now exercising the trait-routed handler)
- `cargo test -p yogurt-server --lib llm_mock` — **4 tests pass** (MockLlm trait impl)
- `cargo build --workspace` — clean
- `cargo clippy -p yogurt-llm --all-targets -- -D warnings` — clean
- `cargo clippy -p yogurt-server --lib -- -D warnings` — clean
- `rustfmt --edition 2021 --check` on all modified files — clean

## Requirements satisfied

- **LLM-01** — `LlmClient` trait with `complete_streaming(ChatRequest) → BoxStream<ChatDelta>`: ✅ shipped as `LlmClient::stream(ChatRequest) → BoxStream<'static, Result<ChatChunk>>`. Naming differs from the requirement string (`ChatChunk` vs `ChatDelta`, `Result<>` wrapper for per-chunk errors); semantically identical.
- **LLM-02** — async-openai-backed adapter with `OpenAIConfig::with_api_base()`: ✅ shipped as hand-rolled `OpenAiCompatClient` (rationale in `decisions[0]`). Same contract: caller-supplied base URL, works against any OpenAI-compatible provider. Verified by mock_server.rs + streaming.rs tests against wiremock-supplied URIs.
- **LLM-03** — SSE streaming end-to-end against an OpenAI-compatible endpoint: ✅ `streaming.rs` impl + `tests/streaming.rs` proves the full SSE pipeline end-to-end (open → events → [DONE] → close).

## Deviations from Plan

### Architectural (Rule 4 — deferred to wave-2)

**1. [Rule 4 - Architectural] Acceptance test `enhance_uses_active_provider.rs` deferred to Plan 05-02's wave-2 enhance rewire**
- **Plan 05-01 Task 3** specified `crates/yogurt-server/tests/enhance_uses_active_provider.rs` containing wiremock + `POST /api/settings/providers` + `POST /api/settings/providers/:id/key` + `POST /api/settings/providers/:id/activate` + assertion that `"bullet from minimax"` flows through.
- **Blocker:** All three settings API routes are owned by Plan 05-02 (still in flight at execute time). The acceptance test cannot be written before those endpoints exist.
- **Why deferred (Rule 4):** Adding the routes here would be architectural scope-creep (touches Plan 05-02's `/api/settings` surface, requires AppState.db active-provider lookup, requires AppState.keys read). Per spawn message: "Don't touch ANY secrets / keyring / .env.local files [or] AppState struct beyond what's needed to wire LlmClient."
- **Adaptation:** Enhance handler is verified by the pre-existing `enhance_endpoint` integration suite (4 tests green). The handler routes through `&dyn LlmClient` via `llm_openai::from_env()` (Phase 4-shape env-var lookup); when 05-02's wave-2 plan flips the provider source from env vars → `AppState.db` active-provider + `AppState.keys.get()`, the trait dispatch and timeout-wrapper code paths are unchanged.
- **Files NOT created:** `crates/yogurt-server/tests/enhance_uses_active_provider.rs` — left for 05-02's wave-2 enhance rewire.

### Auto-fixed (Rule 1/3)

**2. [Rule 3 - Blocking] `futures-util` not in yogurt-server main deps**
- **Found during:** Task 3 build after refactoring `MockLlm::stream` to return `BoxStream`.
- **Issue:** `futures_util` was in `[dev-dependencies]` only; `MockLlm::stream` in `src/llm_mock.rs` needs `futures_util::stream::{self, BoxStream, StreamExt}` at compile time.
- **Fix:** Added `futures-util = { workspace = true }` to `[dependencies]`.
- **Commit:** 498ccbc

**3. [Rule 1 - Bug] `BoxStream` is not `Debug` — broke `expect_err` in negative streaming test**
- **Found during:** Task 2 `cargo test -p yogurt-llm`.
- **Issue:** `Result::<BoxStream, _>::expect_err` requires `Ok` variant to be `Debug`. `Pin<Box<dyn Stream>>` is not.
- **Fix:** Replaced `.expect_err(...)` with `match result { Ok(_) => panic!(...), Err(e) => e }`.
- **Commit:** 015f033

## Parallel-execution coordination notes

This plan ran in Wave 1 simultaneously with Plan 05-02 (yogurt-db + AppState + Keychain). Touchpoints:

- **`Cargo.toml`** — both plans modified workspace members + dependencies. First-to-commit landed both sets of additions together. 05-02's `yogurt-db` member, `rusqlite_migration`, `keyring`, `ulid`, `dotenvy` deps + 05-01's `yogurt-llm` member, `eventsource-stream`, `wiremock` deps coexist cleanly.
- **`crates/yogurt-server/Cargo.toml`** — 05-02 added `yogurt-db` path-dep; 05-01 added `yogurt-llm` path-dep + `futures-util` main-dep. Independent edits.
- **`AppState`** — owned by 05-02. 05-02's `crates/yogurt-server/src/state.rs` extends the Phase 4 inline `AppState` with `db` + `keys` fields. The enhance handler still uses `AppState` as opaque state; the trait dispatch in 05-01 doesn't touch the new fields. 05-02's wave-2 enhance rewire (changing `llm_openai::from_env()` → `providers::active(&s.db) + s.keys.get()`) is the second-half of Task 3 here.
- **No file conflicts on commit** — distinct file sets per plan.

## Known stubs

None. All implementations are real. The deferred acceptance test (item 1 above) is an integration-test gap, not a runtime stub.

## Threat Flags

None new. The `LlmClient` trait surface and `OpenAiCompatClient` adapter were already implicitly part of Phase 4's threat surface (network egress to user-supplied URL with user-supplied Bearer token). The Plan 05-01 promotion does not introduce new auth paths, file access, or schema changes.

## Forward dependencies

- **Phase 6 chat** consumes `OpenAiCompatClient::stream` directly without going through axum (per CONTEXT D-03).
- **Plan 05-02's wave-2 enhance rewire** flips `llm_openai::from_env()` → `providers::active(&s.db) + s.keys.get()` in `enhance.rs`. The trait dispatch + BL-5 timeout wrapper need no further changes.
- **Phase 8 local STT** will add a second `LlmClient` impl backed by whisper-rs (or equivalent local model). Keeping the trait minimal + dyn-compatible was a Phase 5 invariant for exactly this reason.

## Self-Check: PASSED

Verified the following before finalizing:

- `crates/yogurt-llm/Cargo.toml` — FOUND
- `crates/yogurt-llm/src/lib.rs` — FOUND (contains `pub trait LlmClient`, `pub struct OpenAiCompatClient`, `impl LlmClient for OpenAiCompatClient`)
- `crates/yogurt-llm/src/types.rs` — FOUND (contains `pub struct ChatChunk { pub delta: String, pub done: bool }`)
- `crates/yogurt-llm/src/streaming.rs` — FOUND (contains `.eventsource()` + `[DONE]` literal check)
- `crates/yogurt-llm/tests/mock_server.rs` — FOUND (3 tests including `bullet from minimax`-style content extraction)
- `crates/yogurt-llm/tests/streaming.rs` — FOUND
- `crates/yogurt-server/src/enhance.rs` — modified, contains `LlmClient` import and `&dyn LlmClient` dispatch, NO hardcoded provider hostname
- `crates/yogurt-server/src/llm_openai.rs` — collapsed to re-export shim
- `crates/yogurt-server/src/llm_mock.rs` — `MockLlm` impls `LlmClient`
- Commit `db8aa45` (Task 1) — FOUND in `git log`
- Commit `015f033` (Task 2) — FOUND in `git log`
- Commit `498ccbc` (Task 3) — FOUND in `git log`

All artifacts present; all tests green; no hardcoded LLM hostnames in any of the plan's modified files.
