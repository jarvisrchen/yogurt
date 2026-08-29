import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams } from "react-router";
import { YogurtEditor } from "../editor";
import { TranscriptDock } from "../components/TranscriptDock";
import { AskExperience } from "../components/AskExperience";
import { MicDevicePicker } from "../components/MicDevicePicker";
import { MeetingLabels } from "../components/labels/MeetingLabels";
import { InlineTitle } from "../components/library/InlineTitle";
import { ensureSessionToken } from "../lib/session";
import { postEnhance } from "../lib/api";
import { meetingsApi, useActiveRecording, useMeeting } from "../lib/api/meetings";
import {
  storedSegmentToEvent,
  useSttError,
  type StoredTranscriptSegment,
  type TranscriptEvent,
} from "../lib/ws";

const INK = "#211D18";
const LINE = "#EBE3D5";
const BLUE = "#5B4FC7";
const STRAW_SOFT = "#FBE6E0";
const STRAW = "#E07A66";

const NOTES_PLACEHOLDER =
  "Take sparse notes during the meeting — AI enhances on End.";

interface MeetingCreatedResponse {
  id: string;
  title?: string;
}

interface ServerError {
  error?: string;
}

/**
 * Meeting view — the in-progress recording surface.
 *
 * Two URL shapes drive this route:
 *   - `/meeting/new`   — bootstraps a fresh meeting (POST /api/meetings, then
 *                        `navigate("/meeting/{id}", { replace: true })`), and
 *                        auto-starts recording (task NOTES-01 — Granola is
 *                        one-click-and-recording, not "create then hunt for
 *                        Start").
 *   - `/meeting/:id`   — the in-meeting view for a known meeting id. Also
 *                        hydrates title + any already-saved notes_md via
 *                        `useMeeting` (covers refresh / resuming a meeting
 *                        that was created but never finished).
 *
 * Layout invariants (unchanged from earlier phases):
 *   - Wrapper has `pr-7` to reserve the 28px gutter the dock's closed-tab
 *     occupies. When the dock opens the 330px panel overlays via
 *     `position: fixed`; the notes column does NOT reflow.
 *   - Main column maxes at 660px (Design Board line 307 / NOTES-01).
 *   - Notes editor is the shared `<YogurtEditor>` (same markdown
 *     serializer + placeholder support as the post-meeting hero editor) —
 *     the aiGrey/transcriptLink marks it also registers are simply unused
 *     here (nothing grey exists pre-enhance).
 *
 * Persistence:
 *   - Notes autosave debounced 800ms via `PATCH {notes_md}`, flushed on
 *     unmount and before End meeting.
 *   - End meeting: stop recording (so the server flushes the transcript)
 *     → enhance (current notes_md + `transcript_json: "[]"` — the server
 *     prefers its own stored transcript when the body's is empty — + the
 *     current title) → navigate to `/meeting/:id/post`.
 */
export function Meeting() {
  const params = useParams<{ id: string }>();
  const location = useLocation();
  const navigate = useNavigate();

  // `id` is either the routed UUID, or `"new"` (the bootstrap shape).
  const routeId = params.id ?? null;
  const [meetingId, setMeetingId] = useState<string | null>(
    routeId && routeId !== "new" ? routeId : null,
  );
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // True when `error` stems from a failed recording start — surfaces an
  // "Open Settings" link since that's almost always a missing/bad STT
  // provider key (task NOTES-01).
  const [errorIsStartFailure, setErrorIsStartFailure] = useState(false);
  const [token, setToken] = useState<string | null>(null);
  const [enhancing, setEnhancing] = useState(false);
  const [notesMd, setNotesMd] = useState<string>("");
  // Populated exactly once (task NOTES-02 hydration) when `useMeeting`
  // resolves a non-empty `notes_md` — fed into YogurtEditor's
  // `enrichedMarkdown` prop, which swaps editor content WITHOUT firing
  // onChange (so it never re-triggers autosave with its own value).
  const [hydratedNotesMd, setHydratedNotesMd] = useState<string | undefined>(
    undefined,
  );

  const sttError = useSttError(meetingId, token);

  function setErrorMessage(message: string | null, isStartFailure = false) {
    setError(message);
    setErrorIsStartFailure(isStartFailure);
  }

  // Hydration source for title + previously-saved notes. Also doubles as
  // the "has the server round-trip for this meeting settled at least
  // once" gate for autosave below — without it, autosave could fire
  // before hydration and PATCH an empty notes_md over real saved content
  // (see the `hydrationSettled` effect further down).
  const meetingQuery = useMeeting(meetingId ?? undefined);
  const meetingRow = meetingQuery.data;
  const hydrationSettled = meetingRow !== undefined || meetingQuery.isError;

  // Live-dock-loses-history-on-remount fix: parse the meeting row's
  // persisted `transcript_json` the same way MeetingPost's static dock
  // does (NOTES-09), map it onto live `TranscriptEvent`s via
  // `storedSegmentToEvent`, and hand it to TranscriptDock as a live-mode
  // seed — see that component's `history` prop doc. Navigating away from
  // a live meeting and back otherwise showed an empty dock that only
  // filled with new lines, even though the full transcript was already
  // persisted server-side.
  const transcriptHistory = useMemo<TranscriptEvent[]>(() => {
    const raw = meetingRow?.transcript_json;
    if (!raw) return [];
    try {
      const parsed: unknown = JSON.parse(raw);
      if (!Array.isArray(parsed)) return [];
      return parsed
        .filter(
          (s): s is StoredTranscriptSegment =>
            s !== null &&
            typeof s === "object" &&
            typeof s.ts_ms === "number" &&
            (s.channel === "me" || s.channel === "them") &&
            typeof s.text === "string",
        )
        .map(storedSegmentToEvent);
    } catch {
      // Malformed transcript JSON — seed nothing rather than throw.
      return [];
    }
  }, [meetingRow?.transcript_json]);

  // Resync `recording` when landing/returning on a meeting that's already
  // live server-side (recording continues across navigation/reload/back —
  // the server `Registry` owns it, not this component). This only flips
  // local UI state to match reality; it must NEVER call startRecording()
  // (no POST /start) — that would 400 "already started" and paint a
  // spurious error banner for a meeting that's already recording fine.
  const activeRecording = useActiveRecording();
  useEffect(() => {
    if (meetingId && activeRecording.data?.id === meetingId) {
      setRecording(true);
    }
  }, [meetingId, activeRecording.data]);

  const title = meetingId
    ? (meetingRow?.title ?? "Untitled meeting")
    : "New meeting";

  // Always-latest refs so the unmount-flush effect (which only runs its
  // cleanup once, at true unmount) and `flushNotes` never read a stale
  // closure. Plain assignment during render is the standard React pattern
  // for this — no separate effect needed.
  const meetingIdRef = useRef(meetingId);
  meetingIdRef.current = meetingId;
  const notesMdRef = useRef(notesMd);
  notesMdRef.current = notesMd;
  const lastSavedNotesRef = useRef<string>("");

  const hydratedNotesRef = useRef(false);
  useEffect(() => {
    if (hydratedNotesRef.current) return;
    if (meetingRow?.notes_md) {
      hydratedNotesRef.current = true;
      setHydratedNotesMd(meetingRow.notes_md);
      // Sync the tracked-markdown state too — YogurtEditor's
      // `enrichedMarkdown` swap intentionally does NOT fire onChange, so
      // without this a user who hits End meeting before typing anything
      // would send an empty notes_md, clobbering their hydrated notes.
      setNotesMd(meetingRow.notes_md);
      lastSavedNotesRef.current = meetingRow.notes_md;
    }
  }, [meetingRow]);

  const flushNotes = useCallback(async () => {
    const id = meetingIdRef.current;
    if (!id) return;
    const md = notesMdRef.current;
    if (md === lastSavedNotesRef.current) return;
    lastSavedNotesRef.current = md;
    try {
      await meetingsApi.patch(id, { notes_md: md });
    } catch {
      // Best-effort — a later autosave tick or the End-meeting flush
      // retries. Notes still live in the editor either way; nothing is
      // silently lost, just not yet durable server-side.
    }
  }, []);

  // Debounced autosave (task NOTES-02, 800ms).
  useEffect(() => {
    if (!meetingId || !hydrationSettled) return;
    const t = setTimeout(() => {
      void flushNotes();
    }, 800);
    return () => clearTimeout(t);
  }, [notesMd, meetingId, hydrationSettled, flushNotes]);

  // Flush on unmount (task NOTES-02).
  useEffect(() => {
    return () => {
      void flushNotes();
    };
  }, [flushNotes]);

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
          setErrorMessage(
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

  function authedHeaders(): Record<string, string> {
    return token ? { Authorization: `Bearer ${token}` } : {};
  }

  // Guards the auto-start-on-mount call (below) against StrictMode's
  // dev-only double-invoke of effects — a second POST /start on an
  // already-started meeting would 400 ("meeting already started") and
  // paint a spurious error the user never caused.
  const autoStartedRef = useRef(false);

  // Lets `endMeeting` skip its post-await state updates once the
  // successful-navigate branch has already unmounted this component.
  const isMountedRef = useRef(true);
  useEffect(() => {
    return () => {
      isMountedRef.current = false;
    };
  }, []);

  async function startRecording(idOverride?: string) {
    const id = idOverride ?? meetingId;
    if (!id) return;
    setErrorMessage(null);
    try {
      const res = await fetch(`/api/meetings/${id}/start`, {
        method: "POST",
        headers: authedHeaders(),
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as ServerError;
        setErrorMessage(
          body.error ?? `Failed to start recording (${res.status})`,
          true,
        );
        return;
      }
      setRecording(true);
    } catch (e) {
      setErrorMessage(
        e instanceof Error ? e.message : "Failed to start recording",
        true,
      );
    }
  }

  async function stopRecording() {
    if (!meetingId) return;
    setErrorMessage(null);
    try {
      const res = await fetch(`/api/meetings/${meetingId}/stop`, {
        method: "POST",
        headers: authedHeaders(),
      });
      if (!res.ok) {
        const body = (await res.json().catch(() => ({}))) as ServerError;
        setErrorMessage(body.error ?? `Failed to stop recording (${res.status})`);
        return;
      }
      setRecording(false);
    } catch (e) {
      setErrorMessage(e instanceof Error ? e.message : "Failed to stop recording");
    }
  }

  // /meeting/new bootstrap: as soon as we have a token, POST /api/meetings,
  // replace-navigate to /meeting/:id, and auto-start recording (task
  // NOTES-01 — "+ New meeting" means one click, not create-then-hunt-for-
  // Start). Replace (not push) so the back button doesn't return to the
  // bootstrap URL.
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
            setErrorMessage(
              body.error ?? `Failed to create meeting (${res.status})`,
            );
          }
          return;
        }
        const json = (await res.json()) as MeetingCreatedResponse;
        if (!cancelled) {
          setMeetingId(json.id);
          navigate(`/meeting/${json.id}`, { replace: true });
          if (!autoStartedRef.current) {
            autoStartedRef.current = true;
            void startRecording(json.id);
          }
        }
      } catch (e) {
        if (!cancelled) {
          setErrorMessage(
            e instanceof Error ? e.message : "Failed to create meeting",
          );
        }
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [routeId, token, navigate]);

  // Keep meetingId in sync if the user navigates directly to /meeting/:id.
  useEffect(() => {
    if (routeId && routeId !== "new") {
      setMeetingId(routeId);
    }
  }, [routeId]);

  // Task NOTES-01, the actual production path: the Library sidebar's
  // "+ New meeting" button (and its ⌘N twin) already POST /api/meetings
  // themselves via `useCreateMeeting`, then `navigate(/meeting/:id)`
  // directly with a REAL id — they never hit the `/meeting/new` bootstrap
  // route above (that route has no `:id` segment, so `routeId` is never
  // literally `"new"` when reached that way; it's kept only for direct
  // URL bootstrapping). Sidebar.tsx / Library.tsx thread
  // `{ state: { autoStart: true } }` through that navigation so this
  // effect knows to fire /start immediately, same one-click-and-
  // recording contract, same `autoStartedRef` StrictMode guard.
  const autoStartRequested =
    (location.state as { autoStart?: boolean } | null)?.autoStart === true;
  useEffect(() => {
    if (!autoStartRequested) return;
    if (!meetingId || !token) return;
    if (autoStartedRef.current) return;
    autoStartedRef.current = true;
    void startRecording(meetingId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoStartRequested, meetingId, token]);

  // End meeting: flush notes, stop any active recording (so the server's
  // transcript accumulator has flushed before enhance reads it), then
  // POST /enhance with the CURRENT notes + the always-empty transcript
  // body (the server prefers its own stored transcript), then navigate to
  // the post-meeting view.
  async function endMeeting() {
    if (!meetingId || !token) return;
    setErrorMessage(null);
    setEnhancing(true);
    try {
      await flushNotes();
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
          transcript_json: "[]",
          title: meetingRow?.title,
        },
        token,
      );
      navigate(`/meeting/${meetingId}/post`, {
        state: { enrichedMd: response.enriched_md },
      });
      // Successful navigation unmounts this component — don't touch state
      // afterward (React warns on unmounted-component updates, and there's
      // no UI left to reflect it anyway).
      return;
    } catch (e) {
      if (isMountedRef.current) {
        setErrorMessage(e instanceof Error ? e.message : "Failed to end meeting");
      }
    }
    if (isMountedRef.current) setEnhancing(false);
  }

  return (
    <div
      className="min-h-screen pr-7"
      style={{ backgroundColor: "#FBF7EF" }}
    >
      <main className="max-w-[660px] mx-auto px-10 py-12 space-y-6">
        <Link
          to="/"
          className="inline-flex items-center gap-1.5 text-[12px] font-mono uppercase tracking-wider text-mut hover:text-ink transition-colors"
          aria-label="Back to library — recording continues in the background"
          title={
            recording
              ? "Back to library — recording continues in the background"
              : "Back to library"
          }
        >
          <span aria-hidden>←</span>
          <span>Library</span>
          {recording && (
            <span
              aria-hidden
              className="inline-block w-1.5 h-1.5 rounded-full bg-[var(--color-straw)] ml-1 animate-recpulse"
              title="recording"
            />
          )}
        </Link>
        <header className="space-y-2">
          {/* Row 1: title + STT chip (left, gap-3 so the InlineTitle edit
              border never touches the chip) and the recording controls
              (right). flex-wrap lets the button group drop below the
              title group at narrow widths instead of clipping. */}
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-3 min-w-0">
              {meetingId ? (
                <InlineTitle
                  id={meetingId}
                  title={title}
                  className="font-serif text-[32px] leading-none text-ink"
                />
              ) : (
                <h1 className="font-serif text-[32px] leading-none" style={{ color: INK }}>
                  {title}
                </h1>
              )}
              {meetingId && recording && activeRecording.data?.stt && (
                <span className="shrink-0 px-2 py-1 rounded-button border border-line bg-paper text-[11px] font-mono uppercase text-mut">
                  {activeRecording.data.stt === "cloud" ? "Cloud STT" : "Local STT"}
                </span>
              )}
            </div>
            <div className="flex items-center gap-2 shrink-0">
              {meetingId && !recording && (
                <button
                  type="button"
                  onClick={() => startRecording()}
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
          </div>
          {/* Row 2: label chips + picker, left-aligned under the title. */}
          {meetingId && <MeetingLabels meetingId={meetingId} />}
          {/* Row 3: mic picker, left-aligned under the title, on its own
              (smaller) line instead of crowding row 1. */}
          {meetingId && recording && (
            <div className="flex items-center">
              <MicDevicePicker meetingId={meetingId} />
            </div>
          )}
        </header>

        {error && (
          <div
            role="alert"
            className="rounded-card px-4 py-3 text-[13px] space-y-1.5"
            style={{
              backgroundColor: STRAW_SOFT,
              color: STRAW,
              border: `1px solid ${STRAW}`,
            }}
          >
            <p>{error}</p>
            {errorIsStartFailure && (
              <Link
                to="/settings"
                className="inline-block underline font-semibold hover:opacity-80"
              >
                Open Settings
              </Link>
            )}
          </div>
        )}

        {sttError.message && (
          <div
            role="alert"
            data-testid="stt-error-banner"
            className="rounded-button px-4 py-3 text-[13px] font-semibold bg-strsoft text-ink border border-straw/40 flex items-center justify-between gap-3"
          >
            <span>{sttError.message}</span>
            <button
              type="button"
              onClick={sttError.dismiss}
              aria-label="Dismiss transcription error"
              className="shrink-0 text-mut hover:text-ink"
            >
              ✕
            </button>
          </div>
        )}

        <section
          className="rounded-card bg-white p-6"
          style={{ border: `1px solid ${LINE}` }}
        >
          <YogurtEditor
            initialMarkdown=""
            enrichedMarkdown={hydratedNotesMd}
            placeholder={NOTES_PLACEHOLDER}
            onChange={setNotesMd}
          />
        </section>
      </main>

      <TranscriptDock meetingId={meetingId} token={token} history={transcriptHistory} />

      {/* Phase 6 (Plan 06-02): floating Ask-pill / chat window. Mounted at
          the route root (sibling to TranscriptDock) so the fixed-position
          pill never reflows under the notes column and persists across
          editor remounts. */}
      <AskExperience meetingId={meetingId} token={token} />
    </div>
  );
}
