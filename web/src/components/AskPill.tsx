import { useKeyboardShortcut } from "../hooks/useKeyboardShortcut";

interface AskPillProps {
  /** Invoked on click OR when ⌘K / Ctrl+K is pressed. */
  onExpand: () => void;
}

/**
 * Phase 6 (Plan 06-02) — collapsed "Ask this meeting…" floating pill.
 *
 * Geometry per PRD §16.8 + UI spec: 480px wide, fixed bottom-center,
 * 24px above the viewport bottom edge, on top of the meeting view with
 * `z-30`. Tap or ⌘K expands into the full `<ChatWindow />`.
 *
 * The `translate(-50%)` plays nicely with the popUp keyframe used by the
 * expanded window — both surfaces share the same horizontal anchor so the
 * 260ms transition reads as a single morph rather than a re-center.
 */
export function AskPill({ onExpand }: AskPillProps) {
  useKeyboardShortcut({ key: "k", metaOrCtrl: true }, onExpand);
  return (
    <button
      type="button"
      onClick={onExpand}
      aria-label="Ask this meeting"
      className="fixed bottom-6 left-1/2 -translate-x-1/2 w-[480px] h-12 flex items-center justify-between gap-3 px-4 py-2 bg-[var(--color-card)] border border-[var(--color-line)] rounded-full shadow-pop text-left text-[14px] text-[var(--color-mut)] hover:border-[var(--color-blue)] hover:text-[var(--color-ink)] transition-colors z-30"
    >
      <span className="flex-1 truncate">Ask this meeting…</span>
      <span
        aria-hidden="true"
        className="inline-flex items-center justify-center h-6 px-2 rounded-md text-[11px] font-mono text-[var(--color-mut)] bg-[var(--color-blsoft)]"
      >
        ⌘K
      </span>
      <span
        aria-hidden="true"
        className="inline-flex items-center justify-center h-7 w-7 rounded-full bg-[var(--color-blue)] text-white"
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="none">
          <path
            d="M6 9.5V2.5M6 2.5L2.5 6M6 2.5L9.5 6"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>
    </button>
  );
}
