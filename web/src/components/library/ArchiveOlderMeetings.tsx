import { useEffect, useRef, useState } from "react";
import { Archive } from "lucide-react";
import { Button } from "../Button";
import { useArchiveBefore, useArchivePreview } from "../../lib/api/meetings";
import { formatCount } from "./pagination";

const pad = (n: number) => String(n).padStart(2, "0");
const isoDay = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** Local midnight of a `YYYY-MM-DD` date input value, or null while the field is empty. */
export function localMidnight(value: string): number | null {
  const [y, m, d] = value.split("-").map(Number);
  return y && m && d ? new Date(y, m - 1, d).getTime() : null;
}

export function ArchiveOlderMeetings() {
  const [open, setOpen] = useState(false);
  const [day, setDay] = useState(() => `${new Date().getFullYear()}-01-01`);
  const wrapperRef = useRef<HTMLDivElement>(null);
  const today = isoDay(new Date());
  // `max` only limits the picker, so a typed date after today counts as no date.
  const before = open && day <= today ? localMidnight(day) : null;
  const preview = useArchivePreview(before);
  const archive = useArchiveBefore();

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!wrapperRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const count = preview.data?.count;
  const label =
    before == null
      ? ""
      : new Date(before).toLocaleDateString(undefined, {
          month: "short",
          day: "numeric",
          year: "numeric",
        });

  const onArchive = async () => {
    if (before == null) return;
    try {
      await archive.mutateAsync(before);
      setOpen(false);
    } catch (e) {
      console.error("archive older meetings failed", e);
    }
  };

  return (
    <div ref={wrapperRef} className="relative">
      <Button
        variant="secondary"
        size="sm"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <Archive size={14} aria-hidden />
        Archive older meetings
      </Button>
      {open && (
        <div
          role="alertdialog"
          aria-label="Archive older meetings"
          onKeyDown={(e) => {
            if (e.key === "Escape") setOpen(false);
          }}
          className="absolute top-full right-0 mt-1 z-20 w-64 px-3 py-2 flex flex-col gap-2 bg-card border border-line rounded-card shadow-pop text-[12px] text-ink"
        >
          <label className="flex flex-col gap-1">
            <span className="text-[11px] text-mut">Started before</span>
            <input
              type="date"
              value={day}
              max={today}
              onChange={(e) => setDay(e.target.value)}
              className="rounded-button border border-line bg-paper px-2 py-1.5 text-[13px] text-ink focus:border-blue focus:outline-none"
            />
          </label>
          <p data-testid="archive-older-count">
            {before == null
              ? "Pick a date no later than today."
              : count == null
                ? "Counting meetings…"
                : count === 0
                  ? `No meetings started before ${label}.`
                  : `Archive ${formatCount(count)} ${count === 1 ? "meeting" : "meetings"} that started before ${label}?`}
          </p>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              disabled={!count || preview.isPlaceholderData || archive.isPending}
              onClick={() => void onArchive()}
            >
              Archive
            </Button>
            <Button variant="ghost" size="sm" onClick={() => setOpen(false)}>
              Cancel
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
