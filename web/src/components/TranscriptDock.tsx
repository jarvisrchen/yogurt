import { useEffect, useRef, useState } from "react";
import { useTranscriptWs } from "../lib/ws";
import { TranscriptLine } from "./TranscriptLine";

const INK = "#211D18";
const GREY = "#A89F90";
const LINE = "#EBE3D5";
const PAPER = "#FCFAF5"; // Panel surface — slightly warmer than app paper (D-14).
const MATCHA = "#5E9E73";

/**
 * Right-edge collapsible Live Transcript dock.
 *
 * Layout (Phase 3 D-13 / D-14 / D-15):
 *   - Fixed to the right edge, full-height, z-30 — overlays the notes column
 *     without reflowing it (the notes wrapper reserves `pr-7` for the tab
 *     gutter).
 *   - Collapsed state: vertical "Live transcript" tab, always rendered so
 *     the user can re-collapse the panel from outside.
 *   - Open state: 330px-wide panel that slides in via the `.dock-open`
 *     keyframe (340ms cubic-bezier(.2,.7,.2,1) — TRANS-04 + PRD §16.5).
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
}

export function TranscriptDock({
  meetingId,
  token,
  forceOpen,
  onOpenChange,
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
  const { events, connected } = useTranscriptWs(meetingId, token);

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

  // NOTES-11: listen for `yogurt:transcript:scrollTo` and scroll the list
  // to the line whose `ts_ms / 1000` matches the requested `ts` (in
  // seconds — that's the unit AI bullets carry via the merge step).
  // Finds the closest transcript line by absolute distance — exact matches
  // are common but the AI's word-overlap heuristic (D-09) can land on any
  // segment, and the user's expectation is "scroll to roughly that moment".
  useEffect(() => {
    function onScrollTo(ev: Event) {
      const detail = (ev as CustomEvent<{ ts?: number }>).detail;
      if (!detail || typeof detail.ts !== "number") return;
      // Disable sticky so the new programmatic scroll position survives the
      // next render.
      stickyRef.current = false;

      const root = listRef.current;
      if (!root) return;
      const lines = Array.from(
        root.querySelectorAll<HTMLElement>("[data-transcript-ts-sec]"),
      );
      if (lines.length === 0) return;
      let bestEl: HTMLElement | null = null;
      let bestDist = Number.POSITIVE_INFINITY;
      for (const line of lines) {
        const raw = line.getAttribute("data-transcript-ts-sec");
        const sec = Number(raw);
        if (!Number.isFinite(sec)) continue;
        const dist = Math.abs(sec - detail.ts);
        if (dist < bestDist) {
          bestDist = dist;
          bestEl = line;
        }
      }
      bestEl?.scrollIntoView({ behavior: "smooth", block: "center" });
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

  return (
    <div
      className="fixed right-0 top-0 h-full z-30 pointer-events-none"
      data-testid="transcript-dock"
    >
      <div className="relative h-full flex pointer-events-auto">
        {/* Collapsed tab — always rendered. Click toggles `open`. */}
        <button
          type="button"
          aria-label={open ? "Hide live transcript" : "Show live transcript"}
          onClick={() => setOpen((v) => !v)}
          className="absolute h-24 w-7 flex items-center justify-center text-[12px] font-semibold bg-white"
          style={{
            top: "22px",
            right: open ? "330px" : "0",
            writingMode: "vertical-rl",
            color: INK,
            border: `1px solid ${LINE}`,
            borderRight: "none",
            borderRadius: "11px 0 0 11px",
            boxShadow: "-8px 8px 22px rgba(40,30,15,.08)",
            transition: "right 340ms cubic-bezier(.2, .7, .2, 1)",
            zIndex: 31,
          }}
        >
          {open ? "▶ Live transcript" : "◀ Live transcript"}
        </button>

        {/* Sliding panel — only mounted when open === true so the keyframe
         * re-runs on every reopen (D-14). */}
        {open && (
          <aside
            className="dock-open w-[330px] h-full flex flex-col"
            data-testid="transcript-dock-panel"
            style={{
              backgroundColor: PAPER,
              borderLeft: `1px solid ${LINE}`,
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
                className="text-[13px] font-semibold"
                style={{ color: INK }}
              >
                Live transcript
              </span>
              <span
                className="text-[11px] inline-flex items-center gap-1"
                style={{ color: connected ? MATCHA : GREY }}
              >
                <span aria-hidden="true">{connected ? "●" : "○"}</span>
                {connected ? "connected" : "offline"}
              </span>
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
                  Waiting for audio…
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
