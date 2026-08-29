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
 * The Delete item + its confirm live in `DeleteMeetingConfirm`, shared
 * with the post-meeting header so both surfaces delete identically.
 */

import { useEffect, useRef, useState } from "react";
import { MoreHorizontal, Star, Tag } from "lucide-react";
import {
  copyMeetingMarkdown,
  revealMeetingInFinder,
  useToggleStarred,
} from "../../lib/api/meetings";
import { DeleteMeetingConfirm } from "./DeleteMeetingConfirm";
import type { Label } from "../../lib/api/labels";
import { LabelPicker } from "../labels/LabelPicker";

interface Props {
  id: string;
  starred: boolean;
  labels: Label[];
}

export function MeetingCardActions({ id, starred, labels }: Props) {
  const [open, setOpen] = useState(false);
  const [labelPickerOpen, setLabelPickerOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);
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
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

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
        aria-label="Edit labels"
        // Also stop mousedown (not just click) so LabelPicker's own
        // outside-click listener — which fires on mousedown, before this
        // click handler runs — doesn't see this button as "outside" and
        // close what we're about to open (or reopen what we're closing).
        onMouseDown={stop}
        onClick={(e) => {
          stop(e);
          setLabelPickerOpen((o) => !o);
          setOpen(false);
        }}
        className={`${iconButton} text-mut hover:text-ink opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100`}
      >
        <Tag size={16} aria-hidden />
      </button>
      <LabelPicker
        meetingId={id}
        selected={labels}
        open={labelPickerOpen}
        onClose={() => setLabelPickerOpen(false)}
        anchorClassName="absolute right-0 top-full mt-1 bg-card border border-line rounded-card shadow-pop py-1 min-w-[220px] z-20 text-[13px]"
      />
      <button
        type="button"
        aria-label="Meeting actions"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(e) => {
          stop(e);
          setOpen((o) => !o);
          setLabelPickerOpen(false);
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
          <DeleteMeetingConfirm
            id={id}
            variant="menuitem"
            onDeleted={() => setOpen(false)}
          />
        </div>
      )}
    </div>
  );
}
