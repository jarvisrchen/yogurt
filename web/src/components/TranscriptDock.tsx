import { useEffect, useRef, useState } from "react";
import { usePointerDrag } from "../lib/usePointerDrag";
import {
  useTranscriptWs,
  useAudioLevels,
  useLiveRecap,
  storedSegmentToEvent,
  type StoredTranscriptSegment,
  type TranscriptEvent,
} from "../lib/ws";
import { TranscriptLine, scrollToTranscriptTime } from "./TranscriptLine";
import { AudioWaveIcon } from "./AudioWaveIcon";
import { RecapBullets, RecapFooter, recapActions, useStickyBottom, type RecapMode } from "./Recap";
import { DockTextSize } from "./DockTextSize";
import {
  DOCK_DEFAULT,
  DOCK_MAX,
  DOCK_MIN,
  clampDockWidth,
  dockLayout,
  loadDockText,
  loadDockWidth,
  saveDockText,
  saveDockWidth,
} from "../lib/dockLayout";

const INK = "var(--color-ink)";
const GREY = "var(--color-grey)";
const LINE = "var(--color-line)";
const PAPER = "var(--color-card)";
const MATCHA = "var(--color-matcha)";
const BLUE = "var(--color-blue)";

export type DockView = "recap" | "transcript";

/**
 * Right-edge dock. While open it sets `--dock-pad`, which the page's `.dock-gutter` wrapper uses to keep the notes clear of it (see `dockLayout`),
 * `--dock-clear`, its width plus the tabs, which `.dock-clear` full-width rows use to end at its edge, and `--dock-vw`, the window width
 * without a classic scrollbar, which the CSS uses in place of 100vw.
 * Its width (left-edge drag) and list text size persist in localStorage.
 * Live mode (no `segments`) subscribes via `useTranscriptWs`. Static mode renders the persisted `segments` with no WS.
 * Auto-scroll sticks to the bottom until the user scrolls more than 24px up, and resumes within 24px.
 * With `recap` set it gains a Recap view and a second edge tab. New bullets never open it, and a dot marks them instead.
 * A `yogurt:transcript:scrollTo` event opens it on Transcript at that moment.
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
  /** One mic, as a meeting recorded on a phone: lines show with no speaker. */
  single?: boolean;
}

type DockPhase = "closed" | "open" | "closing";

// Excludes a classic scrollbar, as the fixed dock and the page's content box do. jsdom reports 0, hence the fallback.
const viewportWidth = () => document.documentElement.clientWidth || window.innerWidth;

function useWindowWidth(): number {
  const [w, setW] = useState(viewportWidth);
  useEffect(() => {
    const onResize = () => setW(viewportWidth());
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return w;
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
  single,
}: TranscriptDockProps) {
  const [openLocal, setOpenLocal] = useState(false);
  const open = forceOpen ?? openLocal;
  const setOpen = (next: boolean) => {
    setOpenLocal(next);
    onOpenChange?.(next);
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

  const windowWidth = useWindowWidth();
  const [savedWidth, setSavedWidth] = useState(loadDockWidth);
  const [textSize, setTextSize] = useState(loadDockText);
  const { dragging, start: startDrag } = usePointerDrag("dock-dragging");
  const width = clampDockWidth(savedWidth, windowWidth);
  const { pad, clear, overlaps } = dockLayout(width, windowWidth);

  useEffect(() => {
    if (!open) return;
    const root = document.documentElement.style;
    const vars = { "--dock-pad": pad, "--dock-clear": clear, "--dock-vw": windowWidth };
    for (const [k, v] of Object.entries(vars)) root.setProperty(k, `${v}px`);
    return () => {
      for (const k of Object.keys(vars)) root.removeProperty(k);
    };
  }, [open, pad, clear, windowWidth]);

  // The page eases along only while the dock opens or closes, so a drag or a window resize tracks it exactly.
  useEffect(() => {
    const root = document.documentElement.classList;
    root.add("dock-animating");
    const id = setTimeout(() => root.remove("dock-animating"), 400);
    return () => {
      clearTimeout(id);
      root.remove("dock-animating");
    };
  }, [open]);

  function setWidth(px: number) {
    setSavedWidth(px);
    saveDockWidth(px);
  }

  function onHandlePointerDown(e: React.PointerEvent) {
    const startWidth = width;
    let last = width;
    startDrag(
      e,
      (dx) => {
        last = clampDockWidth(startWidth - dx, viewportWidth());
        setSavedWidth(last);
      },
      // A click without a move must not overwrite a saved width that a small window is clamping.
      (moved) => moved && saveDockWidth(last),
    );
  }

  function onHandleKeyDown(e: React.KeyboardEvent) {
    const steps: Record<string, number> = {
      ArrowLeft: width + 20,
      ArrowRight: width - 20,
      Home: DOCK_MIN,
      End: DOCK_MAX,
    };
    const next = steps[e.key];
    if (next === undefined) return;
    e.preventDefault();
    setWidth(clampDockWidth(next, windowWidth));
  }

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

  // A request arriving while the dock is closed finds no DOM nodes, since the open lands on the next render. Stash it and retry on mount.
  const pendingScrollRef = useRef<number | null>(null);

  function scrollToSec(ts: number) {
    setOpen(true);
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
        {hasRecap && (
          <EdgeTab
            top={22}
            open={open}
            right={width}
            dragging={dragging}
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
        )}
        <EdgeTab
          top={hasRecap ? 112 : 22}
          open={open}
          right={width}
          dragging={dragging}
          active={view === "transcript"}
          plain={!hasRecap}
          name="Transcript"
          ariaName={label.toLowerCase()}
          onClick={() => toggleTab("transcript")}
        >
          {wave}
        </EdgeTab>

        {/* Mounted while phase !== "closed" so the slide-out keyframe plays before unmount. */}
        {mounted && (
          <aside
            className={`${phase === "closing" ? "dock-closed" : "dock-open"} relative h-full flex flex-col`}
            data-testid="transcript-dock-panel"
            style={{
              width,
              backgroundColor: PAPER,
              borderLeft: `1px solid ${LINE}`,
              // A layer over the notes rather than notes cut off.
              boxShadow: overlaps ? "-24px 0 48px -8px var(--shadow-window-color)" : undefined,
            }}
            onAnimationEnd={() => {
              if (phase === "closing") setPhase("closed");
            }}
          >
            <div
              role="separator"
              aria-orientation="vertical"
              aria-label="Resize transcript panel"
              aria-valuemin={DOCK_MIN}
              aria-valuemax={clampDockWidth(DOCK_MAX, windowWidth)}
              aria-valuenow={width}
              tabIndex={0}
              title="Drag to resize, double-click to reset"
              data-testid="dock-resize-handle"
              className={`dock-handle ${dragging ? "is-dragging" : ""}`}
              onPointerDown={onHandlePointerDown}
              onDoubleClick={() => setWidth(DOCK_DEFAULT)}
              onKeyDown={onHandleKeyDown}
            />
            <header
              className="flex items-center justify-between gap-2"
              style={{
                padding: hasRecap ? "18px 12px 14px 14px" : "18px 12px 12px 20px",
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
              <div className="flex items-center gap-1.5">
                {!isStatic && (
                  <span
                    className="text-[11px] inline-flex items-center gap-1 whitespace-nowrap"
                    style={{ color: connected ? MATCHA : GREY }}
                  >
                    <span aria-hidden="true">{connected ? "●" : "○"}</span>
                    {connected ? "connected" : "offline"}
                  </span>
                )}
                <DockTextSize
                  value={textSize}
                  onChange={(px) => {
                    setTextSize(px);
                    saveDockText(px);
                  }}
                />
              </div>
            </header>

            {view === "recap" ? (
              <>
                <RecapBullets
                  listRef={recapList.ref}
                  onScroll={recapList.onScroll}
                  mode={recap!}
                  state={recapState}
                  onJump={(ms) => scrollToSec(Math.floor(ms / 1000))}
                  className="flex-1"
                  style={{ padding: "10px 20px", fontSize: textSize }}
                />
                <RecapFooter mode={recap!} state={recapState} {...recapActions(meetingId, setRecap)} />
              </>
            ) : (
              <div
                ref={transcriptList.ref}
                onScroll={transcriptList.onScroll}
                data-testid="transcript-list"
                className="flex-1 overflow-y-auto"
                style={{ padding: "12px 20px", fontSize: textSize }}
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
                    <TranscriptLine key={`${ev.channel}-${ev.ts_ms}-${i}`} ev={ev} single={single} />
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
  /** The open dock's width, which the tab sits just left of. */
  right: number;
  dragging: boolean;
  active: boolean;
  /** The lone Transcript tab keeps today's ink label rather than turning blue when open. */
  plain?: boolean;
  name: string;
  ariaName: string;
  onClick: () => void;
  children?: React.ReactNode;
}

function EdgeTab({ top, open, right, dragging, active, plain, name, ariaName, onClick, children }: EdgeTabProps) {
  const showing = open && active;
  return (
    <button
      type="button"
      aria-label={showing ? `Hide ${ariaName}` : `Show ${ariaName}`}
      onClick={onClick}
      className="absolute flex flex-col items-center justify-center gap-1.5 w-7 text-[12px] font-semibold bg-card"
      style={{
        top: `${top}px`,
        right: open ? `${right}px` : "0",
        padding: "12px 4px",
        overflow: "hidden",
        color: showing && !plain ? BLUE : INK,
        border: `1px solid ${LINE}`,
        borderRight: "none",
        borderRadius: "11px 0 0 11px",
        boxShadow: "-8px 8px 22px rgba(40,30,15,.08)",
        // The tab tracks the pointer while dragging instead of easing behind it.
        transition: dragging ? "none" : "right 340ms cubic-bezier(.2, .7, .2, 1)",
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
