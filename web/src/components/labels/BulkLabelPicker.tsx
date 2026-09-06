/**
 * `BulkLabelPicker` - thin multi-meeting sibling of `LabelPicker` for the
 * Library's multi-select action bar (MTG-14).
 *
 * `LabelPicker` replaces ONE meeting's full label set and shows a
 * checkmark against that meeting's current labels. Across a bulk
 * selection there's no single "current labels" to check against, so this
 * picker skips the checkmark and just fans a single add/remove over every
 * selected meeting via `useBulkSetLabel` (no bulk server route - the
 * existing per-meeting PATCH, `Promise.all`'d).
 */

import { useEffect, useRef, useState } from "react";
import { useCreateLabel, useLabels } from "../../lib/api/labels";
import { useBulkSetLabel, type Meeting } from "../../lib/api/meetings";
import { LABEL_COLORS } from "./LabelChip";

interface Props {
  meetings: Pick<Meeting, "id" | "labels">[];
  mode: "add" | "remove";
  open: boolean;
  onClose: () => void;
}

export function BulkLabelPicker({ meetings, mode, open, onClose }: Props) {
  const [query, setQuery] = useState("");
  const wrapperRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const labels = useLabels();
  const createLabel = useCreateLabel();
  const bulkSetLabel = useBulkSetLabel();

  const trimmed = query.trim();
  const filtered = (labels.data ?? []).filter((l) =>
    l.name.toLowerCase().includes(trimmed.toLowerCase()),
  );
  const hasExactMatch = (labels.data ?? []).some(
    (l) => l.name.toLowerCase() === trimmed.toLowerCase(),
  );
  // Only "Add label" can mint a brand-new label - "Remove label" only
  // makes sense against labels that already exist.
  const showCreateRow = mode === "add" && trimmed.length > 0 && !hasExactMatch;

  useEffect(() => {
    if (!open) return;
    setQuery("");
    inputRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open, onClose]);

  if (!open) return null;

  function apply(labelId: string) {
    bulkSetLabel.mutate({ meetings, labelId, mode });
    onClose();
  }

  async function createAndApply() {
    if (!trimmed) return;
    const created = await createLabel.mutateAsync({ name: trimmed });
    bulkSetLabel.mutate({ meetings, labelId: created.id, mode: "add" });
    setQuery("");
    onClose();
  }

  return (
    <div
      ref={wrapperRef}
      className="absolute bottom-full left-0 mb-1 bg-card border border-line rounded-card shadow-pop py-1 min-w-[220px] z-20 text-[13px]"
    >
      <div className="px-2 pb-1">
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.stopPropagation();
              onClose();
            } else if (e.key === "Enter" && showCreateRow) {
              void createAndApply();
            }
          }}
          placeholder={mode === "add" ? "Search or create label" : "Search label to remove"}
          className="w-full px-2 py-1 rounded-button border border-line bg-paper text-[13px] text-ink placeholder:text-mut focus:outline-none focus:ring-2 focus:ring-blue/30"
        />
      </div>
      {filtered.length === 0 && !showCreateRow && (
        <p className="px-3 py-2 text-[12px] text-mut">
          {(labels.data ?? []).length === 0 ? "No labels yet." : "No matches."}
        </p>
      )}
      {filtered.map((l) => {
        const tone = LABEL_COLORS[l.color] ?? { bg: "var(--color-line)", fg: "var(--color-mut)" };
        return (
          <button
            key={l.id}
            type="button"
            role="option"
            onClick={() => apply(l.id)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-paper"
          >
            <span
              className="w-[9px] h-[9px] rounded-[3px] shrink-0"
              style={{ background: tone.fg }}
              aria-hidden
            />
            <span className="flex-1 truncate text-ink">{l.name}</span>
          </button>
        );
      })}
      {showCreateRow && (
        <button
          type="button"
          onClick={() => void createAndApply()}
          className="w-full text-left px-3 py-1.5 text-blue hover:bg-paper"
        >
          Create &quot;{trimmed}&quot;
        </button>
      )}
    </div>
  );
}
