import { useEffect, useRef, useState } from "react";
import { useUpdateMeetingTitle } from "../../lib/api/meetings";

// M/D/YY in local time, the form the server's default titles and date search use.
export function shortDate(unixMs: number): string {
  return new Date(unixMs).toLocaleDateString("en-US", {
    month: "numeric",
    day: "numeric",
    year: "2-digit",
  });
}

export function splitTitle(title: string, startedAt?: number): { name: string; date: string | null } {
  if (startedAt == null) return { name: title, date: null };
  const date = shortDate(startedAt);
  const name = title.endsWith(` ${date}`) ? title.slice(0, -date.length - 1) : title;
  return { name, date };
}

interface Props {
  id: string;
  title: string;
  startedAt?: number;
  className?: string;
}

export function InlineTitle({ id, title, startedAt, className }: Props) {
  const { name, date } = splitTitle(title, startedAt);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(name);
  const ref = useRef<HTMLInputElement>(null);
  const update = useUpdateMeetingTitle();

  useEffect(() => {
    setDraft(name);
  }, [name]);

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
      // Keep the surrounding <Link> from navigating while editing.
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
