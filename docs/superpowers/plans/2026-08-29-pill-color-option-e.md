# Pill Color Option E Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Repaint the meeting-row pills under Option E from the lavish mockup. STT cloud → outlined matcha (filled matcha stays for local STT). LLM pill → outlined blueberry (replaces the previous filled-blueberry style). STT family stays always matcha; LLM family stays always blueberry.

**Architecture:** Touch `EnginePill` and `LlmPill`. STT already has the `<provider> · <model>` stamp (`meetings.stt_engine`, set in `routes.rs:283-289` at recording start), so `parseSttEngine` can drive a two-class switch without backend changes. The LLM pill renders as outlined blueberry unconditionally because the user chose to drop the LLM locality distinction (the model text stays as a bare name, so the pill cannot know historical locality).

**Tech Stack:** React 19 + Tailwind 4 (existing `--color-blue` / `--color-matcha` tokens, hex border literals to mirror the lavish mockup), vitest, cargo test.

---

## File Structure

**Modify**
- `web/src/components/MeetingMetaPills.tsx` - restyle `EnginePill`'s cloud case AND restyle `LlmPill` to outlined blueberry unconditionally.
- `web/src/components/MeetingMetaPills.test.tsx` - extend tests for the cloud-outlined style.

**No backend changes.** `meetings.stt_engine` already carries the locality stamp from recording start; `meetings.llm_model` stays as the bare model name (no `local · ` / `cloud · ` prefix). No DB migration. No new helpers.

---

## Task 1: Restyle `EnginePill` (cloud STT → outlined matcha)

**Files:**
- Modify: `web/src/components/MeetingMetaPills.tsx:81-93`

- [ ] **Step 1: Extend the existing tests**

Add a new `it` to `describe("MeetingMetaPills")` in `MeetingMetaPills.test.tsx`:

```tsx
it("paints cloud STT as outlined matcha, not filled blueberry", () => {
  const start = Date.now();
  render(
    <MeetingMetaPills
      startedAt={start}
      endedAt={null}
      sttEngine="cloud · nova-3"
    />,
  );
  const pill = screen.getByTestId("engine-pill");
  expect(pill).toHaveClass("text-matcha");
  expect(pill).not.toHaveClass("bg-blsoft");
  expect(pill).not.toHaveClass("text-blue");
  expect(pill.className).toMatch(/border/);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && pnpm vitest run src/components/MeetingMetaPills.test.tsx`
Expected: FAIL - current `EnginePill` cloud branch sets `bg-blsoft text-blue`.

- [ ] **Step 3: Restyle `EnginePill`**

Replace `MeetingMetaPills.tsx:81-93` with:

```tsx
export function EnginePill({ sttEngine }: { sttEngine: string | null | undefined }) {
  const engine = parseSttEngine(sttEngine);
  if (!engine) return null;
  // Option E: STT family is always matcha. Filled = on this Mac,
  // outlined (#CBE0D2 border per the lavish mockup) = went to a provider.
  const tone = engine.cloud
    ? "border border-[#CBE0D2] bg-transparent text-matcha"
    : "bg-mtsoft text-matcha";
  return (
    <span
      className={`${PILL} ${tone}`}
      data-testid="engine-pill"
      title={engine.cloud ? "Transcribed by cloud STT" : "Transcribed on this Mac"}
    >
      {engine.cloud ? <Cloud size={11} aria-hidden /> : <HardDrive size={11} aria-hidden />}
      {engine.text}
    </span>
  );
}
```

Add `data-testid="engine-pill"` so tests can target the pill directly (the row's child position shifts based on whether `duration` is present).

- [ ] **Step 4: Re-run the test + full MeetingMetaPills suite**

Run: `cd web && pnpm vitest run src/components/MeetingMetaPills.test.tsx`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/MeetingMetaPills.tsx web/src/components/MeetingMetaPills.test.tsx
git commit -m "feat(web): paint cloud STT as outlined matcha (Option E)"
```

---

## Task 2: Restyle `LlmPill` to outlined blueberry

**Files:**
- Modify: `web/src/components/MeetingMetaPills.tsx:103-112` (`LlmPill` component)

- [ ] **Step 1: Add the failing test**

Add to `describe("MeetingMetaPills")` in `web/src/components/MeetingMetaPills.test.tsx`:

```tsx
it("paints LLM pill as outlined blueberry, not filled", () => {
  const start = Date.now();
  render(<MeetingMetaPills startedAt={start} llmModel="gpt-5-mini" />);
  const pill = screen.getByTestId("llm-pill");
  expect(pill).toHaveClass("text-blue");
  expect(pill.className).not.toMatch(/bg-blsoft/);
  expect(pill.className).toMatch(/border/);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && pnpm vitest run src/components/MeetingMetaPills.test.tsx`
Expected: FAIL - current `LlmPill` always uses `bg-blsoft text-blue`.

- [ ] **Step 3: Restyle `LlmPill`**

Replace `web/src/components/MeetingMetaPills.tsx:103-112` with:

```tsx
export function LlmPill({ llmModel }: { llmModel: string | null | undefined }) {
  const model = llmModel?.trim();
  if (!model) return null;
  // Option E: LLM family is always blueberry. Outlined (#C5BEEF border per
  // the lavish mockup) reads as "AI touched this" without competing with
  // the brand-blue button chrome.
  return (
    <span
      className={`${PILL} border border-[#C5BEEF] bg-transparent text-blue`}
      data-testid="llm-pill"
      title={`Enhanced by ${model}`}
    >
      <Sparkles size={11} aria-hidden />
      {model}
    </span>
  );
}
```

Drop the obsolete "blueberry tone to read as AI touched this" sentence from the file-level doc comment above `LlmPill` - the inline rationale now lives next to the return.

- [ ] **Step 4: Re-run the test + full MeetingMetaPills suite**

Run: `cd web && pnpm vitest run src/components/MeetingMetaPills.test.tsx`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/components/MeetingMetaPills.tsx web/src/components/MeetingMetaPills.test.tsx
git commit -m "feat(web): paint LLM pill as outlined blueberry (Option E)"
```

---

## Task 3: Verify E2E (lint + build + smoke)

**Files:**
- Read-only verification; no edits.

- [ ] **Step 1: Web lint + typecheck**

Run: `cd web && pnpm typecheck && pnpm lint`
Expected: PASS.

- [ ] **Step 2: Cargo workspace lint + tests**

Run: `cargo clippy -p yogurt-server --tests -- -D warnings && cargo test -p yogurt-server --lib --tests`
Expected: PASS.

- [ ] **Step 3: Build the full app**

Run: `just build`
Expected: PASS. The `yogurt-server` build embeds `web/dist` so this rebuilds the bundle too - verifies the new Tailwind class (`border-[#CBE0D2]`) survives the Tailwind 4 JIT pass and doesn't get tree-shaken as unused.

- [ ] **Step 4: Hand-smoke the new STT pill**

Run the binary against a real recording:
- Start with cloud STT → row should show `Cloud · <cloud-model>` as outlined matcha with a cloud glyph.
- Start with local STT → row should show `Local · small.en` as filled matcha with a HardDrive glyph (unchanged from today).
- Open a library card for a meeting recorded before `stt_engine` existed (`stt_engine: null`) → no STT pill at all (unchanged).

If any of those don't match the mockup, fix and re-run before committing.

- [ ] **Step 5: Final commit (if any smoke-fix tweaks landed)**

```bash
git add -p
git commit -m "fix(web): smoke-driven tweaks to Option E STT pill"
```

(No commit if no changes.)

---

## Self-Review

**Spec coverage:** the lavish mockup's Option E (rendered alongside A/B/C/D/F/G in `docs/.lavish/pill-color-system.html`) was the design. The original plan included four tasks (backend locality stamp, frontend parser, STT restyle, LLM restyle). The user revised scope mid-implementation across two iterations:

1. The LLM model text must stay as a bare name (no `local · ` / `cloud · ` prefix) - the backend locality stamp task was reverted in commit `4a32a12`.
2. The user initially chose to drop the LLM pill change entirely (keep current filled blueberry), then in a follow-up asked to repaint the LLM pill to outlined blueberry - the option E "cloud LLM" half of the mockup, applied unconditionally because `meetings.llm_model` carries no locality information.

Result: STT family split (matcha filled local, matcha outlined cloud) plus LLM pill repainted to outlined blueberry. Tasks 1 and 2 cover those two changes; Task 3 is the verification sweep.

**Placeholder scan:** no "TBD", "TODO", or "implement later" strings. Every code step shows the actual code.

**Type consistency:** no new types or helpers introduced. `parseSttEngine` shape preserved. The added `data-testid` is unique within the file and never collides with another component.

**Backwards compatibility:** zero impact on existing data. `stt_engine` was already carrying the locality stamp at recording start; the pill just paints differently now. Meetings recorded before that column existed (`stt_engine: null`) still render nothing.