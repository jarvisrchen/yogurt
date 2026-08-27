/**
 * Phase 7 (Plan 07-02) — search input pill at the top-right of the
 * Library main pane.
 *
 * The component is a controlled input whose `value` is debounced (200ms)
 * before being lifted to the parent via `onChange`. Local state keeps
 * typing snappy — every keystroke updates `local`, and only the
 * debounced trailing edge updates the parent (which then triggers a
 * `useMeetingsSearch` refetch).
 *
 * The pill itself is a 280px-wide rounded container with the matching
 * `border-line` token border the rest of the Library uses (cf.
 * MeetingCard, Sidebar). Leading glyph is the lucide Search icon at the
 * app-standard 16px.
 */

import { useEffect, useState } from "react";
import { Search } from "lucide-react";

interface SearchPillProps {
  /** Current canonical query held by the parent (debounced). */
  value: string;
  /** Called with the new debounced query string. */
  onChange: (next: string) => void;
}

export function SearchPill({ value, onChange }: SearchPillProps) {
  // Local mirror of the input — every keystroke updates this for
  // snappiness; only the 200ms debounce timer commits upward.
  const [local, setLocal] = useState(value);

  // If the parent forces a new `value` (e.g. clearing on route change),
  // sync the local mirror so the input visually follows.
  useEffect(() => {
    setLocal(value);
  }, [value]);

  // Debounce the lift-up: 200ms is fast enough to feel live while still
  // collapsing a burst of keystrokes into one server query.
  useEffect(() => {
    if (local === value) return;
    const id = setTimeout(() => onChange(local), 200);
    return () => clearTimeout(id);
  }, [local, value, onChange]);

  return (
    <div
      role="search"
      className="flex items-center gap-2 bg-card border border-line rounded-pill px-4 py-2 text-[13px] text-ink w-[280px] focus-within:ring-2 focus-within:ring-blue/40"
    >
      <Search size={16} className="text-mut shrink-0" aria-hidden />
      <input
        type="text"
        value={local}
        onChange={(e) => setLocal(e.target.value)}
        placeholder="Search notes & transcripts"
        aria-label="Search notes and transcripts"
        className="flex-1 bg-transparent outline-none placeholder:text-mut"
      />
    </div>
  );
}
