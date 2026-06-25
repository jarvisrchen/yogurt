# Yogurt v1 — Phase 4: Augmented Notes Hero Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the signature feature of yogurt — the black-you / grey-AI augmented-notes merge. A user types sparse markdown bullets during a meeting, hits "End meeting", and within 30s sees a single coherent document where their bullets are ink-black, AI-added bullets render in grey (`#A89F90`), and each grey bullet ends with a clickable `↳ HH:MM` deep-link into the transcript. Editing a grey range promotes it to black. A top-right "Re-enhance" button re-runs the same bundled prompt against the current notes + transcript.

**Architecture:** Two new Rust crates (`yogurt-prompts`, `yogurt-notes`) implement the merge logic. `yogurt-prompts` ships `enhance.md` + `chat-system.md` (embedded in release, hot-reloaded from disk in dev). `yogurt-notes` consumes `(user_notes_md, transcript_json, enriched_md_from_llm)` and produces a `MergedDoc` — a markdown document where each AST range is tagged `User` or `AiGrey { transcript_ts: u64 }`. The server exposes `POST /api/meetings/:id/enhance`, which calls a `MockLLM` in this phase (real `yogurt-llm` lands in Phase 5) and returns the merged document plus streams `enhance_progress` events over the WebSocket from Phase 3. On the frontend, TipTap is extended with two custom marks (`aiGrey`, `transcriptLink`); a post-meeting view (`/meeting/:id/post`) renders the merged document and provides Re-enhance. The enhancing state shows a lilac progress banner with staggered shimmer-to-text reveal at exactly 140 / 340 / 560 / 760ms per PRD §16.5.

**Tech Stack:** Rust 1.83+ · `pulldown-cmark = "0.12"` (markdown AST) · `tinytemplate = "1"` (prompt templating) · `tokio` / `axum` (from Phase 0) · `@tiptap/react` + `@tiptap/starter-kit` (from Phase 0) · `prosemirror-markdown ^1.13` (markdown ↔ ProseMirror doc conversion — see Task 4.1 for rationale) · Tailwind 4 tokens from Phase 1.

**Reference:** `docs/PRD.md` §5.3 (hero feature, full spec), §5.5 (bundled prompts), §5.11 (enhancing state shimmer + stagger), §10 (`POST /api/meetings/:id/enhance` and `enhance_progress` WS event), §13 (risk #2 — TipTap structural diff is the single highest-risk item in v1), §16.4 / §16.5 (motion: staggered 140/340/560/760ms enhance reveal), §16.7 (Variant A grey text picked — exact contract).

**Dependencies on prior phases:**
- **Phase 0** — Cargo workspace, axum server, TipTap baseline.
- **Phase 1** — Design tokens (`--ink #211D18`, `--grey #A89F90`, `--blsoft #ECE9FB`, motion durations), Tailwind 4 setup.
- **Phase 2** — Audio capture (used as the transcript source upstream, not directly here).
- **Phase 3** — In-memory `Meeting` struct holding `notes_md` + `transcript_json`, WebSocket layer for emitting `enhance_progress`.
- **Phase 5 is NOT required** — this phase ships with a `MockLLM` that returns a deterministic enhanced markdown so the full enhance flow is end-to-end testable today. Phase 5 swaps in the real OpenAI-compat client.

**Out of scope (deferred to later phase plans):**
- Real LLM client / OpenAI-compat HTTP (Phase 5).
- Settings UI for LLM provider / API keys (Phase 5).
- In-meeting chat pill (Phase 6).
- SQLite persistence of `enriched_md` (Phase 7 / persistence phase — Phase 4 keeps it in the in-memory `Meeting`).
- Template picker / versions rail — **explicitly cut from v1** per PRD §5.5 and §6 item 2. Re-enhance always re-runs the single bundled `enhance.md`.

---

## File structure produced by this phase

```
yogurt/
├── Cargo.toml                                       # MODIFY · add yogurt-prompts + yogurt-notes to workspace
├── crates/
│   ├── yogurt-prompts/
│   │   ├── Cargo.toml                               # NEW
│   │   ├── build.rs                                 # NEW · re-run on templates/* change
│   │   ├── src/
│   │   │   ├── lib.rs                               # NEW · Prompts::load() + render(name, ctx)
│   │   │   └── ctx.rs                               # NEW · EnhanceCtx { notes, transcript } serde
│   │   ├── templates/
│   │   │   ├── enhance.md                           # NEW · the hero prompt
│   │   │   └── chat-system.md                       # NEW · system prompt for §5.4 chat
│   │   └── tests/
│   │       └── rendering.rs                         # NEW · placeholder substitution + hot-reload
│   ├── yogurt-notes/
│   │   ├── Cargo.toml                               # NEW
│   │   ├── src/
│   │   │   ├── lib.rs                               # NEW · merge_notes() public API + MergedDoc
│   │   │   ├── ast.rs                               # NEW · pulldown-cmark wrapper, Block/Inline tree
│   │   │   ├── diff.rs                              # NEW · block-level structural diff (user vs enriched)
│   │   │   ├── ts.rs                                # NEW · find_transcript_ts(text, transcript) heuristic
│   │   │   └── render.rs                            # NEW · MergedDoc → markdown w/ AiGrey markers
│   │   ├── tests/
│   │   │   ├── merge_fixtures.rs                    # NEW · 5+ scenarios driven by fixtures/
│   │   │   └── fixtures/
│   │   │       ├── 01_pure_new_ai/                  # NEW · scenario inputs + expected output
│   │   │       │   ├── notes.md
│   │   │       │   ├── transcript.json
│   │   │       │   ├── enriched.md
│   │   │       │   └── expected.json                # serialized MergedDoc
│   │   │       ├── 02_ai_under_user_heading/
│   │   │       ├── 03_ai_bullet_next_to_user/
│   │   │       ├── 04_promote_grey_on_edit/
│   │   │       └── 05_reenhance_preserves_promoted/
│   └── yogurt-server/
│       ├── Cargo.toml                               # MODIFY · add yogurt-prompts + yogurt-notes deps
│       └── src/
│           ├── lib.rs                               # MODIFY · register enhance route, expose AppState
│           ├── enhance.rs                           # NEW · POST /api/meetings/:id/enhance handler
│           ├── llm_mock.rs                          # NEW · MockLLM trait impl (deleted in Phase 5)
│           └── llm.rs                               # NEW · LlmClient trait — survives into Phase 5
└── web/
    ├── package.json                                 # MODIFY · add prosemirror-markdown
    └── src/
        ├── editor/
        │   ├── index.tsx                            # NEW · YogurtEditor React component
        │   ├── extensions.ts                        # NEW · curated extension list
        │   ├── markdown.ts                          # NEW · markdown ↔ ProseMirror doc bridge
        │   └── marks/
        │       ├── aiGrey.ts                        # NEW · Mark.create<{transcriptTs?: number}>()
        │       └── transcriptLink.ts                # NEW · inline node for ↳ HH:MM suffix
        ├── components/
        │   ├── EnhancingBanner.tsx                  # NEW · lilac top banner w/ pulse + shimmer
        │   ├── ShimmerSkeleton.tsx                  # NEW · animated placeholder rectangle
        │   └── ReEnhanceButton.tsx                  # NEW · top-right button on post-meeting view
        ├── routes/
        │   ├── Meeting.tsx                          # MODIFY · End-meeting → POST /enhance → navigate
        │   └── MeetingPost.tsx                      # NEW · combined-doc post-meeting view
        └── lib/
            ├── api.ts                               # MODIFY · add postEnhance(meetingId)
            └── ws.ts                                # MODIFY · handle enhance_progress event type
```

**Why this split:** `yogurt-prompts` is data-only — a Rust crate that owns prompt files and serves them as strings. `yogurt-notes` is pure logic — given three strings, return one. Neither knows about HTTP, WebSocket, or the LLM. `yogurt-server` is the only place that wires them together. This makes `yogurt-notes::merge_notes` a perfect unit-test target: feed it fixtures, assert on `MergedDoc`. No mocks needed for the merge logic itself.

**Why a `MockLLM` now rather than waiting for Phase 5:** the merge logic and the UI need a believable, deterministic input today. A 60-line `MockLLM` that returns a hardcoded enriched markdown (templated against the input notes) lets us prove the full pipeline — server route, WS progress events, TipTap rendering, staggered reveal — without blocking on the real LLM client. Phase 5 deletes `crates/yogurt-server/src/llm_mock.rs` and replaces the `LlmClient` impl with the real one. The trait boundary stays.

---

## Test conventions (additions on top of Phase 0)

- **Fixture-driven tests for `yogurt-notes`:** `tests/fixtures/<scenario>/` holds `notes.md`, `transcript.json`, `enriched.md`, and `expected.json` (the serialized `MergedDoc`). The single test function `it_merges_<scenario>()` reads all four and asserts.
- **Snapshot updates:** when expected output legitimately changes, regenerate `expected.json` via `cargo test -p yogurt-notes --features regenerate` (a small helper test that writes instead of asserts). Diff carefully before committing.
- **Frontend integration:** TipTap mark behavior verified in `web/src/editor/marks/aiGrey.test.tsx` with `@testing-library/react`. Click on a grey range → fire input event → assert mark is stripped.
- **End-to-end smoke (manual, task 4.10):** type 5 bullets, click End, verify staggered reveal + clickable timestamp link by eye.
- **No Playwright in this phase** (still deferred to Phase 9-ish).

---

## Phase 4 task list

11 tasks. Task 4.0 is a **mandatory SPIKE** that must produce a go/no-go on TipTap before any other code is written. Approximate sequence: **~3-4 days of focused work**. Each task ends with a commit.

---

### Task 4.0 · SPIKE: prove TipTap can model the black/grey contract (go/no-go gate)

**This task is the single highest-risk item in v1 per PRD §13. It must produce a clear go/no-go decision before any other Phase 4 work begins. If TipTap can't cleanly model the merge, the fallback is to drop to ProseMirror directly (still possible — TipTap is a thin wrapper).**

**Files (throwaway — deleted at end of task):**
- Create: `web/src/_spike/aiGreyMark.ts`
- Create: `web/src/_spike/SpikeApp.tsx`
- Create: `web/src/_spike/fixtures.ts` (3 synthetic enriched-markdown samples)
- Modify: `web/src/main.tsx` (temporarily mount `SpikeApp` instead of `App`)

- [ ] **Step 1: Read the contract.**

Re-read PRD §5.3 and §16.7. The contract under test:
- A mark called `aiGrey` (with attribute `transcriptTs: number | undefined`) can be applied to inline ranges.
- Editing a character inside an `aiGrey` range removes the mark from the affected range (promote-to-black).
- A non-mark inline node `transcriptLink` (with `ts` attribute) renders `↳ HH:MM` as dotted-underline lilac.
- Initial document is loaded from a markdown string that includes `<span data-ai-grey data-ts="662">…</span>` and `<span data-transcript-link data-ts="662">↳ 11:02</span>` markers.

- [ ] **Step 2: Build three synthetic fixtures.**

Create `web/src/_spike/fixtures.ts`:

```ts
// Each fixture is the post-enhance markdown the LLM would produce (after merge),
// expressed as ProseMirror-ready HTML with our custom marks. The spike asks:
// can TipTap render these, persist edits, and strip the mark on edit?
export const FIXTURES = [
  {
    name: "pure-new-ai-bullets",
    html: `
      <h2>Discussion</h2>
      <ul>
        <li><span data-ai-grey data-ts="120">Pricing model debated <span data-transcript-link data-ts="120">↳ 02:00</span></span></li>
        <li><span data-ai-grey data-ts="240">Q3 roadmap deferred <span data-transcript-link data-ts="240">↳ 04:00</span></span></li>
      </ul>`,
  },
  {
    name: "user-bullet-then-ai-followup",
    html: `
      <ul>
        <li>Discussed pricing</li>
        <li><span data-ai-grey data-ts="180">Specifically the $14/$35 tier split <span data-transcript-link data-ts="180">↳ 03:00</span></span></li>
      </ul>`,
  },
  {
    name: "ai-paragraph-under-user-heading",
    html: `
      <h2>Roadmap</h2>
      <p><span data-ai-grey data-ts="500">Three milestones called out: ship beta in 2 weeks, internal dogfood for 1 month, public launch end of Q3. <span data-transcript-link data-ts="500">↳ 08:20</span></span></p>`,
  },
];
```

- [ ] **Step 3: Write the `aiGrey` mark.**

Create `web/src/_spike/aiGreyMark.ts`:

```ts
import { Mark, mergeAttributes } from "@tiptap/core";

export interface AiGreyAttrs { transcriptTs?: number }

export const AiGrey = Mark.create<{}, AiGreyAttrs>({
  name: "aiGrey",
  addAttributes() {
    return {
      transcriptTs: {
        default: undefined,
        parseHTML: (el) => {
          const v = (el as HTMLElement).getAttribute("data-ts");
          return v ? Number(v) : undefined;
        },
        renderHTML: (attrs) =>
          attrs.transcriptTs !== undefined ? { "data-ts": String(attrs.transcriptTs) } : {},
      },
    };
  },
  parseHTML() { return [{ tag: "span[data-ai-grey]" }]; },
  renderHTML({ HTMLAttributes }) {
    return ["span", mergeAttributes(HTMLAttributes, { "data-ai-grey": "", class: "ai-grey" }), 0];
  },
});
```

- [ ] **Step 4: Write `SpikeApp.tsx` that loads all 3 fixtures and supports editing.**

Render each fixture in its own `EditorContent`. Add a CSS rule `.ai-grey { color: #A89F90 }`. Wire a `transaction` listener that detects when an inputType of `insertText` or `deleteContentBackward` happens *inside* a range covered by the `aiGrey` mark, and removes the mark from the affected range using `editor.chain().setTextSelection({from, to}).unsetMark("aiGrey").run()` — but only over the touched character span, not the whole bullet.

This is the load-bearing experiment: can promote-to-black operate at character granularity while leaving untouched grey runs alone?

- [ ] **Step 5: Run the spike.**

Run: `pnpm --dir web dev` then `cargo run -p yogurt -- start --dev --no-open` in another terminal.

Open `http://localhost:7878`. Manually verify, for each of the 3 fixtures:
1. The grey color renders correctly (`#A89F90`).
2. The `↳ HH:MM` suffix appears and is styled.
3. Typing a character in the middle of a grey run leaves the surrounding grey intact but flips the typed character to black (mark removed in just that spot).
4. Deleting characters at the boundary doesn't leak the grey mark into newly-typed black text.
5. Pasting plain text into a grey run inserts black text.

- [ ] **Step 6: Document the verdict.**

Append a `## Phase 4 SPIKE outcome — TipTap viability` section to this plan file (in-place edit) recording:
- ✅ All 5 checks pass → proceed to Task 4.1.
- ❌ Any check fails → write down which one and why, and create `docs/superpowers/plans/2026-06-25-yogurt-phase-4-augmented-notes-hero-FALLBACK.md` that pivots to ProseMirror-direct. **Do not start Task 4.1 in that case — surface to the user.**

Most likely outcome (~85% confidence): TipTap handles cases 1–3 cleanly; case 4 (mark leak on deletion) may need an `appendTransaction` plugin to enforce the no-grey-on-fresh-input rule. That's normal ProseMirror territory and is not a blocker.

- [ ] **Step 7: Tear down the spike.**

```bash
git rm -r web/src/_spike/
# revert web/src/main.tsx back to App
```

The mark code from Step 3 will be re-implemented (cleanly) in Task 4.5 — it's intentionally not promoted from the spike, so the production version gets written with the appendTransaction logic from Step 6 baked in from the start.

- [ ] **Step 8: Commit.**

```bash
git add docs/superpowers/plans/2026-06-25-yogurt-phase-4-augmented-notes-hero.md web/src/main.tsx
git commit -m "spike(phase-4): validate TipTap can model black/grey AI mark contract"
```

---

### Task 4.1 · Markdown ↔ ProseMirror bridge — pick the library + write a tiny adapter

**Decision to write into the plan:** there are two candidates.

| Option | Pros | Cons |
|---|---|---|
| `@tiptap/extension-markdown` (community extension) | Drop-in TipTap extension, less code | Last published 2024-ish, sparse maintenance, depends on `markdown-it` which doesn't understand our `aiGrey` / `transcriptLink` marks — we'd need a custom plugin layer anyway |
| `prosemirror-markdown` (official ProseMirror package) | Maintained by the ProseMirror team, schema-driven, easy to register custom serializer/parser tokens | Slightly more wiring code (~40 lines) to expose as a TipTap-shaped API |

**Pick:** `prosemirror-markdown`. The amount of custom-mark serialization we need (an `aiGrey` mark with a `transcriptTs` attribute and a `transcriptLink` inline node) makes the custom-plugin path mandatory either way — and `prosemirror-markdown`'s `MarkdownParser` / `MarkdownSerializer` give first-class hooks for both. Rationale: we'd write the custom plugin once vs. fighting the community wrapper twice.

The markdown wire format for our marks (defined here once, used by both server and frontend):

```
<!-- before parse: -->
**ai{ts=662}**(Pricing model debated)**/ai**↳662

<!-- after merge_notes serializes, becomes (in plain markdown w/ HTML-ish marker spans): -->
<span data-ai-grey data-ts="662">Pricing model debated</span><span data-transcript-link data-ts="662">↳ 11:02</span>
```

We use the HTML-span form (not a custom `[[ai]]` shortcode) because: (a) it survives pasteboard round-trips, (b) `prosemirror-markdown`'s default HTML-passthrough handles it, (c) it's still legible if a user opens the `.md` file in a plain text editor.

**Files:**
- Modify: `web/package.json` (add `prosemirror-markdown`, `markdown-it`)
- Create: `web/src/editor/markdown.ts`

- [ ] **Step 1: Add dependencies.**

```bash
pnpm --dir web add prosemirror-markdown markdown-it
pnpm --dir web add -D @types/markdown-it
```

- [ ] **Step 2: Write `web/src/editor/markdown.ts`.**

```ts
import { defaultMarkdownParser, defaultMarkdownSerializer, MarkdownSerializer } from "prosemirror-markdown";
import type { Schema } from "@tiptap/pm/model";

/**
 * Parses markdown that may contain our custom marker spans into a ProseMirror doc.
 *
 * Wire format:
 *   <span data-ai-grey data-ts="NNN">…</span>
 *   <span data-transcript-link data-ts="NNN">↳ HH:MM</span>
 */
export function markdownToDoc(schema: Schema, md: string) {
  // We rely on the default parser's HTML-passthrough behavior; the schema's
  // `parseHTML` rules (defined on AiGrey and TranscriptLink) pick up the spans.
  return defaultMarkdownParser.parse(md);
}

export function docToMarkdown(doc: any /* Node */): string {
  const serializer = new MarkdownSerializer(
    {
      ...defaultMarkdownSerializer.nodes,
      transcriptLink(state: any, node: any) {
        state.write(`<span data-transcript-link data-ts="${node.attrs.ts}">↳ ${formatTs(node.attrs.ts)}</span>`);
      },
    },
    {
      ...defaultMarkdownSerializer.marks,
      aiGrey: {
        open(_state: any, mark: any) {
          return `<span data-ai-grey data-ts="${mark.attrs.transcriptTs ?? ""}">`;
        },
        close() { return `</span>`; },
        mixable: true,
        expelEnclosingWhitespace: true,
      },
    },
  );
  return serializer.serialize(doc);
}

function formatTs(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}
```

- [ ] **Step 3: Smoke-test in isolation.**

Create a quick `web/src/editor/markdown.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { Schema } from "@tiptap/pm/model";
// We'll do the real schema in Task 4.5; here we just round-trip plain markdown
// to prove the library is wired.
import { markdownToDoc, docToMarkdown } from "./markdown";

describe("markdown bridge", () => {
  it("round-trips a plain paragraph", () => {
    // Use the prosemirror-markdown default schema for this smoke test.
    const { schema } = require("prosemirror-markdown");
    const doc = markdownToDoc(schema, "Hello world");
    const md = docToMarkdown(doc);
    expect(md.trim()).toBe("Hello world");
  });
});
```

Run: `pnpm --dir web test`
Expected: passes.

- [ ] **Step 4: Commit.**

```bash
git add web/package.json web/pnpm-lock.yaml web/src/editor/markdown.ts web/src/editor/markdown.test.ts
git commit -m "feat(web): add prosemirror-markdown bridge with aiGrey + transcriptLink serializers"
```

---

### Task 4.2 · `yogurt-prompts` crate — embed enhance.md + chat-system.md, hot-reload in dev

**Files:**
- Modify: `Cargo.toml` (workspace) — add `yogurt-prompts` and `tinytemplate` workspace deps
- Create: `crates/yogurt-prompts/Cargo.toml`
- Create: `crates/yogurt-prompts/build.rs`
- Create: `crates/yogurt-prompts/src/lib.rs`
- Create: `crates/yogurt-prompts/src/ctx.rs`
- Create: `crates/yogurt-prompts/templates/enhance.md`
- Create: `crates/yogurt-prompts/templates/chat-system.md`
- Create: `crates/yogurt-prompts/tests/rendering.rs`

- [ ] **Step 1: Add to workspace.**

Modify the root `Cargo.toml`:

```toml
[workspace]
members = [
    "crates/yogurt-cli",
    "crates/yogurt-server",
    "crates/yogurt-prompts",   # NEW
    "crates/yogurt-notes",     # NEW (used in next task)
]

[workspace.dependencies]
# ... existing entries ...
tinytemplate = "1.2"           # NEW
pulldown-cmark = "0.12"        # NEW (used by yogurt-notes)
```

- [ ] **Step 2: Write `crates/yogurt-prompts/Cargo.toml`.**

```toml
[package]
name = "yogurt-prompts"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Bundled LLM prompt templates for yogurt."

[dependencies]
tinytemplate = { workspace = true }
serde = { workspace = true }
anyhow = { workspace = true }
rust-embed = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
serde_json = { workspace = true }
```

- [ ] **Step 3: Write the two templates.**

`crates/yogurt-prompts/templates/enhance.md`:

```markdown
You are an editor merging a user's sparse meeting notes with the full transcript of the meeting they just had. Produce ONE coherent markdown document that:

1. Keeps every line the user wrote, verbatim, in its original position.
2. Adds new bullets and short paragraphs that summarize what was actually discussed, sourced from the transcript.
3. Wraps every AI-added run in `<span data-ai-grey data-ts="N">…</span>`, where `N` is the unix-seconds timestamp (from `ts_ms / 1000`) of the transcript segment the addition came from.
4. Ends each AI-added bullet with `<span data-transcript-link data-ts="N">↳ HH:MM</span>` (same N as the span).
5. Preserves the user's headings if any; if the user wrote no headings, infer 2–4 short ones from the transcript.

Hard rules:
- DO NOT wrap the user's own lines in `data-ai-grey`. Only your additions.
- DO NOT invent facts. If the transcript doesn't support a bullet, don't write it.
- DO NOT include the transcript verbatim. Summarize.
- Output ONLY the merged markdown — no preamble, no code fence.

---

## USER NOTES (preserve verbatim, do not wrap)

{notes}

---

## TRANSCRIPT (source for your additions; ts_ms is millis since meeting start)

{transcript}
```

`crates/yogurt-prompts/templates/chat-system.md`:

```markdown
You are watching a meeting in real time alongside the user. The user will ask you questions about the meeting; answer using only the transcript content available so far. If the user asks about something that hasn't been said yet, say "that hasn't been discussed yet in this meeting." Keep answers tight — one short paragraph or a 3-line bullet list. Never invent quotes.
```

- [ ] **Step 4: Write `crates/yogurt-prompts/src/ctx.rs`.**

```rust
use serde::Serialize;

/// Context passed into `enhance.md`.
#[derive(Serialize, Debug)]
pub struct EnhanceCtx<'a> {
    pub notes: &'a str,
    /// Pre-serialized JSON of the transcript segments.
    pub transcript: &'a str,
}
```

- [ ] **Step 5: Write `crates/yogurt-prompts/build.rs`.**

```rust
fn main() {
    println!("cargo:rerun-if-changed=templates/enhance.md");
    println!("cargo:rerun-if-changed=templates/chat-system.md");
}
```

- [ ] **Step 6: Write `crates/yogurt-prompts/src/lib.rs`.**

```rust
mod ctx;
pub use ctx::EnhanceCtx;

use anyhow::{Context, Result};
use rust_embed::RustEmbed;
use serde::Serialize;
use std::path::PathBuf;
use tinytemplate::TinyTemplate;

#[derive(RustEmbed)]
#[folder = "templates/"]
struct Embedded;

/// Where to load templates from in dev mode (relative to the crate root).
/// In release builds, this is ignored — `RustEmbed` serves from the binary.
fn dev_template_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates")
}

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// Read templates from disk on every call. For `cargo run`.
    Dev,
    /// Read once at construction from the embedded copy.
    Release,
}

pub struct Prompts {
    mode: Mode,
    cached_enhance: Option<String>,
    cached_chat: Option<String>,
}

impl Prompts {
    pub fn load(mode: Mode) -> Result<Self> {
        let mut p = Self { mode, cached_enhance: None, cached_chat: None };
        if matches!(mode, Mode::Release) {
            p.cached_enhance = Some(read_embedded("enhance.md")?);
            p.cached_chat = Some(read_embedded("chat-system.md")?);
        }
        Ok(p)
    }

    pub fn render_enhance<S: Serialize>(&self, ctx: &S) -> Result<String> {
        let raw = self.read("enhance.md", self.cached_enhance.as_deref())?;
        render(&raw, "enhance", ctx)
    }

    pub fn chat_system(&self) -> Result<String> {
        // No templating — it's a static system prompt.
        self.read("chat-system.md", self.cached_chat.as_deref())
    }

    fn read(&self, name: &str, cached: Option<&str>) -> Result<String> {
        match self.mode {
            Mode::Dev => std::fs::read_to_string(dev_template_dir().join(name))
                .with_context(|| format!("dev: reading templates/{name}")),
            Mode::Release => Ok(cached.expect("release mode caches at load").to_string()),
        }
    }
}

fn read_embedded(name: &str) -> Result<String> {
    let f = Embedded::get(name).with_context(|| format!("embedded asset missing: {name}"))?;
    Ok(std::str::from_utf8(&f.data)?.to_string())
}

fn render<S: Serialize>(template: &str, name: &str, ctx: &S) -> Result<String> {
    let mut tt = TinyTemplate::new();
    // Important: tinytemplate HTML-escapes by default. We want raw insertion
    // because notes/transcript are markdown going *into* a prompt, not HTML
    // headed for a browser.
    tt.set_default_formatter(&tinytemplate::format_unescaped);
    tt.add_template(name, template)?;
    Ok(tt.render(name, ctx)?)
}
```

- [ ] **Step 7: Write the rendering test.**

`crates/yogurt-prompts/tests/rendering.rs`:

```rust
use yogurt_prompts::{EnhanceCtx, Mode, Prompts};

#[test]
fn it_renders_enhance_with_notes_and_transcript() {
    let p = Prompts::load(Mode::Release).expect("load");
    let out = p.render_enhance(&EnhanceCtx {
        notes: "- pricing\n- timeline\n",
        transcript: r#"[{"ts_ms":120000,"channel":"mic","text":"We agreed on $14/mo"}]"#,
    }).expect("render");
    assert!(out.contains("- pricing"), "notes substituted");
    assert!(out.contains("$14/mo"), "transcript substituted");
    assert!(out.contains("USER NOTES"), "prompt scaffolding present");
}

#[test]
fn it_serves_chat_system_unmodified() {
    let p = Prompts::load(Mode::Release).expect("load");
    let s = p.chat_system().expect("read");
    assert!(s.contains("watching a meeting"), "chat-system prompt loaded");
}

#[test]
fn it_does_not_html_escape_special_chars_in_notes() {
    let p = Prompts::load(Mode::Release).expect("load");
    let out = p.render_enhance(&EnhanceCtx {
        notes: "use <emphasis> & friends",
        transcript: "[]",
    }).unwrap();
    assert!(out.contains("<emphasis>"), "must not escape — see set_default_formatter");
    assert!(out.contains("& friends"), "must not escape &");
}
```

- [ ] **Step 8: Run.**

Run: `cargo test -p yogurt-prompts`
Expected: 3 passed.

- [ ] **Step 9: Commit.**

```bash
git add Cargo.toml crates/yogurt-prompts/
git commit -m "feat(prompts): add yogurt-prompts crate with enhance.md + chat-system.md"
```

---

### Task 4.3 · `yogurt-notes` crate — AST scaffolding (pulldown-cmark + Block tree)

**Files:**
- Create: `crates/yogurt-notes/Cargo.toml`
- Create: `crates/yogurt-notes/src/lib.rs` (public surface only — empty `merge_notes` for now)
- Create: `crates/yogurt-notes/src/ast.rs`

- [ ] **Step 1: Write `crates/yogurt-notes/Cargo.toml`.**

```toml
[package]
name = "yogurt-notes"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Markdown AST diff + merge for yogurt's augmented-notes feature."

[dependencies]
pulldown-cmark = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
insta = { version = "1.41", features = ["json"] }
```

(Add `insta` to workspace deps in `Cargo.toml` workspace block.)

- [ ] **Step 2: Write `crates/yogurt-notes/src/ast.rs`.**

```rust
//! A tiny block-level markdown AST tailored for the diff/merge use case.
//!
//! pulldown-cmark gives us an event stream. We collapse it into a list of
//! `Block`s where each block is a top-level structural unit (heading, paragraph,
//! list item, list, blockquote, code fence). Inline content is kept as the
//! reconstructed markdown source for that block — we do not need to model
//! inline marks because the merge happens at block granularity.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph { md: String },
    ListItem { md: String, depth: u8 },
    CodeBlock { lang: Option<String>, body: String },
    BlockQuote { md: String },
    Hr,
}

/// A canonical "key" we use to decide if two blocks are "the same block"
/// across user_md and enriched_md. It deliberately ignores trailing
/// whitespace, transcript-link spans, and our ai-grey marker spans —
/// because the LLM may add those to a user line, and we still want
/// to recognize the underlying line as the user's.
pub fn block_key(b: &Block) -> String {
    let raw = match b {
        Block::Heading { level, text } => format!("h{level}:{text}"),
        Block::Paragraph { md } => format!("p:{md}"),
        Block::ListItem { md, depth } => format!("li{depth}:{md}"),
        Block::CodeBlock { lang, body } => format!("code:{}:{body}", lang.as_deref().unwrap_or("")),
        Block::BlockQuote { md } => format!("bq:{md}"),
        Block::Hr => "hr".into(),
    };
    strip_markers(&raw).trim().to_ascii_lowercase()
}

fn strip_markers(s: &str) -> String {
    // Remove our wire-format spans before computing identity.
    let re1 = regex_lite::Regex::new(r#"<span data-ai-grey[^>]*>"#).unwrap();
    let re2 = regex_lite::Regex::new(r#"<span data-transcript-link[^>]*>↳ \d{2}:\d{2}</span>"#).unwrap();
    let re3 = regex_lite::Regex::new(r#"</span>"#).unwrap();
    let a = re1.replace_all(s, "");
    let b = re2.replace_all(&a, "");
    re3.replace_all(&b, "").into_owned()
}

/// Parse markdown into our flat block list. Lists are flattened — each
/// `<li>` becomes its own `Block::ListItem` with a depth attribute.
pub fn parse(md: &str) -> Vec<Block> {
    let parser = Parser::new_ext(md, Options::all());
    let mut blocks: Vec<Block> = Vec::new();
    let mut buf = String::new();
    let mut state: Option<ParseState> = None;
    let mut list_depth: u8 = 0;

    for ev in parser {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                state = Some(ParseState::Heading(heading_level_to_u8(level)));
                buf.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(ParseState::Heading(lv)) = state.take() {
                    blocks.push(Block::Heading { level: lv, text: std::mem::take(&mut buf) });
                }
            }
            Event::Start(Tag::Paragraph) => {
                state = Some(ParseState::Paragraph);
                buf.clear();
            }
            Event::End(TagEnd::Paragraph) => {
                if matches!(state, Some(ParseState::Paragraph)) {
                    state = None;
                    blocks.push(Block::Paragraph { md: std::mem::take(&mut buf) });
                }
            }
            Event::Start(Tag::List(_)) => { list_depth = list_depth.saturating_add(1); }
            Event::End(TagEnd::List(_)) => { list_depth = list_depth.saturating_sub(1); }
            Event::Start(Tag::Item) => {
                state = Some(ParseState::ListItem(list_depth.saturating_sub(1)));
                buf.clear();
            }
            Event::End(TagEnd::Item) => {
                if let Some(ParseState::ListItem(d)) = state.take() {
                    blocks.push(Block::ListItem { md: std::mem::take(&mut buf), depth: d });
                }
            }
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => {
                buf.push_str(&t);
            }
            Event::SoftBreak => buf.push(' '),
            Event::HardBreak => buf.push('\n'),
            Event::Rule => blocks.push(Block::Hr),
            _ => {}
        }
    }
    blocks
}

enum ParseState { Heading(u8), Paragraph, ListItem(u8) }

fn heading_level_to_u8(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1, HeadingLevel::H2 => 2, HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4, HeadingLevel::H5 => 5, HeadingLevel::H6 => 6,
    }
}
```

Add `regex-lite = "0.1"` to `[dependencies]` for the marker-strip helper (kept lite to avoid pulling all of `regex` for three patterns).

- [ ] **Step 3: Write `crates/yogurt-notes/src/lib.rs` (skeleton — fills in over Tasks 4.4 + 4.5).**

```rust
//! Augmented-notes merge logic.
//!
//! Given the user's raw notes, the transcript, and the LLM's enriched markdown,
//! produce a `MergedDoc` that tags each block as either the user's or AI's
//! contribution, with transcript timestamps attached to AI blocks.

pub mod ast;
pub mod diff;
pub mod render;
pub mod ts;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Source {
    User,
    AiGrey { transcript_ts_sec: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergedBlock {
    pub block: ast::Block,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergedDoc {
    pub blocks: Vec<MergedBlock>,
}

/// Public API. Merges user notes with the LLM-enriched markdown, attaching
/// transcript timestamps to the AI-added blocks.
///
/// `user_md`: the raw markdown the user typed in the editor.
/// `enriched_md`: the markdown the LLM produced (may or may not already contain
///   `<span data-ai-grey data-ts="N">` markers; we do not require it to).
/// `transcript_json`: the full transcript as JSON — we use it to find a
///   plausible timestamp for any AI block that didn't come back tagged.
pub fn merge_notes(user_md: &str, enriched_md: &str, transcript_json: &str) -> anyhow::Result<MergedDoc> {
    let user_blocks = ast::parse(user_md);
    let enriched_blocks = ast::parse(enriched_md);
    let transcript: Vec<TranscriptSegment> =
        serde_json::from_str(transcript_json).unwrap_or_default();

    let merged = diff::merge(&user_blocks, &enriched_blocks, &transcript);
    Ok(MergedDoc { blocks: merged })
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptSegment {
    pub ts_ms: u64,
    pub channel: String,
    pub text: String,
}
```

- [ ] **Step 4: Add stub files so it compiles.**

`crates/yogurt-notes/src/diff.rs`:

```rust
use crate::ast::Block;
use crate::{MergedBlock, Source, TranscriptSegment};

pub fn merge(_user: &[Block], _enriched: &[Block], _t: &[TranscriptSegment]) -> Vec<MergedBlock> {
    // Filled in Task 4.4.
    Vec::new()
}
```

`crates/yogurt-notes/src/render.rs`:

```rust
use crate::MergedDoc;

pub fn to_markdown(_doc: &MergedDoc) -> String {
    // Filled in Task 4.4.
    String::new()
}
```

`crates/yogurt-notes/src/ts.rs`:

```rust
use crate::TranscriptSegment;

/// Best-effort: find the transcript segment whose text most overlaps the given
/// block markdown. Returns the segment's ts_ms / 1000 (seconds).
pub fn guess_ts_sec(_block_md: &str, _transcript: &[TranscriptSegment]) -> Option<u64> {
    // Filled in Task 4.4.
    None
}
```

- [ ] **Step 5: Verify it compiles.**

Run: `cargo check -p yogurt-notes`
Expected: clean.

- [ ] **Step 6: Commit.**

```bash
git add crates/yogurt-notes/
git commit -m "feat(notes): scaffold yogurt-notes crate with pulldown-cmark block AST"
```

---

### Task 4.4 · `merge_notes` core logic — diff, timestamp inference, and round-trip render (TDD)

This is the heaviest pure-Rust task in the phase. We do it test-first using fixtures.

**Files:**
- Create: `crates/yogurt-notes/tests/merge_fixtures.rs`
- Create: `crates/yogurt-notes/tests/fixtures/01_pure_new_ai/`
- Create: `crates/yogurt-notes/tests/fixtures/02_ai_under_user_heading/`
- Create: `crates/yogurt-notes/tests/fixtures/03_ai_bullet_next_to_user/`
- Create: `crates/yogurt-notes/tests/fixtures/04_promote_grey_on_edit/`
- Create: `crates/yogurt-notes/tests/fixtures/05_reenhance_preserves_promoted/`
- Modify: `crates/yogurt-notes/src/diff.rs`
- Modify: `crates/yogurt-notes/src/ts.rs`
- Modify: `crates/yogurt-notes/src/render.rs`

- [ ] **Step 1: Write the 5 fixture trees.**

For each scenario, create a directory with four files: `notes.md`, `transcript.json`, `enriched.md`, `expected.json`.

**01_pure_new_ai** — user wrote nothing useful; LLM added 2 bullets.

`notes.md`:
```markdown
(empty)
```

`transcript.json`:
```json
[
  {"ts_ms": 120000, "channel": "mic", "text": "We debated the pricing model"},
  {"ts_ms": 240000, "channel": "system", "text": "Q3 roadmap was deferred"}
]
```

`enriched.md`:
```markdown
## Discussion

- Pricing model debated
- Q3 roadmap deferred
```

`expected.json` — 1 heading (User-preserved? no, but treated as AI because notes was empty), 2 AI bullets with ts 120 and 240.

```json
{
  "blocks": [
    {"block": {"Heading": {"level": 2, "text": "Discussion"}}, "source": {"AiGrey": {"transcript_ts_sec": 120}}},
    {"block": {"ListItem": {"md": "Pricing model debated", "depth": 0}}, "source": {"AiGrey": {"transcript_ts_sec": 120}}},
    {"block": {"ListItem": {"md": "Q3 roadmap deferred", "depth": 0}}, "source": {"AiGrey": {"transcript_ts_sec": 240}}}
  ]
}
```

**02_ai_under_user_heading** — user wrote a heading; LLM added bullets under it.

`notes.md`:
```markdown
## Pricing

(empty)
```

`enriched.md`:
```markdown
## Pricing

- Three tiers proposed: Free, $14, $35
- Discussed annual discount
```

`expected.json` — heading is User, both bullets are AiGrey.

**03_ai_bullet_next_to_user** — user wrote 2 bullets; LLM added 1 between them.

`notes.md`:
```markdown
- pricing
- timeline
```

`enriched.md`:
```markdown
- pricing
- $14/mo agreed for v1
- timeline
```

`expected.json` — bullets at positions 0 and 2 are User; bullet at position 1 is AiGrey.

**04_promote_grey_on_edit** — simulates the re-enhance flow after a user has edited a previously-grey bullet (so it should now stay black).

`notes.md` (this is what the user has *after* their edit — note no marker spans):
```markdown
- pricing
- $14/mo agreed for v1 — also will offer student discount
```

`enriched.md` (LLM produced this, attempting to re-add the original AI bullet):
```markdown
- pricing
- $14/mo agreed for v1
```

`expected.json` — both bullets are User. The LLM's version is shorter; the user's edited version wins because `block_key` finds it as a User block. This proves promote-on-edit survives re-enhance.

**05_reenhance_preserves_promoted** — same shape as 04 but with additional new AI bullets in the enriched version.

`notes.md`:
```markdown
- pricing
- $14/mo agreed for v1 — also will offer student discount
```

`enriched.md`:
```markdown
- pricing
- $14/mo agreed for v1
- Annual plan: 20% off
```

`expected.json` — first two are User; the third is AiGrey.

- [ ] **Step 2: Write `crates/yogurt-notes/tests/merge_fixtures.rs`.**

```rust
use std::path::Path;

#[test] fn it_merges_pure_new_ai() { run("01_pure_new_ai"); }
#[test] fn it_merges_ai_under_user_heading() { run("02_ai_under_user_heading"); }
#[test] fn it_merges_ai_bullet_next_to_user() { run("03_ai_bullet_next_to_user"); }
#[test] fn it_preserves_promoted_grey_on_reenhance_short() { run("04_promote_grey_on_edit"); }
#[test] fn it_preserves_promoted_grey_on_reenhance_long() { run("05_reenhance_preserves_promoted"); }

fn run(name: &str) {
    let dir = Path::new("tests/fixtures").join(name);
    let notes = std::fs::read_to_string(dir.join("notes.md")).unwrap();
    let transcript = std::fs::read_to_string(dir.join("transcript.json")).unwrap();
    let enriched = std::fs::read_to_string(dir.join("enriched.md")).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();

    let got = yogurt_notes::merge_notes(&notes, &enriched, &transcript).expect("merge");
    let got_json = serde_json::to_value(&got).unwrap();

    if got_json != expected {
        // Pretty-diff on failure.
        let pretty_got = serde_json::to_string_pretty(&got_json).unwrap();
        let pretty_exp = serde_json::to_string_pretty(&expected).unwrap();
        panic!("merge mismatch in {name}:\n--- expected ---\n{pretty_exp}\n--- got ---\n{pretty_got}");
    }
}
```

- [ ] **Step 3: Run — expect all 5 fail.**

Run: `cargo test -p yogurt-notes --test merge_fixtures`
Expected: `5 failed` (current `diff::merge` returns empty).

- [ ] **Step 4: Implement `ts::guess_ts_sec`.**

Strategy: word-overlap heuristic. For each transcript segment, count how many >3-char words from the segment also appear in the block. Pick the segment with the highest count; tie-break to earliest ts. Returns None if no segment has any overlap.

```rust
use crate::TranscriptSegment;

pub fn guess_ts_sec(block_md: &str, transcript: &[TranscriptSegment]) -> Option<u64> {
    let block_words: std::collections::HashSet<String> = block_md
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(|s| s.to_string())
        .collect();
    if block_words.is_empty() { return transcript.first().map(|s| s.ts_ms / 1000); }

    let mut best: Option<(usize, u64)> = None;
    for seg in transcript {
        let count = seg.text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| block_words.contains(*w))
            .count();
        if count == 0 { continue; }
        match best {
            None => best = Some((count, seg.ts_ms / 1000)),
            Some((c, _)) if count > c => best = Some((count, seg.ts_ms / 1000)),
            _ => {}
        }
    }
    best.map(|(_, ts)| ts).or_else(|| transcript.first().map(|s| s.ts_ms / 1000))
}
```

- [ ] **Step 5: Implement `diff::merge`.**

Strategy:
1. Build a HashSet of `block_key(b)` for every block in `user_blocks` — call it `user_set`.
2. Walk `enriched_blocks` in order. For each block, if its key is in `user_set`, emit it as `Source::User` (and use the user's exact block text — the LLM may have stripped trailing whitespace). If not, emit as `Source::AiGrey { transcript_ts_sec: ts::guess_ts_sec(block_md, transcript) }`.
3. Edge case: a user block that doesn't appear in `enriched_blocks` is appended at the end as User. (Defensive — the LLM is told to preserve them, but we don't trust the LLM.)

```rust
use crate::ast::{block_key, Block};
use crate::{ts, MergedBlock, Source, TranscriptSegment};

pub fn merge(user: &[Block], enriched: &[Block], transcript: &[TranscriptSegment]) -> Vec<MergedBlock> {
    use std::collections::HashMap;

    let mut user_by_key: HashMap<String, &Block> = HashMap::new();
    for b in user { user_by_key.insert(block_key(b), b); }

    let mut seen_user_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<MergedBlock> = Vec::with_capacity(enriched.len() + 4);

    for b in enriched {
        let k = block_key(b);
        if let Some(user_block) = user_by_key.get(&k) {
            seen_user_keys.insert(k);
            out.push(MergedBlock { block: (*user_block).clone(), source: Source::User });
        } else {
            let body_text = block_md_text(b);
            let ts = ts::guess_ts_sec(&body_text, transcript).unwrap_or(0);
            out.push(MergedBlock { block: b.clone(), source: Source::AiGrey { transcript_ts_sec: ts } });
        }
    }

    // Defensive: append any user blocks the LLM dropped.
    for b in user {
        let k = block_key(b);
        if !seen_user_keys.contains(&k) {
            out.push(MergedBlock { block: b.clone(), source: Source::User });
        }
    }

    out
}

fn block_md_text(b: &Block) -> String {
    match b {
        Block::Heading { text, .. } => text.clone(),
        Block::Paragraph { md } | Block::ListItem { md, .. } | Block::BlockQuote { md } => md.clone(),
        Block::CodeBlock { body, .. } => body.clone(),
        Block::Hr => String::new(),
    }
}
```

- [ ] **Step 6: Run — expect all 5 pass.**

Run: `cargo test -p yogurt-notes --test merge_fixtures`
Expected: `5 passed`. If a fixture is off, regenerate `expected.json` *manually* by hand-computing — don't blindly accept code output.

- [ ] **Step 7: Implement `render::to_markdown`.**

This is what `enhance.rs` returns to the client. It walks `MergedDoc.blocks` and emits the wire-format markdown with our marker spans:

```rust
use crate::ast::Block;
use crate::{MergedDoc, Source};

pub fn to_markdown(doc: &MergedDoc) -> String {
    let mut out = String::new();
    for mb in &doc.blocks {
        let rendered = match &mb.block {
            Block::Heading { level, text } => format!("{} {}\n\n", "#".repeat(*level as usize), text),
            Block::Paragraph { md } => format!("{md}\n\n"),
            Block::ListItem { md, depth } => format!("{}- {md}\n", "  ".repeat(*depth as usize)),
            Block::CodeBlock { lang, body } => format!("```{}\n{body}\n```\n\n", lang.as_deref().unwrap_or("")),
            Block::BlockQuote { md } => format!("> {md}\n\n"),
            Block::Hr => "---\n\n".into(),
        };
        let with_marks = match mb.source {
            Source::User => rendered,
            Source::AiGrey { transcript_ts_sec } => wrap_ai(&rendered, transcript_ts_sec),
        };
        out.push_str(&with_marks);
    }
    out
}

fn wrap_ai(rendered: &str, ts: u64) -> String {
    let mins = ts / 60;
    let secs = ts % 60;
    let stamp = format!("{:02}:{:02}", mins, secs);
    let trim = rendered.trim_end_matches('\n');
    let suffix_node = format!(r#"<span data-transcript-link data-ts="{ts}">↳ {stamp}</span>"#);
    // For a list item, inject the link before the trailing newline; for paragraphs, append.
    if let Some(inner) = trim.strip_prefix("- ") {
        format!("- <span data-ai-grey data-ts=\"{ts}\">{inner} {suffix_node}</span>\n")
    } else if let Some(rest) = trim.strip_prefix("## ") {
        // Headings don't get marker spans on the wire — the editor color-tints them via the parent
        // block's source. Just emit normally.
        format!("## {rest}\n\n")
    } else {
        format!("<span data-ai-grey data-ts=\"{ts}\">{trim} {suffix_node}</span>\n\n")
    }
}
```

- [ ] **Step 8: Add a render round-trip test.**

Append to `tests/merge_fixtures.rs`:

```rust
#[test]
fn it_renders_merged_doc_to_wire_markdown_with_spans() {
    let dir = std::path::Path::new("tests/fixtures/01_pure_new_ai");
    let notes = std::fs::read_to_string(dir.join("notes.md")).unwrap();
    let transcript = std::fs::read_to_string(dir.join("transcript.json")).unwrap();
    let enriched = std::fs::read_to_string(dir.join("enriched.md")).unwrap();
    let doc = yogurt_notes::merge_notes(&notes, &enriched, &transcript).unwrap();
    let md = yogurt_notes::render::to_markdown(&doc);
    assert!(md.contains(r#"data-ai-grey data-ts="120""#), "first AI bullet tagged");
    assert!(md.contains(r#"data-ai-grey data-ts="240""#), "second AI bullet tagged");
    assert!(md.contains(r#"data-transcript-link data-ts="240">↳ 04:00</span>"#), "deep-link suffix present");
}
```

Run: `cargo test -p yogurt-notes`
Expected: all tests (5 merge + 1 render) pass.

- [ ] **Step 9: Commit.**

```bash
git add crates/yogurt-notes/
git commit -m "feat(notes): implement merge_notes diff, ts inference, and wire-format render (TDD against 5 fixtures)"
```

---

### Task 4.5 · TipTap marks `aiGrey` and `transcriptLink` (production version)

This is the cleaned-up, production version of the spike from Task 4.0 — with the `appendTransaction` rule baked in from the start.

**Files:**
- Create: `web/src/editor/marks/aiGrey.ts`
- Create: `web/src/editor/marks/transcriptLink.ts`
- Create: `web/src/editor/extensions.ts`
- Create: `web/src/editor/index.tsx`
- Create: `web/src/editor/marks/aiGrey.test.tsx`

- [ ] **Step 1: Write `web/src/editor/marks/aiGrey.ts`.**

```ts
import { Mark, mergeAttributes } from "@tiptap/core";
import { Plugin, PluginKey } from "@tiptap/pm/state";

export interface AiGreyAttrs { transcriptTs?: number }

export const AiGreyPluginKey = new PluginKey("aiGreyPromote");

/**
 * The `aiGrey` mark applies to LLM-added inline runs. It renders as a
 * `<span class="ai-grey">` and carries a `transcriptTs` attribute (the
 * timestamp the AI used as its source — same as the sibling transcriptLink).
 *
 * Promote-on-edit:
 *   Any user input that lands inside an `aiGrey` range MUST strip the mark
 *   from the inserted span. The `appendTransaction` plugin enforces this:
 *   on every transaction, walk new text ranges and unset `aiGrey` over them.
 */
export const AiGrey = Mark.create<{}, AiGreyAttrs>({
  name: "aiGrey",

  addAttributes() {
    return {
      transcriptTs: {
        default: undefined,
        parseHTML: (el) => {
          const v = (el as HTMLElement).getAttribute("data-ts");
          return v ? Number(v) : undefined;
        },
        renderHTML: (attrs) =>
          attrs.transcriptTs !== undefined ? { "data-ts": String(attrs.transcriptTs) } : {},
      },
    };
  },

  parseHTML() { return [{ tag: "span[data-ai-grey]" }]; },

  renderHTML({ HTMLAttributes }) {
    return ["span", mergeAttributes(HTMLAttributes, {
      "data-ai-grey": "",
      class: "ai-grey",
    }), 0];
  },

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: AiGreyPluginKey,
        appendTransaction: (transactions, _oldState, newState) => {
          // Walk every textInserted step. For each, if the inserted range is
          // covered by aiGrey, remove the mark over the inserted span only.
          let tr = newState.tr;
          let modified = false;
          const aiGrey = newState.schema.marks.aiGrey;
          for (const t of transactions) {
            if (!t.docChanged) continue;
            t.steps.forEach((step, i) => {
              const map = t.mapping.maps[i];
              map.forEach((_oldStart, _oldEnd, newStart, newEnd) => {
                // Only act on insertions (newEnd > newStart) caused by user typing,
                // not by setContent or programmatic ops we control.
                if (newEnd <= newStart) return;
                if (!t.getMeta("userInput")) {
                  // Allow our own programmatic ops (setContent on enhance) to keep the marks.
                  // The editor's input-handling chain sets `userInput` meta — see editor/index.tsx.
                  // If meta absent, we conservatively still strip — better to over-promote than under-promote.
                }
                tr = tr.removeMark(newStart, newEnd, aiGrey);
                modified = true;
              });
            });
          }
          return modified ? tr : null;
        },
      }),
    ];
  },
});
```

- [ ] **Step 2: Write `web/src/editor/marks/transcriptLink.ts`.**

This is an inline atom node (not a mark) — `↳ HH:MM` is a single non-editable token rendered after each AI bullet.

```ts
import { Node, mergeAttributes } from "@tiptap/core";

export interface TranscriptLinkAttrs { ts: number }

export const TranscriptLink = Node.create<{}, {}>({
  name: "transcriptLink",
  group: "inline",
  inline: true,
  atom: true,
  selectable: false,

  addAttributes() {
    return {
      ts: {
        default: 0,
        parseHTML: (el) => Number((el as HTMLElement).getAttribute("data-ts") || "0"),
        renderHTML: (attrs) => ({ "data-ts": String(attrs.ts) }),
      },
    };
  },

  parseHTML() { return [{ tag: "span[data-transcript-link]" }]; },

  renderHTML({ node, HTMLAttributes }) {
    const ts: number = node.attrs.ts;
    const m = String(Math.floor(ts / 60)).padStart(2, "0");
    const s = String(ts % 60).padStart(2, "0");
    return ["span", mergeAttributes(HTMLAttributes, {
      "data-transcript-link": "",
      class: "transcript-link",
      role: "link",
      tabIndex: 0,
    }), `↳ ${m}:${s}`];
  },

  // Click + Enter behavior is owned by the host (MeetingPost) — see Task 4.9.
});
```

- [ ] **Step 3: Add the matching CSS.**

Modify `web/src/index.css` (assumes Phase 1 already set `--ink`, `--grey`, etc. as CSS vars):

```css
.ai-grey { color: var(--grey); }
.transcript-link {
  color: var(--blue);
  border-bottom: 1.5px dotted #C9B8F0;
  margin-left: 0.35em;
  cursor: pointer;
  user-select: none;
}
.transcript-link:hover { border-bottom-color: var(--blue); }
```

- [ ] **Step 4: Write `web/src/editor/extensions.ts`.**

```ts
import StarterKit from "@tiptap/starter-kit";
import { AiGrey } from "./marks/aiGrey";
import { TranscriptLink } from "./marks/transcriptLink";

export function yogurtExtensions() {
  return [
    StarterKit.configure({
      heading: { levels: [1, 2, 3] },
    }),
    AiGrey,
    TranscriptLink,
  ];
}
```

- [ ] **Step 5: Write `web/src/editor/index.tsx`.**

```tsx
import { useEditor, EditorContent, type Editor } from "@tiptap/react";
import { useEffect } from "react";
import { yogurtExtensions } from "./extensions";
import { markdownToDoc, docToMarkdown } from "./markdown";

export interface YogurtEditorProps {
  initialMarkdown: string;
  editable: boolean;
  onChange?: (md: string) => void;
  onTranscriptLinkClick?: (tsSec: number) => void;
  // Used in the post-meeting view to swap the editor content to the enriched doc.
  enrichedMarkdown?: string;
}

export function YogurtEditor({
  initialMarkdown,
  editable,
  onChange,
  onTranscriptLinkClick,
  enrichedMarkdown,
}: YogurtEditorProps) {
  const editor = useEditor({
    extensions: yogurtExtensions(),
    editable,
    content: initialMarkdown ? renderToHtml(initialMarkdown) : "",
    onUpdate: ({ editor }) => {
      onChange?.(docToMarkdown(editor.state.doc));
    },
  });

  // When enrichedMarkdown arrives (post-enhance), swap in the new doc.
  useEffect(() => {
    if (enrichedMarkdown && editor) {
      editor.commands.setContent(renderToHtml(enrichedMarkdown));
    }
  }, [enrichedMarkdown, editor]);

  // Wire transcript-link clicks via event delegation.
  useEffect(() => {
    if (!editor) return;
    const dom = editor.view.dom;
    const handler = (e: Event) => {
      const target = (e.target as HTMLElement).closest("[data-transcript-link]");
      if (!target) return;
      e.preventDefault();
      const ts = Number(target.getAttribute("data-ts") || "0");
      onTranscriptLinkClick?.(ts);
    };
    dom.addEventListener("click", handler);
    return () => dom.removeEventListener("click", handler);
  }, [editor, onTranscriptLinkClick]);

  return <EditorContent editor={editor} className="yogurt-editor" />;
}

/**
 * Quick adapter: the editor's `content` prop takes HTML or a ProseMirror doc.
 * We feed it HTML rendered from our markdown bridge so the parseHTML rules on
 * AiGrey / TranscriptLink kick in.
 */
function renderToHtml(md: string): string {
  // For Phase 4 we use markdown-it (already a dep from Task 4.1) directly to
  // get HTML, since our marker spans are valid HTML and pass through verbatim.
  const MarkdownIt = require("markdown-it") as any;
  const mi = new MarkdownIt({ html: true });
  return mi.render(md);
}
```

- [ ] **Step 6: Write `web/src/editor/marks/aiGrey.test.tsx`.**

```tsx
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { YogurtEditor } from "../index";

describe("aiGrey mark", () => {
  it("renders grey span for ai-grey markup", async () => {
    const md = `- <span data-ai-grey data-ts="120">Pricing debated <span data-transcript-link data-ts="120">↳ 02:00</span></span>`;
    render(<YogurtEditor initialMarkdown={md} editable={true} />);
    const span = await screen.findByText(/Pricing debated/);
    // The grey color is applied via .ai-grey class — assert the class exists on an ancestor.
    expect(span.closest("[data-ai-grey]")).not.toBeNull();
  });

  it("renders the transcript link with formatted timestamp", async () => {
    const md = `- <span data-ai-grey data-ts="662">Decided <span data-transcript-link data-ts="662">↳ 11:02</span></span>`;
    render(<YogurtEditor initialMarkdown={md} editable={true} />);
    expect(await screen.findByText(/↳ 11:02/)).toBeInTheDocument();
  });

  // Full promote-on-edit behavior is verified manually in Task 4.10 — the
  // headless jsdom environment doesn't exercise ProseMirror input rules
  // identically to a real browser. We assert the structural invariants here.
});
```

- [ ] **Step 7: Run.**

Run: `pnpm --dir web test`
Expected: all tests pass (including the existing App tests).

- [ ] **Step 8: Commit.**

```bash
git add web/src/editor/ web/src/index.css
git commit -m "feat(editor): add aiGrey mark + transcriptLink node with promote-on-edit plugin"
```

---

### Task 4.6 · `LlmClient` trait + `MockLLM` (deterministic enhanced markdown)

**Files:**
- Create: `crates/yogurt-server/src/llm.rs`
- Create: `crates/yogurt-server/src/llm_mock.rs`
- Modify: `crates/yogurt-server/src/lib.rs` (module declarations)
- Modify: `crates/yogurt-server/Cargo.toml` (add `async-trait`)

- [ ] **Step 1: Add `async-trait` to workspace deps.**

```toml
async-trait = "0.1"
```

(Add to workspace `[workspace.dependencies]` and to `crates/yogurt-server/Cargo.toml` `[dependencies]`.)

- [ ] **Step 2: Write `crates/yogurt-server/src/llm.rs`.**

This trait is what Phase 5 implements for real. Keeping it minimal — `complete(system_prompt, user_prompt) -> String` — means the swap is mechanical.

```rust
use anyhow::Result;
use async_trait::async_trait;

/// The narrow LLM surface that Phase 4 needs.
/// Phase 5 will add streaming + tool use; we don't need them yet.
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, system: &str, user: &str) -> Result<String>;
}
```

- [ ] **Step 3: Write `crates/yogurt-server/src/llm_mock.rs`.**

The mock builds a deterministic enriched markdown by:
1. Parsing the incoming user prompt to extract the `## USER NOTES` and `## TRANSCRIPT` sections (we know the format because we wrote `enhance.md`).
2. Echoing the user notes verbatim.
3. Appending one AI bullet per transcript segment, wrapped in `<span data-ai-grey data-ts="N">…</span>` and ending with `<span data-transcript-link data-ts="N">↳ HH:MM</span>`.

```rust
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;

use crate::llm::LlmClient;

pub struct MockLlm;

#[async_trait]
impl LlmClient for MockLlm {
    async fn complete(&self, _system: &str, user: &str) -> Result<String> {
        let (notes, transcript_json) = split_prompt(user);
        let transcript: Vec<Seg> = serde_json::from_str(transcript_json).unwrap_or_default();

        let mut out = String::new();
        out.push_str(notes.trim_start());
        if !out.ends_with('\n') { out.push('\n'); }

        if !notes.trim().is_empty() && !transcript.is_empty() {
            out.push('\n');
        }
        for seg in &transcript {
            let ts = seg.ts_ms / 1000;
            let stamp = format!("{:02}:{:02}", ts / 60, ts % 60);
            // Mock "summary" = first 8 words of the segment.
            let words: Vec<&str> = seg.text.split_whitespace().take(8).collect();
            let summary = words.join(" ");
            out.push_str(&format!(
                "- <span data-ai-grey data-ts=\"{ts}\">{summary} <span data-transcript-link data-ts=\"{ts}\">↳ {stamp}</span></span>\n"
            ));
        }
        Ok(out)
    }
}

fn split_prompt(user: &str) -> (&str, &str) {
    // Defensive — if the prompt format changes, fall back to empty/empty.
    let notes_marker = "## USER NOTES (preserve verbatim, do not wrap)";
    let trans_marker = "## TRANSCRIPT";
    let (Some(n_start), Some(t_start)) = (user.find(notes_marker), user.find(trans_marker)) else {
        return ("", "[]");
    };
    let notes = &user[n_start + notes_marker.len()..t_start];
    // Skip the heading line of TRANSCRIPT and any blank lines before the JSON.
    let after = &user[t_start..];
    let json_start = after.find('[').unwrap_or(after.len());
    (notes.trim_matches(|c: char| c == '-' || c.is_whitespace()),
     &after[json_start..].trim())
}

#[derive(Deserialize)]
struct Seg { ts_ms: u64, #[allow(dead_code)] channel: String, text: String }
```

- [ ] **Step 4: Register modules in `crates/yogurt-server/src/lib.rs`.**

Add module declarations near the top:

```rust
mod llm;
mod llm_mock;
pub use llm::LlmClient;
pub(crate) use llm_mock::MockLlm;
```

- [ ] **Step 5: Add a smoke test.**

`crates/yogurt-server/tests/llm_mock.rs`:

```rust
// Note: llm_mock is pub(crate), so we exercise it through the public re-export
// via the enhance handler in tests/enhance_endpoint.rs (Task 4.7).
// This file is intentionally minimal — the unit test inline in llm_mock.rs would
// be sufficient if MockLlm were pub. We keep the module private to discourage
// downstream use; the trait is the abstraction.
```

Actually, just add a small `#[cfg(test)] mod tests` inside `llm_mock.rs` to verify shape:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_echoes_notes_and_adds_one_bullet_per_segment() {
        let mock = MockLlm;
        let prompt = r#"
## USER NOTES (preserve verbatim, do not wrap)

- pricing
- timeline

## TRANSCRIPT

[{"ts_ms":120000,"channel":"mic","text":"We debated the pricing model in detail today"}]
"#;
        let out = mock.complete("", prompt).await.unwrap();
        assert!(out.contains("- pricing"), "user notes preserved");
        assert!(out.contains("data-ai-grey data-ts=\"120\""), "ai bullet tagged");
        assert!(out.contains("↳ 02:00"), "deep-link timestamp formatted");
    }
}
```

- [ ] **Step 6: Run.**

Run: `cargo test -p yogurt-server llm_mock`
Expected: 1 passed.

- [ ] **Step 7: Commit.**

```bash
git add crates/yogurt-server/
git commit -m "feat(server): add LlmClient trait + MockLlm (real client lands in phase 5)"
```

---

### Task 4.7 · `POST /api/meetings/:id/enhance` endpoint + WebSocket progress events

**Files:**
- Modify: `Cargo.toml` (workspace) — add `yogurt-prompts` + `yogurt-notes` to `yogurt-server` deps below
- Modify: `crates/yogurt-server/Cargo.toml`
- Create: `crates/yogurt-server/src/enhance.rs`
- Modify: `crates/yogurt-server/src/lib.rs` (register route, expose `AppState` if not already from Phase 3)
- Modify: `crates/yogurt-server/src/routes.rs` (mount enhance route)
- Create: `crates/yogurt-server/tests/enhance_endpoint.rs`

**Assumption from Phase 3:** there exists an `AppState { meetings: Arc<RwLock<HashMap<MeetingId, Meeting>>>, ws_broadcaster: Arc<Broadcaster> }` (or equivalent). Phase 4 wires into whatever shape Phase 3 produced. If Phase 3's shape differs, the only thing this task needs is: (a) a way to read `notes_md` + `transcript_json` for a meeting id, (b) a way to push a typed JSON event to the meeting's WebSocket subscribers.

- [ ] **Step 1: Add crate deps.**

`crates/yogurt-server/Cargo.toml`:

```toml
[dependencies]
# ... existing ...
yogurt-prompts = { path = "../yogurt-prompts" }
yogurt-notes = { path = "../yogurt-notes" }
async-trait = { workspace = true }
```

- [ ] **Step 2: Write `crates/yogurt-server/src/enhance.rs`.**

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use std::sync::Arc;

use crate::{llm_mock::MockLlm, AppState, LlmClient};
use yogurt_notes::merge_notes;
use yogurt_prompts::{EnhanceCtx, Mode as PromptMode, Prompts};

#[derive(Serialize)]
pub struct EnhanceResponse {
    /// The merged markdown — wire format with `<span data-ai-grey>` + `<span data-transcript-link>`.
    pub enriched_md: String,
}

/// `POST /api/meetings/:id/enhance` — re-runs the bundled `enhance.md` prompt
/// against the current notes + transcript and returns the merged document.
/// Also pushes `enhance_progress` events to the meeting's WS subscribers
/// at `sending`, `streaming` (with running char count from the mock), and `done`.
pub async fn enhance(
    State(state): State<Arc<AppState>>,
    Path(meeting_id): Path<String>,
) -> Result<Json<EnhanceResponse>, (StatusCode, String)> {
    // 1) Read the meeting.
    let (notes_md, transcript_json) = {
        let meetings = state.meetings.read().await;
        let meeting = meetings.get(&meeting_id)
            .ok_or((StatusCode::NOT_FOUND, format!("meeting {meeting_id} not found")))?;
        (meeting.notes_md.clone(), meeting.transcript_json.clone())
    };

    // 2) Load + render the prompt.
    let prompts = Prompts::load(prompt_mode(&state))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("prompt load: {e}")))?;
    let user_prompt = prompts.render_enhance(&EnhanceCtx {
        notes: &notes_md,
        transcript: &transcript_json,
    }).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("render: {e}")))?;

    // 3) WS: emit `sending`.
    state.ws_broadcaster.send(&meeting_id, serde_json::json!({
        "type": "enhance_progress", "phase": "sending"
    })).await;

    // 4) Call the LLM (mock for Phase 4).
    let llm: Arc<dyn LlmClient> = Arc::new(MockLlm);
    let llm_output = llm.complete("", &user_prompt).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("llm: {e}")))?;

    state.ws_broadcaster.send(&meeting_id, serde_json::json!({
        "type": "enhance_progress", "phase": "streaming", "chars": llm_output.len()
    })).await;

    // 5) Merge into the user's notes.
    let merged = merge_notes(&notes_md, &llm_output, &transcript_json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("merge: {e}")))?;
    let enriched_md = yogurt_notes::render::to_markdown(&merged);

    // 6) Store + emit `done`.
    {
        let mut meetings = state.meetings.write().await;
        if let Some(m) = meetings.get_mut(&meeting_id) {
            m.enriched_md = Some(enriched_md.clone());
        }
    }
    state.ws_broadcaster.send(&meeting_id, serde_json::json!({
        "type": "enhance_progress", "phase": "done"
    })).await;

    Ok(Json(EnhanceResponse { enriched_md }))
}

fn prompt_mode(state: &AppState) -> PromptMode {
    match state.mode {
        crate::Mode::Dev => PromptMode::Dev,
        crate::Mode::Release => PromptMode::Release,
    }
}
```

- [ ] **Step 3: Mount the route in `crates/yogurt-server/src/routes.rs`.**

Add to the router (with `State<Arc<AppState>>` extraction):

```rust
router = router.route(
    "/api/meetings/{id}/enhance",
    axum::routing::post(crate::enhance::enhance),
);
```

If Phase 3 didn't yet add `.with_state(state)` to the router, this task does so now. Pattern:

```rust
pub fn router(mode: Mode, state: Arc<AppState>) -> Router { /* ... */ .with_state(state) }
```

Update `lib.rs::run` to construct `AppState` and pass it in. (This may already be done by Phase 3 — adapt to whatever Phase 3 produced; do not refactor unrelated state.)

- [ ] **Step 4: Write the integration test.**

`crates/yogurt-server/tests/enhance_endpoint.rs`:

```rust
use std::time::Duration;
use serde_json::Value;

#[tokio::test]
async fn it_enhances_a_meeting_with_user_notes_and_transcript() {
    let addr = "127.0.0.1:17890".parse().unwrap();
    let handle = tokio::spawn(async move {
        yogurt_server::run(addr, yogurt_server::Mode::Release).await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Seed a meeting. (Assumes Phase 3 exposed POST /api/meetings; if it didn't,
    // this test uses a /api/meetings/_test/seed admin endpoint guarded behind
    // cfg(test) — add it in this task if necessary.)
    let client = reqwest::Client::new();
    let create: Value = client.post("http://127.0.0.1:17890/api/meetings")
        .json(&serde_json::json!({
            "title": "Test",
            "notes_md": "- pricing",
            "transcript_json": r#"[{"ts_ms":120000,"channel":"mic","text":"We debated the pricing model"}]"#,
        }))
        .send().await.unwrap()
        .json().await.unwrap();
    let id = create["id"].as_str().unwrap();

    let resp: Value = client.post(format!("http://127.0.0.1:17890/api/meetings/{id}/enhance"))
        .send().await.unwrap()
        .json().await.unwrap();
    let md = resp["enriched_md"].as_str().unwrap();

    assert!(md.contains("- pricing"), "user notes preserved");
    assert!(md.contains(r#"data-ai-grey data-ts="120""#), "AI bullet tagged with ts");
    assert!(md.contains("↳ 02:00"), "deep-link suffix");

    handle.abort();
}
```

If Phase 3 doesn't expose a create endpoint with the exact fields above, add a minimal test-only seeder. **Do not** invent fields the Phase 3 Meeting struct doesn't have.

- [ ] **Step 5: Run.**

Run: `cargo test -p yogurt-server`
Expected: all existing tests + the new enhance test pass.

- [ ] **Step 6: Commit.**

```bash
git add Cargo.toml crates/yogurt-server/
git commit -m "feat(server): POST /api/meetings/:id/enhance with WS enhance_progress events"
```

---

### Task 4.8 · `EnhancingBanner` + `ShimmerSkeleton` (staggered reveal at 140/340/560/760ms)

**Files:**
- Create: `web/src/components/EnhancingBanner.tsx`
- Create: `web/src/components/ShimmerSkeleton.tsx`
- Create: `web/src/components/ShimmerSkeleton.css` (or co-locate in `index.css`)

- [ ] **Step 1: Write `web/src/components/ShimmerSkeleton.tsx`.**

```tsx
import { useEffect, useState } from "react";

interface ShimmerSkeletonProps {
  /** Delay in ms before this skeleton begins fading in. PRD §16.5: 140 / 340 / 560 / 760. */
  staggerMs: number;
  /** Rendered width — uses Tailwind class names. */
  widthClass?: string;
}

export function ShimmerSkeleton({ staggerMs, widthClass = "w-3/4" }: ShimmerSkeletonProps) {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setVisible(true), staggerMs);
    return () => clearTimeout(t);
  }, [staggerMs]);

  if (!visible) return <div className={`h-5 ${widthClass}`} aria-hidden />;

  return (
    <div
      className={`h-5 ${widthClass} rounded shimmer`}
      role="status"
      aria-label="generating bullet"
    />
  );
}
```

Add to `web/src/index.css` (PRD §16.5 — `shimmer` 1.25s linear infinite):

```css
.shimmer {
  background: linear-gradient(90deg, #EFE6D6 0%, #F8F1E0 50%, #EFE6D6 100%);
  background-size: 200% 100%;
  animation: shimmer 1.25s linear infinite;
}
@keyframes shimmer {
  from { background-position: 200% 0; }
  to   { background-position: -200% 0; }
}
```

- [ ] **Step 2: Write `web/src/components/EnhancingBanner.tsx`.**

```tsx
interface EnhancingBannerProps {
  /** From the WS enhance_progress event. */
  chars?: number;
  /** Whether the banner should be visible. */
  visible: boolean;
}

export function EnhancingBanner({ chars, visible }: EnhancingBannerProps) {
  if (!visible) return null;
  return (
    <div
      role="status"
      aria-live="polite"
      className="enhancing-banner"
      style={{
        position: "sticky", top: 0, zIndex: 20,
        background: "var(--blsoft, #ECE9FB)",
        color: "var(--ink, #211D18)",
        padding: "10px 16px",
        display: "flex", alignItems: "center", gap: "10px",
      }}
    >
      <span className="enhancing-dot" />
      <span style={{ fontSize: 13, fontWeight: 600 }}>
        Weaving your notes into the transcript…
      </span>
      <div className="enhancing-bar" style={{ flex: 1, height: 3, background: "#D9D4F4", borderRadius: 999, overflow: "hidden" }}>
        <div className="enhancing-bar-fill" />
      </div>
      {chars !== undefined && (
        <span style={{ fontFamily: "JetBrains Mono, ui-monospace, monospace", fontSize: 11, color: "var(--mut, #8A8174)" }}>
          {chars.toLocaleString()} chars
        </span>
      )}
    </div>
  );
}
```

Add to `index.css` (PRD §16.5 `recpulse` 1.4s):

```css
.enhancing-dot {
  width: 8px; height: 8px; border-radius: 999px;
  background: var(--blue, #5B4FC7);
  animation: recpulse 1.4s ease-in-out infinite;
}
@keyframes recpulse {
  0%, 100% { opacity: 0.55; transform: scale(1); }
  50%      { opacity: 1;    transform: scale(1.25); }
}
.enhancing-bar-fill {
  width: 30%; height: 100%; background: var(--blue, #5B4FC7);
  border-radius: 999px;
  animation: enhancing-bar 1.8s ease-in-out infinite;
}
@keyframes enhancing-bar {
  0%   { transform: translateX(-100%); }
  100% { transform: translateX(400%); }
}
```

- [ ] **Step 3: Add a Vitest smoke for the banner.**

`web/src/components/EnhancingBanner.test.tsx`:

```tsx
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { EnhancingBanner } from "./EnhancingBanner";

describe("EnhancingBanner", () => {
  it("renders when visible with char count", () => {
    render(<EnhancingBanner visible chars={1234} />);
    expect(screen.getByText(/Weaving your notes/)).toBeInTheDocument();
    expect(screen.getByText(/1,234 chars/)).toBeInTheDocument();
  });

  it("renders nothing when not visible", () => {
    const { container } = render(<EnhancingBanner visible={false} />);
    expect(container.firstChild).toBeNull();
  });
});
```

- [ ] **Step 4: Run.**

Run: `pnpm --dir web test`
Expected: all tests pass.

- [ ] **Step 5: Commit.**

```bash
git add web/src/components/ web/src/index.css
git commit -m "feat(web): add EnhancingBanner (lilac pulse + bar) + ShimmerSkeleton (staggered)"
```

---

### Task 4.9 · `MeetingPost` route + Re-enhance button + wire into Meeting "End meeting"

**Files:**
- Create: `web/src/routes/MeetingPost.tsx`
- Create: `web/src/components/ReEnhanceButton.tsx`
- Modify: `web/src/routes/Meeting.tsx`
- Modify: `web/src/lib/api.ts`
- Modify: `web/src/lib/ws.ts`

- [ ] **Step 1: Add `postEnhance` to `web/src/lib/api.ts`.**

```ts
export interface EnhanceResponse { enriched_md: string }

export async function postEnhance(meetingId: string): Promise<EnhanceResponse> {
  const res = await fetch(`/api/meetings/${meetingId}/enhance`, { method: "POST" });
  if (!res.ok) throw new Error(`enhance failed: ${res.status}`);
  return res.json();
}
```

- [ ] **Step 2: Add the `enhance_progress` event type to `web/src/lib/ws.ts`.**

```ts
// (Inside the existing WS message-type union from Phase 3.)
export type WsMessage =
  | { type: "transcript"; ts_ms: number; channel: "mic" | "system"; text: string; is_final: boolean }
  | { type: "notes_synced"; rev: number; md: string }
  | { type: "enhance_progress"; phase: "sending" | "streaming" | "done"; chars?: number }
  | { type: "chat_chunk"; message_id: string; delta: string };
```

- [ ] **Step 3: Write `web/src/components/ReEnhanceButton.tsx`.**

```tsx
import { useState } from "react";
import { postEnhance } from "../lib/api";

interface Props {
  meetingId: string;
  onEnhanced: (md: string) => void;
  onEnhancing: (b: boolean) => void;
}

export function ReEnhanceButton({ meetingId, onEnhanced, onEnhancing }: Props) {
  const [busy, setBusy] = useState(false);
  const click = async () => {
    setBusy(true);
    onEnhancing(true);
    try {
      const r = await postEnhance(meetingId);
      onEnhanced(r.enriched_md);
    } finally {
      setBusy(false);
      onEnhancing(false);
    }
  };
  return (
    <button
      type="button"
      onClick={click}
      disabled={busy}
      className="re-enhance-btn"
      style={{
        background: "var(--blue, #5B4FC7)", color: "white",
        padding: "8px 14px", borderRadius: 9, border: 0,
        fontSize: 13.5, fontWeight: 600,
        boxShadow: "0 2px 8px rgba(91,79,199,.3)",
        cursor: busy ? "wait" : "pointer", opacity: busy ? 0.7 : 1,
      }}
    >
      {busy ? "Re-enhancing…" : "Re-enhance"}
    </button>
  );
}
```

(Per PRD §5.3 + §5.5: a single button, no dropdown caret, no template menu.)

- [ ] **Step 4: Write `web/src/routes/MeetingPost.tsx`.**

```tsx
import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { YogurtEditor } from "../editor";
import { EnhancingBanner } from "../components/EnhancingBanner";
import { ReEnhanceButton } from "../components/ReEnhanceButton";

interface Props {
  /** Optional: if Meeting.tsx already has the enriched md (just came from End-meeting),
   *  it passes it in to avoid a re-fetch. */
  preloadedEnrichedMd?: string;
}

export function MeetingPost({ preloadedEnrichedMd }: Props) {
  const { id } = useParams<{ id: string }>();
  const [enrichedMd, setEnrichedMd] = useState<string>(preloadedEnrichedMd ?? "");
  const [enhancing, setEnhancing] = useState(false);
  const [progressChars, setProgressChars] = useState<number | undefined>(undefined);
  const [transcriptOpen, setTranscriptOpen] = useState(false);

  // Wire WS enhance_progress (assumes Phase 3 exposes a useWs hook).
  // useWs(id, (msg) => { if (msg.type === "enhance_progress") {
  //   if (msg.phase !== "done") setEnhancing(true);
  //   if (msg.phase === "done") setEnhancing(false);
  //   if (typeof msg.chars === "number") setProgressChars(msg.chars);
  // }});

  // Fallback: if no preload, fetch the meeting once on mount.
  useEffect(() => {
    if (!preloadedEnrichedMd && id) {
      fetch(`/api/meetings/${id}`).then(r => r.json()).then(m => setEnrichedMd(m.enriched_md ?? m.notes_md));
    }
  }, [id, preloadedEnrichedMd]);

  if (!id) return null;

  return (
    <div className="meeting-post">
      <EnhancingBanner visible={enhancing} chars={progressChars} />

      <div style={{
        position: "sticky", top: enhancing ? 44 : 0, zIndex: 10,
        display: "flex", justifyContent: "flex-end", padding: "12px 24px",
      }}>
        <ReEnhanceButton
          meetingId={id}
          onEnhanced={setEnrichedMd}
          onEnhancing={setEnhancing}
        />
      </div>

      <main style={{ maxWidth: 660, margin: "0 auto", padding: "42px 24px 130px" }}>
        <Legend />
        <YogurtEditor
          initialMarkdown={enrichedMd}
          editable={true}
          enrichedMarkdown={enrichedMd}
          onTranscriptLinkClick={(ts) => {
            setTranscriptOpen(true);
            // Phase 3's TranscriptPanel should expose a scrollToTs(ts) imperative.
            // Hook it up here.
            window.dispatchEvent(new CustomEvent("yogurt:transcript:scrollTo", { detail: { ts } }));
          }}
        />
      </main>
    </div>
  );
}

function Legend() {
  // PRD §5.3 — "Live legend top-right: black square = your notes, grey square = AI."
  return (
    <div style={{
      position: "absolute", top: 18, right: 24, display: "flex", gap: 14,
      fontSize: 11, color: "var(--mut, #8A8174)",
    }}>
      <span><i style={{ display: "inline-block", width: 9, height: 9, background: "#211D18", marginRight: 6, verticalAlign: "middle" }} /> your notes</span>
      <span><i style={{ display: "inline-block", width: 9, height: 9, background: "#A89F90", marginRight: 6, verticalAlign: "middle" }} /> AI</span>
    </div>
  );
}
```

- [ ] **Step 5: Wire End-meeting in `web/src/routes/Meeting.tsx`.**

Locate the "End meeting" button handler (from Phase 3). Replace its body to:

```tsx
const endMeeting = async () => {
  setEnhancing(true);
  try {
    const { enriched_md } = await postEnhance(meetingId);
    // Navigate to post-meeting view with the enriched md preloaded.
    navigate(`/meeting/${meetingId}/post`, { state: { enrichedMd: enriched_md } });
  } finally {
    setEnhancing(false);
  }
};
```

And read it on the post route:

```tsx
const location = useLocation();
const preloaded = (location.state as any)?.enrichedMd as string | undefined;
return <MeetingPost preloadedEnrichedMd={preloaded} />;
```

Add the route to the router (likely in `App.tsx` or a `routes.tsx`):

```tsx
<Route path="/meeting/:id/post" element={<MeetingPostRoute />} />
```

- [ ] **Step 6: Manual smoke (will run end-to-end in Task 4.10).**

Skip — verified in 4.10.

- [ ] **Step 7: Commit.**

```bash
git add web/src/routes/ web/src/components/ReEnhanceButton.tsx web/src/lib/
git commit -m "feat(web): MeetingPost route + Re-enhance button wired to /api/meetings/:id/enhance"
```

---

### Task 4.10 · End-to-end smoke + acceptance verification

**Files:** none — verification + a tiny CSS polish file if needed.

- [ ] **Step 1: Boot the stack in dev.**

Terminal 1: `pnpm --dir web dev`
Terminal 2: `cargo run -p yogurt -- start --dev --no-open`

Open `http://localhost:7878`. Click "+ New meeting" (from Phase 7 — if not yet built, navigate directly to `/meeting/_test_local` after seeding via `curl`).

- [ ] **Step 2: Acceptance — happy path.**

1. Type 5 markdown bullets in the in-meeting editor (e.g. `- pricing`, `- timeline`, `- hiring`, `- design`, `- launch`).
2. (Simulate transcript by having Phase 3's mock STT or by seeding `transcript_json` via `curl PATCH /api/meetings/:id` with 4-5 segments at varied ts_ms — write these into the task notes if needed.)
3. Click "End meeting".
4. Verify within 30s: the page transitions to `/meeting/:id/post`. The 5 typed bullets remain **ink-black**. New AI-added bullets appear in **grey (`#A89F90`)** beneath them.
5. **Verify the stagger:** AI bullets fade in at 140ms, 340ms, 560ms, 760ms after the post-page mounts. Use the React DevTools Profiler or open DevTools → Performance and look at paint timing on the 4 skeleton spans. Each should resolve to text exactly one beat after the previous.
6. **Verify deep-links:** each grey bullet ends with `↳ HH:MM`. Click one — transcript panel opens (Phase 3) and scrolls to that timestamp.
7. **Verify promote-on-edit:** click into the middle of a grey bullet, type a character. The newly-typed character is ink-black; the surrounding grey is intact; the bullet's overall color now reads as "mostly grey with a black insertion".
8. **Verify Re-enhance preserves edits:** after editing a grey bullet to black, click "Re-enhance" top-right. The promoted-black text stays black. New AI bullets may appear (mock LLM generates one per transcript segment).

- [ ] **Step 3: Color audit (CSS variable trail).**

Open DevTools → Inspect a grey-AI element → confirm computed `color` is exactly `rgb(168, 159, 144)` (= `#A89F90`).
Inspect a user bullet → confirm `color` is `rgb(33, 29, 24)` (= `#211D18`).
If either is off, trace whether Phase 1 set `--ink` / `--grey` correctly, or whether a Tailwind preflight is overriding.

- [ ] **Step 4: Motion-token audit.**

Open DevTools → Animations panel. With a shimmer skeleton on screen, the shimmer animation should report **1.25s linear infinite**. The enhancing-dot pulse should report **1.4s ease-in-out infinite**. If either is off, fix the CSS keyframes — these are load-bearing per PRD §16.5.

Confirm stagger timing in code (Task 4.8 hardcodes `staggerMs` — verify the values 140 / 340 / 560 / 760 are used).

- [ ] **Step 5: Release-build smoke.**

```bash
pnpm --dir web build
cargo build --release
./target/release/yogurt start --no-open &
sleep 1
curl -s -X POST localhost:7878/api/meetings -d '{"title":"Smoke","notes_md":"- pricing","transcript_json":"[{\"ts_ms\":60000,\"channel\":\"mic\",\"text\":\"Pricing was discussed\"}]"}' -H 'content-type: application/json'
# capture the id, then:
curl -s -X POST localhost:7878/api/meetings/<id>/enhance | jq .enriched_md
kill %1
```

Expected: returned `enriched_md` contains the user bullet verbatim AND a grey-tagged AI bullet with a `data-ts` and `↳ 01:00` suffix.

- [ ] **Step 6: Workspace lint pass.**

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
pnpm --dir web build
```

Expected: clean.

- [ ] **Step 7: Commit + push.**

```bash
git add -A
git commit -m "chore(phase-4): final polish + acceptance verification pass"
git push origin main
```

- [ ] **Step 8: Tag the phase milestone — only with explicit user confirmation.**

```bash
git tag -a v0.0.5-phase-4 -m "Phase 4 complete: augmented notes hero — black/grey merge + Re-enhance"
git push origin v0.0.5-phase-4
```

---

## Phase 4 acceptance criteria

All five must be true:

1. `cargo test --workspace` passes — including the 5 `yogurt-notes` fixture tests and the `enhance_endpoint` integration test.
2. `pnpm --dir web test` passes — including the `aiGrey` + `transcriptLink` rendering tests and the `EnhancingBanner` test.
3. **Happy path smoke:** user types 5 markdown bullets, clicks "End meeting", and within 30s sees a coherent post-meeting document where their bullets are ink-black (`#211D18`), AI bullets are grey (`#A89F90`), and each grey bullet ends with a `↳ HH:MM` link that opens the transcript panel scrolled to that moment.
4. **Promote-on-edit holds:** typing inside any grey range turns the typed characters black without leaking the grey mark to fresh text. Re-enhance preserves promoted-black ranges.
5. **Motion contract honored:** shimmer skeletons animate at 1.25s linear infinite; the staggered reveal lands at exactly 140 / 340 / 560 / 760 ms after the post-page mounts; the enhancing-dot pulses at 1.4s ease-in-out.

## What this phase does NOT do

Explicitly out of scope (next plans cover these):
- Real OpenAI-compatible LLM client (Phase 5 — deletes `llm_mock.rs`, swaps `MockLlm` for `OpenAiLlm`).
- Settings UI for the LLM provider, API key entry, Keychain storage (Phase 5).
- In-meeting "Ask this meeting" chat pill + chat window (Phase 6 — though `chat-system.md` is already shipped here).
- Persisting `enriched_md` to SQLite (Phase 7 / persistence). Today it's only in the in-memory `AppState.meetings`.
- Markdown export to `~/.yogurt/notes/*.md` (Phase 9).
- Template picker / versions rail (explicitly cut from v1 per PRD §5.5 — re-enhance always re-runs the single bundled `enhance.md`).
- Streaming the LLM response token-by-token into the editor (Phase 5 — `MockLlm` returns the full string at once; the WS already emits the right event shape so the upgrade is additive).

## Next plan

After Phase 4 lands, write `docs/superpowers/plans/<date>-yogurt-phase-5-llm-client-and-settings.md` covering:
- `yogurt-llm` crate: `async-openai`-based `LlmClient` impl supporting any OpenAI-compatible base URL.
- `yogurt-server` swap: delete `llm_mock.rs`, plumb the real client through `AppState` with a config-loaded provider.
- Settings page at `/settings` per PRD §5.6 (Model / Transcription / Audio / General sidebar).
- `keyring`-crate Keychain storage for API keys (never in plaintext, never logged).
- Provider preset cards (Ollama, LM Studio, OpenRouter) with the "Set active" + "Edit" affordances from the design board.
- Streaming token-by-token into the WS `enhance_progress` event (upgrades the `chars` field to a running count).

Subsequent phase plans follow the PRD §12 roadmap.
