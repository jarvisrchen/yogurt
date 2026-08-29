# Research Summary — Yogurt

**Domain:** macOS local-first meeting copilot (Granola-style augmented notes, no bot)
**Synthesized:** 2026-06-25
**Overall confidence:** HIGH on stack/features/architecture/documented pitfalls. MEDIUM on a small number of runtime behaviors (SCK audio loopback, whisper streaming quality on Intel) that require a Phase 2 / Phase 8 spike to retire.

## Executive Summary

Yogurt sits in a wedge no one currently occupies: **Granola-quality augmented-notes UX + Meetily/Hyprnote-grade privacy + single static binary + BYO OpenAI-compatible LLM**. Research across all four dimensions converges: the PRD-locked stack and architecture are the correct 2026 answers; open questions are concentrated in (1) ScreenCaptureKit audio-loopback maturity, (2) TipTap mark persistence across markdown round-trip, and (3) macOS code-signing / TCC discipline. None reshape architecture; each is a designed-in mitigation rather than a refactor.

Recommended path: ship the PRD's locked stack (Rust 1.82+ / tokio 1.51 LTS / axum 0.8 / whisper-rs 0.16 / async-openai 0.36 / rusqlite 0.40 bundled / React 19 / Vite 7 / Tailwind 4.3 / TipTap 3) with two flagged risk areas — `screencapturekit` 2.x audio surface and pre-1.0 `deepgram` Rust SDK — each gated behind trait boundaries (`SttEngine`, `LlmClient`) so a Swift sidecar or hand-rolled `tokio-tungstenite` fallback is a one-file change.

The PRD's 10-phase roadmap (§12) is independently validated by architecture research and should be preserved, with one schema change folded in early: add `enriched_doc_json TEXT` to `meetings` so TipTap marks survive restart.

The hero feature (augmented notes black/grey merge) carries the highest single risk: structural AST diffing through TipTap marks must round-trip through markdown without losing the `aiGrey` mark or `transcriptTs` deep-link. The fix is architectural — persist ProseMirror JSON alongside markdown — and must be designed into Phase 4, not retrofitted.

## Key Findings

### Stack (HIGH; two MEDIUM)

- **Locked, validated:** tokio 1.51 LTS, axum 0.8, `screencapturekit` 2.x (`Send + Sync` now present), whisper-rs 0.16 + `metal`, `deepgram` 0.6, `async-openai` 0.36 (single adapter covers ~10 providers via `OpenAIConfig::with_api_base()`), rusqlite 0.40 `bundled` (SQLite 3.53.2 self-contained), `keyring` 3 post-2026 refactor + `apple-native-keyring-store`, React 19 (CSR only), Vite 7 (NOT 8 — Rolldown ecosystem too fresh), Tailwind 4.3, **TipTap 3 with official `@tiptap/extension-markdown`** (bidirectional round-trip), `rust-embed` 8.
- **Two MEDIUM-confidence risks, both behind trait boundaries:**
  - SCK audio-loopback surface — Swift sidecar fallback per PRD §13
  - `deepgram` Rust SDK pre-1.0 — hand-rolled `tokio-tungstenite` ~200 LOC fallback
- **Rejected:** Tauri, Electron, BlackHole, `getUserMedia`, sqlx, Next.js, Vite 8, Tailwind v3, Tiptap v2, Yew/Leptos.

### Features (HIGH)

- 11 PRD v1 features hit ~95% of category table stakes.
- **Yogurt is the only product in the comp matrix (Granola, Meetily, Hyprnote, Otter, Fireflies, tldv, Read.ai) shipping augmented notes + local-first + BYO-LLM + single static binary + no telemetry.** That intersection is the entire wedge.
- **4 small table-stakes gaps to add (<1d each):**
  1. SQLite FTS5 keyword search (search pill already in §5.9 design)
  2. Copy markdown / Reveal in Finder affordance
  3. Inline-editable meeting title
  4. Delete-meeting card UI
- **1 schema gap:** PRD §9 has no `folders` table though §5.9 design shows folders with counts. Defer to v1.1.
- Anti-features validated; flag for v1.1 follow-up: per-meeting "keep audio" toggle and `## Action items` section in `enhance.md`.

### Architecture (HIGH)

- **PRD §7/§8 sound; no structural changes.** Independently validated against Meetily, Hyprnote, axum chat example, `whisper-cpp-plus`.
- **Three independent runtime pipelines:**
  1. Audio → Live Transcript (firehose; two STT tasks per meeting for Me/Them labeling; `broadcast::Sender<AudioFrame>` and `<TranscriptEvent>` as crate-boundary contracts)
  2. Notes ⇄ Enhance (orchestrated by `yogurt-notes`; AST diff runs server-side, testable with `cargo test`)
  3. In-meeting Chat (independent of audio at call-graph level; only reads transcript)
- **Two trait boundaries (only polymorphism):**
  - `SttEngine` (`open_session(channel) → SttSession` with `push_frame` / `events()` / `close` — fits Deepgram WS, AssemblyAI WS, whisper.cpp VAD-chunked batch)
  - `LlmClient` (one method `complete_streaming(ChatRequest) → BoxStream<ChatDelta>` — don't split enhance vs chat)
- **Build order DAG matches PRD §12 exactly.** Tension: Phase 4 needs *some* LLM client → ship minimal hardcoded `OpenAiCompatClient` (~50 LOC) in Phase 4, promote in Phase 5.
- **5 patterns to follow:** channels as crate boundaries; per-meeting task supervisor; WS = read-only projection; `rust-embed` with `--dev` Vite proxy split; markdown source of truth + SQLite mirror.
- **6 anti-patterns to avoid:** unified STT+LLM trait; `Provider` trait erasing local/cloud; stringly-typed WS messages; business logic in `yogurt-cli`; `audio_tx` capacity=1; calling whisper.cpp without `spawn_blocking`.

### Pitfalls (HIGH)

- **11 critical pitfalls; 5 net-new beyond PRD §13.** Concentration in Phase 0 (set-it-now decisions) and Phase 2 (audio capture gates downstream).
- **Top 5 project-killers:**
  1. **TCC re-prompts on every dev build / unsigned binary** — sign dev builds with stable self-signed cert; pin bundle ID `ai.yogurt.app` now; notarize releases; ship `yogurt doctor --reset-permissions`.
  2. **`screencapturekit` audio-loopback gaps** — Phase 2 spike (30s dual-channel PCM → WAV → ear-test) BEFORE committing to crate-only. `excludesCurrentProcessAudio = true` day one.
  3. **Mic/system timestamp drift** — meeting-relative clock from `Instant::now()` at start; use `CMSampleBuffer.presentationTimeStamp` for SCK. Post-launch fix = near-total rewrite.
  4. **TipTap mark loss on markdown round-trip** — persist BOTH `notes_md` AND `enriched_doc_json` (ProseMirror JSON with marks). Schema-version JSON day one.
  5. **`rust-embed` / Vite SPA breakage** — `base: './'` in `vite.config.ts`; explicit SPA fallback returning embedded `index.html`; `cargo run --release` as default CI smoke test.
- **Pitfalls 6–11:** whisper.cpp streaming-quality cliff (VAD + batch-per-utterance + dual `whisper_state`); `keyring` cold-boot hangs (eager-load at startup); `whisper-rs` CMake build failures (gate behind `local-stt` Cargo feature); universal binary lipo + notarization order; localhost binding / WS Origin / session token; SQLite single-writer contention (separate read pool + `Mutex<Connection>` writer).
- **Net-new beyond PRD §13:** audio drift, rust-embed/Vite SPA, Keychain async/cold-boot, localhost/WS Origin, SQLite single-writer, universal binary notarization order, whisper-rs CMake build cost.

## Implications for Roadmap

PRD §12's 10-phase plan (~18 working days) is independently validated. Preserve structure with these adjustments folded in.

**Phase notes:**

1. **Phase 0 — Skeleton.** Design in NOW: SQLite pool model (read pool + `Mutex<Connection>` writer + WAL pragmas), `rust-embed` SPA fallback with `base: './'`, port-conflict UX, localhost-only binding hard-coded for release, WS Origin check + session token. Pitfalls 6, 10, 11 are Phase 0 deliverables.
2. **Phase 1 — Design system.** Tokens + primitives BEFORE screens.
3. **Phase 2 — Audio capture (HIGHEST RISK).** Spike is a gate, not a checkbox. 30s dual-channel PCM → WAV → ear-test. Meeting-relative clock. `excludesCurrentProcessAudio = true` from first commit. Swift sidecar fallback documented. Blocks Phase 3.
4. **Phase 3 — Cloud STT + live transcript.** Deepgram via `SttEngine` trait. WS Origin enforcement lands here.
5. **Phase 4 — Augmented notes hero (HIGHEST PAYOFF).** Schema change: add `enriched_doc_json TEXT` at start of phase. Minimal hardcoded `OpenAiCompatClient` (~50 LOC). AST diff server-side, `cargo test` fixtures.
6. **Phase 5 — LLM client + settings + Keychain.** Promote Phase 4's hardcoded client; eager-load secrets at startup.
7. **Phase 6 — In-meeting chat.** ~1d, trivially additive.
8. **Phase 7 — Library + onboarding + empty/error + 4 table-stakes adds** (FTS5 search, copy/reveal-in-finder, inline-editable title, delete-card UI).
9. **Phase 8 — Local STT.** Gate behind `local-stt` Cargo feature. Dual `whisper_state` preview/settled. Benchmark on M1 Air.
10. **Phase 9 — Distribution polish.** Per-arch tarballs first; universal optional. Order: tag → release binaries → cargo publish → tap PR. `yogurt doctor` subcommand.

**Decisions the roadmap should explicitly reflect:**

- Phase 4 ships a minimal hardcoded LLM client; Phase 5 generalizes.
- `enriched_doc_json TEXT` column added in Phase 4 migration.
- Four table-stakes adds fold into Phase 7 (not separate phases).
- Phase 2 spike is a gate, not a checkbox.
- `folders` table deferred to v1.1.

**Research flags (which phases need `/gsd-plan-phase --research-phase`):**

| Phase | Needs deeper research? | Rationale |
|-------|------------------------|-----------|
| 0 | Light | Verify rust-embed + SPA fallback end-to-end |
| 1 | No | Design system fully specified in PRD §16 |
| **2** | **YES — heaviest** | SCK audio loopback spike, clock model, Swift sidecar protocol contingency |
| 3 | Light | Deepgram WS protocol well-documented |
| **4** | **YES — hero** | TipTap mark + AST diff against 3 transcripts; ProseMirror schema versioning |
| 5 | Light | Keychain eager-load pattern |
| 6 | No | Trivially additive |
| 7 | No | Pure UI composition |
| **8** | **YES — local STT quality** | VAD + dual `whisper_state` benchmarking on M1 Air |
| 9 | Light | Notarization order + universal binary recipe must be exact |

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Verified against 2026 crate registries, release blogs, official docs. Two MEDIUM areas have concrete fallback paths. |
| Features | HIGH | 7 named comps cross-checked; 4 gaps quantified (<1d each). Anti-features validated. |
| Architecture | HIGH | Validated against Meetily, Hyprnote, axum idioms, `whisper-cpp-plus`. |
| Pitfalls | HIGH (documented), MEDIUM on a few runtime behaviors | 11 critical with concrete prevention + recovery; 5 net-new beyond PRD §13. |

**Gaps to address during planning:**

1. SQLite `folders` table — defer to v1.1 explicitly or add to Phase 7.
2. `enriched_doc_json` column — must land in Phase 4 schema migration.
3. Auto-save "Saved · 2s ago" affordance — unspecified.
4. Recording indicator state — defined in §16.6 but not §5.1 acceptance criterion.
5. Action items as bolded `## Action items` in `enhance.md` — competitor pressure.
6. `yogurt doctor` subcommand — needed for TCC reset / model re-download / port diagnostics; add to Phase 9.
7. Phase 2 spike acceptance test must be a deliverable, not assumption.

---
*Synthesized from STACK.md, FEATURES.md, ARCHITECTURE.md, PITFALLS.md on 2026-06-25.*
