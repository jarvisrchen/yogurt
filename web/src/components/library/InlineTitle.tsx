/**
 * Phase 7 (Plan 07-03) — Inline rename for a meeting title.
 *
 * Behavior (LIB-11):
 *   - Read-only state: renders the title as a `<span>`. Double-click flips
 *     into edit mode.
 *   - Edit state: text `<input>` autoselects on entry. Enter / blur commits
 *     via PATCH (`useUpdateMeetingTitle`); Escape reverts to the original.
 *   - Empty / whitespace-only inputs commit as "Untitled meeting" — matches
 *     the server-side LIB-08 fallback so the optimistic UI never disagrees
 *     with the persisted row.
 *
 * Layout invariants:
 *   - `className` is forwarded so callers can style the surface uniformly
 *     across read + edit modes (MeetingCard wraps it with `text-[15px]
 *     font-bold text-ink truncate`).
 *   - Click + double-click events `stopPropagation` so the surrounding
 *     `<Link>` doesn't navigate into `/meeting/:id` when the user is just
 *     trying to rename.
 */

import { useEffect, useRef, useState } from "react";
import { useUpdateMeetingTitle } from "../../lib/api/meetings";

interface Props {
  id: string;
  title: string;
  className?: string;
}

export function InlineTitle({ id, title, className }: Props) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(title);
  const ref = useRef<HTMLInputElement>(null);
  const update = useUpdateMeetingTitle();

  // Sync the local draft when the server pushes a new title (e.g. after
  // a successful PATCH the parent re-renders us with the canonical value).
  useEffect(() => {
    setDraft(title);
  }, [title]);

  // Autoselect the text on entering edit mode so the user can start
  // typing immediately to replace the whole title.
  useEffect(() => {
    if (editing) ref.current?.select();
  }, [editing]);

  const commit = () => {
    const next = draft.trim().length === 0 ? "Untitled meeting" : draft.trim();
    if (next !== title) update.mutate({ id, title: next });
    setEditing(false);
  };

  if (!editing) {
    return (
      <span
        className={className}
        onDoubleClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setEditing(true);
        }}
        title="Double-click to rename"
      >
        {title || "Untitled meeting"}
      </span>
    );
  }
  return (
    <input
      ref={ref}
      value={draft}
      // Prevent the surrounding <Link>'s click handler from firing while
      // the user is inside the input.
      onClick={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commit();
        } else if (e.key === "Escape") {
          setDraft(title);
          setEditing(false);
        }
      }}
      className={`${className ?? ""} bg-white border border-blue rounded-button px-1 -mx-1 outline-none`}
    />
  );
}
