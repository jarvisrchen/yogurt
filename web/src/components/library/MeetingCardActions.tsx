/**
 * Phase 7 (Plan 07-03) — kebab-overflow menu for a meeting card.
 *
 * Items (in display order):
 *   - Copy markdown   — `GET /api/meetings/:id/markdown` → clipboard.
 *   - Reveal in Finder — `POST /api/meetings/:id/reveal` → `open -R` on macOS.
 *   - Delete          — `window.confirm` → `useDeleteMeeting().mutateAsync`.
 *
 * Layout invariants:
 *   - Wrapped in a positioned `<div>` so the dropdown is anchored to the
 *     kebab button (top-right of the card).
 *   - Every click handler `stopPropagation`s so the surrounding `<Link>`
 *     in MeetingCard doesn't navigate into `/meeting/:id` while the user
 *     is trying to use the menu.
 *   - `role="menu"` / `role="menuitem"` on the dropdown items so VoiceOver
 *     announces it as a menu (matches PRD §16 accessibility posture).
 *
 * D-10 / PRD §5.7 reminder: Delete removes the SQLite row + cascades
 * `chat_messages`, but the on-disk markdown file in `~/.yogurt/notes/`
 * survives. The confirm-dialog copy makes that explicit.
 */

import { useEffect, useRef, useState } from "react";
import {
  copyMeetingMarkdown,
  revealMeetingInFinder,
  useDeleteMeeting,
} from "../../lib/api/meetings";

interface Props {
  id: string;
}

export function MeetingCardActions({ id }: Props) {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const del = useDeleteMeeting();

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
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const onCopy = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setOpen(false);
    await copyMeetingMarkdown(id);
  };
  const onReveal = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setOpen(false);
    await revealMeetingInFinder(id);
  };
  const onDelete = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setOpen(false);
    if (
      window.confirm(
        "Delete this meeting from the library? The markdown file in ~/.yogurt/notes/ stays put.",
      )
    ) {
      await del.mutateAsync(id);
    }
  };

  return (
    <div
      ref={wrapperRef}
      className="relative shrink-0"
      onClick={(e) => e.stopPropagation()}
    >
      <button
        type="button"
        aria-label="Meeting actions"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setOpen((o) => !o);
        }}
        className="px-2 py-1 text-mut hover:text-ink rounded-button"
      >
        ⋯
      </button>
      {open && (
        <div
          role="menu"
          className="absolute right-0 top-full mt-1 bg-card border border-line rounded-card shadow-pop py-1 min-w-[180px] z-10 text-[13px]"
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
          <button
            type="button"
            role="menuitem"
            onClick={onDelete}
            className="block w-full text-left px-3 py-2 text-straw hover:bg-paper"
          >
            Delete
          </button>
        </div>
      )}
    </div>
  );
}
