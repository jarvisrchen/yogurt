/**
 * Phase 7 (Plan 07-01) — Meeting card.
 *
 * Layout (PRD §5.9 + D-06):
 *   [42px tinted avatar with 2-letter serif initials]
 *   ╎ Title (Hanken-bold 15px)
 *   ╎ {2:45 PM} · {47 min}             ← mono 12px; duration omitted while
 *   ╎                                     ended_at is null; `· not enhanced`
 *   ╎                                     appended only for the exception:
 *   ╎                                     ended but never enhanced
 *   [Local pill, right-aligned]
 *
 * Avatar tint is deterministic per id (hash → 3-palette cycle) so the
 * same meeting always shows the same color across reloads.
 */

import { Link } from "react-router";
import { Star } from "lucide-react";
import type { Meeting } from "../../lib/api/meetings";
import { LabelChip } from "../labels/LabelChip";
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
  const parts = [
    new Date(m.started_at).toLocaleTimeString(undefined, {
      hour: "numeric",
      minute: "2-digit",
    }),
  ];
  if (m.ended_at != null && m.ended_at > m.started_at) {
    const minutes = Math.max(1, Math.round((m.ended_at - m.started_at) / 60_000));
    parts.push(`${minutes} min`);
  }
  // Enhancement is the normal outcome of ending a meeting, so tagging every
  // card "enhanced" is noise. Flag only the exception: ended, no enrichment
  // (enhance failed or was skipped), so the user knows to hit Re-enhance.
  if (m.ended_at != null && m.enriched_md == null) parts.push("not enhanced");
  return parts.join(" · ");
}

/**
 * Right-aligned engine pill text. `stt_engine` is stamped at recording
 * start as "local · <model>" or "cloud · <model>" (routes.rs
 * `start_meeting`) — we only need the provider half here. `null` covers
 * meetings recorded before this column existed; default to "Local" since
 * that was the only value the old static chip ever showed.
 */
export function engineLabel(sttEngine: string | null): string {
  return sttEngine?.startsWith("cloud") ? "Cloud" : "Local";
}

interface Props {
  meeting: Meeting;
  /**
   * Id of the currently-recording meeting (from `useActiveRecording()`),
   * or `null`/`undefined` when nothing is recording. Passed down from the
   * Library route so only ONE poll exists for the whole list — not one
   * per card.
   */
  activeId?: string | null;
}

export function MeetingCard({ meeting, activeId }: Props) {
  const isLive = activeId != null && activeId === meeting.id;
  return (
    <Link
      // A meeting that's still recording routes to the LIVE capture surface
      // (`/meeting/:id`) — the post view is a frozen read of saved notes and
      // has no Start/Stop controls or live transcript. Everything else opens
      // the post-meeting READ view, which hydrates saved notes via
      // GET /api/meetings/:id.
      to={isLive ? `/meeting/${meeting.id}` : `/meeting/${meeting.id}/post`}
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
        <div className="flex items-center gap-1.5 min-w-0">
          <InlineTitle
            id={meeting.id}
            title={meeting.title}
            className="block min-w-0 text-[15px] font-bold text-ink truncate"
          />
          {meeting.starred && (
            <Star
              size={12}
              className="shrink-0 text-straw fill-straw"
              role="img"
              aria-label="Starred"
            />
          )}
        </div>
        <div className="text-[12px] font-mono text-mut flex items-center gap-1.5">
          {isLive ? (
            <>
              <span
                aria-hidden="true"
                className="inline-block w-2 h-2 rounded-pill bg-straw animate-recpulse"
              />
              <span>Recording</span>
            </>
          ) : (
            formatMeta(meeting)
          )}
        </div>
        {meeting.labels.length > 0 && (
          <div className="mt-1 flex flex-wrap gap-1">
            {meeting.labels.map((l) => (
              <LabelChip key={l.id} label={l} size="sm" />
            ))}
          </div>
        )}
      </div>
      <span className="text-[11px] font-mono text-mut border border-line rounded-pill px-2 py-0.5">
        {engineLabel(meeting.stt_engine)}
      </span>
      <MeetingCardActions id={meeting.id} starred={meeting.starred} labels={meeting.labels} />
    </Link>
  );
}
