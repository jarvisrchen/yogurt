import { useEffect, useRef, useState } from "react";
import {
  useTranscriptWs,
  useAudioLevels,
  storedSegmentToEvent,
  type StoredTranscriptSegment,
  type TranscriptEvent,
} from "../lib/ws";
import { TranscriptLine } from "./TranscriptLine";
import { AudioWaveIcon } from "./AudioWaveIcon";

const INK = "#211D18";
const GREY = "#A89F90";
const LINE = "#EBE3D5";
const PAPER = "#FCFAF5"; // Panel surface — slightly warmer than app paper (D-14).
const MATCHA = "#5E9E73";

/**
 * Right-edge collapsible transcript dock.
 *
 * Layout (Phase 3 D-13 / D-14 / D-15):
 *   - Fixed to the right edge, full-height, z-30 — overlays the notes column
 *     without reflowing it (the notes wrapper reserves `pr-7` for the tab
 *     gutter).
 *   - Collapsed state: vertical tab, always rendered so the user can
 *     re-collapse the panel from outside.
 *   - Open state: 330px-wide panel that slides in via the `.dock-open`
 *     keyframe (340ms cubic-bezier(.2,.7,.2,1) — TRANS-04 + PRD §16.5) and
 *     slides out via `.dock-closed` before unmounting (task NOTES-14 —
 *     symmetric open/close motion).
 *
 * Two modes:
 *   - Live (default): `segments` is omitted. Subscribes to the per-meeting
 *     WS via `useTranscriptWs` — connection chip + "Live transcript" label.
 *   - Static (MeetingPost task 9): caller passes the meeting's persisted
 *     `transcript_json` rows. No WS connection, no connection chip, label
 *     reads "Transcript", empty state differs.
 *
 * Auto-scroll (Phase 3 D-17 / TRANS-06):
 *   - `stickyRef` starts true; auto-scrolls to bottom on each new event.
 *   - User scrolling up beyond 24px from the bottom flips it false;
 *     returning to within 24px flips it back. New events arriving while
 *     paused preserve the user's read position.
 */
export interface TranscriptDockProps {
  meetingId: string | null;
  // WR-06: session token threaded from Meeting.tsx (fetched via ensureSessionToken).
  // The WS hook waits until token is non-null before connecting; passing null is
  // valid during the brief bootstrap window before /api/session-token resolves.
  token: string | null;
  /**
   * Phase 4 NOTES-11: when the editor's `↳ HH:MM` link is clicked, the
   * post-meeting route flips this prop to `true` to force-open the dock
   * even if the user had collapsed it. `onOpenChange` keeps the caller's
   * mirror in sync when the user clicks the tab themselves.
   */
  forceOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  /**
   * Static mode (MeetingPost task 9): pre-recorded segments parsed from
   * the meeting row's `transcript_json`. When provided, the dock renders
   * these instead of subscribing to the live WS — no connection chip, no
   * "Waiting for audio…" (the meeting is over, not live).
   */
  segments?: StoredTranscriptSegment[];
}

type DockPhase = "closed" | "open" | "closing";

export function TranscriptDock({
  meetingId,
  token,
  forceOpen,
  onOpenChange,
  segments,
}: TranscriptDockProps) {
  const [openLocal, setOpenLocal] = useState(false);
  // `open` is controlled by `forceOpen` if provided, else local state.
  const open = forceOpen ?? openLocal;
  const setOpen = (next: boolean | ((prev: boolean) => boolean)) => {
    setOpenLocal((prev) => {
      const value = typeof next === "function" ? next(prev) : next;
      onOpenChange?.(value);
      return value;
    });
  };

  const isStatic = segments !== undefined;
  // In static mode the live WS is never opened (`useTranscriptWs` is a
  // no-op when meetingId/token are null) — the meeting is over, there's
  // nothing to subscribe to.
  const { events: liveEvents, connected } = useTranscriptWs(
    isStatic ? null : meetingId,
    isStatic ? null : token,
  );
  const events: TranscriptEvent[] = isStatic
    ? segments!.map(storedSegmentToEvent)
    : liveEvents;
  const label = isStatic ? "Transcript" : "Live transcript";

  // Real amplitude wave (Feature A) — only subscribed in live mode; static
  // playback has no audio on the wire.
  const audioLevels = useAudioLevels(
    isStatic ? null : meetingId,
    isStatic ? null : token,
  );

  // Task NOTES-14: symmetric open/close animation. `phase` lags `open` by
  // one animation — closing plays `.dock-closed` (slideOutRight) and only
  // unmounts once `onAnimationEnd` fires, instead of the old hard cut.
  const [phase, setPhase] = useState<DockPhase>("closed");
  useEffect(() => {
    if (open) {
      setPhase("open");
    } else {
      setPhase((prev) => (prev === "closed" ? "closed" : "closing"));
    }
  }, [open]);
  const mounted = phase !== "closed";

  // Sticky auto-scroll ref. `true` = follow new events, `false` = user
  // has scrolled up and is reading history.
  const stickyRef = useRef(true);
  const listRef = useRef<HTMLDivElement | null>(null);

  // After every events change: if we're sticky and the list is mounted,
  // pin to bottom. Runs after layout so scrollHeight is up to date.
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

  // Task NOTES-10 (MeetingPost deep links): find the transcript line
  // closest to `ts` (seconds) and scroll to it with a brief highlight.
  // Returns false if the panel isn't mounted yet (caller retries once
  // `mounted` flips true — see the pending-scroll effect below).
  function tryScrollTo(ts: number): boolean {
    const root = listRef.current;
    if (!root) return false;
    const lines = Array.from(
      root.querySelectorAll<HTMLElement>("[data-transcript-ts-sec]"),
    );
    if (lines.length === 0) return false;
    let bestEl: HTMLElement | null = null;
    let bestDist = Number.POSITIVE_INFINITY;
    for (const line of lines) {
      const raw = line.getAttribute("data-transcript-ts-sec");
      const sec = Number(raw);
      if (!Number.isFinite(sec)) continue;
      const dist = Math.abs(sec - ts);
      if (dist < bestDist) {
        bestDist = dist;
        bestEl = line;
      }
    }
    if (!bestEl) return false;
    bestEl.scrollIntoView({ behavior: "smooth", block: "center" });
    // Brief 1.5s bg tint (CSS keyframe in index.css) so the user's eye
    // lands on the right line even mid-smooth-scroll.
    bestEl.classList.add("transcript-highlight");
    const el = bestEl;
    setTimeout(() => el.classList.remove("transcript-highlight"), 1500);
    return true;
  }

  // A scrollTo request that arrives while the dock is still closed (the
  // common case — MeetingPost force-opens the dock and dispatches the
  // event in the same tick) can't find any DOM nodes yet. Stash it and
  // retry once the panel mounts.
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

  // Flush a pending scroll request once the panel is actually in the DOM.
  // rAF gives the browser one paint so `scrollIntoView` has real layout.
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
        {/* Collapsed tab — always rendered. Click toggles `open`. Sized to
         * fit its own content (no fixed h-24 — "Transcript" at 12px in
         * vertical-rl needs ~110px, the old fixed 96px let letters
         * protrude past the pill). `overflow: hidden` + auto width/height
         * from padding is the actual fix; the shortened label + AudioWaveIcon
         * (live only) keep the pill compact. */}
        <button
          type="button"
          aria-label={open ? `Hide ${label.toLowerCase()}` : `Show ${label.toLowerCase()}`}
          onClick={() => setOpen((v) => !v)}
          className="absolute flex flex-col items-center justify-center gap-1.5 w-7 text-[12px] font-semibold bg-white"
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

        {/* Sliding panel — mounted while `phase !== "closed"` so the
         * slide-out keyframe gets to play before unmount (NOTES-14). */}
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
