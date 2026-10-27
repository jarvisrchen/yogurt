// No caret of its own: the note format lives in the sibling <TemplatePicker>, and this button just carries its value.

import { useEffect, useState } from "react";
import {
  postEnhance,
  type EnhanceRequestBody,
  type EnhanceResponse,
} from "../lib/api";

const BLUE = "var(--color-blue)";

export interface ReEnhanceButtonProps {
  meetingId: string;
  notesMd: string;
  /** JSON-serialized transcript segments. Empty array if no transcript. */
  transcriptJson: string;
  title?: string;
  /** Optional meeting start time (unix ms) — drives the filename date. */
  startedAtUnixMs?: number;
  endedAtUnixMs?: number;
  /** Note format id to force; undefined lets the model auto-detect. */
  template?: string;
  /** When set, the first click asks this before enhancing. */
  confirmMessage?: string;
  onEnhanced: (response: EnhanceResponse) => void;
  /** Called with `true` on click, `false` when the POST resolves/throws. */
  onEnhancing?: (busy: boolean) => void;
  onError?: (message: string) => void;
}

export function ReEnhanceButton(props: ReEnhanceButtonProps) {
  const {
    meetingId,
    notesMd,
    transcriptJson,
    title,
    startedAtUnixMs,
    endedAtUnixMs,
    template,
    confirmMessage,
    onEnhanced,
    onEnhancing,
    onError,
  } = props;

  const [busy, setBusy] = useState(false);
  const [confirming, setConfirming] = useState(false);

  // Same auto-cancel as the delete confirm, a little longer to fit a sentence.
  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(false), 6000);
    return () => clearTimeout(t);
  }, [confirming]);

  function handleClick() {
    if (busy) return;
    if (confirmMessage) setConfirming(true);
    else void run();
  }

  async function run() {
    setConfirming(false);
    setBusy(true);
    onEnhancing?.(true);
    try {
      const body: EnhanceRequestBody = {
        notes_md: notesMd,
        transcript_json: transcriptJson,
        title,
        started_at_unix_ms: startedAtUnixMs,
        ended_at_unix_ms: endedAtUnixMs,
        template,
      };
      const response = await postEnhance(meetingId, body);
      onEnhanced(response);
    } catch (e) {
      const message =
        e instanceof Error ? e.message : "Re-enhance failed";
      onError?.(message);
    } finally {
      setBusy(false);
      onEnhancing?.(false);
    }
  }

  const button = (
    <button
      type="button"
      data-testid="re-enhance-button"
      onClick={handleClick}
      disabled={busy}
      aria-busy={busy}
      style={{
        padding: "8px 14px",
        borderRadius: 9,
        backgroundColor: BLUE,
        color: "#FFFFFF",
        fontSize: 13,
        fontWeight: 600,
        border: "none",
        boxShadow: "0 2px 8px rgba(91,79,199,0.3)",
        cursor: busy ? "wait" : "pointer",
        opacity: busy ? 0.7 : 1,
      }}
    >
      {busy ? "Re-enhancing…" : "Re-enhance"}
    </button>
  );

  if (!confirmMessage) return button;
  return (
    <div className="relative inline-block">
      {button}
      {confirming && (
        <div
          role="alertdialog"
          aria-label="Confirm re-enhance"
          data-testid="re-enhance-confirm"
          className="absolute top-full right-0 mt-1 z-20 w-64 px-3 py-2 flex flex-col gap-2 bg-card border border-line rounded-card shadow-pop text-[12px] text-ink"
        >
          <p>{confirmMessage}</p>
          <div className="flex items-center gap-2">
            <button
              type="button"
              autoFocus
              onClick={() => void run()}
              className="px-2 py-1 rounded-button bg-strsoft text-ink border border-straw/40 font-semibold hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-straw/50"
            >
              Replace
            </button>
            <button
              type="button"
              onClick={() => setConfirming(false)}
              className="px-2 py-1 rounded-button text-mut hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40"
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
