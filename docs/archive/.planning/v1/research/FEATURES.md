# Feature Research

**Domain:** macOS local-first meeting copilot (Granola-style augmented notes)
**Researched:** 2026-06-25
**Confidence:** HIGH (extensive primary references — Granola, Meetily, Hyprnote, Otter, Fireflies, tldv, Read.ai — cross-checked against PRD §5)

## Reference Comp Matrix

| Product | Type | Distribution | Audio capture | Notes UX | Privacy posture |
|---------|------|--------------|---------------|----------|-----------------|
| **Granola.ai** | Commercial SaaS (Series C, $1.5B) | macOS / Windows / iOS | System audio (no bot) | **Augmented notes (black/grey merge)** — the gold standard | Cloud STT (Deepgram), cloud LLM, audio deleted, notes stored in their AWS VPC; opt-out training |
| **Meetily** | OSS (~11.5k stars, MIT) | macOS / Windows | Bot-less; uses Parakeet + Whisper | Real-time transcript + post-meeting summary (no augmented notes) | 100% local processing, Ollama for summarization |
| **Hyprnote** (Anarlog) | OSS | macOS / cross-platform | System + mic audio | Real-time transcript + customizable templated summaries; "autonomy slider" | Local-first, runs Whisper + HyprLLM locally; Obsidian + Apple Calendar integrations |
| **Otter.ai** | SaaS | Web / mobile | Bot-based (joins meeting) | Transcript-first with summary tab + AI chat | Cloud; enterprise compliance available |
| **Fireflies** | SaaS | Web | Bot-based | Transcript + AI summary + action items + Slack/Notion/CRM push | Cloud |
| **tl;dv** | SaaS | Web/desktop | Bot-based with video clips | Transcript + AI summary + clip highlights | Cloud; 5,000+ Zapier integrations |
| **Read.ai** | SaaS | Web/desktop | Bot-based | Sentiment + engagement scoring + summary | Cloud |

**Yogurt's wedge:** Granola-quality augmented-notes UX + Meetily/Hyprnote-grade privacy posture. Nobody currently occupies this intersection.

---

## Feature Landscape

### Table Stakes (Users Expect These)

If Yogurt is missing one of these, users will bounce within their first session.

| Feature | Why Expected | Complexity | Yogurt v1 status | Notes |
|---------|--------------|------------|------------------|-------|
| **Record mic + system audio without a bot** | Granola's defining wedge; all bot-less analogs ship this | HIGH (ScreenCaptureKit FFI, permission UX) | ✅ PRD §5.1 | Genuine table stakes for the bot-less category |
| **Live transcript visible during meeting** | Otter/Fireflies/Meetily/Hyprnote all show this; users glance at it mid-call | MEDIUM (WebSocket fan-out, partials) | ✅ PRD §5.2 | Cited as a "real win from live" in PRD Q1 |
| **Post-meeting AI summary / enhanced notes** | Universal across the category — even Meetily ships Ollama summarization | HIGH (LLM streaming, prompt eng) | ✅ PRD §5.3 (augmented notes is a stronger form) | Yogurt's augmented notes subsumes this |
| **Persistent meeting library / history** | Users expect their notes to come back when they reopen the app | MEDIUM (SQLite + list UI) | ✅ PRD §5.9 | Added during design handoff — correct call |
| **Meeting metadata** (title, date, duration) | Every product shows this; sortable library is broken without it | LOW | ✅ Implicit in §5.9 cards | Auto-title from first heading is a common pattern; PRD doesn't specify — flag |
| **Search across notes & transcripts** | Otter, Fireflies, Granola, Meetily all have it; PRD §5.9 mentions a search pill but doesn't define implementation | MEDIUM (SQLite FTS5) | ⚠️ Mentioned in §5.9 ("Search notes & transcripts" pill) but not specified as a v1 feature | **Gap to flag** — search affordance is in the design but not in the 11-feature list. See "PRD Gaps" below. |
| **Export notes** | Every comp ships at least markdown/PDF/clipboard export | LOW (markdown is already the storage format) | ✅ Implicit in §5.7 (`~/.yogurt/notes/*.md`) | Markdown files are the export — elegant. But UI "Copy/Share" affordance not mentioned. |
| **Delete a meeting** | Users will create test meetings and need to clean up | LOW | ✅ DELETE endpoint in §10 | UI affordance not specified in §5.9 cards; minor gap |
| **Permission denial recovery** | macOS Screen Recording is a known friction wall | MEDIUM | ✅ PRD §5.11 | Well-handled |
| **Settings / configuration UI** | BYO-LLM requires somewhere to paste keys | MEDIUM | ✅ PRD §5.6 | Comprehensive |
| **Local storage of notes** | Implicit in "local-first" positioning | MEDIUM | ✅ PRD §5.7 | SQLite + markdown is the right shape |
| **First-run onboarding** | Especially critical because of macOS permission step | MEDIUM | ✅ PRD §5.10 | Strong design |
| **In-meeting recording indicator** | Users need confirmation that recording is happening (Granola recording badge, Otter timer) | LOW | ⚠️ PRD §16.6 defines the recording badge component but §5.1/5.2 don't list it as a feature | **Minor gap** — call out explicitly as part of §5.1 |

### Differentiators (Yogurt's Competitive Advantage)

These are where Yogurt wins. They map directly to Yogurt's stated Core Value.

| Feature | Value Proposition | Complexity | Yogurt v1 status | Notes |
|---------|-------------------|------------|------------------|-------|
| **Augmented notes (black-user / grey-AI in-place merge)** | The hero UX. Granola has it; no OSS tool does. This is the single reason Yogurt exists. | HIGH (TipTap marks, AST diff, transcript deep-links) | ✅ PRD §5.3 — extensively designed | If this doesn't feel indistinguishable from Granola, product fails (per PROJECT.md Core Value) |
| **Transcript deep-link from every AI bullet (`↳ HH:MM`)** | Trust affordance: "show me where this came from." Granola has it; OSS tools don't. | MEDIUM (TipTap mark with data attr, scroll-to-timestamp) | ✅ PRD §5.3 | Tight design |
| **Local-first** (audio + notes never leave machine) | Compliance-bound buyers (legal, finance, security) have no option here today | HIGH (whisper.cpp on Metal) | ✅ PRD §5.7, §5.8 | Yogurt + Meetily + Hyprnote — Yogurt's edge is augmented-notes UX on top |
| **BYO-LLM via OpenAI-compatible endpoint** | One config field covers ~10 providers including self-hosted (Ollama, vLLM, llama.cpp) | LOW (one adapter) | ✅ PRD §5.6 + Q6 | Strictly better than Granola's locked GPT/Claude |
| **Single static binary distribution** (`brew install yogurt`) | No Electron, no Node runtime, no sidecar — OSS-tool credibility | HIGH (rust-embed + whisper.cpp static link) | ✅ PRD §11 | Major dev-credibility signal vs Tauri-based Meetily/Hyprnote |
| **In-meeting AI chat ("Ask this meeting…" ⌘K)** | Granola has it; Meetily doesn't; pairs naturally with transcript-as-context | MEDIUM (streaming, transcript context window) | ✅ PRD §5.4 | Q1 calls this one of "the two real wins from live" |
| **Audio deleted after transcription** | Smallest privacy footprint; matches Granola, exceeds most comps | LOW | ✅ PRD Q7 | Document this prominently — buyer-relevant |
| **No telemetry, ever** | Hard differentiator vs Granola (training data by default), Otter, etc. | LOW (do nothing) | ✅ PRD constraint | Mention in README + landing page |
| **CLI-launched, browser UI** | Dev-tooly credibility; matches positioning ("warm + editorial + a touch dev-tooly") | MEDIUM | ✅ PRD §2 | Unusual choice — likely positive for target audience, neutral for normies |

### Anti-Features (Deliberately NOT Built)

PRD already calls most of these out. This table validates each anti-goal against domain reality and flags whether the reasoning holds.

| Anti-feature | Why Users Request It | Why Yogurt Shouldn't Build It | Alternative |
|--------------|---------------------|-------------------------------|-------------|
| **Meeting bot** ("Yogurt has joined the call") | Otter/Fireflies/tldv/Read.ai all do this — it's the most common pattern | Kills the "magical" feeling; participants self-censor under AI surveillance (research finding); contradicts core wedge | System audio capture via ScreenCaptureKit (PRD §5.1) |
| **Yogurt cloud / SaaS** | Easier onboarding, syncs across devices | Defeats privacy posture; takes on infra cost; opens compliance attack surface | BYO-LLM + BYO-STT (or fully local); markdown export for portability |
| **Default telemetry / phone-home** | Helps fix bugs faster | Privacy-focused users will not tolerate it; even opt-in Sentry is too much for v1 | Document logs at `~/.yogurt/logs/`; user-initiated bug reports |
| **Subscription billing** | Funds the product | OSS positioning, single-machine usage, BYO infrastructure — no recurring service to bill | MIT license; future sponsorship / pro tier deferred |
| **Calendar OAuth (Google/Outlook) in v1** | Auto-detect meetings; saves a click | OAuth scope, refresh tokens, calendar parser — significant complexity for marginal gain; v2 candidate | "+ New meeting" button (PRD §5.9) |
| **Cross-meeting chat / semantic search in v1** | Granola Spaces ships this; clearly useful at scale | Embeddings + vector store + cross-meeting prompt eng — only valuable when meeting history has weight (≥50 meetings) | Plain SQLite FTS within a single meeting; cross-meeting via grep on `~/.yogurt/notes/*.md` |
| **Slack / Notion / Linear push integrations** | Fireflies/Granola/tldv all ship this; "where do my notes go?" is a real question | Each integration is its own auth + retry + idempotency surface; markdown export covers 80% of value | Markdown files at `~/.yogurt/notes/`; user pipes into anything |
| **Mobile / web-hosted version** | Granola has iOS; Otter has mobile | Mobile system-audio capture is a totally different problem; web-hosted breaks local-first | macOS only |
| **Multi-user / sync / accounts** | Team usage | Single-user, single-machine is the whole point | Sync deferred to v2+ (user-hosted only, encrypted) |
| **Per-speaker diarization beyond Me/Them** | Otter/Fireflies do this | Requires pyannote (Python sidecar) — breaks single-binary; Granola itself only does Me/Them on desktop | Mic/system split (PRD §5.1) |
| **Template picker UI in v1** | Granola has standup / 1:1 / interview templates | User reported never using Granola's picker; designed but cut 2026-06-24 | Single bundled `enhance.md`; power users edit the file |
| **Audio playback / re-listen** | Granola users complain about this missing | Delete-after-transcription is a positive privacy property; opt-in retention is a v1.1 schema toggle | "Trust the transcript" model |
| **MCP server / external API** | Granola shipped MCP connector in Series C | Useful only once history is worth querying | Deferred until cross-meeting search exists |
| **Custom template authoring UI** | Power users want this | Templates are markdown files; power users can edit them | `crates/yogurt-prompts/*.md` |
| **Action item extraction as a separate feature** | Standard across all comps; users expect a "tasks" section | Augmented notes already include action items as bullets; separating creates redundant UX | Prompt-engineer `enhance.md` to surface action items as a bolded section |
| **Speaker identification with names** | Otter/Fireflies do it; users expect it | Requires speaker diarization (pyannote/WhisperX) — Python sidecar territory | Me/Them split |
| **Video clips / shareable highlights** | tldv's main wedge | Out of scope for audio-only product; massive complexity (video buffer + encode) | N/A |
| **Sentiment / engagement scoring** | Read.ai's wedge | Privacy-creepy, low-trust output, doesn't match Yogurt's positioning | N/A |

---

## PRD Gaps & Flags

Cross-referencing the 11 v1 features against table-stakes domain expectations:

### Likely Gaps (recommend addressing in v1 or explicitly deferring)

1. **Search across library/transcripts is in the design but not in the feature list.** PRD §5.9 shows a "Search notes & transcripts" pill in the library sidebar, but search is not one of the 11 listed features. Every comparable product ships this. Recommendation: either (a) add SQLite FTS5 as part of §5.7 (local storage) and a search results route, or (b) explicitly defer to v1.1 and ship the pill as disabled / "coming soon."

2. **Meeting metadata defaults are unspecified.** Library cards show titles like "Sales sync — Acme", but PRD doesn't say how titles are assigned. Granola auto-titles from calendar event; without calendar OAuth, Yogurt needs either (a) a "New meeting" title prompt, (b) inline-editable title in the meeting view, or (c) LLM-generated title from the first heading. Recommend: inline-editable title with a sensible default like "Untitled meeting — 2:00 PM".

3. **"Copy markdown to clipboard" / share button is unspecified.** Users will want to grab the enhanced notes and paste them into Slack/email/Notion. The markdown file exists at `~/.yogurt/notes/`, but a one-click "Copy" or "Open in Finder" affordance is missing from §5.3 / §5.9 spec.

4. **Delete-meeting UI not specified.** The DELETE endpoint exists in §10 but the UI affordance (hover menu on library card, destructive confirm dialog) is not specified in §5.9.

5. **Recording indicator state not in features list.** §16.6 defines the recording-badge component (pulsing strawberry dot + mono timer), but §5.1 ("Record meeting") doesn't list "visible recording indicator" as an acceptance criterion. Minor but worth folding in.

6. **Auto-save reliability not specified.** §10 has a `notes_edit` WS message and a `notes_synced` server response, but the user-facing "Saved · 2s ago" affordance (which Granola has and users find reassuring) isn't specified.

### Table Stakes Yogurt Is Consciously Skipping (and whether reasoning holds)

| Skipped feature | PRD reasoning | Does it hold? |
|-----------------|---------------|---------------|
| Calendar OAuth | "Manual click is acceptable for v1" | ✅ Holds — onboarding screen mentions "click + New meeting"; users tolerate this for local-first OSS tools |
| Action items as separate section | (Implicit) augmented notes cover this | ⚠️ Marginal — recommend the `enhance.md` prompt explicitly outputs an "## Action items" section so users see one. Otherwise it feels missing vs Otter/Fireflies. |
| Speaker labels with names | Mic/System split only | ✅ Holds — matches Granola; competing with Otter on diarization is a losing battle |
| Slack/Notion push integrations | Markdown export covers it | ✅ Holds for v1; revisit if users complain |
| Mobile app | Out of scope | ✅ Holds — different audio architecture entirely |
| Audio playback | Delete-after-transcription is a feature | ⚠️ Granola users complain about this in 2026 reviews. Holds for v1 (privacy posture) but plan for "Keep audio per-meeting" toggle in v1.1 |
| Default search-across-meetings | "Embeddings deferred to v2" | ⚠️ User likely means semantic / vector search. Plain keyword search via SQLite FTS5 is cheap (one-day add) and meets the table-stakes bar. Recommend keyword FTS in v1. |
| Telemetry / crash reporting | "Trust posture" | ✅ Holds — competing on privacy means no exceptions |
| Subscription / accounts | OSS positioning | ✅ Holds |

---

## Feature Dependencies

```
Record meeting (§5.1)
    └── requires ──> ScreenCaptureKit Rust bindings (audio crate)
    └── requires ──> macOS Screen Recording permission grant (onboarding §5.10)
    └── enables ──> Live transcript (§5.2)

Live transcript (§5.2)
    └── requires ──> STT engine + WS fan-out
    └── enables ──> Augmented notes (§5.3)  [needs final transcript]
    └── enables ──> In-meeting chat (§5.4)  [needs transcript-so-far context]
    └── enables ──> Transcript deep-links from AI bullets (§5.3)

Augmented notes (§5.3)
    └── requires ──> LLM client + OpenAI-compat adapter (§5.6 settings to configure)
    └── requires ──> Bundled enhance.md prompt (§5.5)
    └── requires ──> TipTap with custom aiGrey mark + transcript-link node
    └── enhanced-by ──> Re-enhance button (also §5.3)

In-meeting chat (§5.4)
    └── requires ──> LLM client (§5.6)
    └── requires ──> Bundled chat-system.md prompt (§5.5)
    └── shares ──> LLM adapter with augmented notes

Settings UI (§5.6)
    └── requires ──> Keychain access via `keyring` crate
    └── requires ──> ~/.yogurt/config.toml
    └── gates ──> Augmented notes, In-meeting chat, Cloud STT
    └── allows ──> Local-only mode (whisper.cpp + Ollama)

Local storage (§5.7)
    └── requires ──> SQLite schema + migrations
    └── requires ──> Markdown round-trip (notes crate)
    └── enables ──> Library home view (§5.9)

Library home (§5.9)
    └── requires ──> Local storage (§5.7)
    └── consumes ──> Search affordance [GAP — see PRD Gaps]
    └── consumes ──> Folder model [schema addition — not specified in §9]

Onboarding (§5.10)
    └── orchestrates ──> Permission grant → Settings (§5.6) → Library (§5.9)

Empty/error states (§5.11)
    └── decorates ──> Library (empty), Audio capture (perm denied),
                      Augmented notes (enhancing), Local STT (model download)

macOS only (§5.8)
    └── constraint, not a "feature" — informs all others
```

### Dependency Notes

- **§5.3 (augmented notes) depends on §5.2, §5.5, §5.6, §5.7 simultaneously.** This is the hero feature — schedule it after the four prerequisite features are stable. Roadmap Phase 4 in PRD §12 reflects this correctly.
- **§5.4 (chat) is cheap once §5.6 + §5.5 + transcript stream exist.** PRD §12 puts it in Phase 6 as a 1-day item — correct.
- **§5.9 (library), §5.10 (onboarding), §5.11 (empty/error)** can be built earlier with mock data, but their *content* depends on every other feature being done. Recommend: build shells in Phase 7, fill in with real data as features land.
- **Folder model is undocumented in §9 schema.** Library design shows folders with counts; SQLite schema in §9 has no `folders` table or `meetings.folder_id` column. **Schema gap to flag.**

---

## MVP Definition

### Launch With (v1) — Maps to PRD §5

These are the 11 explicit PRD features plus 1-2 small additions to hit table stakes.

- [x] **Record meeting** (§5.1) — table stakes for category
- [x] **Live transcript panel** (§5.2) — table stakes
- [x] **Augmented notes editor** (§5.3) — *the* differentiator
- [x] **In-meeting AI chat** (§5.4) — differentiator, cheap to add
- [x] **Bundled prompts** (§5.5) — invisible plumbing
- [x] **Settings UI** (§5.6) — table stakes
- [x] **Local storage** (§5.7) — table stakes
- [x] **macOS only** (§5.8) — platform constraint
- [x] **Meeting library** (§5.9) — table stakes
- [x] **Onboarding** (§5.10) — table stakes (because of macOS perm)
- [x] **Empty / error states** (§5.11) — polish but cheap given design exists

**Recommended additions to hit full table-stakes bar:**

- [ ] **Keyword search across notes & transcripts** (SQLite FTS5) — 1-day add; the search pill is already in the §5.9 design
- [ ] **Copy markdown / "Reveal in Finder" affordance** on meeting view — half-day; otherwise users have to `cd ~/.yogurt/notes/` manually
- [ ] **Inline-editable meeting title** with sensible default — quarter-day; otherwise meetings are all named "Untitled" forever
- [ ] **Delete-meeting UI** (hover menu on card + confirm) — half-day; DELETE endpoint already exists

These four are not in the 11-feature list but each costs <1 day and meaningfully raises the table-stakes floor.

### Add After Validation (v1.x)

Triggered by user feedback after first 50+ users:

- [ ] **Per-meeting "keep audio" toggle** — Granola users complain about lost audio; schema addition is trivial
- [ ] **Auto-title generation from first heading or LLM** — quality-of-life
- [ ] **Folder model with drag-to-reorder** — design has folders but core grouping (by date) suffices for v1
- [ ] **Markdown export with chosen format** (front-matter only / notes only / notes + transcript appendix)
- [ ] **Action items as bolded `## Action items` section** in enhance prompt — likely needed once power users adopt
- [ ] **Template picker** (deferred from v1; revisit if users report wanting it)
- [ ] **Strawberry + Matcha-dark themes** (deferred to keep design phase tight)

### Future Consideration (v2+) — per PRD §6

- [ ] Calendar OAuth (Google + Outlook)
- [ ] Cross-meeting semantic search (embeddings + sqlite-vss or LanceDB)
- [ ] Slack / Notion / Linear push integrations
- [ ] Custom template authoring UI
- [ ] Per-speaker diarization (pyannote sidecar, opt-in)
- [ ] Menu-bar / global-hotkey UI (Tauri wrap)
- [ ] Windows + Linux support
- [ ] MCP server
- [ ] Optional encrypted sync (user-hosted only — no Yogurt cloud)

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Augmented notes (black/grey merge) | HIGH | HIGH | P1 — hero |
| Record mic + system audio | HIGH | HIGH | P1 — gate |
| Live transcript panel | HIGH | MEDIUM | P1 — gate |
| Settings UI (BYO-LLM + Keychain) | HIGH | MEDIUM | P1 — gate |
| Local storage (SQLite + .md) | HIGH | MEDIUM | P1 — gate |
| Library home view | HIGH | MEDIUM | P1 — table stakes |
| Onboarding flow | HIGH | MEDIUM | P1 — table stakes (macOS perm) |
| In-meeting AI chat | MEDIUM | LOW | P1 — cheap differentiator |
| Empty/error states | MEDIUM | LOW | P1 — design exists, cheap |
| Bundled prompts | HIGH | LOW | P1 — invisible plumbing |
| Keyword search (FTS5) | HIGH | LOW | **P1 add** — table stakes gap |
| Copy markdown / Reveal in Finder | MEDIUM | LOW | **P1 add** — UX gap |
| Inline-editable meeting title | MEDIUM | LOW | **P1 add** — UX gap |
| Delete-meeting UI | MEDIUM | LOW | **P1 add** — UX gap |
| Local STT (whisper.cpp) | HIGH | HIGH | P1 — privacy escape hatch |
| Re-enhance button | MEDIUM | LOW | P1 (already in §5.3) |
| Auto-title generation | MEDIUM | MEDIUM | P2 — v1.1 |
| Per-meeting audio retention toggle | MEDIUM | LOW | P2 — v1.1 |
| Folder model | MEDIUM | MEDIUM | P2 — design exists, defer |
| Template picker | LOW | MEDIUM | P3 — deferred |
| Calendar OAuth | HIGH | HIGH | P3 — v2 |
| Cross-meeting semantic search | HIGH | HIGH | P3 — v2 |
| Slack/Notion/Linear push | MEDIUM | HIGH | P3 — v2 |
| Speaker diarization | MEDIUM | HIGH | P3 — v2 (Python sidecar) |
| Menu-bar / Tauri | LOW | HIGH | P3 — v2 |
| Windows / Linux | MEDIUM | HIGH | P3 — v2 |
| MCP server | LOW | MEDIUM | P3 — v2 |
| Mobile | HIGH | VERY HIGH | P3 — v2+ |

**Priority key:**
- P1: Must have for v1 launch
- P2: Should have soon after launch (v1.1)
- P3: Future / v2+ (deferred deliberately)

---

## Competitor Feature Analysis

| Feature | Granola | Meetily | Hyprnote | Otter/Fireflies | **Yogurt v1** |
|---------|---------|---------|----------|-----------------|---------------|
| Audio capture | System audio (no bot) | System audio (no bot) | System audio (no bot) | Bot joins meeting | System audio (no bot) ✅ |
| Live transcript | ✅ | ✅ | ✅ | ✅ | ✅ |
| Augmented notes (black/grey) | ✅ (gold standard) | ❌ summary only | ❌ templated summary | ❌ summary | ✅ (Yogurt's hero) |
| Transcript deep-links | ✅ | partial | ❌ | ✅ | ✅ |
| In-meeting chat | ✅ | ❌ | partial | ✅ (Otter) | ✅ |
| Templates | ✅ (picker) | ✅ | ✅ | ✅ | ❌ deferred to v2 |
| Cross-meeting search | ✅ (Spaces) | ❌ | ❌ | ✅ | ❌ deferred to v2 |
| Calendar OAuth | ✅ | ✅ | ✅ (Apple Cal) | ✅ | ❌ deferred to v2 |
| Slack/Notion push | ✅ | ❌ | partial (Obsidian) | ✅ | ❌ markdown export |
| BYO-LLM | ❌ (locked) | ✅ (Ollama) | ✅ (HyprLLM) | ❌ | ✅ (OpenAI-compat) |
| Fully local mode | ❌ | ✅ | ✅ | ❌ | ✅ |
| Single static binary | ❌ (Electron) | ❌ (Tauri) | ❌ (Tauri+Swift) | ❌ (web) | ✅ (Rust + rust-embed) |
| Open source | ❌ | ✅ MIT | ✅ | ❌ | ✅ MIT |
| Meeting bot | ❌ (no bot) | ❌ | ❌ | ✅ | ❌ |
| Mobile | ✅ iOS | ❌ | ❌ | ✅ | ❌ |
| Telemetry default-on | ✅ training data | ❌ | ❌ | ✅ | ❌ |
| Audio playback | ❌ deleted | partial | partial | ✅ | ❌ deleted (Granola model) |
| Action items as section | ✅ | ✅ | ✅ | ✅ | ⚠️ should be in enhance prompt |
| Speaker diarization | weak | ✅ | partial | ✅ strong | ❌ Me/Them only |

**Take-away:** Yogurt is the only product that ships augmented notes + local-first + BYO-LLM + single static binary + no telemetry. That intersection is the entire wedge. Don't dilute it by adding features from the Otter/Fireflies column.

---

## Sources

### Competitor product references
- [Granola — The AI Notepad for back-to-back meetings](https://www.granola.ai/)
- [Granola raises $125M, hits $1.5B valuation (TechCrunch, Mar 2026)](https://techcrunch.com/2026/03/25/granola-raises-125m-hits-1-5b-valuation-as-it-expands-from-meeting-notetaker-to-enterprise-ai-app/)
- [Granola Review 2026 — limitations-first look (Medium)](https://medium.com/@trelvek/granola-review-2026-a-limitations-first-look-at-ai-meeting-notes-0bcd97d68bc4)
- [Granola's 'Private' AI Notes Are Public by Default (TechBuzz)](https://techbuzz.ai/articles/granola-s-private-ai-notes-are-public-by-default)
- [Meetily GitHub repo (Zackriya-Solutions/meetily)](https://github.com/Zackriya-Solutions/meetily)
- [Meetily — Open Source Note-Taking App](https://meetily.ai/open-source)
- [Hyprnote vs Granola comparison](https://hyprnote.com/vs/granola)
- [Best Granola Alternatives for Private Meeting Notes 2026 (BuildBetter)](https://blog.buildbetter.ai/best-granola-alternatives-private-meeting-notes-2026/)
- [Best AI Meeting Recorders Without Bots 2026 (BuildBetter)](https://blog.buildbetter.ai/best-ai-meeting-recorders-without-bots-2026/)
- [Otter vs Notta vs Fireflies vs TL;DV comparison (Umevo)](https://www.umevo.ai/blogs/ume-all-posts/otter-vs-notta-vs-fireflies-vs-tl-dv-the-ultimate-2026-comparison-for-meeting-transcription)
- [Bot vs Desktop Recording — Circleback](https://circleback.ai/blog/bot-vs-desktop-recording)
- [Bot-Free Meeting Transcription 2026 (Meetily)](https://meetily.ai/blog/bot-free-self-hosted-meeting-transcription/)

### Standard feature references (table stakes)
- [Best Meeting Note-Taking Apps in 2026 (MeetGeek)](https://meetgeek.ai/blog/best-meeting-note-taking-app)
- [11 Best AI Note-Taking Apps for Meetings 2026 (Krisp)](https://krisp.ai/blog/ai-note-taking-apps/)
- [10 best AI meeting assistants in 2026 (Zapier)](https://zapier.com/blog/best-ai-meeting-assistant/)
- [Top 12 Meeting Minutes Apps Tested 2026 (Lindy)](https://www.lindy.ai/blog/best-meeting-minutes-app)

### Primary project documents
- `/Users/rchen/Documents/code/yogurt/.planning/PROJECT.md`
- `/Users/rchen/Documents/code/yogurt/docs/PRD.md` (especially §3 users, §5 v1 features, §6 v2 deferred, §16 design system)

---
*Feature research for: macOS local-first meeting copilot (Yogurt)*
*Researched: 2026-06-25*
