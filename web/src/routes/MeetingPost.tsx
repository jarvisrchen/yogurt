import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, Navigate, useLocation, useNavigate, useParams } from "react-router";
import { ArrowLeft } from "lucide-react";
import { YogurtEditor } from "../editor";
import { EnhancingBanner } from "../components/EnhancingBanner";
import { Legend } from "../components/Legend";
import { ReEnhanceButton } from "../components/ReEnhanceButton";
import { TemplatePicker } from "../components/TemplatePicker";
import { AUTO_TEMPLATE } from "../lib/api/templates";
import { TranscriptDock } from "../components/TranscriptDock";
import { AskExperience } from "../components/AskExperience";
import { InlineTitle } from "../components/library/InlineTitle";
import { MeetingLabels } from "../components/labels/MeetingLabels";
import { ImportedFromLine } from "../components/ImportedFromLine";
import { MeetingMetaPills } from "../components/MeetingMetaPills";
import { DeleteMeetingConfirm } from "../components/library/DeleteMeetingConfirm";
import { ArchiveMeetingButton } from "../components/library/ArchiveMeetingButton";
import { CopyUrlButton } from "../components/CopyUrlButton";
import { bearerFetch, ensureSessionToken } from "../lib/session";
import { useEnhanceProgress, type StoredTranscriptSegment } from "../lib/ws";
import { useQueryClient } from "@tanstack/react-query";
import {
  meetingsApi,
  meetingsKey,
  useActiveRecording,
  useMeeting,
  type MeetingSourceJson,
} from "../lib/api/meetings";
import { postEnhance, type EnhanceResponse } from "../lib/api";
import { useSettings } from "../lib/api/settings";

const PAPER = "var(--color-paper)";
const INK = "var(--color-ink)";
const LINE = "var(--color-line)";
const STRAW = "var(--color-straw)"; // error accent
const STRAW_SOFT = "var(--color-strsoft)";

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
  template?: string | null;
  source?: string | null;
  source_json?: MeetingSourceJson | null;
}

interface LocationStateShape {
  autoEnhance?: { notes_md: string; title?: string };
  /** The live dock was open on Recap at End. */
  dockRecap?: boolean;
}

/** Long enough to read, short enough not to feel stuck. */
const TOO_SHORT_REDIRECT_MS = 1400;

export function MeetingPost() {
  const params = useParams<{ id: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const meetingId = params.id ?? null;

  const qc = useQueryClient();
  const stateShape = (location.state ?? {}) as LocationStateShape;
  const autoEnhance = stateShape.autoEnhance;
  const [tooShort, setTooShort] = useState(false);

  // Deep links, refresh, and back can land here for a meeting still recording; bounce to the live view.
  const activeRecording = useActiveRecording();

  const [token, setToken] = useState<string | null>(null);
  // undefined: not loaded yet. null: loaded but empty.
  const [enrichedMd, setEnrichedMd] = useState<string | undefined>(undefined);
  const [notesMd, setNotesMd] = useState<string>("");
  const [liveEnrichedMarkdown, setLiveEnrichedMarkdown] = useState<string>("");
  const [activeDocument, setActiveDocument] = useState<"enhanced" | "notes">(
    "enhanced",
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
  const [template, setTemplate] = useState<string>(AUTO_TEMPLATE);
  const [source, setSource] = useState<{ name: string; json: MeetingSourceJson } | null>(null);
  const [enhancing, setEnhancing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [transcriptOpen, setTranscriptOpen] = useState(stateShape.dockRecap ?? false);
  const liveRecap = useSettings().data?.general?.live_recap ?? false;

  const ws = useEnhanceProgress(meetingId, token);
  // The server emits `done` (clearing `ws.text`) before the enhance POST
  // response arrives. `previewHold` keeps the last streamed snapshot on
  // screen through that gap. It is cleared explicitly, not derived from
  // `enrichedMd`, because a re-enhance can produce byte-identical output.
  const [previewHold, setPreviewHold] = useState<string | null>(null);
  useEffect(() => {
    if (typeof ws.text === "string") setPreviewHold(ws.text);
  }, [ws.text]);
  useEffect(() => {
    if (ws.errorMessage) setPreviewHold(null);
  }, [ws.errorMessage]);
  const preview = ws.text ?? previewHold;

  // Reflects a rename from <InlineTitle> immediately; the GET below owns the initial load.
  const meetingCacheQuery = useMeeting(meetingId ?? undefined);
  const displayTitle = meetingCacheQuery.data?.title ?? title;

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

  // A stale in-flight GET must not overwrite state from a newer operation
  // (e.g. the initial GET resolving after a Re-enhance completes).
  const generationRef = useRef(0);
  // `loadMeeting` memoizes on [meetingId, token], so read the live value through a ref.
  const enrichedMdRef = useRef(enrichedMd);
  enrichedMdRef.current = enrichedMd;

  // `force` skips the "only apply if enrichedMd is unset" guard, which protects
  // preloaded state on first load. A reload-mid-stream re-fetch must always win.
  const loadMeeting = useCallback(
    (opts?: { force?: boolean }) => {
      if (!meetingId || !token) return () => {};
      const myGen = ++generationRef.current;
      const abortCtrl = new AbortController();
      let cancelled = false;
      (async () => {
        try {
          const res = await bearerFetch(`/api/meetings/${meetingId}`, {
            signal: abortCtrl.signal,
          });
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
          if (opts?.force || enrichedMdRef.current === undefined) {
            const enriched = json.enriched_md;
            setEnrichedMd(enriched ?? "");
            if (!enriched || enriched.trim() === "") setActiveDocument("notes");
            setPreviewHold(null);
          }
          setNotesMd(json.notes_md ?? "");
          setTranscriptJson(json.transcript_json ?? "[]");
          setTitle(json.title ?? undefined);
          setStartedAtUnixMs(json.started_at ?? undefined);
          setEndedAtUnixMs(json.ended_at ?? undefined);
          setSttEngine(json.stt_engine ?? undefined);
          setLlmModel(json.llm_model ?? undefined);
          if (json.template) setTemplate(json.template);
          setSource(json.source ? { name: json.source, json: json.source_json ?? {} } : null);
        } catch (e) {
          // AbortError from cleanup is expected.
          if (cancelled || generationRef.current !== myGen) return;
          if (e instanceof DOMException && e.name === "AbortError") return;
          setError(e instanceof Error ? e.message : "Failed to load meeting");
        }
      })();
      return () => {
        cancelled = true;
        abortCtrl.abort();
      };
    },
    [meetingId, token],
  );

  useEffect(() => {
    return loadMeeting();
  }, [loadMeeting]);

  const handleTranscriptLinkClick = useCallback((ts: number) => {
    setTranscriptOpen(true);
    window.dispatchEvent(
      new CustomEvent("yogurt:transcript:scrollTo", { detail: { ts } }),
    );
  }, []);

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
      return [];
    }
  }, [transcriptJson]);

  // A native `title` tooltip avoids a popover dependency.
  useEffect(() => {
    if (activeDocument !== "enhanced" || enrichedMd === undefined) return;
    if (segments.length === 0) return;

    // The editor mounts after `enrichedMd` arrives; wait for TipTap to flush renderHTML.
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
        let best: { text: string; dist: number } | null = null;
        for (const seg of segments) {
          const dist = Math.abs(seg.ts_ms / 1000 - ts);
          if (best === null || dist < best.dist) {
            best = { text: seg.text, dist };
          }
        }
        if (best) {
          const excerpt =
            best.text.length > 280
              ? best.text.slice(0, 277) + "…"
              : best.text;
          link.setAttribute("title", excerpt);
        }
      });
    }, 0);
    return () => clearTimeout(handle);
  }, [activeDocument, enrichedMd, segments]);

  useEffect(() => {
    if (enrichedMd !== undefined) {
      setLiveEnrichedMarkdown(enrichedMd);
    }
  }, [enrichedMd]);

  const meetingIdForSaveRef = useRef(meetingId);
  meetingIdForSaveRef.current = meetingId;
  const notesMdRef = useRef(notesMd);
  notesMdRef.current = notesMd;
  const lastSavedNotesRef = useRef<string | undefined>(undefined);
  // Autosave persists only user-produced notes. The Enhanced document is read-only.
  const notesEditedRef = useRef(false);

  const flushDocuments = useCallback(async () => {
    const id = meetingIdForSaveRef.current;
    if (!id) return;
    const notes = notesMdRef.current;
    if (!notesEditedRef.current || notes === lastSavedNotesRef.current) return;
    try {
      await meetingsApi.patch(id, { notes_md: notes });
      lastSavedNotesRef.current = notes;
    } catch {
      // Best-effort — a later autosave tick or the next unmount retries.
    }
  }, []);

  useEffect(() => {
    if (enrichedMd === undefined) return;
    const t = setTimeout(() => {
      void flushDocuments();
    }, 1000);
    return () => clearTimeout(t);
  }, [notesMd, enrichedMd, flushDocuments]);

  useEffect(() => {
    return () => {
      void flushDocuments();
    };
  }, [flushDocuments]);

  const handleEnhanced = useCallback((response: EnhanceResponse) => {
    // Drop any in-flight GET result that would overwrite this.
    generationRef.current += 1;
    setEnrichedMd(response.enriched_md);
    if (response.llm_model) setLlmModel(response.llm_model);
    if (response.template) setTemplate(response.template);
    setActiveDocument("enhanced");
    setPreviewHold(null);
  }, []);
  const handleEditorChange = useCallback((markdown: string) => {
    notesEditedRef.current = true;
    setNotesMd(markdown);
  }, []);
  const handleEnhancing = useCallback((busy: boolean) => {
    setEnhancing(busy);
  }, []);
  const handleError = useCallback((message: string) => {
    setError(message);
  }, []);

  useEffect(() => {
    if (preview != null) setActiveDocument("enhanced");
  }, [preview]);

  // A ref so the reload effect below doesn't re-fire on every `enhancing` toggle.
  const enhancingRef = useRef(enhancing);
  enhancingRef.current = enhancing;

  // A `done` frame with no local enhance POST in flight means a page reload landed
  // mid-stream. Re-fetch (forced) to pick up the final `enriched_md` and `llm_model`.
  useEffect(() => {
    if (ws.phase !== "done") return;
    if (enhancingRef.current) return;
    loadMeeting({ force: true });
  }, [ws.phase, loadMeeting]);

  // StrictMode-safe via a ref.
  const autoEnhanceStartedRef = useRef(false);
  useEffect(() => {
    if (!autoEnhance) return;
    if (!meetingId || !token) return;
    if (autoEnhanceStartedRef.current) return;
    autoEnhanceStartedRef.current = true;
    setEnhancing(true);
    (async () => {
      try {
        const response = await postEnhance(
          meetingId,
          {
            notes_md: autoEnhance.notes_md,
            transcript_json: "[]",
            title: autoEnhance.title,
          },
        );
        handleEnhanced(response);
        if (response.too_short) setTooShort(true);
      } catch (e) {
        handleError(e instanceof Error ? e.message : "Enhance failed");
      } finally {
        setEnhancing(false);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoEnhance, meetingId, token]);

  const bannerVisible = enhancing || ws.enhancing;
  const bannerChars = ws.chars ?? undefined;
  const bannerError = ws.errorMessage;
  const dismissBannerError = useCallback(() => {
    // `ws.errorMessage` can't be reset from here, so track a local dismiss flag.
    setBannerDismissed(true);
  }, []);
  const [bannerDismissed, setBannerDismissed] = useState(false);
  useEffect(() => {
    if (bannerError) setBannerDismissed(false);
  }, [bannerError]);

  useEffect(() => {
    if (!params.id) {
      navigate("/", { replace: true });
    }
  }, [params.id, navigate]);

  // Too-short meetings never ran enhance server-side.
  useEffect(() => {
    if (!tooShort) return;
    // The server drops the stub row, so the cached library list is stale.
    void qc.invalidateQueries({ queryKey: meetingsKey });
    const t = setTimeout(
      () => navigate("/", { replace: true }),
      TOO_SHORT_REDIRECT_MS,
    );
    return () => clearTimeout(t);
  }, [tooShort, navigate, qc]);

  if (!meetingId) return null;

  if (tooShort) {
    return (
      <div
        className="min-h-screen flex items-center justify-center"
        style={{ backgroundColor: PAPER }}
        data-testid="meeting-too-short"
      >
        <div role="status" aria-live="polite" className="text-center space-y-1.5">
          <p className="heading-card">Meeting too short</p>
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
            className="inline-flex items-center gap-1.5 shrink-0 text-[13px] font-medium text-mut hover:text-ink transition-colors"
          >
            <ArrowLeft size={14} aria-hidden="true" />
            <span>Library</span>
          </Link>
          <div className="min-w-0">
            <InlineTitle
              id={meetingId}
              title={displayTitle ?? "Untitled meeting"}
              startedAt={startedAtUnixMs}
              className="block heading-card truncate"
            />
            <div className="mt-1 space-y-1.5">
              <MeetingMetaPills
                startedAt={startedAtUnixMs}
                endedAt={endedAtUnixMs}
                sttEngine={sttEngine}
                llmModel={llmModel}
              />
              {meetingId && <MeetingLabels meetingId={meetingId} />}
              {source?.name === "granola" && <ImportedFromLine source={source.json} />}
            </div>
          </div>
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
        {meetingId && <CopyUrlButton />}
        {token && (
          <TemplatePicker
            value={template}
            onChange={setTemplate}
            disabled={bannerVisible}
          />
        )}
        {token && (
          <ReEnhanceButton
            meetingId={meetingId}
            notesMd={notesMd}
            transcriptJson={transcriptJson}
            title={displayTitle}
            startedAtUnixMs={startedAtUnixMs}
            endedAtUnixMs={endedAtUnixMs}
            template={template === AUTO_TEMPLATE ? undefined : template}
            confirmMessage={source?.name === "granola" ? "This replaces the notes imported from Granola." : undefined}
            onEnhanced={handleEnhanced}
            onEnhancing={handleEnhancing}
            onError={handleError}
          />
        )}
        {meetingId && <ArchiveMeetingButton id={meetingId} />}
        {meetingId && (
          <DeleteMeetingConfirm
            id={meetingId}
            variant="icon"
            onDeleted={() => navigate("/")}
          />
        )}
        </div>
      </div>

      <main
        style={{
          position: "relative",
          maxWidth: 660,
          margin: "0 auto",
          padding: "42px 24px 130px",
          color: INK,
        }}
      >
        <div
          role="tablist"
          aria-label="Meeting document"
          className="mb-7 inline-flex gap-0.5 rounded-card bg-line/60 p-1"
        >
          {(["enhanced", "notes"] as const).map((document) => {
            const selected = activeDocument === document;
            return (
              <button
                key={document}
                type="button"
                role="tab"
                aria-selected={selected}
                onClick={() => setActiveDocument(document)}
                className={`rounded-button px-4 py-[7px] text-[13px] font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue ${
                  selected
                    ? "bg-card text-ink shadow-card"
                    : "text-mut hover:text-ink"
                }`}
              >
                {document === "enhanced" ? "Enhanced" : "My notes"}
              </button>
            );
          })}
        </div>

        {activeDocument === "enhanced" && <Legend />}

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
          initialMarkdown={
            activeDocument === "enhanced"
              ? (enrichedMd ?? "")
              : notesMd
          }
          enrichedMarkdown={
            activeDocument === "enhanced"
              ? // The preview is never written into `liveEnrichedMarkdown`, so it can't reach autosave.
                (preview != null ? preview : liveEnrichedMarkdown)
              : notesMd
          }
          editable={activeDocument === "notes"}
          streaming={activeDocument === "enhanced" && preview != null}
          onChange={activeDocument === "notes" ? handleEditorChange : undefined}
          onTranscriptLinkClick={
            activeDocument === "enhanced"
              ? handleTranscriptLinkClick
              : undefined
          }
          placeholder={
            activeDocument === "notes" ? "Add your notes…" : undefined
          }
        />
      </main>

      <TranscriptDock
        meetingId={meetingId}
        token={token}
        forceOpen={transcriptOpen}
        onOpenChange={setTranscriptOpen}
        segments={segments}
        recap={liveRecap ? "final" : undefined}
        initialView={stateShape.dockRecap ? "recap" : "transcript"}
      />

      <AskExperience meetingId={meetingId} token={token} />
    </div>
  );
}
