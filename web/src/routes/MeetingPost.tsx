// MeetingPost — the hero post-meeting route (Phase 4 NOTES-02/07/08/09/11/12/13).
//
// Layout (top → bottom):
//
//   ┌───────────────────────────────────────────────────────────────┐
//   │  ● Weaving your notes into the transcript…   ━━━     1,234c   │  ← EnhancingBanner (visible while enhancing)
//   ├───────────────────────────────────────────────────────────────┤
//   │                                       [ Re-enhance ]          │  ← top-right toolbar (sticky)
//   │                                                                │
//   │                              ┌──── 660px column ───┐           │
//   │                              │                     │           │
//   │                              │  YogurtEditor       │           │
//   │                              │  (aiGrey + links)   │           │
//   │                              │                     │           │
//   │                              └─────────────────────┘           │
//   │                                                  □ your notes  │  ← Legend (top-right swatch)
//   │                                                  ▢ AI          │
//   └───────────────────────────────────────────────────────────────┘
//
// Data flow:
//   1. location.state.enrichedMd populates the editor immediately if
//      navigated from End-meeting (no re-fetch).
//   2. Otherwise GET /api/meetings/:id and use enriched_md (falling back
//      to notes_md if enriched is empty / null).
//   3. useEnhanceProgress drives the EnhancingBanner during Re-enhance
//      bursts (phase + chars from the WS broadcast).
//   4. ReEnhanceButton triggers a fresh POST /enhance, then onEnhanced
//      replaces the editor content via the YogurtEditor `enrichedMarkdown`
//      prop (which calls `editor.commands.setContent(html, false)`).
//   5. Clicking a `↳ HH:MM` deep-link inside the editor dispatches
//      `yogurt:transcript:scrollTo` on `window` and opens the transcript
//      dock — TranscriptDock listens for that event and scrolls.
//
// 660px max-width is doubly enforced (NOTES-01): YogurtEditor sets it
// inline AND we wrap the inner column in a `max-w-[660px]` div so the
// Legend can be absolutely positioned relative to a known container.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, Navigate, useLocation, useNavigate, useParams } from "react-router";
import { ArrowLeft } from "lucide-react";
import { YogurtEditor } from "../editor";
import { EnhancingBanner } from "../components/EnhancingBanner";
import { Legend } from "../components/Legend";
import { ReEnhanceButton } from "../components/ReEnhanceButton";
import { TranscriptDock } from "../components/TranscriptDock";
import { AskExperience } from "../components/AskExperience";
import { InlineTitle } from "../components/library/InlineTitle";
import { MeetingLabels } from "../components/labels/MeetingLabels";
import { MeetingMetaPills } from "../components/MeetingMetaPills";
import { DeleteMeetingConfirm } from "../components/library/DeleteMeetingConfirm";
import { ensureSessionToken } from "../lib/session";
import { useEnhanceProgress, type StoredTranscriptSegment } from "../lib/ws";
import { meetingsApi, useActiveRecording, useMeeting } from "../lib/api/meetings";
import type { EnhanceResponse } from "../lib/api";

const PAPER = "#FBF7EF"; // --color-paper
const INK = "#211D18"; // --color-ink
const LINE = "#EBE3D5"; // --color-line
const STRAW = "#E07A66"; // --color-straw — error accent
const STRAW_SOFT = "#FBE6E0"; // --color-strsoft

/**
 * Matches `yogurt_db::Meeting`'s actual wire field names (`started_at` /
 * `ended_at`, NOT `*_unix_ms`) — see `crates/yogurt-db/src/meetings.rs`.
 * The previous `*_unix_ms` field names here never matched a real response
 * field, so `startedAtUnixMs`/`endedAtUnixMs` below were always undefined
 * and the header could never show a date or duration.
 */
interface MeetingFetchResponse {
  id?: string;
  title?: string | null;
  notes_md?: string | null;
  enriched_md?: string | null;
  transcript_json?: string | null;
  started_at?: number | null;
  ended_at?: number | null;
  stt_engine?: string | null;
  llm_model?: string | null;
}

interface LocationStateShape {
  enrichedMd?: string;
  /** Set by `Meeting.tsx`'s endMeeting when `EnhanceResponse.too_short` came
   * back true — the server skipped enhancing a meeting with no notes and a
   * trivial transcript. Drives the brief "Meeting too short" state below
   * instead of rendering a near-empty editor. */
  tooShort?: boolean;
}

/** How long the "Meeting too short" state stays up before bouncing to the
 * library — long enough to read, short enough not to feel stuck. */
const TOO_SHORT_REDIRECT_MS = 1400;

export function MeetingPost() {
  const params = useParams<{ id: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const meetingId = params.id ?? null;

  const stateShape = (location.state ?? {}) as LocationStateShape;
  const preloadedEnrichedMd = stateShape.enrichedMd;
  const tooShort = stateShape.tooShort === true;

  // Covers deep links, refresh, and the back button landing on the frozen
  // post view for a meeting that's STILL recording server-side — bounce to
  // the live capture surface instead (it has the controls + live
  // transcript this route doesn't). Checked below, after every hook in
  // this component has run, so hooks-order rules stay intact.
  const activeRecording = useActiveRecording();

  const [token, setToken] = useState<string | null>(null);
  // Editor content. `enrichedMd` undefined → editor renders blank until
  // load resolves; null → "loaded but empty" sentinel (rare); string →
  // content to render. We collapse undefined / null into "" for display.
  const [enrichedMd, setEnrichedMd] = useState<string | undefined>(
    preloadedEnrichedMd,
  );
  const [notesMd, setNotesMd] = useState<string>("");
  // BL-3: track the LIVE markdown serialization of the editor so Re-enhance
  // posts the user's current edits, not the stale GET-fetched DB row. Seeded
  // from `enrichedMd` on mount/swap; updated by YogurtEditor's onChange every
  // time the user types. Without this, a user who edits a grey bullet
  // (promotes to black per NOTES-10) then clicks Re-enhance LOSES the edit.
  const [liveMarkdown, setLiveMarkdown] = useState<string>(
    preloadedEnrichedMd ?? "",
  );
  const [transcriptJson, setTranscriptJson] = useState<string>("[]");
  const [title, setTitle] = useState<string | undefined>(undefined);
  const [startedAtUnixMs, setStartedAtUnixMs] = useState<number | undefined>(
    undefined,
  );
  const [endedAtUnixMs, setEndedAtUnixMs] = useState<number | undefined>(
    undefined,
  );
  const [sttEngine, setSttEngine] = useState<string | undefined>(undefined);
  const [llmModel, setLlmModel] = useState<string | undefined>(undefined);
  const [enhancing, setEnhancing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [transcriptOpen, setTranscriptOpen] = useState(false);

  // WS progress drives the banner: any `sending` / `streaming` event from
  // the server flips `enhancing` true; `done` flips it false. The
  // `enhancing` local state above also reflects the in-flight POST so the
  // banner appears IMMEDIATELY on click (not after the first WS hop).
  const ws = useEnhanceProgress(meetingId, token);

  // Task NOTES-08: subscribe to the shared react-query cache purely so a
  // rename via the header's <InlineTitle> (which writes through
  // `useUpdateMeetingTitle`'s `setQueryData`) is reflected immediately —
  // the generation-guarded GET below is the source of truth for the
  // INITIAL title/notes/transcript load and is left untouched.
  const meetingCacheQuery = useMeeting(meetingId ?? undefined);
  const displayTitle = meetingCacheQuery.data?.title ?? title;

  // Bootstrap: fetch the session token once on mount.
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

  // HI-2: track a monotonically-increasing "generation" so that a stale
  // in-flight GET (or future Re-enhance trigger) does not overwrite state
  // that a more-recent operation already updated. Each operation captures
  // generationRef.current AT SCHEDULE TIME and bumps the ref; results are
  // applied only when the captured value still matches the current ref.
  // This closes the documented race where the initial GET resolves AFTER
  // a Re-enhance completes — without this guard, the GET's notes_md (the
  // pre-enhance state) stomps the just-updated notesMd.
  const generationRef = useRef(0);

  // Fallback fetch: if location.state didn't pre-load the enriched markdown,
  // GET /api/meetings/:id and pull enriched_md (falling back to notes_md).
  useEffect(() => {
    if (!meetingId || !token) return;
    // Capture the generation at schedule time.
    const myGen = ++generationRef.current;
    const abortCtrl = new AbortController();
    let cancelled = false;
    (async () => {
      try {
        const res = await fetch(`/api/meetings/${meetingId}`, {
          headers: { Authorization: `Bearer ${token}` },
          signal: abortCtrl.signal,
        });
        // HI-2: discard if a newer operation has bumped the generation
        // counter (we're now stale).
        if (cancelled || generationRef.current !== myGen) return;
        if (!res.ok) {
          if (res.status === 404) {
            setError(`Meeting ${meetingId.slice(0, 8)} not found.`);
            return;
          }
          setError(`Failed to load meeting (${res.status})`);
          return;
        }
        const json = (await res.json()) as MeetingFetchResponse;
        if (cancelled || generationRef.current !== myGen) return;
        // Only overwrite preloaded enrichedMd if we don't already have it.
        // `??` alone is wrong here: an empty-STRING enriched_md must also
        // fall back to the raw notes, or the reader gets a blank document
        // for a meeting that has notes but was never (successfully)
        // enhanced.
        if (enrichedMd === undefined) {
          const enriched = json.enriched_md;
          setEnrichedMd(
            enriched && enriched.trim() !== "" ? enriched : (json.notes_md ?? ""),
          );
        }
        setNotesMd(json.notes_md ?? "");
        setTranscriptJson(json.transcript_json ?? "[]");
        setTitle(json.title ?? undefined);
        setStartedAtUnixMs(json.started_at ?? undefined);
        setEndedAtUnixMs(json.ended_at ?? undefined);
        setSttEngine(json.stt_engine ?? undefined);
        setLlmModel(json.llm_model ?? undefined);
      } catch (e) {
        // AbortError from cleanup is expected — ignore.
        if (cancelled || generationRef.current !== myGen) return;
        if (e instanceof DOMException && e.name === "AbortError") return;
        setError(e instanceof Error ? e.message : "Failed to load meeting");
      }
    })();
    return () => {
      cancelled = true;
      // HI-2: abort in-flight fetch on unmount so dangling responses don't
      // touch unmounted state and don't waste browser HTTP slots.
      abortCtrl.abort();
    };
    // We intentionally don't depend on `enrichedMd` here — refetching every
    // time the editor content updates would create a loop. The fetch
    // should fire once per (meetingId, token) pair.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [meetingId, token]);

  // Click-to-jump: dispatch the scroll event AND open the dock (the dock
  // ignores the event if it's already mounted, but its `open` state is
  // controlled by the tab button — we keep our own mirror so the post
  // route can force-open it on link click).
  const handleTranscriptLinkClick = useCallback((ts: number) => {
    setTranscriptOpen(true);
    window.dispatchEvent(
      new CustomEvent("yogurt:transcript:scrollTo", { detail: { ts } }),
    );
  }, []);

  // Task NOTES-09: parse the meeting row's persisted `transcript_json`
  // once per string value — feeds both the static TranscriptDock below
  // and the tooltip-excerpt effect that used to re-parse the same JSON
  // independently.
  const segments = useMemo<StoredTranscriptSegment[]>(() => {
    try {
      const parsed: unknown = JSON.parse(transcriptJson);
      if (!Array.isArray(parsed)) return [];
      return parsed.filter(
        (s): s is StoredTranscriptSegment =>
          s !== null &&
          typeof s === "object" &&
          typeof s.ts_ms === "number" &&
          (s.channel === "me" || s.channel === "them") &&
          typeof s.text === "string",
      );
    } catch {
      // Malformed transcript JSON — render with no segments rather than throw.
      return [];
    }
  }, [transcriptJson]);

  // NOTES-11: after each enriched_md change, walk the editor DOM and set a
  // native browser tooltip (`title` attribute) on every `↳ HH:MM` link to
  // the closest transcript segment's text. We use `title` rather than a
  // floating-popover library because (a) zero deps, (b) hover-on-text is
  // the universal browser pattern users already know, (c) the design
  // contract calls for "tooltip showing the transcript excerpt around that
  // timestamp" — plain `title` satisfies it for v1.
  useEffect(() => {
    if (enrichedMd === undefined) return;
    if (segments.length === 0) return;

    // The editor mounts asynchronously after `enrichedMd` arrives. Defer
    // the DOM walk to the next microtask so TipTap has flushed renderHTML.
    const handle = setTimeout(() => {
      const root = document.querySelector("[data-testid='yogurt-editor']");
      if (!root) return;
      const links = root.querySelectorAll<HTMLElement>(
        "[data-transcript-link]",
      );
      links.forEach((link) => {
        const tsRaw = link.getAttribute("data-ts");
        const ts = Number(tsRaw ?? "");
        if (!Number.isFinite(ts)) return;
        // Find closest segment by absolute distance (ts is seconds, ts_ms
        // is ms).
        let best: { text: string; dist: number } | null = null;
        for (const seg of segments) {
          const dist = Math.abs(seg.ts_ms / 1000 - ts);
          if (best === null || dist < best.dist) {
            best = { text: seg.text, dist };
          }
        }
        if (best) {
          // Trim to a sensible excerpt so the tooltip isn't a wall of text.
          const excerpt =
            best.text.length > 280
              ? best.text.slice(0, 277) + "…"
              : best.text;
          link.setAttribute("title", excerpt);
        }
      });
    }, 0);
    return () => clearTimeout(handle);
  }, [enrichedMd, segments]);

  // BL-3: whenever the editor swaps to new enriched content (initial load
  // from GET, or after a Re-enhance completes), seed `liveMarkdown` so a
  // subsequent Re-enhance click that fires BEFORE any user keystroke still
  // posts the server's latest version (not an empty string).
  useEffect(() => {
    if (enrichedMd !== undefined) {
      setLiveMarkdown(enrichedMd);
    }
  }, [enrichedMd]);

  // Task NOTES-11 — persist post-meeting edits (including grey-to-black
  // promotions) so a reload doesn't lose them. Debounced 1s PATCH of the
  // editor's live wire-format markdown, flushed on unmount. Gated on
  // `enrichedMd !== undefined` (hydration settled — either preloaded via
  // location.state or the GET resolved) so an empty initial `liveMarkdown`
  // can't race the fetch and PATCH blank content over real saved notes.
  const meetingIdForSaveRef = useRef(meetingId);
  meetingIdForSaveRef.current = meetingId;
  const liveMarkdownRef = useRef(liveMarkdown);
  liveMarkdownRef.current = liveMarkdown;
  const lastSavedEnrichedRef = useRef<string | undefined>(undefined);
  // DATA-LOSS GUARD: autosave may only persist content the USER produced.
  // Set exclusively by the editor's onChange (which TipTap fires for user
  // transactions, never for our programmatic `setContent(html, false)`
  // hydration). Without this gate, a mount that merely LOADED content —
  // or failed to — would bulldoze the stored row on unmount/debounce with
  // whatever it happened to hold (observed live: a stale mount PATCHed
  // `enriched_md: ""` over a real document).
  const userEditedRef = useRef(false);

  const flushEnriched = useCallback(async () => {
    const id = meetingIdForSaveRef.current;
    if (!id) return;
    if (!userEditedRef.current) return;
    const md = liveMarkdownRef.current;
    if (md === lastSavedEnrichedRef.current) return;
    // Never persist an empty serialization: the enriched doc never
    // legitimately becomes empty in this flow, but a hydration/parse
    // failure serializes to "" — see the data-loss guard above.
    if (md.trim() === "") return;
    lastSavedEnrichedRef.current = md;
    try {
      await meetingsApi.patch(id, { enriched_md: md });
    } catch {
      // Best-effort — a later autosave tick or the next unmount retries.
    }
  }, []);

  useEffect(() => {
    if (enrichedMd === undefined) return;
    const t = setTimeout(() => {
      void flushEnriched();
    }, 1000);
    return () => clearTimeout(t);
  }, [liveMarkdown, enrichedMd, flushEnriched]);

  useEffect(() => {
    return () => {
      void flushEnriched();
    };
  }, [flushEnriched]);

  // Re-enhance callbacks.
  const handleEnhanced = useCallback((response: EnhanceResponse) => {
    // HI-2: bump the generation so any in-flight GET fetch's eventual
    // result is recognized as stale and DROPPED rather than overwriting
    // the just-enhanced state.
    generationRef.current += 1;
    setEnrichedMd(response.enriched_md);
    if (response.llm_model) setLlmModel(response.llm_model);
  }, []);
  // BL-3: capture every editor mutation so Re-enhance has the live content.
  const handleEditorChange = useCallback((markdown: string) => {
    userEditedRef.current = true;
    setLiveMarkdown(markdown);
  }, []);
  const handleEnhancing = useCallback((busy: boolean) => {
    setEnhancing(busy);
  }, []);
  const handleError = useCallback((message: string) => {
    setError(message);
  }, []);

  // The banner is visible if EITHER the local fetch is in flight OR the WS
  // has fired a non-`done` event. Local wins on first click; WS keeps it
  // alive in case the network round-trip is slow.
  const bannerVisible = enhancing || ws.enhancing;
  // Char count: prefer the WS-reported value (it's authoritative once any
  // streaming event has fired). If WS hasn't reported, leave it undefined.
  const bannerChars = ws.chars ?? undefined;
  // BL-5: surface ws-reported error state so the banner transitions to the
  // strawberry pill instead of spinning forever.
  const bannerError = ws.errorMessage;
  // BL-5: dismiss banner error after a successful retry. The retry handler
  // sets a local "trigger" flag; the Re-enhance button picks it up via a
  // ref and fires. Since ReEnhanceButton owns its own click handler, we
  // expose a simple "clear the error message" affordance for now —
  // re-enabling the user to click Re-enhance again clears the banner.
  const dismissBannerError = useCallback(() => {
    // We can't reset ws.errorMessage from outside the hook directly, but
    // any subsequent non-error enhance_progress frame will null it. Until
    // then we drop the error state by tracking a local dismiss flag.
    setBannerDismissed(true);
  }, []);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  // Reset dismiss flag whenever a new error arrives (so a fresh failure
  // resurfaces the banner).
  useEffect(() => {
    if (bannerError) setBannerDismissed(false);
  }, [bannerError]);

  // Bounce back to library if no meeting id (shouldn't happen via the
  // router, but guard anyway).
  useEffect(() => {
    if (!params.id) {
      navigate("/", { replace: true });
    }
  }, [params.id, navigate]);

  // Too-short meetings never ran enhance server-side (see enhance.rs'
  // TOO_SHORT_TRANSCRIPT_WORDS gate) — show the brief state below, then
  // auto-return to the library instead of leaving the user on a blank
  // editor they'd have to navigate out of manually.
  useEffect(() => {
    if (!tooShort) return;
    const t = setTimeout(
      () => navigate("/", { replace: true }),
      TOO_SHORT_REDIRECT_MS,
    );
    return () => clearTimeout(t);
  }, [tooShort, navigate]);

  if (!meetingId) return null;

  if (tooShort) {
    return (
      <div
        className="min-h-screen flex items-center justify-center"
        style={{ backgroundColor: PAPER }}
        data-testid="meeting-too-short"
      >
        <div role="status" aria-live="polite" className="text-center space-y-1.5">
          <p className="font-serif text-[19px] text-ink">Meeting too short</p>
          <p className="text-[13px] text-mut">
            Nothing to enhance — back to the library…
          </p>
        </div>
      </div>
    );
  }

  if (activeRecording.data?.id === meetingId) {
    return <Navigate to={`/meeting/${meetingId}`} replace />;
  }

  return (
    <div
      className="min-h-screen pr-7"
      style={{ backgroundColor: PAPER }}
      data-testid="meeting-post-route"
    >
      <EnhancingBanner
        visible={bannerVisible}
        chars={bannerChars}
        errorMessage={bannerDismissed ? null : bannerError}
        onRetry={bannerError ? dismissBannerError : undefined}
      />

      {/* Task NOTES-08 — page header: "← Library" + inline-editable title
        + date/duration line on the left, Re-enhance on the right. Sticky,
        BELOW the EnhancingBanner (z-index 10 vs banner's 20). */}
      <div
        style={{
          position: "sticky",
          top: bannerVisible ? 44 : 0,
          zIndex: 10,
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 16,
          padding: "12px 24px",
          background: PAPER,
          borderBottom: `1px solid ${LINE}`,
        }}
      >
        <div className="flex items-center gap-4 min-w-0">
          <Link
            to="/"
            className="inline-flex items-center gap-1.5 shrink-0 text-[12px] font-mono uppercase tracking-wider text-mut hover:text-ink transition-colors"
          >
            <ArrowLeft size={14} aria-hidden="true" />
            <span>Library</span>
          </Link>
          <div className="min-w-0">
            <InlineTitle
              id={meetingId}
              title={displayTitle ?? "Untitled meeting"}
              className="block font-serif text-[19px] leading-tight text-ink truncate"
            />
            <div className="mt-1 flex flex-wrap items-center gap-1.5">
              <MeetingMetaPills
                startedAt={startedAtUnixMs}
                endedAt={endedAtUnixMs}
                sttEngine={sttEngine}
                llmModel={llmModel}
              />
              {meetingId && <MeetingLabels meetingId={meetingId} />}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
        {token && (
          <ReEnhanceButton
            meetingId={meetingId}
            // BL-3: post the LIVE editor markdown (user edits + any
            // promoted-grey-to-black content), NOT the stale notes_md row
            // from the DB. Falling back to notesMd keeps the very first
            // Re-enhance (no edits yet, no enriched_md hydrated) sensible.
            notesMd={liveMarkdown || notesMd}
            transcriptJson={transcriptJson}
            title={displayTitle}
            startedAtUnixMs={startedAtUnixMs}
            endedAtUnixMs={endedAtUnixMs}
            token={token}
            onEnhanced={handleEnhanced}
            onEnhancing={handleEnhancing}
            onError={handleError}
          />
        )}
        {/* Delete lives here too, not just on the Library card — you decide
          a meeting was useless while reading it, not while scanning the
          list. Same confirm + .md checkbox; on success we bounce to the
          Library since this route's meeting no longer exists. */}
        {meetingId && (
          <DeleteMeetingConfirm
            id={meetingId}
            variant="icon"
            onDeleted={() => navigate("/")}
          />
        )}
        </div>
      </div>

      {/* Hero column. `position: relative` so the Legend's absolute
        top-right placement is relative to this container. The 660px
        max-width is enforced here AND inside YogurtEditor (NOTES-01). */}
      <main
        style={{
          position: "relative",
          maxWidth: 660,
          margin: "0 auto",
          padding: "42px 24px 130px",
          color: INK,
        }}
      >
        <Legend />

        {error && (
          <div
            role="alert"
            className="rounded-card px-4 py-3 text-[13px] mb-4"
            style={{
              backgroundColor: STRAW_SOFT,
              color: STRAW,
              border: `1px solid ${STRAW}`,
            }}
          >
            {error}
          </div>
        )}

        <YogurtEditor
          initialMarkdown={enrichedMd ?? ""}
          enrichedMarkdown={enrichedMd}
          editable={true}
          onChange={handleEditorChange}
          onTranscriptLinkClick={handleTranscriptLinkClick}
        />
      </main>

      {/* Task NOTES-09: static mode — the meeting is over, so the dock
        renders the persisted transcript instead of subscribing to the
        (necessarily empty) live WS. */}
      <TranscriptDock
        meetingId={meetingId}
        token={token}
        forceOpen={transcriptOpen}
        onOpenChange={setTranscriptOpen}
        segments={segments}
      />

      {/* Phase 6 (Plan 06-02): CHAT-02 persistence — the Ask experience
          renders identically on the post-meeting view. Prior chat history
          hydrates via GET /api/meetings/:id/chat on mount of useChat. */}
      <AskExperience meetingId={meetingId} token={token} />
    </div>
  );
}
