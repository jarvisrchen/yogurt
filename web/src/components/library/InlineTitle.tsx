/**
 * Phase 7 (Plan 07-03) — Inline rename for a meeting title.
 *
 * Behavior (LIB-11):
 *   - Read-only state: renders the title as a `<span>`. Double-click flips
 *     into edit mode.
 *   - Edit state: text `<input>` autoselects on entry. Enter / blur commits
 *     via PATCH (`useUpdateMeetingTitle`); Escape reverts to the original.
 *   - Given `startedAt`, the trailing M/D/YY date is fixed: the input edits
 *     only the name before it, and commit re-appends the date (adding it to
 *     older titles that lack one). An empty name commits as "Meeting".
 *   - Without `startedAt`, empty / whitespace-only inputs commit as
 *     "Untitled meeting", matching the server-side LIB-08 fallback.
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

/** M/D/YY in local time, the form the server's default titles and date search use. */
export function shortDate(unixMs: number): string {
  return new Date(unixMs).toLocaleDateString("en-US", {
    month: "numeric",
    day: "numeric",
    year: "2-digit",
  });
}

/** Split `title` into the editable name and its fixed date suffix. */
export function splitTitle(title: string, startedAt?: number): { name: string; date: string | null } {
  if (startedAt == null) return { name: title, date: null };
  const date = shortDate(startedAt);
  const name = title.endsWith(` ${date}`) ? title.slice(0, -date.length - 1) : title;
  return { name, date };
}

interface Props {
  id: string;
  title: string;
  /** Meeting start (unix ms). Enables the fixed date suffix. */
  startedAt?: number;
  className?: string;
}

export function InlineTitle({ id, title, startedAt, className }: Props) {
  const { name, date } = splitTitle(title, startedAt);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(name);
  const ref = useRef<HTMLInputElement>(null);
  const update = useUpdateMeetingTitle();

  // Sync the local draft when the server pushes a new title (e.g. after
  // a successful PATCH the parent re-renders us with the canonical value).
  useEffect(() => {
    setDraft(name);
  }, [name]);

  // Autoselect the text on entering edit mode so the user can start
  // typing immediately to replace the whole title.
  useEffect(() => {
    if (editing) ref.current?.select();
  }, [editing]);

  const commit = () => {
    const base = draft.trim();
    const next = date
      ? `${base || "Meeting"} ${date}`
      : base || "Untitled meeting";
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
  const input = (
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
          setDraft(name);
          setEditing(false);
        }
      }}
      className={`${className ?? ""} bg-card border border-blue rounded-button px-1 -mx-1 outline-none`}
    />
  );
  if (!date) return input;
  return (
    <span className="flex min-w-0 items-baseline gap-2">
      {input}
      <span className={`${className ?? ""} shrink-0 text-mut!`}>{date}</span>
    </span>
  );
}
