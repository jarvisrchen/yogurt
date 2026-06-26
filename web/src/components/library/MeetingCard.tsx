/**
 * Phase 7 (Plan 07-01) — Meeting card.
 *
 * Layout (PRD §5.9 + D-06):
 *   [42px tinted avatar with 2-letter serif initials]
 *   ╎ Title (Hanken-bold 15px)
 *   ╎ {HH:MM} · {N} min · enhanced     ← mono 12px
 *   ╎                                     `· enhanced` only if enriched_md != null
 *   [Local pill, right-aligned]
 *
 * Avatar tint is deterministic per id (hash → 3-palette cycle) so the
 * same meeting always shows the same color across reloads.
 */

import { Link } from "react-router";
import type { Meeting } from "../../lib/api/meetings";
import { InlineTitle } from "./InlineTitle";
import { MeetingCardActions } from "./MeetingCardActions";

const PALETTE = [
  "var(--color-blsoft)", // blueberry-soft
  "var(--color-mtsoft)", // matcha-soft
  "#FBE6E0", // strawberry-soft
];

export function avatarTint(id: string): string {
  let h = 0;
  for (let i = 0; i < id.length; i++) {
    h = (h * 31 + id.charCodeAt(i)) | 0;
  }
  // |0 produces a 32-bit signed int — coerce to non-negative.
  return PALETTE[Math.abs(h) % PALETTE.length]!;
}

export function initials(title: string): string {
  const words = title
    .trim()
    .split(/\s+/)
    .filter((w) => w.length > 0);
  if (words.length === 0) return "·";
  if (words.length === 1) {
    return words[0]!.slice(0, 2).toUpperCase();
  }
  return (words[0]![0]! + words[1]![0]!).toUpperCase();
}

export function formatMeta(m: Meeting): string {
  const d = new Date(m.started_at);
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  let duration = "—";
  if (m.ended_at != null && m.ended_at > m.started_at) {
    const minutes = Math.max(1, Math.round((m.ended_at - m.started_at) / 60_000));
    duration = `${minutes} min`;
  }
  const base = `${hh}:${mm} · ${duration}`;
  return m.enriched_md != null ? `${base} · enhanced` : base;
}

interface Props {
  meeting: Meeting;
}

export function MeetingCard({ meeting }: Props) {
  return (
    <Link
      to={`/meeting/${meeting.id}`}
      className="group flex items-center gap-3 py-2 px-2 -mx-2 rounded-button hover:bg-line/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40"
    >
      <div
        className="w-[42px] h-[42px] rounded-[10px] flex items-center justify-center font-serif text-[18px] text-ink shrink-0"
        style={{ background: avatarTint(meeting.id) }}
        aria-hidden
      >
        {initials(meeting.title || "Untitled meeting")}
      </div>
      <div className="flex-1 min-w-0">
        <InlineTitle
          id={meeting.id}
          title={meeting.title}
          className="block text-[15px] font-bold text-ink truncate"
        />
        <div className="text-[12px] font-mono text-mut">{formatMeta(meeting)}</div>
      </div>
      <span className="text-[11px] font-mono text-mut border border-line rounded-pill px-2 py-0.5">
        Local
      </span>
      <MeetingCardActions id={meeting.id} />
    </Link>
  );
}
