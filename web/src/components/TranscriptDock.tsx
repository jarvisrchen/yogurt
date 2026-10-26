import { useEffect, useRef, useState } from "react";
import {
  useTranscriptWs,
  useAudioLevels,
  storedSegmentToEvent,
  type StoredTranscriptSegment,
  type TranscriptEvent,
} from "../lib/ws";
import { TranscriptLine, scrollToTranscriptTime } from "./TranscriptLine";
import { AudioWaveIcon } from "./AudioWaveIcon";

const INK = "var(--color-ink)";
const GREY = "var(--color-grey)";
const LINE = "var(--color-line)";
const PAPER = "var(--color-card)";
const MATCHA = "var(--color-matcha)";

/**
 * Right-edge dock that overlays the notes column without reflowing it (the notes wrapper reserves `pr-7` for the tab gutter).
 * Live mode (no `segments`) subscribes via `useTranscriptWs`. Static mode renders the persisted `segments` with no WS.
 * Auto-scroll sticks to the bottom until the user scrolls more than 24px up, and resumes within 24px.
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
}

type DockPhase = "closed" | "open" | "closing";

export function TranscriptDock({
  meetingId,
  token,
  forceOpen,
  onOpenChange,
  segments,
  history,
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

  const stickyRef = useRef(true);
  const listRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (stickyRef.current && listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [events]);

  function handleScroll(e: React.UIEvent<HTMLDivElement>) {
    const el = e.currentTarget;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
    stickyRef.current = atBottom;
  }

  // Scroll to the line closest to `ts` (seconds). Returns false while the panel is unmounted so the caller can retry.
  function tryScrollTo(ts: number): boolean {
    const root = listRef.current;
    if (!root) return false;
    return scrollToTranscriptTime(root, ts);
  }

  // A request arriving while the dock is closed (MeetingPost force-opens and dispatches in the same tick) finds no DOM nodes. Stash it and retry on mount.
  const pendingScrollRef = useRef<number | null>(null);

  useEffect(() => {
    function onScrollTo(ev: Event) {
      const detail = (ev as CustomEvent<{ ts?: number }>).detail;
      if (!detail || typeof detail.ts !== "number") return;
      // Disable sticky so the new programmatic scroll position survives the
      // next render.
      stickyRef.current = false;
      if (!tryScrollTo(detail.ts)) {
        pendingScrollRef.current = detail.ts;
      }
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // rAF gives one paint so scrollIntoView has real layout.
  useEffect(() => {
    if (!mounted || pendingScrollRef.current === null) return;
    const ts = pendingScrollRef.current;
    const id = requestAnimationFrame(() => {
      if (tryScrollTo(ts)) pendingScrollRef.current = null;
    });
    return () => cancelAnimationFrame(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mounted]);

  return (
    <div
      className="fixed right-0 top-0 h-full z-30 pointer-events-none"
      data-testid="transcript-dock"
    >
      <div className="relative h-full flex pointer-events-auto">
        {/* Collapsed tab, always rendered. Auto width/height with overflow hidden keeps the vertical-rl label from protruding past the pill. */}
        <button
          type="button"
          aria-label={open ? `Hide ${label.toLowerCase()}` : `Show ${label.toLowerCase()}`}
          onClick={() => setOpen((v) => !v)}
          className="absolute flex flex-col items-center justify-center gap-1.5 w-7 text-[12px] font-semibold bg-card"
          style={{
            top: "22px",
            right: open ? "330px" : "0",
            padding: "12px 4px",
            overflow: "hidden",
            color: INK,
            border: `1px solid ${LINE}`,
            borderRight: "none",
            borderRadius: "11px 0 0 11px",
            boxShadow: "-8px 8px 22px rgba(40,30,15,.08)",
            transition: "right 340ms cubic-bezier(.2, .7, .2, 1)",
            zIndex: 31,
          }}
        >
          {!isStatic && (
            <AudioWaveIcon mic={audioLevels.mic} system={audioLevels.system} />
          )}
          <span style={{ writingMode: "vertical-rl", whiteSpace: "nowrap" }}>
            {open ? "▶ Transcript" : "◀ Transcript"}
          </span>
        </button>

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
                padding: "22px 20px 16px",
                borderBottom: `1px solid ${LINE}`,
              }}
            >
              <span
                className="text-[13px] font-semibold inline-flex items-center gap-2"
                style={{ color: INK }}
              >
                {label}
              </span>
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

            <div
              ref={listRef}
              onScroll={handleScroll}
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
          </aside>
        )}
      </div>
    </div>
  );
}
