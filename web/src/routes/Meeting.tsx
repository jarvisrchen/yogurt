import { useState } from "react";
import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { TranscriptDock } from "../components/TranscriptDock";

const INK = "#211D18";
const LINE = "#EBE3D5";
const BLUE = "#5B4FC7";
const STRAW_SOFT = "#FBE6E0";
const STRAW = "#E07A66";

interface MeetingCreatedResponse {
  id: string;
  created_at_ms?: number;
}

interface ServerError {
  error?: string;
}

/**
 * Phase 3 Meeting view.
 *
 * Flow:
 *   1. Page mounts with no meetingId — only the Create button is enabled.
 *   2. Create → POST /api/meetings → server returns {id, created_at_ms}.
 *   3. Start recording → POST /api/meetings/{id}/start. Server reads
 *      YOGURT_DEEPGRAM_API_KEY; if absent surfaces a clean 400 with the
 *      env var name in the body — rendered here as an inline error banner
 *      (TRANS-04 / Phase 3 D-07).
 *   4. Stop recording → POST /api/meetings/{id}/stop. Idempotent.
 *
 * Layout invariants:
 *   - Wrapper has `pr-7` to reserve the 28px gutter the dock's closed-tab
 *     occupies (Phase 3 D-15). When the dock opens the 330px panel overlays
 *     via `position: fixed`; the notes column does NOT reflow.
 *   - Main column maxes at 660px (Design Board line 307).
 *   - Notes editor is a TipTap StarterKit instance; Phase 4 wires the
 *     aiGrey mark + transcript deep-links on top of this same editor.
 */
export function Meeting() {
  const [meetingId, setMeetingId] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const editor = useEditor({
    extensions: [StarterKit],
    content:
      "<p>Take sparse notes during the meeting — AI enhances on End (Phase 4).</p>",
  });

  async function createMeeting() {
    setError(null);
    try {
      const res = await fetch("/api/meetings", { method: "POST" });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as ServerError;
        setError(body.error ?? `Failed to create meeting (${res.status})`);
        return;
      }
      const json = (await res.json()) as MeetingCreatedResponse;
      setMeetingId(json.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create meeting");
    }
  }

  async function startRecording() {
    if (!meetingId) return;
    setError(null);
    try {
      const res = await fetch(`/api/meetings/${meetingId}/start`, {
        method: "POST",
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as ServerError;
        setError(body.error ?? `Failed to start recording (${res.status})`);
        return;
      }
      setRecording(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start recording");
    }
  }

  async function stopRecording() {
    if (!meetingId) return;
    setError(null);
    try {
      const res = await fetch(`/api/meetings/${meetingId}/stop`, {
        method: "POST",
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as ServerError;
        setError(body.error ?? `Failed to stop recording (${res.status})`);
        return;
      }
      setRecording(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to stop recording");
    }
  }

  const title = meetingId ? `Meeting · ${meetingId.slice(0, 8)}` : "New meeting";

  return (
    <div
      className="min-h-screen pr-7"
      style={{ backgroundColor: "#FBF7EF" }}
    >
      <main className="max-w-[660px] mx-auto px-10 py-12 space-y-6">
        <header className="flex items-center justify-between">
          <h1
            className="font-serif text-[32px] leading-none"
            style={{ color: INK }}
          >
            {title}
          </h1>
          <div className="flex items-center gap-2">
            {!meetingId && (
              <button
                type="button"
                onClick={createMeeting}
                className="px-4 py-2 rounded-button text-[13.5px] font-semibold text-white shadow-[0_2px_8px_rgba(91,79,199,0.3)] hover:opacity-90"
                style={{ backgroundColor: BLUE }}
              >
                Create
              </button>
            )}
            {meetingId && !recording && (
              <button
                type="button"
                onClick={startRecording}
                className="px-4 py-2 rounded-button text-[13.5px] font-semibold text-white shadow-[0_2px_8px_rgba(91,79,199,0.3)] hover:opacity-90"
                style={{ backgroundColor: BLUE }}
              >
                Start recording
              </button>
            )}
            {meetingId && recording && (
              <button
                type="button"
                onClick={stopRecording}
                className="px-4 py-2 rounded-button text-[13.5px] font-semibold text-white shadow-[0_2px_8px_rgba(91,79,199,0.3)] hover:opacity-90"
                style={{ backgroundColor: BLUE }}
              >
                Stop recording
              </button>
            )}
          </div>
        </header>

        {error && (
          <div
            role="alert"
            className="rounded-card px-4 py-3 text-[13px]"
            style={{
              backgroundColor: STRAW_SOFT,
              color: STRAW,
              border: `1px solid ${STRAW}`,
            }}
          >
            {error}
          </div>
        )}

        <section
          className="rounded-card bg-white p-6"
          style={{ border: `1px solid ${LINE}` }}
        >
          <EditorContent editor={editor} />
        </section>
      </main>

      <TranscriptDock meetingId={meetingId} />
    </div>
  );
}
