/**
 * Phase 7 (Plan 07-03) — hover actions for a meeting card: star toggle +
 * kebab-overflow menu.
 *
 * Star toggle:
 *   - `PATCH /api/meetings/:id { starred }` via `useToggleStarred`.
 *   - Hidden until the card is hovered / focused, except when already
 *     starred (then it stays visible, filled strawberry).
 *
 * Menu items (in display order):
 *   - Copy markdown    — `GET /api/meetings/:id/markdown` → clipboard.
 *   - Reveal in Finder — `POST /api/meetings/:id/reveal` → `open -R`.
 *   - Delete           — inline confirm: the row morphs into a
 *     "Delete?" / "Cancel" pair that auto-reverts after 3s. No modal,
 *     no `window.confirm`.
 *
 * Layout invariants:
 *   - Wrapped in a positioned `<div>` so the dropdown is anchored to the
 *     kebab button (top-right of the card).
 *   - Every click handler `stopPropagation`s so the surrounding `<Link>`
 *     in MeetingCard doesn't navigate into `/meeting/:id` while the user
 *     is trying to use the actions.
 *   - `role="menu"` / `role="menuitem"` on the dropdown items so VoiceOver
 *     announces it as a menu (matches PRD §16 accessibility posture).
 *
 * D-10 / PRD §5.7 reminder: Delete removes the SQLite row + cascades
 * `chat_messages`, but the on-disk markdown file in `~/.yogurt/notes/`
 * survives. The confirm caption makes that explicit.
 */

import { useEffect, useRef, useState } from "react";
import { MoreHorizontal, Star } from "lucide-react";
import {
  copyMeetingMarkdown,
  revealMeetingInFinder,
  useDeleteMeeting,
  useToggleStarred,
} from "../../lib/api/meetings";

interface Props {
  id: string;
  starred: boolean;
}

export function MeetingCardActions({ id, starred }: Props) {
  const [open, setOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const del = useDeleteMeeting();
  const star = useToggleStarred();

  // Click-outside closes the menu so it doesn't get marooned open when
  // the user clicks elsewhere on the page.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (
        wrapperRef.current &&
        !wrapperRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
        setConfirming(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  // The inline "Delete?" confirm auto-reverts after 3s of inaction.
  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(false), 3000);
    return () => clearTimeout(t);
  }, [confirming]);

  const stop = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  const onToggleStar = (e: React.MouseEvent) => {
    stop(e);
    star.mutate({ id, starred: !starred });
  };
  const onCopy = async (e: React.MouseEvent) => {
    stop(e);
    setOpen(false);
    await copyMeetingMarkdown(id);
  };
  const onReveal = async (e: React.MouseEvent) => {
    stop(e);
    setOpen(false);
    await revealMeetingInFinder(id);
  };
  const onDeleteClick = (e: React.MouseEvent) => {
    stop(e);
    setConfirming(true);
  };
  const onConfirmDelete = async (e: React.MouseEvent) => {
    stop(e);
    setOpen(false);
    setConfirming(false);
    await del.mutateAsync(id);
  };
  const onCancelDelete = (e: React.MouseEvent) => {
    stop(e);
    setConfirming(false);
  };

  const iconButton =
    "px-1.5 py-1 rounded-button focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40";

  return (
    <div
      ref={wrapperRef}
      className="relative shrink-0 flex items-center"
      onClick={(e) => e.stopPropagation()}
    >
      <button
        type="button"
        aria-label={starred ? "Unstar meeting" : "Star meeting"}
        aria-pressed={starred}
        onClick={onToggleStar}
        disabled={star.isPending}
        className={`${iconButton} ${
          starred
            ? "text-straw"
            : "text-mut hover:text-ink opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100"
        }`}
      >
        <Star size={16} className={starred ? "fill-current" : undefined} aria-hidden />
      </button>
      <button
        type="button"
        aria-label="Meeting actions"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(e) => {
          stop(e);
          setOpen((o) => !o);
          setConfirming(false);
        }}
        className={`${iconButton} text-mut hover:text-ink`}
      >
        <MoreHorizontal size={18} aria-hidden />
      </button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 top-full mt-1 bg-card border border-line rounded-card shadow-pop py-1 min-w-[200px] z-10 text-[13px]"
        >
          <button
            type="button"
            role="menuitem"
            onClick={onCopy}
            className="block w-full text-left px-3 py-2 text-ink hover:bg-paper"
          >
            Copy markdown
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={onReveal}
            className="block w-full text-left px-3 py-2 text-ink hover:bg-paper"
          >
            Reveal in Finder
          </button>
          {confirming ? (
            <div className="px-3 py-2 space-y-1.5">
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  role="menuitem"
                  autoFocus
                  onClick={onConfirmDelete}
                  disabled={del.isPending}
                  className="px-2 py-1 rounded-button bg-strsoft text-ink border border-straw/40 font-semibold hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-straw/50 disabled:opacity-50"
                >
                  Delete?
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={onCancelDelete}
                  className="px-2 py-1 rounded-button text-mut hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40"
                >
                  Cancel
                </button>
              </div>
              <p className="text-[11px] font-mono text-mut">
                .md file stays in ~/.yogurt/notes
              </p>
            </div>
          ) : (
            <button
              type="button"
              role="menuitem"
              onClick={onDeleteClick}
              className="block w-full text-left px-3 py-2 text-straw hover:bg-paper"
            >
              Delete
            </button>
          )}
        </div>
      )}
    </div>
  );
}
