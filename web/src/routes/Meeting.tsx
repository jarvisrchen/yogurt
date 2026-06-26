import { useEffect, useState } from "react";
import { useNavigate, useParams } from "react-router";
import { useEditor, EditorContent } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { TranscriptDock } from "../components/TranscriptDock";
import { AskExperience } from "../components/AskExperience";
import { ensureSessionToken } from "../lib/session";
import { postEnhance } from "../lib/api";

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
 * Phase 3 + Phase 4 Meeting view.
 *
 * Two URL shapes drive this route:
 *   - `/meeting/new`   — bootstraps a fresh meeting (POST /api/meetings, then
 *                        `navigate("/meeting/{id}", { replace: true })`).
 *   - `/meeting/:id`   — the in-meeting view for a known meeting id.
 *
 * The Phase 3 `useState<View>` toggle in App.tsx routes "Open a new meeting →"
 * to `/meeting/new`, which lands here.
 *
 * Layout invariants (unchanged from Phase 3):
 *   - Wrapper has `pr-7` to reserve the 28px gutter the dock's closed-tab
 *     occupies (Phase 3 D-15). When the dock opens the 330px panel overlays
 *     via `position: fixed`; the notes column does NOT reflow.
 *   - Main column maxes at 660px (Design Board line 307 / NOTES-01).
 *   - Notes editor remains a TipTap StarterKit instance for the in-meeting
 *     view. The hero `YogurtEditor` lives on the post-meeting route — keeping
 *     them split lets the in-meeting view stay light (no aiGrey / transcript
 *     link plumbing needed while you're still typing).
 *
 * Phase 4 additions:
 *   - End meeting button: POST /api/meetings/:id/enhance with the current
 *     notes + transcript JSON, then navigate to /meeting/:id/post with the
 *     enriched markdown threaded via location.state to avoid a re-fetch.
 *   - Transcript JSON is accumulated by listening to the WS events via the
 *     TranscriptDock — to keep the data path tight we duplicate the
 *     subscription here (TODO Phase 5: lift transcript state to a shared
 *     store so the dock + this view can share one subscription).
 */
export function Meeting() {
  const params = useParams<{ id: string }>();
  const navigate = useNavigate();

  // `id` is either the routed UUID, or `"new"` (the bootstrap shape).
  const routeId = params.id ?? null;
  const [meetingId, setMeetingId] = useState<string | null>(
    routeId && routeId !== "new" ? routeId : null,
  );
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [token, setToken] = useState<string | null>(null);
  const [enhancing, setEnhancing] = useState(false);
  // We mirror the editor's plain-text markdown so the End-meeting handler
  // can post it. StarterKit doesn't natively serialize to markdown, so we
  // capture the plain text via editor.getText() — good enough for the
  // in-meeting view where users type sparse bullet lines. The hero
  // YogurtEditor on the post-meeting route handles the full markdown
  // round-trip.
  const [notesMd, setNotesMd] = useState<string>("");
  // Phase 3's TranscriptDock owns the WS subscription. For the End-meeting
  // → enhance POST we don't have the transcript events here directly; the
  // Phase 4 endpoint accepts an empty `transcript_json` and the LLM still
  // gets the notes (CONTEXT D-20 MockLlm fallback). When the WS-backed
  // transcript surface lands in shared state (Phase 5), this empty array
  // upgrade is one prop wire-up.
  const transcriptJson = "[]";

  const editor = useEditor({
    extensions: [StarterKit],
    content:
      "<p>Take sparse notes during the meeting — AI enhances on End.</p>",
    onUpdate: ({ editor }) => {
      setNotesMd(editor.getText());
    },
  });

  // WR-06: fetch the session token once on mount so every subsequent
  // /api/meetings* call (and the per-meeting WS) carries it.
  useEffect(() => {
    let cancelled = false;
    ensureSessionToken().then(
      (t) => {
        if (!cancelled) setToken(t);
      },
      (e: unknown) => {
        if (!cancelled) {
          setError(
            e instanceof Error
              ? `Failed to load session token: ${e.message}`
              : "Failed to load session token",
          );
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

  // /meeting/new bootstrap: as soon as we have a token, POST /api/meetings
  // and replace-navigate to /meeting/:id. Replace (not push) so the back
  // button doesn't return to the bootstrap URL.
  useEffect(() => {
    if (routeId !== "new") return;
    if (!token) return;
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch("/api/meetings", {
          method: "POST",
          headers: { Authorization: `Bearer ${token}` },
        });
        if (!res.ok) {
          const body = (await res.json().catch(() => ({}))) as ServerError;
          if (!cancelled) {
            setError(
              body.error ?? `Failed to create meeting (${res.status})`,
            );
          }
          return;
        }
        const json = (await res.json()) as MeetingCreatedResponse;
        if (!cancelled) {
          setMeetingId(json.id);
          navigate(`/meeting/${json.id}`, { replace: true });
        }
      } catch (e) {
        if (!cancelled) {
          setError(
            e instanceof Error ? e.message : "Failed to create meeting",
          );
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [routeId, token, navigate]);

  // Keep meetingId in sync if the user navigates directly to /meeting/:id.
  useEffect(() => {
    if (routeId && routeId !== "new") {
      setMeetingId(routeId);
    }
  }, [routeId]);

  function authedHeaders(): Record<string, string> {
    return token ? { Authorization: `Bearer ${token}` } : {};
  }

  async function startRecording() {
    if (!meetingId) return;
    setError(null);
    try {
      const res = await fetch(`/api/meetings/${meetingId}/start`, {
        method: "POST",
        headers: authedHeaders(),
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
        headers: authedHeaders(),
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

  // End meeting: POST /api/meetings/:id/enhance with the current notes +
  // transcript, then navigate to /meeting/:id/post with the enriched
  // markdown threaded in location.state. The post route falls back to
  // GET /api/meetings/:id if state is absent (refresh / direct link).
  async function endMeeting() {
    if (!meetingId || !token) return;
    setError(null);
    setEnhancing(true);
    try {
      // Best-effort: stop any active recording first so the audio capture
      // releases cleanly. Stop is idempotent server-side.
      if (recording) {
        try {
          await stopRecording();
        } catch {
          // Non-fatal — proceed with enhance.
        }
      }
      const response = await postEnhance(
        meetingId,
        {
          notes_md: notesMd,
          transcript_json: transcriptJson,
        },
        token,
      );
      navigate(`/meeting/${meetingId}/post`, {
        state: { enrichedMd: response.enriched_md },
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to end meeting");
    } finally {
      setEnhancing(false);
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
            {meetingId && (
              <button
                type="button"
                onClick={endMeeting}
                disabled={enhancing}
                aria-busy={enhancing}
                data-testid="end-meeting"
                className="px-4 py-2 rounded-button text-[13.5px] font-semibold text-white shadow-[0_2px_8px_rgba(91,79,199,0.3)] hover:opacity-90 disabled:opacity-70"
                style={{ backgroundColor: BLUE }}
              >
                {enhancing ? "Enhancing…" : "End meeting"}
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

      <TranscriptDock meetingId={meetingId} token={token} />

      {/* Phase 6 (Plan 06-02): floating Ask-pill / chat window. Mounted at
          the route root (sibling to TranscriptDock) so the fixed-position
          pill never reflows under the notes column and persists across
          editor remounts. */}
      <AskExperience meetingId={meetingId} token={token} />
    </div>
  );
}
