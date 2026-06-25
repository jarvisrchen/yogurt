# Phase 5: LLM Client + Settings + Keychain - Context

**Gathered:** 2026-06-25
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 4's hardcoded `OpenAiCompatClient` is promoted behind the `LlmClient` trait with `complete_streaming(ChatRequest) → BoxStream<ChatDelta>`. The full settings UI ships (Model / Transcription / Audio / General sidebar) with Keychain-backed API key storage that is eager-loaded at startup to prevent cold-boot hangs. User can paste any OpenAI-compatible base URL + API key + model and the same enhance pipeline runs against it.

Scope = LLM client trait + adapter, secrets infrastructure (Keychain + `.env.local` dev convenience), and the `/settings` page (sidebar + four sections). Phase 6 (chat) consumes the `stream()` method shipped here but is out of scope; Phase 7 (library/onboarding/states) and Phase 8 (local STT) cover the Local-transcription card actually working.

</domain>

<decisions>
## Implementation Decisions

### LLM client architecture
- **D-01:** Promote Phase 4's ~50 LOC hardcoded `OpenAiCompatClient` behind a new `LlmClient` trait. Keep the trait minimal: `async fn complete(req) -> ChatResponse` + `async fn stream(req) -> BoxStream<'static, Result<ChatChunk>>`.
- **D-02:** Adapter uses `reqwest` directly with `OpenAIConfig`-style `with_api_base()` pattern (the plan uses `OpenAiCompatClient::new(base_url, api_key, model)`). SSE streaming via `eventsource-stream` 0.2 (parsed into `ChatChunk { delta, done }`).
- **D-03:** Ship as a new crate `yogurt-llm` (no axum/web deps) so Phase 6 chat can consume it without going through HTTP. Tested via `wiremock`.

### Secrets & Keychain
- **D-04:** Use `keyring` 3.x. Wrap with an `ApiKeyStore` trait so handlers and tests share the same code path (`KeychainStore` for prod, `MemoryKeyStore` for tests).
- **D-05:** **EAGER-LOAD all Keychain secrets at server startup** into an `Arc<RwLock<Secrets>>`-style structure (the plan uses `Arc<dyn ApiKeyStore>` on `AppState` — equivalent for the cold-boot guarantee). Wrap the load with a 5s timeout. Request handlers MUST NEVER block on `keyring` calls during a request — this is the SET-10 cold-boot mitigation.
- **D-06:** Keychain entries are namespaced under `service="yogurt"` so uninstall+reinstall doesn't leak keys. Account name = provider ULID.
- **D-07:** API responses NEVER include the raw API key. Only the masked form `••••XXXX` (last 4 chars) is exposed via `api_key_masked`. This is asserted by a load-bearing test (`api_responses_never_include_the_raw_api_key`).

### Dev-mode `.env.local` convention (SET-11)
- **D-08:** `--dev` CLI flag triggers loading of `.env.local` at repo root via `dotenvy::from_filename(".env.local")` at the very top of `main()`. Failures are silently ignored.
- **D-09:** `bootstrap::seed_from_env()` runs after `AppState` is built, before serving. Maps `YOGURT_*_API_KEY` env vars to preset providers (Minimax, OpenAI, OpenRouter for LLM; Deepgram/AssemblyAI/Groq for STT). Default dev provider is `YOGURT_MINIMAX_API_KEY` against `https://api.minimaxi.chat/v1` (model `MiniMax-Text-01`).
- **D-10:** Bootstrap is idempotent: existing provider rows by name are skipped; first LLM provider seeded becomes active iff no other LLM provider is active.
- **D-11:** **Release builds MUST IGNORE `.env.local` entirely.** Only Keychain is read in release. Verified in test acceptance criteria.

### Settings UI layout
- **D-12:** `/settings` route. Two-column layout: 212px left sidebar + main content on the right. Per PRD §5.6.
- **D-13:** Sidebar sections: Model / Transcription / Audio / General. Active section = lilac background + blueberry text. Inactive = ink-on-paper hover.
- **D-14:** Sidebar footer:
  - Green "Local-only · on" matcha pill rendered iff no active provider has a non-localhost base URL.
  - JetBrains-Mono caption: `keys → macOS Keychain` / `data → ~/.yogurt/` (10px, neutral-500).

### Model section
- **D-15:** Active provider rendered as a 1.5px blueberry-bordered `ProviderCard` (rounded-xl, white bg, shadow-sm). Header: serif name + "ACTIVE" pill in blsoft/blue. Body: BASE URL + MODEL in mono. Footer: API KEY masked with `••••XXXX` + green matcha "✓ stored" badge.
- **D-16:** Inactive providers stack below as plain `ProviderRow`s: name + mono base_url + "✓ key"/"no key" indicator + "Set active" / "Remove" links.
- **D-17:** Preset chips for Ollama, LM Studio, OpenRouter, Minimax, OpenAI rendered as dashed-border, font-mono, uppercase pills. Clicking creates a new provider via `POST /api/settings/providers`. "+ Add" link for custom (UI authoring polish deferred).
- **D-18:** Provider switching: `POST /api/settings/providers/:id/activate` atomically deactivates all other LLM providers and activates the target (enforced via partial unique index on `providers(kind) WHERE is_active=1`).

### Transcription / Audio / General sections
- **D-19:** Transcription section = Cloud (selected, blueberry-bordered) + Local (greyed-out, "Coming in v1" matcha badge). Local card actually working is Phase 8.
- **D-20:** Audio section = input device dropdown wired to Phase 2's `GET /api/audio/devices`, "System default" as the empty value.
- **D-21:** General section = port input (number, 1024-65535) with onBlur save + "Open browser on start" checkbox toggle. Port change applies on next `yogurt start` (caption explains).

### Config persistence
- **D-22:** General settings persist in SQLite `settings` KV table (key/value strings). Loaded via typed `General { port, open_browser_on_start, audio_input_device }` struct. Saved via `GeneralPatch` (all-optional) → upsert via `INSERT … ON CONFLICT(key) DO UPDATE`.
- **D-23:** SQLite tables added in this phase: `providers` (id ULID, name, base_url, model, kind, is_active, created_at) + `settings` (key, value). Migrations live in `crates/yogurt-db/migrations/V001__initial.sql`. Phase 6 adds V002 with `meetings`/`chat_messages`.
- **D-24:** `~/.yogurt/config.toml` (per requirement SET-09) — the SQLite `settings` table is the source of truth; if a TOML mirror is required, the executor produces it as a read-only export. The plan's primary implementation is SQLite; do not regress into TOML round-trip unless explicitly added.

### Frontend infrastructure
- **D-25:** `@tanstack/react-query` 5 for fetch caching. `queryClient` singleton with `staleTime: 30_000`, `refetchOnWindowFocus: false`, `retry: 1` on queries / `retry: 0` on mutations. Wrap `<App />` in `<QueryClientProvider>`.
- **D-26:** Typed fetch wrapper in `web/src/lib/api/settings.ts`. All settings page state goes through `settingsApi` + `useQuery({ queryKey: ["settings"] })` + `useMutation(...)`. `qc.invalidateQueries({ queryKey: ["settings"] })` on success.
- **D-27:** `msw` (dev dep) for Vitest fetch mocking. Vitest smoke test asserts active card renders with masked key + "Local-only · on" pill.

### Claude's Discretion
- Exact Tailwind class names within the component primitives (colors come from Phase 1 tokens — `var(--paper)`, `var(--blue)`, `var(--blsoft)`, `var(--matcha)`, `var(--matchasoft)`, `var(--strawberry)`, `var(--ink)`).
- Whether to introduce a separate `Arc<RwLock<HashMap<String, String>>>` in addition to the `ApiKeyStore` trait for the eager-load cache. The trait + memory store already provide the abstraction; an explicit cache layer is a perf-only refinement if Keychain calls turn out slow under load.
- Exact wiremock test layout (the plan provides templates).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Source of truth
- `docs/superpowers/plans/2026-06-25-yogurt-phase-5-llm-client-and-settings.md` — The full 11-task superpowers implementation plan with copy-pasteable code blocks for every crate, module, handler, component, and test. Authoritative for all symbol names, file paths, dependency pins, and ordering. Tasks 5.1–5.11 (plus 5.7b for `.env.local` bootstrap).

### Product requirements
- `docs/PRD.md` §4 Q6 — OpenAI-compat-only LLM strategy
- `docs/PRD.md` §5.6 — Settings UI layout, sidebar, preset chips, dev-convenience env-var bootstrap
- `docs/PRD.md` §9 — Data model: `providers` + `settings` tables added in this phase; `meetings`/`chat_messages` deferred to Phase 6
- `docs/PRD.md` §10 — REST endpoint shapes: `/api/settings`, `/api/settings/providers`, `/api/audio/devices`
- `docs/PRD.md` §16 — Design tokens: blueberry-bordered active cards, dashed-border preset chips, matcha "Local-only · on" pill, JetBrains-Mono footer caption

### Phase wiring
- `.planning/REQUIREMENTS.md` — "LLM Client + Settings" section (LLM-01..03, SET-01..11)
- `.planning/ROADMAP.md` §"Phase 5: LLM Client + Settings + Keychain" — phase goal + 6 success criteria
- `.planning/PROJECT.md` — Project constraints (single binary, no telemetry, MIT, macOS-only) + `.env.local` convention
- `.planning/phases/04-augmented-notes-hero/04-SUMMARY.md` — **READ AT EXECUTE TIME** (this phase replaces Phase 4's MockLLM/hardcoded client; executor needs to know which symbol names Phase 4 actually shipped to safely rewire `enhance.rs`)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **Phase 4's `OpenAiCompatClient`**: ~50 LOC hardcoded client in `crates/yogurt-server/src/enhance.rs`. This phase **promotes** it into the new `yogurt-llm` crate behind the `LlmClient` trait. Phase 4's enhance handler is rewired to read the active provider from `AppState`, fetch the key from `AppState.keys` (the `ApiKeyStore`), construct an `OpenAiCompatClient`, and call `complete()`.
- **Phase 4's `enhance.md` + prompts module**: still used. The new enhance handler calls `crate::prompts::enhance_system()` for the system message.
- **Phase 1 design tokens**: `var(--paper)`, `var(--blue)`, `var(--blsoft)`, `var(--matcha)`, `var(--matchasoft)`, `var(--strawberry)`, `var(--ink)` — all consumed by the Settings UI. Fonts: `font-serif` (Instrument), `font-mono` (JetBrains).
- **Phase 3 router**: React Router 7 + at least `/` route. This phase adds `<Route path="/settings" element={<Settings />} />` to whichever file (`App.tsx` or `router.tsx`) hosts the existing router.
- **Phase 2 audio API**: `GET /api/audio/devices`. Settings Audio section consumes it via `audioApi.devices()`.
- **Phase 0 SQLite scaffolding**: Phase 5 introduces a fresh `yogurt-db` crate with `rusqlite_migration` rather than reusing Phase 0's connection — the plan explicitly creates a new module structure. Coordinate at execute time with Phase 0's WAL pool if both are present.

### Established Patterns
- **Single-binary distribution**: keep `yogurt-llm` and `yogurt-db` as no-axum library crates; the binary stays static. `rusqlite` MUST use the `bundled` feature.
- **No `getUserMedia` browser audio**: irrelevant here; settings UI is pure HTTP.
- **No telemetry**: settings page must not phone home. Vitest msw mocks replace real fetch.

### Integration Points
- `AppState { db: Db, keys: Arc<dyn ApiKeyStore> }` becomes the shared state. Phase 4's enhance handler is migrated from whatever extension Phase 4 used to `State<AppState>`.
- `crates/yogurt-server/src/bootstrap.rs::seed_from_env(state)` is called inside `run()` after `AppState::production()` is built, before `axum::serve`. Loads `.env.local` only when CLI was invoked with `--dev`.
- New routes mounted under `/api/settings*` via `crate::api::settings::router()`.

</code_context>

<specifics>
## Specific Ideas

- **The 5-second cold-boot guarantee (SET-10) is the load-bearing pitfall mitigation.** Request handlers must never block on `keyring::Entry::get_password()` during a request. The eager-load pattern: at server startup, walk all providers, attempt to read their keys via the `ApiKeyStore` (Keychain), cache the results in an `Arc<...>` carried on `AppState`. A 5s timeout wrapping the whole bulk read prevents a wedged Keychain daemon from blocking server boot indefinitely. Document this prominently in the secrets plan.
- **Minimax is the default dev provider.** `.env.local` will contain `YOGURT_MINIMAX_API_KEY` against base URL `https://api.minimaxi.chat/v1` (note: the plan uses `https://api.minimax.io/v1` in its preset table — these are aliases; defer to whichever the user's `.env.local` actually targets, but document the alias). Default model: `MiniMax-Text-01`.
- **No raw key leaks via API.** The Vitest + Rust integration tests both assert that `••••XXXX` is the only form of any key ever returned by `GET /api/settings*`. This is the single most important security invariant of the phase.
- **Provider switching is atomic.** The DB has a partial unique index `idx_providers_one_active_per_kind ON providers(kind) WHERE is_active=1`. `set_active` runs `UPDATE … is_active=0 WHERE kind='llm'; UPDATE … is_active=1 WHERE id=?` inside a `BEGIN IMMEDIATE` transaction.
- **The Manual acceptance test:** open `/settings`, click `Minimax` preset chip, click `Set active`, paste real key, click `Save key`, navigate to a Phase 4 meeting, hit Re-enhance. Verify via Keychain Access (macOS app) that the key is stored under `service="yogurt"`. Verify the enriched markdown is real Minimax output (not Phase 4's mock output).

</specifics>

<deferred>
## Deferred Ideas

- **Per-provider "Test key" button** — Phase 5.1 quality-of-life (post-v1).
- **Add-custom-provider authoring UI** — `+ Add` link is wired as a button but the modal/form for naming an arbitrary provider is deferred; user can use a preset chip + Edit instead.
- **Additional preset chips** beyond Ollama / LM Studio / OpenRouter / Minimax / OpenAI — additions are a config-only change to the `PRESETS` const slice; v1 ships the five listed.
- **Cross-process file watching of `~/.yogurt/db.sqlite`** — Phase 9 multi-tab story.
- **Local STT card actually working** — Phase 8 (`whisper-rs` adapter behind `local-stt` Cargo feature). For this phase, Local card is intentionally disabled with a "Coming in v1" badge.
- **`meetings` and `chat_messages` tables** — Phase 6 (migration V002).
- **WebSocket chat streaming** — Phase 6 consumes `yogurt-llm::stream()` shipped here.
- **Onboarding `/welcome` route + library `/` polish** — Phase 7.
- **`yogurt doctor` subcommand for TCC/port diagnostics** — Phase 9.

</deferred>

---

*Phase: 05-llm-client-settings-keychain*
*Context gathered: 2026-06-25*
