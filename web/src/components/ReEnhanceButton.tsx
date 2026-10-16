// No caret of its own: the note format lives in the sibling <TemplatePicker>, and this button just carries its value.

import { useState } from "react";
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
