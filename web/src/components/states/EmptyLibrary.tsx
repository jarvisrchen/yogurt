/**
 * Phase 7 (Plan 07-04) — EmptyLibrary state (STATE-01).
 *
 * Rendered by `<Library />` when the user has Screen-Recording granted AND
 * an active provider AND zero meetings stored. PRD §5.10 / §16.5 lock the
 * cadence: the swirl logo gently drifts up and down on a 3.5-second cycle
 * using the `.float-3500` utility — the class name encodes the duration;
 * do NOT parameterize it. The duration string `float 3.5s ease-in-out
 * infinite` appears exactly once in `web/src/index.css` (CSS contract
 * grep enforced by the plan acceptance).
 *
 * Behavior: clicking the blueberry CTA (or pressing ⌘N — handled at the
 * Library level by `useKeyboardShortcut`) creates a fresh "Untitled
 * meeting" via `useCreateMeeting` and navigates to `/meeting/:id`. The
 * route shape was kept at `/meeting/:id` (Phase-3 convention) instead of
 * the plan-suggested `/m/:id` to minimize regression surface. (Auto-fix
 * Rule 3 — Plan 07-01 already established this convention.)
 */

import { useNavigate } from "react-router";
import { useCreateMeeting } from "../../lib/api/meetings";
import { Logo } from "../Logo";
import { Button } from "../Button";

export function EmptyLibrary() {
  const create = useCreateMeeting();
  const nav = useNavigate();

  const start = async () => {
    try {
      const m = await create.mutateAsync(undefined);
      nav(`/meeting/${m.id}`);
    } catch (e) {
      // Surface in console; richer error UX is a v1.1 concern.
      console.error("create meeting failed", e);
    }
  };

  return (
    <div className="flex flex-col items-center text-center mt-24">
      <div className="float-3500 mb-8">
        <Logo size={64} ariaLabel="Yogurt" />
      </div>
      <h2 className="heading-state mb-3">
        No meetings yet
      </h2>
      <p className="text-[15px] text-mut max-w-md mb-6">
        Start one and Yogurt listens to both sides of the call — no bot
        joins. Your notes and audio stay on this Mac.
      </p>
      <Button onClick={start} disabled={create.isPending}>
        Start your first meeting
        <kbd className="bg-white/20 text-white/90 text-[11px] font-mono px-1.5 py-0.5 rounded">
          ⌘N
        </kbd>
      </Button>
      <p className="mt-6 text-[11px] font-mono text-mut">
        notes saved to <code>~/.yogurt/notes/*.md</code>
      </p>
    </div>
  );
}
