---
phase: 8
slug: local-stt-whisper-cpp
status: draft
shadcn_initialized: false
preset: none
created: 2026-06-25
---

# Phase 8 — UI Design Contract

> Visual and interaction contract for Local STT (whisper.cpp). Phase 8 is mostly backend (whisper-rs adapter, VAD chunking, dual `whisper_state` on Metal, SHA256-verified model download manager). The UI surface is narrow: promote the Phase 5/7 "Coming soon" Local card stub to a functional `LocalSTTCard` with a `ModelPicker`, and wire the Phase 7 STATE-04 `ModelDownloadDialog` to live download progress. **No new visual primitives.** Inherits Phase 1 tokens, Phase 5 settings card-pair pattern, Phase 7 STATE-04 modal chrome.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | Inherits from `.planning/phases/01-design-system/01-UI-SPEC.md`. No new tokens. |
| Preset | not applicable |
| Component library | none — custom components only (`LocalSTTCard`, `ModelPicker`, `ModelDownloadDialog`) |
| Icon library | inline Unicode glyphs (Phase 1 convention): `✓` ready, `↓` undownloaded, `slow` warning chip, matcha down-arrow `↓` in dialog header |
| Font | Inherits Phase 1 — Instrument Serif (display, unused here), Hanken Grotesk (UI body/labels/buttons), **JetBrains Mono (model size names + disk-space captions)** |

**Inheritance contract:**

- Tokens, type scale, motion: Phase 1 (`01-UI-SPEC.md`) — no exceptions.
- Settings card-pair pattern (Cloud card LEFT active blueberry-bordered, Local card RIGHT inactive cream surface): Phase 5 (Settings → Transcription section, design-board lines 523–538).
- First-time download modal visual (matcha `#5E9E73` ↓ glyph in `#E7F0E8` rounded square, matcha progress bar, mono progress caption, "Cancel" / "Run in background" button row): Phase 7 STATE-04 (design-board lines 629–637). **Phase 8 MUST NOT redesign this dialog — only wire it to live `stt_model_download_progress` WS events.**

---

## Spacing Scale

Inherits from Phase 1 — multiples of 4. Phase 8 introduces no exceptions.

Specific spacings reused from the design board for Local card + dialog:

| Element | Spacing | Source |
|---------|---------|--------|
| Local card padding | `14px 16px` | design board L532 (card-pair grid) |
| Model pill gap | `6px`, `flex-wrap` | design board L534 |
| Model pill inner padding | `5px 9px` | design board L534 |
| Dialog padding | `24px 26px` | design board L631 (Phase 7 owned) |
| Dialog header gap (glyph ↔ titles) | `11px` | design board L632 |
| Progress bar top/bottom margin | `18px 0 8px` | design board L633 |
| Button row top margin / gap | `18px` / `9px` | design board L636 |

---

## Typography

Inherits Phase 1. Phase-specific role assignments:

| Role | Family | Size | Weight | Used For |
|------|--------|------|--------|----------|
| Section title ("Transcription") | Hanken Grotesk | 18px | 700 | Settings → Transcription heading (Phase 5 inherited) |
| Card title ("Local · whisper.cpp") | Hanken Grotesk | 14px | 700 | LocalSTTCard header |
| Privacy chip ("100% private") | Hanken Grotesk | 11px | 600 | Matcha inline label next to card title |
| **Model size name** (`small.en`, `base.en`, `medium.en`) | **JetBrains Mono** | 11.5px | 600 | Each pill inside `ModelPicker` |
| **Disk-space caption** (`~466MB · ~2 min`, `487 MB`) | **JetBrains Mono** | 11.5px | 400-500 | Sub-caption under model name; dialog progress caption |
| Helper caption ("Models download on first use…") | Hanken Grotesk | 11.5px | 400 | Bottom of LocalSTTCard |
| Dialog primary title ("Downloading small.en") | Hanken Grotesk | 16px | 700 | STATE-04 header (Phase 7 inherited) |
| Dialog body copy | Hanken Grotesk | 12.5px | 400 | Privacy reassurance line in dialog |
| Button | Hanken Grotesk | 13px | 600 | Cancel / Run in background |

**Mono usage rule:** every model identifier (`small.en`, `base.en`, `medium.en`, `~/.yogurt/models/`) and every byte/MB/MB-per-sec/ETA value uses JetBrains Mono. Everything else stays Hanken.

---

## Color

Inherits Phase 1 (Blueberry theme). Phase 8 reserves the following tokens for the listed elements ONLY:

| Token | Hex | Phase 8 reservation |
|-------|-----|---------------------|
| `--color-paper` (`#FBF7EF`) | dominant | App background under Settings; pill backgrounds inside Local card |
| `--color-card` (`#FFFFFF`) | secondary | Cloud card surface (active partner) |
| (off-cream) `#FCFAF5` | secondary variant | LocalSTTCard surface when **not** the selected provider (design-board L532) |
| `--color-blue` (`#5B4FC7`) | accent | **Selected model card border** when Local is the active provider (1.5px solid). Mirrors Phase 5 active-provider-card pattern from the Cloud card (design-board L527). |
| `--color-blsoft` (`#ECE9FB`) | accent wash | Selected Cloud chip background only — NOT used inside the Local card model picker |
| `--color-matcha` (`#5E9E73`) | privacy accent | (1) "100% private" inline chip text, (2) selected-model pill 1.5px border + check glyph `✓`, (3) `ModelDownloadDialog` header glyph + progress bar fill, (4) post-download "is ready · runs on Metal" status chip |
| `--color-mtsoft` (`#E7F0E8`) | privacy accent wash | `ModelDownloadDialog` header glyph background (38×38 rounded-10px square) |
| `--color-grey` (`#A89F90`) | muted text | Disk-space captions, "Models download on first use…" helper, dialog progress caption |
| `--color-mut` (`#8A8174`) | muted text — slightly stronger | Undownloaded pill text, "fastest · audio leaves the Mac" Cloud sub-label, dialog progress numbers |
| `--color-ink` (`#211D18`) | body text | Card titles, selected-model pill text |
| `--color-line` (`#EBE3D5`) | borders | Default model pill border (undownloaded state), card divider, helper-caption hairline |
| `--color-straw` (`#E07A66`) | destructive | Error state band ("Model download failed…", "Whisper crashed — falling back to Cloud (Deepgram)…") + Delete-local-models confirmation accent |
| (off-line warm) `#D9D0C0` | secondary button border | "Cancel" button border in dialog (matches Phase 1 Secondary `Button` border) |

**Accent (blueberry) reserved EXPLICITLY for:** the active-provider card border on the Local card when the user has switched the radio to Local. Inside the Local card, the model picker uses **matcha**, not blueberry, for the selected pill — this is the design board's intentional split (privacy = matcha, primary action = blueberry).

**Matcha reserved EXPLICITLY for:**

1. Selected-model pill border + `✓` glyph (`small.en ✓` per design board L534)
2. `ModelDownloadDialog` header glyph + glyph background tile + progress bar fill
3. Post-download readiness chip ("small.en is ready · runs on Metal" with `✓`)
4. "100% private" inline chip next to Local card title
5. "runs on Metal" mono caption tail when a model is loaded

**Destructive (strawberry) reserved EXPLICITLY for:**

1. Error band copy ("Model download failed…", "Whisper crashed — falling back to Cloud (Deepgram)…")
2. Destructive confirmation modal accent for "Delete local models?"

**Forbidden:** Do NOT introduce a new shade for the "selected model" state. Reuse `--color-matcha` border at 1.5px solid; pill body stays `--color-card` white (design-board L534).

---

## Copywriting Contract

All copy is final. Phase 8 contributes copy strings, not new visual chrome.

| Element | Copy |
|---------|------|
| Section title (inherited) | `Transcription` |
| Cloud card title (inherited from Phase 5) | `Cloud` |
| Cloud card sub-label (inherited) | `fastest · audio leaves the Mac` |
| Local card title | `Local · whisper.cpp` |
| Local card inline chip | `100% private` (matcha) |
| **Pre-download state body** | `Local transcription runs on your Mac — no audio leaves the machine. Pick a model size to get started.` |
| Model picker pills | `small.en` (default highlighted as recommended), `base.en`, `medium.en` — each with mono disk-space caption underneath (e.g., `~466MB`, `~141MB`, `~1.5GB`) |
| Per-model primary CTA (on row hover / focus) | `Use this model` |
| Helper caption under model picker | `Models download on first use · stored in ~/.yogurt/models/` |
| **Downloading state heading** (in dialog) | `Downloading small.en…` |
| Dialog mono sub-caption | `whisper.cpp · 487 MB` |
| Dialog progress caption (mono, live) | `~466MB · ~2 min` form — e.g., `302 / 487 MB` + `18 MB/s · ~10s left` (design-board L634 format) |
| Dialog reassurance body | `One-time download, stored in ~/.yogurt/models. Most users stay on cloud STT and never see this.` |
| Dialog secondary CTA | `Cancel` (closes dialog; in v1 download continues in background — D-13) |
| Dialog tertiary CTA | `Run in background` |
| **Post-download status row** | `small.en is ready · runs on Metal` with matcha `✓` glyph |
| **Error state — download** | `Model download failed — check your internet connection and retry.` |
| **Error state — runtime** | `Whisper crashed — falling back to Cloud (Deepgram). Check yogurt doctor logs.` |
| **Destructive confirmation** | `Delete local models? You'll need to re-download to use Local transcription again. Cloud (Deepgram) keeps working.` |
| Intel slow-warning chip (medium.en / large) | `slow` (mono, muted) inline next to the affected model pill — per CONTEXT D-12 |
| Empty state heading | (none — Phase 8 augments the existing Transcription card; no standalone empty state) |
| Empty state body | (none) |

**Voice rules carried from Phase 1:** warm + editorial + dev-tooly. Mono carries the technical truth (`small.en`, `~466MB`, `~/.yogurt/models/`, `runs on Metal`); Hanken carries the human reassurance ("no audio leaves the machine", "Most users stay on cloud STT and never see this").

**Privacy reassurance rule (load-bearing):** The string "no audio leaves the machine" MUST appear verbatim in the pre-download state. This is the entire reason Local STT exists — eliding it is a spec violation, not a copy choice.

**Baseline model rule (LOCAL-04):** `small.en` MUST be the highlighted default in the picker AND the model named in the "Downloading…" dialog heading mock + post-download success copy. The other sizes ship in the registry; `small.en` is the recommendation.

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | none | not required (`shadcn_initialized: false`) |
| Any third-party | none | not applicable |

**Custom components introduced this phase:**

| Component | Status | Notes |
|-----------|--------|-------|
| `LocalSTTCard` | **NEW** (Phase 8) | Replaces Phase 5 "Coming soon" stub. Renders inside the Settings → Transcription card-pair grid as the RIGHT column. Off-cream `#FCFAF5` surface when inactive; 1.5px blueberry border when the user has switched the radio to Local. Contains the inline matcha "100% private" chip, the `ModelPicker`, and the helper caption. |
| `ModelPicker` | **NEW** (Phase 8) | Pill row with three sizes (`small.en`, `base.en`, `medium.en`). Selected pill uses 1.5px matcha border + `✓` glyph + ink text. Undownloaded pills use 1px `--color-line` border + mono name + `↓` glyph + muted text. Intel CPU shows an inline `slow` mono chip next to `medium.en`. Disk-space caption sits under each pill in mono. Triggers `ModelDownloadDialog` on first selection. |
| `ModelDownloadDialog` | **DESIGNED in Phase 7 STATE-04 — WIRED in Phase 8** | Visual is approved and locked: 440px-wide rounded-16px card, `0 26px 60px -28px rgba(40,30,15,.34)` shadow, matcha `↓` glyph in 38×38 `#E7F0E8` tile, mono sub-caption, matcha progress bar (`#5E9E73` over `#EFEAE0` track, 5px radius, 8px height), mono progress caption row, Hanken reassurance body, Cancel + Run-in-background button row. **Phase 8 must NOT redesign — only wire `stt_model_download_progress`, `stt_model_download_complete`, `stt_model_download_error` WS events to it.** |

**Visual lock note:** if Phase 7 STATE-04's modal visual differs in any way from design-board lines 629–637 at execute time, raise the conflict in Phase 7's UI-SPEC first — do not branch the design here.

---

## Interaction Notes

These are not registry concerns but they bind UI behavior to backend events the Phase 8 plan ships.

| Trigger | UI behavior |
|---------|-------------|
| User clicks an undownloaded model pill | Open `ModelDownloadDialog`; backend kicks off SHA256-verified `Range:`-resumable download (D-08, D-09); dialog reads live `stt_model_download_progress` WS events from the global app socket. |
| `stt_model_download_complete` | Dialog closes; LocalSTTCard's selected pill swaps to matcha-bordered `✓` state; helper row beneath model picker shows the post-download status copy. |
| `stt_model_download_error` | Dialog stays open; matcha header glyph swaps to a strawberry `!` glyph in `--color-strsoft` tile; body swaps to the strawberry error copy; tertiary button row collapses to a single `Retry` (secondary variant). |
| User clicks `Cancel` (v1) | Dialog closes; download continues in background (D-13). No toast. The pill stays in `↓` state until completion event arrives. |
| User clicks `Run in background` | Same backend behavior as Cancel; dialog closes; the only difference is wording — both are documented as "Phase 8 cosmetic split, v1.1 wires true cancellation." |
| Whisper runtime panic during meeting | Banner-level error using the runtime error string ("Whisper crashed — falling back to Cloud…"). Banner uses the Phase 7 strawberry error-state band (inherited). |
| Settings → "Delete local models" (advanced action) | Destructive confirmation modal with the exact destructive copy above; Confirm button = Phase 1 Button `variant="primary"` overridden to strawberry — match Phase 7's "Delete meeting" treatment when that lands. |

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting: PASS
- [ ] Dimension 2 Visuals: PASS
- [ ] Dimension 3 Color: PASS
- [ ] Dimension 4 Typography: PASS
- [ ] Dimension 5 Spacing: PASS
- [ ] Dimension 6 Registry Safety: PASS

**Approval:** pending
