/**
 * `LabelPicker` — popover to search/create labels and toggle them on one
 * meeting. Reusable across the Library card hover actions, the live
 * meeting header, and the post-meeting header (`MeetingLabels` mounts it).
 *
 * Visual language matches the kebab menu in `MeetingCardActions`. Every
 * click inside `stopPropagation`s + `preventDefault`s so the picker can
 * live inside a card `<Link>` without triggering navigation.
 */

import { useEffect, useRef, useState } from "react";
import { Check } from "lucide-react";
import {
  useCreateLabel,
  useLabels,
  type Label,
} from "../../lib/api/labels";
import { useSetMeetingLabels } from "../../lib/api/meetings";
import { LABEL_COLORS } from "./LabelChip";

interface Props {
  meetingId: string;
  selected: Label[];
  open: boolean;
  onClose: () => void;
  anchorClassName?: string;
}

export function LabelPicker({ meetingId, selected, open, onClose, anchorClassName }: Props) {
  const [query, setQuery] = useState("");
  const wrapperRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const labels = useLabels();
  const createLabel = useCreateLabel();
  const setLabels = useSetMeetingLabels();

  const selectedIds = new Set(selected.map((l) => l.id));
  const trimmed = query.trim();
  const filtered = (labels.data ?? []).filter((l) =>
    l.name.toLowerCase().includes(trimmed.toLowerCase()),
  );
  const hasExactMatch = (labels.data ?? []).some(
    (l) => l.name.toLowerCase() === trimmed.toLowerCase(),
  );
  const showCreateRow = trimmed.length > 0 && !hasExactMatch;

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

  const stop = (e: React.SyntheticEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  function toggle(labelId: string) {
    const next = selectedIds.has(labelId)
      ? selected.filter((l) => l.id !== labelId).map((l) => l.id)
      : [...selected.map((l) => l.id), labelId];
    setLabels.mutate({ id: meetingId, label_ids: next });
  }

  async function createAndApply() {
    if (!trimmed) return;
    const created = await createLabel.mutateAsync({ name: trimmed });
    setLabels.mutate({
      id: meetingId,
      label_ids: [...selected.map((l) => l.id), created.id],
    });
    setQuery("");
  }

  return (
    <div
      ref={wrapperRef}
      onClick={stop}
      onMouseDown={stop}
      className={
        anchorClassName ??
        "absolute mt-1 bg-card border border-line rounded-card shadow-pop py-1 min-w-[220px] z-20 text-[13px]"
      }
    >
      <div className="px-2 pb-1">
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              onClose();
            } else if (e.key === "Enter" && showCreateRow) {
              void createAndApply();
            }
          }}
          // ponytail: no up/down arrow-key navigation between rows — mouse
          // + Enter-to-create covers the common path; add arrow nav if
          // keyboard-only users report friction.
          placeholder="Search or create label"
          className="w-full px-2 py-1 rounded-button border border-line bg-paper text-[13px] text-ink placeholder:text-mut focus:outline-none focus:ring-2 focus:ring-blue/30"
        />
      </div>
      {filtered.length === 0 && !showCreateRow && (
        <p className="px-3 py-2 text-[12px] font-mono text-mut">
          {(labels.data ?? []).length === 0
            ? "No labels yet. Type to create one."
            : "No matches."}
        </p>
      )}
      {filtered.map((l) => {
        const tone = LABEL_COLORS[l.color] ?? { bg: "var(--color-line)", fg: "var(--color-mut)" };
        const isSelected = selectedIds.has(l.id);
        return (
          <button
            key={l.id}
            type="button"
            role="option"
            aria-selected={isSelected}
            onClick={() => toggle(l.id)}
            className="w-full flex items-center gap-2 px-3 py-1.5 text-left hover:bg-paper"
          >
            <span
              className="w-2 h-2 rounded-pill shrink-0"
              style={{ background: tone.fg }}
              aria-hidden
            />
            <span className="flex-1 truncate text-ink">{l.name}</span>
            {isSelected && <Check size={14} className="text-blue shrink-0" aria-hidden />}
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
