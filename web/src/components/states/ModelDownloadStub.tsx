/**
 * Phase 7 (Plan 07-04) — ModelDownloadStub (STATE-04).
 *
 * UI stub per PRD §5.11. Phase 8 wires the real whisper.cpp model-download
 * pipeline; this Phase-7 file just ships the visual surface so the design
 * is locked in and any consuming surface can mount it.
 *
 * Not routed in Phase 7 — lives in the components tree and is referenced
 * by future plans (08-xx) when local STT goes live.
 */

export function ModelDownloadStub() {
  return (
    <div className="flex flex-col items-center text-center mt-20 px-6 max-w-xl mx-auto">
      <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-mtsoft text-matcha text-[22px] mb-6">
        <span aria-hidden>↓</span>
        <span className="sr-only">Download</span>
      </div>

      <h2 className="font-bold tracking-tight text-[26px] text-ink mb-2">
        Fetching the local model
      </h2>

      <p className="text-[14px] text-mut max-w-md mb-6">
        Yogurt is downloading <code className="font-mono">whisper.cpp</code>{" "}
        so transcription can run fully on this Mac. This happens once.
      </p>

      <div className="w-full bg-line/40 h-2 rounded-pill overflow-hidden mb-3">
        <div
          className="h-full bg-matcha"
          style={{ width: "42%" }}
          aria-hidden
        />
      </div>

      <p className="text-[11px] font-mono text-mut">
        ~150 MB · stays on this Mac
      </p>
    </div>
  );
}
