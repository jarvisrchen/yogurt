import { useEffect, useId, useRef, useState } from "react";
import { ChevronDown, ChevronUp, Pause, Play, RefreshCw } from "lucide-react";
import { useLiveRecap, type RecapState } from "../lib/ws";
import { refreshRecap, setRecapPaused } from "../lib/api/meetings";
// `TranscriptLine` has its own `formatTs` (HH:MM:SS). This one writes `mm:ss` with minutes past 59, as the server and `ctl` do.
import { formatTs as mmss } from "../editor/html";
import { Button } from "./Button";
import { LABEL } from "./settings/labelClass";

/**
 * `recording` offers Update now and Pause, `stopped` only Pause, and `final` (the post-meeting page
 * while the summary generates) neither, showing the recap only while bullets exist.
 */
export type RecapMode = "recording" | "stopped" | "final";

/** Four one-line rows, the most one update adds. Longer bullets wrap and scroll under the top fade. */
const CARD_LIST_HEIGHT = 148;
const LEAVE_MS = 200;

/** Sticks a scroll container to the bottom as `deps` change, until the user scrolls more than 24px up. */
export function useStickyBottom(deps: unknown[]) {
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

function useNow(intervalMs: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}

function recapFlags(state: RecapState | null) {
  const paused = state?.paused ?? false;
  return { paused, updating: state?.status === "updating", failed: state?.status === "error" && !paused };
}

/** "Paused", "Updating…", "Updated 2 min ago", or "" before the first update. */
function useRecapStatus(state: RecapState | null): string {
  const now = useNow(30_000);
  const { paused, updating } = recapFlags(state);
  if (paused) return "Paused";
  if (updating) return "Updating…";
  if (state?.updated_at == null) return "";
  const min = Math.floor((now - state.updated_at) / 60_000);
  return min < 1 ? "Updated just now" : `Updated ${min} min ago`;
}

export function recapActions(meetingId: string | null, setRecap: (s: RecapState) => void) {
  return {
    onRefresh: () => {
      if (meetingId) refreshRecap(meetingId).catch(() => {});
    },
    onPause: (paused: boolean) => {
      if (meetingId) setRecapPaused(meetingId, paused).then(setRecap, () => {});
    },
  };
}

interface RecapBulletsProps {
  listRef: React.RefObject<HTMLDivElement | null>;
  onScroll: (e: React.UIEvent<HTMLDivElement>) => void;
  mode: RecapMode;
  state: RecapState | null;
  onJump: (ts_ms: number) => void;
  id?: string;
  className?: string;
  style?: React.CSSProperties;
}

/** The bullet list with its shimmer, waiting, paused, and error lines. The container sets the size, and a `fontSize` style scales the bullets. */
export function RecapBullets({ listRef, onScroll, mode, state, onJump, id, className, style }: RecapBulletsProps) {
  const bullets = state?.bullets ?? [];
  // Only bullets that arrive while the list is mounted pop in, not the whole list on every switch.
  const shownAtMount = useRef<number | null>(null);
  if (shownAtMount.current === null && state !== null) shownAtMount.current = bullets.length;
  const { paused, updating, failed } = recapFlags(state);

  return (
    <div
      ref={listRef}
      id={id}
      onScroll={onScroll}
      data-testid="recap-list"
      className={`overflow-y-auto text-[14px] ${className ?? ""}`}
      style={style}
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
          <span className="shrink-0 min-w-[2.7em] pt-[2px] font-mono text-[0.857em] text-grey group-hover:text-blue">
            {mmss(b.ts_ms / 1000)}
          </span>
          <span className="leading-snug text-ink">{b.text}</span>
        </button>
      ))}
      {updating && (
        <div className="flex gap-3 py-2" aria-live="polite" aria-label="Updating the recap">
          <span className="shrink-0 w-[2.7em] pt-[5px]">
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
          {mode === "recording" && " Trying again at the next update."}
        </p>
      )}
    </div>
  );
}

interface RecapControlsProps {
  mode: RecapMode;
  state: RecapState | null;
  onRefresh: () => void;
  onPause: (paused: boolean) => void;
}

/** Resume while paused, otherwise Update now (recording, after an update) and Pause. Nothing in `final` mode. */
function RecapControls({ mode, state, onRefresh, onPause }: RecapControlsProps) {
  if (mode === "final") return null;
  const { paused, updating, failed } = recapFlags(state);
  return (
    <span className="flex items-center">
      {paused ? (
        <Button variant="secondary" size="sm" className="whitespace-nowrap" aria-label="Resume recap" onClick={() => onPause(false)}>
          <Play size={13} aria-hidden="true" />
          Resume
        </Button>
      ) : (
        <>
          {mode === "recording" && !updating && (state?.updated_at != null || failed) && (
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
  );
}

/** The dock's footer: status and controls, or the final-mode line. */
export function RecapFooter(props: RecapControlsProps) {
  const status = useRecapStatus(props.state);
  if (props.mode === "final") {
    return (
      <footer className="border-t border-line px-5 py-3 text-[12px] text-mut">
        Final recap · the summary replaces it when ready
      </footer>
    );
  }
  return (
    <footer className="flex items-center justify-between gap-2 border-t border-line pl-5 pr-3 py-3">
      <span className="text-[12px] text-mut whitespace-nowrap">{status}</span>
      <RecapControls {...props} />
    </footer>
  );
}

interface RecapCardProps {
  meetingId: string | null;
  token: string | null;
  mode: RecapMode;
  /** Spacing below the card, inside the part that folds away, so the gap leaves with it. */
  gapClassName?: string;
}

/**
 * The recap as a card above the notes. The list keeps a fixed height from the start of recording, so the notes never
 * move as bullets arrive. A bullet click opens the transcript dock at that moment.
 */
export function RecapCard({ meetingId, token, mode, gapClassName = "" }: RecapCardProps) {
  const { recap: state, setRecap } = useLiveRecap(meetingId, token);
  const bulletCount = state?.bullets.length ?? 0;
  // A paused recap stays after Stop, so Resume is still reachable.
  const visible = mode === "recording" || bulletCount > 0 || (mode === "stopped" && (state?.paused ?? false));

  // Keep the last bullets on screen while the card folds away after the summary clears them.
  const lastShown = useRef<RecapState | null>(null);
  if (visible) lastShown.current = state;
  // Set during render, so the card stays mounted through the frame where `visible` turns false and the fold can play.
  const [prevVisible, setPrevVisible] = useState(visible);
  const [leaving, setLeaving] = useState(false);
  if (visible !== prevVisible) {
    setPrevVisible(visible);
    setLeaving(!visible);
  }
  useEffect(() => {
    if (!leaving) return;
    const id = setTimeout(() => setLeaving(false), LEAVE_MS);
    return () => clearTimeout(id);
  }, [leaving]);

  const [collapsed, setCollapsed] = useState(false);
  const [countAtCollapse, setCountAtCollapse] = useState(0);
  const unseen = collapsed ? Math.max(0, bulletCount - countAtCollapse) : 0;

  const shown = visible ? state : lastShown.current;
  const list = useStickyBottom([shown, collapsed]);
  const [faded, setFaded] = useState(false);
  const status = useRecapStatus(shown);
  const listId = useId();
  const { onRefresh, onPause } = recapActions(meetingId, setRecap);

  if (!visible && !leaving) return null;

  const statusText = mode === "final" ? "Final, the summary replaces it when ready" : status;

  return (
    <div
      data-testid="recap-card"
      className={`grid transition-[grid-template-rows,opacity] ease-out motion-reduce:transition-none ${
        leaving ? "grid-rows-[0fr] opacity-0" : "grid-rows-[1fr]"
      }`}
      style={{ transitionDuration: `${LEAVE_MS}ms` }}
    >
      <div className="min-h-0 overflow-hidden">
        <section aria-label="Recap" className="rounded-card bg-card border border-line overflow-hidden">
          <header
            className={`flex items-center justify-between gap-2 pl-5 pr-2 py-1.5 ${collapsed ? "" : "border-b border-line"}`}
          >
            <div className="flex items-center gap-2 min-w-0">
              <span className={LABEL}>Recap</span>
              {unseen > 0 && (
                <>
                  <span data-testid="recap-unseen-dot" className="block w-2 h-2 rounded-full bg-blue shrink-0" />
                  <span className="text-[12px] font-semibold text-blue whitespace-nowrap">{unseen} new</span>
                </>
              )}
              {statusText && (
                <>
                  <span className="text-[12px] text-mut" aria-hidden="true">·</span>
                  <span className="text-[12px] text-mut truncate">{statusText}</span>
                </>
              )}
            </div>
            <div className="flex items-center shrink-0">
              <RecapControls mode={mode} state={shown} onRefresh={onRefresh} onPause={onPause} />
              <Button
                variant="ghost"
                size="sm"
                className="px-2!"
                aria-expanded={!collapsed}
                aria-controls={listId}
                aria-label={collapsed ? "Expand recap" : "Collapse recap"}
                onClick={() => {
                  if (!collapsed) setCountAtCollapse(bulletCount);
                  // The list remounts at the top on expand: land on the newest bullets, unfaded.
                  list.sticky.current = true;
                  setFaded(false);
                  setCollapsed(!collapsed);
                }}
              >
                {collapsed ? <ChevronDown size={15} aria-hidden="true" /> : <ChevronUp size={15} aria-hidden="true" />}
              </Button>
            </div>
          </header>
          {!collapsed && (
            <RecapBullets
              id={listId}
              listRef={list.ref}
              onScroll={(e) => {
                list.onScroll(e);
                setFaded(e.currentTarget.scrollTop > 4);
              }}
              mode={mode}
              state={shown}
              onJump={(ms) =>
                window.dispatchEvent(
                  new CustomEvent("yogurt:transcript:scrollTo", { detail: { ts: Math.floor(ms / 1000) } }),
                )
              }
              style={{
                height: CARD_LIST_HEIGHT,
                padding: "3px 20px",
                maskImage: faded ? "linear-gradient(to bottom, transparent 0, #000 20px)" : undefined,
              }}
            />
          )}
        </section>
        {gapClassName && <div className={gapClassName} />}
      </div>
    </div>
  );
}
