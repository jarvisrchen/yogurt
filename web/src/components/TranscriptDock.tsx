import { useEffect, useRef, useState } from "react";
import { Pause, Play, RefreshCw } from "lucide-react";
import {
  useTranscriptWs,
  useAudioLevels,
  useLiveRecap,
  storedSegmentToEvent,
  type RecapState,
  type StoredTranscriptSegment,
  type TranscriptEvent,
} from "../lib/ws";
import { refreshRecap, setRecapPaused } from "../lib/api/meetings";
import { TranscriptLine, scrollToTranscriptTime } from "./TranscriptLine";
import { AudioWaveIcon } from "./AudioWaveIcon";
import { Button } from "./Button";

const INK = "var(--color-ink)";
const GREY = "var(--color-grey)";
const LINE = "var(--color-line)";
const PAPER = "var(--color-card)";
const MATCHA = "var(--color-matcha)";
const BLUE = "var(--color-blue)";

export type DockView = "recap" | "transcript";

/**
 * `recording` offers Update now and Pause, `stopped` only Pause, and `final` (the post-meeting page
 * while the summary generates) neither, showing the view only while bullets exist.
 */
export type RecapMode = "recording" | "stopped" | "final";

/**
 * Right-edge dock that overlays the notes column without reflowing it (the notes wrapper reserves `pr-7` for the tab gutter).
 * Live mode (no `segments`) subscribes via `useTranscriptWs`. Static mode renders the persisted `segments` with no WS.
 * Auto-scroll sticks to the bottom until the user scrolls more than 24px up, and resumes within 24px.
 * With `recap` set it gains a Recap view and a second edge tab. It never opens itself; a dot marks unseen bullets.
 */
export interface TranscriptDockProps {
  meetingId: string | null;
  // The WS hook waits for a non-null token; null is valid while /api/session-token resolves.
  token: string | null;
  /** Force-opens the dock (editor timestamp links). `onOpenChange` keeps the caller in sync with tab clicks. */
  forceOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** Static mode: renders these persisted segments instead of subscribing to the live WS. */
  segments?: StoredTranscriptSegment[];
  /** Seeds live-mode history (once per meetingId) without switching to static mode. The WS still connects and appends. */
  history?: TranscriptEvent[];
  recap?: RecapMode;
  initialView?: DockView;
  onViewChange?: (view: DockView) => void;
}

type DockPhase = "closed" | "open" | "closing";

/** Sticks a scroll container to the bottom as `deps` change, until the user scrolls more than 24px up. */
function useStickyBottom(deps: unknown[]) {
  const ref = useRef<HTMLDivElement | null>(null);
  const sticky = useRef(true);
  useEffect(() => {
    if (sticky.current && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  const onScroll = (e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    sticky.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  };
  return { ref, sticky, onScroll };
}

export function TranscriptDock({
  meetingId,
  token,
  forceOpen,
  onOpenChange,
  segments,
  history,
  recap,
  initialView = "transcript",
  onViewChange,
}: TranscriptDockProps) {
  const [openLocal, setOpenLocal] = useState(false);
  const open = forceOpen ?? openLocal;
  const setOpen = (next: boolean | ((prev: boolean) => boolean)) => {
    setOpenLocal((prev) => {
      const value = typeof next === "function" ? next(prev) : next;
      onOpenChange?.(value);
      return value;
    });
  };

  const isStatic = segments !== undefined;
  // Static mode never opens the WS: useTranscriptWs is a no-op when meetingId/token are null.
  const { events: liveEvents, connected } = useTranscriptWs(
    isStatic ? null : meetingId,
    isStatic ? null : token,
    isStatic ? undefined : history,
  );
  const events: TranscriptEvent[] = isStatic
    ? segments!.map(storedSegmentToEvent)
    : liveEvents;
  const label = isStatic ? "Transcript" : "Live transcript";

  // Only subscribed in live mode; static playback has no audio on the wire.
  const audioLevels = useAudioLevels(
    isStatic ? null : meetingId,
    isStatic ? null : token,
  );

  const { recap: recapState, setRecap } = useLiveRecap(
    recap ? meetingId : null,
    recap ? token : null,
  );
  const bulletCount = recapState?.bullets.length ?? 0;
  const hasRecap = recap !== undefined && (recap !== "final" || bulletCount > 0);
  const [viewLocal, setViewLocal] = useState<DockView>(initialView);
  const view: DockView = hasRecap ? viewLocal : "transcript";
  const setView = (v: DockView) => {
    setViewLocal(v);
    onViewChange?.(v);
  };

  // `phase` lags `open` by one animation so `.dock-closed` plays before unmount.
  const [phase, setPhase] = useState<DockPhase>("closed");
  useEffect(() => {
    if (open) {
      setPhase("open");
    } else {
      setPhase((prev) => (prev === "closed" ? "closed" : "closing"));
    }
  }, [open]);
  const mounted = phase !== "closed";

  // The summary replaced the recap: close a dock showing it, so the summary is in view.
  const hadRecapRef = useRef(hasRecap);
  useEffect(() => {
    if (hadRecapRef.current && !hasRecap && open && viewLocal === "recap") setOpen(false);
    hadRecapRef.current = hasRecap;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasRecap]);

  // Bullets already there on load count as seen.
  const viewingRecap = open && view === "recap";
  const [seenCount, setSeenCount] = useState<number | null>(null);
  const recapLoaded = recapState !== null;
  useEffect(() => {
    if (!recapLoaded) return;
    setSeenCount((seen) =>
      seen === null || viewingRecap || bulletCount < seen ? bulletCount : seen,
    );
  }, [recapLoaded, bulletCount, viewingRecap]);
  const unseen = seenCount !== null && bulletCount > seenCount && !viewingRecap;

  const transcriptList = useStickyBottom([events, view]);
  const recapList = useStickyBottom([recapState, view]);

  // Scroll to the line closest to `ts` (seconds). Returns false while the list is unmounted so the caller can retry.
  function tryScrollTo(ts: number): boolean {
    const root = transcriptList.ref.current;
    if (!root) return false;
    return scrollToTranscriptTime(root, ts);
  }

  // A request arriving while the dock is closed (MeetingPost force-opens and dispatches in the same tick) finds no DOM nodes. Stash it and retry on mount.
  const pendingScrollRef = useRef<number | null>(null);

  function scrollToSec(ts: number) {
    // Disable sticky so the new programmatic scroll position survives the next render.
    transcriptList.sticky.current = false;
    if (view !== "transcript") {
      setView("transcript");
      pendingScrollRef.current = ts;
    } else if (!tryScrollTo(ts)) {
      pendingScrollRef.current = ts;
    }
  }
  const scrollToSecRef = useRef(scrollToSec);
  scrollToSecRef.current = scrollToSec;

  useEffect(() => {
    function onScrollTo(ev: Event) {
      const detail = (ev as CustomEvent<{ ts?: number }>).detail;
      if (!detail || typeof detail.ts !== "number") return;
      scrollToSecRef.current(detail.ts);
    }
    window.addEventListener(
      "yogurt:transcript:scrollTo",
      onScrollTo as EventListener,
    );
    return () => {
      window.removeEventListener(
        "yogurt:transcript:scrollTo",
        onScrollTo as EventListener,
      );
    };
  }, []);

  // rAF gives one paint so scrollIntoView has real layout.
  useEffect(() => {
    if (!mounted || view !== "transcript" || pendingScrollRef.current === null) return;
    const ts = pendingScrollRef.current;
    const id = requestAnimationFrame(() => {
      if (tryScrollTo(ts)) pendingScrollRef.current = null;
    });
    return () => cancelAnimationFrame(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mounted, view]);

  function toggleTab(v: DockView) {
    if (open && view === v) {
      setOpen(false);
    } else {
      setView(v);
      setOpen(true);
    }
  }

  const wave = !isStatic && (
    <AudioWaveIcon mic={audioLevels.mic} system={audioLevels.system} />
  );

  return (
    <div
      className="fixed right-0 top-0 h-full z-30 pointer-events-none"
      data-testid="transcript-dock"
    >
      <div className="relative h-full flex pointer-events-auto">
        {/* Collapsed tabs, always rendered. Auto width/height with overflow hidden keeps the vertical-rl label from protruding past the pill. */}
        {hasRecap ? (
          <>
            <EdgeTab
              top={22}
              open={open}
              active={view === "recap"}
              name="Recap"
              ariaName="recap"
              onClick={() => toggleTab("recap")}
            >
              {unseen && (
                <span
                  data-testid="recap-unseen-dot"
                  className="block w-2 h-2 rounded-full bg-blue"
                />
              )}
            </EdgeTab>
            <EdgeTab
              top={112}
              open={open}
              active={view === "transcript"}
              name="Transcript"
              ariaName={label.toLowerCase()}
              onClick={() => toggleTab("transcript")}
            >
              {wave}
            </EdgeTab>
          </>
        ) : (
          <EdgeTab
            top={22}
            open={open}
            active
            plain
            name="Transcript"
            ariaName={label.toLowerCase()}
            onClick={() => setOpen((v) => !v)}
          >
            {wave}
          </EdgeTab>
        )}

        {/* Mounted while phase !== "closed" so the slide-out keyframe plays before unmount. */}
        {mounted && (
          <aside
            className={`${phase === "closing" ? "dock-closed" : "dock-open"} w-[330px] h-full flex flex-col`}
            data-testid="transcript-dock-panel"
            style={{
              backgroundColor: PAPER,
              borderLeft: `1px solid ${LINE}`,
            }}
            onAnimationEnd={() => {
              if (phase === "closing") setPhase("closed");
            }}
          >
            <header
              className="flex items-center justify-between"
              style={{
                padding: hasRecap ? "18px 16px 14px 14px" : "22px 20px 16px",
                borderBottom: `1px solid ${LINE}`,
              }}
            >
              {hasRecap ? (
                <div role="tablist" aria-label="Dock view" className="flex items-center gap-1">
                  {(["recap", "transcript"] as const).map((v) => (
                    <button
                      key={v}
                      type="button"
                      role="tab"
                      aria-selected={view === v}
                      onClick={() => setView(v)}
                      className={`px-3 py-1 rounded-pill text-[12.5px] font-semibold ${
                        view === v ? "bg-blsoft text-blue" : "text-mut hover:text-ink"
                      }`}
                    >
                      {v === "recap" ? "Recap" : "Transcript"}
                    </button>
                  ))}
                </div>
              ) : (
                <span
                  className="text-[13px] font-semibold inline-flex items-center gap-2"
                  style={{ color: INK }}
                >
                  {label}
                </span>
              )}
              {!isStatic && (
                <span
                  className="text-[11px] inline-flex items-center gap-1"
                  style={{ color: connected ? MATCHA : GREY }}
                >
                  <span aria-hidden="true">{connected ? "●" : "○"}</span>
                  {connected ? "connected" : "offline"}
                </span>
              )}
            </header>

            {view === "recap" ? (
              <RecapView
                listRef={recapList.ref}
                onScroll={recapList.onScroll}
                mode={recap!}
                state={recapState}
                onJump={(ms) => scrollToSec(Math.floor(ms / 1000))}
                onRefresh={() => {
                  if (meetingId) refreshRecap(meetingId).catch(() => {});
                }}
                onPause={(paused) => {
                  if (meetingId) setRecapPaused(meetingId, paused).then(setRecap, () => {});
                }}
              />
            ) : (
              <div
                ref={transcriptList.ref}
                onScroll={transcriptList.onScroll}
                data-testid="transcript-list"
                className="flex-1 overflow-y-auto"
                style={{ padding: "12px 20px" }}
              >
                {events.length === 0 ? (
                  <p
                    className="text-[13px] mt-2"
                    style={{ color: GREY }}
                  >
                    {isStatic
                      ? "No transcript was captured for this meeting."
                      : "Waiting for audio…"}
                  </p>
                ) : (
                  events.map((ev, i) => (
                    <TranscriptLine key={`${ev.channel}-${ev.ts_ms}-${i}`} ev={ev} />
                  ))
                )}
              </div>
            )}
          </aside>
        )}
      </div>
    </div>
  );
}

interface EdgeTabProps {
  top: number;
  open: boolean;
  active: boolean;
  /** The lone Transcript tab keeps today's ink label rather than turning blue when open. */
  plain?: boolean;
  name: string;
  ariaName: string;
  onClick: () => void;
  children?: React.ReactNode;
}

function EdgeTab({ top, open, active, plain, name, ariaName, onClick, children }: EdgeTabProps) {
  const showing = open && active;
  return (
    <button
      type="button"
      aria-label={showing ? `Hide ${ariaName}` : `Show ${ariaName}`}
      onClick={onClick}
      className="absolute flex flex-col items-center justify-center gap-1.5 w-7 text-[12px] font-semibold bg-card"
      style={{
        top: `${top}px`,
        right: open ? "330px" : "0",
        padding: "12px 4px",
        overflow: "hidden",
        color: showing && !plain ? BLUE : INK,
        border: `1px solid ${LINE}`,
        borderRight: "none",
        borderRadius: "11px 0 0 11px",
        boxShadow: "-8px 8px 22px rgba(40,30,15,.08)",
        transition: "right 340ms cubic-bezier(.2, .7, .2, 1)",
        zIndex: 31,
      }}
    >
      {children}
      <span style={{ writingMode: "vertical-rl", whiteSpace: "nowrap" }}>
        {showing ? "▶" : "◀"} {name}
      </span>
    </button>
  );
}

/** `mm:ss`, or `h:mm:ss` past the first hour. */
function formatBulletTs(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mmss = `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return h > 0 ? `${h}:${mmss}` : mmss;
}

function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

function updatedAgo(updatedAt: number, now: number): string {
  const min = Math.floor((now - updatedAt) / 60_000);
  return min < 1 ? "Updated just now" : `Updated ${min} min ago`;
}

interface RecapViewProps {
  listRef: React.RefObject<HTMLDivElement | null>;
  onScroll: (e: React.UIEvent<HTMLDivElement>) => void;
  mode: RecapMode;
  state: RecapState | null;
  onJump: (ts_ms: number) => void;
  onRefresh: () => void;
  onPause: (paused: boolean) => void;
}

function RecapView({ listRef, onScroll, mode, state, onJump, onRefresh, onPause }: RecapViewProps) {
  const now = useNow(30_000);
  const bullets = state?.bullets ?? [];
  // Only bullets that arrive while the view is open pop in, not the whole list on every switch.
  const shownAtMount = useRef<number | null>(null);
  if (shownAtMount.current === null && state !== null) shownAtMount.current = bullets.length;
  const updating = state?.status === "updating";
  const paused = state?.paused ?? false;
  const failed = state?.status === "error" && !paused;
  const recording = mode === "recording";

  let left = "";
  if (paused) left = "Paused";
  else if (updating) left = "Updating…";
  else if (state?.updated_at != null) left = updatedAgo(state.updated_at, now);

  return (
    <>
      <div
        ref={listRef}
        onScroll={onScroll}
        data-testid="recap-list"
        className="flex-1 overflow-y-auto"
        style={{ padding: "10px 20px" }}
      >
        {bullets.map((b, i) => (
          <button
            key={`${b.ts_ms}-${i}`}
            type="button"
            onClick={() => onJump(b.ts_ms)}
            className={`group w-full text-left flex gap-3 py-2 rounded-chip -mx-2 px-2 hover:bg-paper ${
              i >= (shownAtMount.current ?? Infinity) ? "animate-pop-up" : ""
            }`}
          >
            <span className="shrink-0 w-[38px] pt-[2px] font-mono text-[12px] text-grey group-hover:text-blue">
              {formatBulletTs(b.ts_ms)}
            </span>
            <span className="text-[14px] leading-snug text-ink">{b.text}</span>
          </button>
        ))}
        {updating && (
          <div className="flex gap-3 py-2" aria-live="polite" aria-label="Updating the recap">
            <span className="shrink-0 w-[38px] pt-[5px]">
              <span className="block h-[9px] w-[30px] rounded-[6px] shimmer" />
            </span>
            <span className="flex-1 space-y-1.5 pt-[3px]">
              <span className="block h-[11px] w-[92%] rounded-[6px] shimmer" />
              <span className="block h-[11px] w-[60%] rounded-[6px] shimmer" />
            </span>
          </div>
        )}
        {bullets.length === 0 && !updating && !paused && (
          <p className="text-[13px] mt-2 text-grey">
            {state?.updated_at == null
              ? "The recap starts after about 3 minutes of conversation."
              : "Nothing worth a bullet yet."}
          </p>
        )}
        {paused && (
          <p className="mt-2 text-[12.5px] leading-snug text-mut">
            Paused for this meeting. Nothing said from here is sent until you resume.
          </p>
        )}
        {failed && state?.error && (
          <p
            role="alert"
            className="mt-2 text-[12.5px] leading-snug text-ink rounded-button bg-strsoft border border-straw/40 px-3 py-2"
          >
            {state.error.replace(/\.?$/, ".")}
            {recording && " Trying again at the next update."}
          </p>
        )}
      </div>
      {mode === "final" ? (
        <footer className="border-t border-line px-5 py-3 text-[12px] text-mut">
          Final recap · the summary replaces it when ready
        </footer>
      ) : (
        <footer className="flex items-center justify-between gap-2 border-t border-line pl-5 pr-3 py-3">
          <span className="text-[12px] text-mut whitespace-nowrap">{left}</span>
          <span className="flex items-center">
            {paused ? (
              <Button variant="secondary" size="sm" className="whitespace-nowrap" aria-label="Resume recap" onClick={() => onPause(false)}>
                <Play size={13} aria-hidden="true" />
                Resume
              </Button>
            ) : (
              <>
                {recording && !updating && (state?.updated_at != null || failed) && (
                  <Button variant="ghost" size="sm" className="whitespace-nowrap px-2.5!" aria-label="Update recap now" onClick={onRefresh}>
                    <RefreshCw size={13} aria-hidden="true" />
                    Update now
                  </Button>
                )}
                <Button variant="ghost" size="sm" className="whitespace-nowrap px-2.5!" aria-label="Pause recap" onClick={() => onPause(true)}>
                  <Pause size={13} aria-hidden="true" />
                  Pause
                </Button>
              </>
            )}
          </span>
        </footer>
      )}
    </>
  );
}
