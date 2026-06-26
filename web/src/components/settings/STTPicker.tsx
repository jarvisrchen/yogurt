/**
 * STTPicker — Transcription section card pair (Phase 5 Plan 05-04, SET-07).
 *
 * Two cards side-by-side:
 *   - Cloud (selected, 1.5px blueberry border, "Selected" mono badge)
 *     - Sub-chips for Deepgram (active, blsoft pill) / AssemblyAI / Groq
 *   - Local (disabled, opacity-60, "Coming in v1" matcha badge)
 *     - Sub-chips for tiny.en / small.en / medium.en / large-v3 in mono neutral
 *
 * No interaction wiring yet — Phase 8 lands STT routes. This component is
 * a visual contract for the UI-SPEC §STT section.
 */
export function STTPicker() {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* ─── Cloud card (selected) ────────────────────────────────────── */}
      <article className="rounded-xl border-[1.5px] border-[var(--blue)] bg-white p-5 space-y-3">
        <header className="flex items-center justify-between">
          <h3 className="font-serif text-xl">Cloud</h3>
          <span className="text-[10px] font-mono uppercase tracking-wider text-[var(--blue)]">
            Selected
          </span>
        </header>
        <p className="text-[13px] text-mut">
          Real-time partials, ~2s end-to-end. Audio is sent to the
          provider.
        </p>
        <div className="flex flex-wrap gap-2 pt-1">
          <span className="text-[11px] font-mono uppercase tracking-wider px-2.5 py-1 rounded-full bg-[var(--blsoft)] text-[var(--blue)]">
            Deepgram
          </span>
          <span className="text-[11px] font-mono uppercase tracking-wider px-2.5 py-1 rounded-full border border-neutral-300 text-neutral-500">
            AssemblyAI
          </span>
          <span className="text-[11px] font-mono uppercase tracking-wider px-2.5 py-1 rounded-full border border-neutral-300 text-neutral-500">
            Groq
          </span>
        </div>
      </article>

      {/* ─── Local card (disabled, "Coming in v1") ────────────────────── */}
      <article className="rounded-xl border border-neutral-300 bg-neutral-50 p-5 opacity-60 space-y-3">
        <header className="flex items-center justify-between">
          <h3 className="font-serif text-xl">Local · whisper.cpp</h3>
          <span className="text-[10px] font-mono uppercase tracking-wider bg-[var(--matchasoft)] text-[var(--matcha)] px-2 py-0.5 rounded">
            Coming in v1
          </span>
        </header>
        <p className="text-[13px] text-mut">
          Fully on-device transcription via Metal-accelerated whisper.cpp.
        </p>
        <div className="flex flex-wrap gap-2 pt-1">
          {["tiny.en", "small.en", "medium.en", "large-v3"].map((m) => (
            <span
              key={m}
              className="text-[11px] font-mono uppercase tracking-wider px-2.5 py-1 rounded-full border border-neutral-300 text-neutral-400"
            >
              {m}
            </span>
          ))}
        </div>
      </article>
    </div>
  );
}
