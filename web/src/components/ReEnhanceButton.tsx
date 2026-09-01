// ReEnhanceButton — single-button affordance per PRD §5.5 / NOTES-12.
//
// Explicitly NO caret, NO template-picker dropdown — template picker is
// v2 (PRD §5.5: "Re-enhance always re-runs the single bundled enhance.md").
//
// On click:
//   1. setBusy(true) + onEnhancing(true).
//   2. POST /api/meetings/{id}/enhance with raw notes + transcript.
//   3. On success: onEnhanced(enriched_md, notes_file). The host swaps the
//      editor content via the YogurtEditor `enrichedMarkdown` prop.
//   4. setBusy(false) + onEnhancing(false), even on error.
//
// Styling: blueberry button matching the existing Meeting.tsx Create /
// Start recording buttons — same shadow, same radius, same hover.

import { useState } from "react";
import {
  postEnhance,
  type EnhanceRequestBody,
  type EnhanceResponse,
} from "../lib/api";

const BLUE = "var(--color-blue)";

export interface ReEnhanceButtonProps {
  meetingId: string;
  /** Raw user notes, kept separate from the generated summary. */
  notesMd: string;
  /** JSON-serialized transcript segments. Empty array if no transcript. */
  transcriptJson: string;
  /** Optional title for the on-disk markdown filename slug. */
  title?: string;
  /** Optional meeting start time (unix ms) — drives the filename date. */
  startedAtUnixMs?: number;
  /** Optional meeting end time (unix ms). */
  endedAtUnixMs?: number;
  /** Session token from ensureSessionToken(). */
  token: string;
  /** Called with the enriched markdown when the POST succeeds. */
  onEnhanced: (response: EnhanceResponse) => void;
  /** Called with `true` on click, `false` when the POST resolves/throws. */
  onEnhancing?: (busy: boolean) => void;
  /** Called with an Error message string when the POST fails. */
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
    token,
    onEnhanced,
    onEnhancing,
    onError,
  } = props;

  const [busy, setBusy] = useState(false);

  async function handleClick() {
    if (busy) return;
    setBusy(true);
    onEnhancing?.(true);
    try {
      const body: EnhanceRequestBody = {
        notes_md: notesMd,
        transcript_json: transcriptJson,
        title,
        started_at_unix_ms: startedAtUnixMs,
        ended_at_unix_ms: endedAtUnixMs,
      };
      const response = await postEnhance(meetingId, body, token);
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

  return (
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
}
