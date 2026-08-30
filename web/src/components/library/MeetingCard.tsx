/**
 * Phase 7 (Plan 07-01) — Meeting card.
 *
 * Layout (PRD §5.9 + D-06):
 *   [42px tinted avatar with 2-letter serif initials]
 *   ╎ Title (Hanken-bold 15px)
 *   ╎ [2:45 PM] [47 min]               ← mono 11px pills (MetaPill); duration
 *   ╎                                     omitted while ended_at is null;
 *   ╎                                     [not enhanced] only for the
 *   ╎                                     exception: ended but never enhanced
 *   [Local · medium.en engine pill, right-aligned; omitted when unknown]
 *
 * Avatar tint is deterministic per id (hash → 3-palette cycle) so the
 * same meeting always shows the same color across reloads.
 */

import { Link } from "react-router";
import { Star } from "lucide-react";
import type { Meeting } from "../../lib/api/meetings";
import { LabelChip } from "../labels/LabelChip";
import { EnginePill, LlmPill, MetaPill } from "../MeetingMetaPills";
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

/**
 * Card metadata as pills, same visual language as `MeetingMetaPills` in the
 * meeting headers. Time only (the list is already grouped by day); duration
 * once ended; and a strawberry "not enhanced" flag for the one case worth
 * noticing: ended without an enrichment pass (enhance failed or skipped).
 * Tagging the normal enhanced case would be noise.
 */
export function metaParts(m: Meeting): { text: string; tone: "neutral" | "warn" }[] {
  const parts: { text: string; tone: "neutral" | "warn" }[] = [
    {
      text: new Date(m.started_at).toLocaleTimeString(undefined, {
        hour: "numeric",
        minute: "2-digit",
      }),
      tone: "neutral",
    },
  ];
  if (m.ended_at != null && m.ended_at > m.started_at) {
    const minutes = Math.max(1, Math.round((m.ended_at - m.started_at) / 60_000));
    parts.push({ text: `${minutes} min`, tone: "neutral" });
  }
  if (m.ended_at != null && m.enriched_md == null) {
    parts.push({ text: "not enhanced", tone: "warn" });
  }
  return parts;
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
        <div className="mt-1 flex flex-wrap items-center gap-1.5">
          {isLive ? (
            <MetaPill tone="warn">
              <span
                aria-hidden="true"
                className="inline-block w-1.5 h-1.5 rounded-pill bg-straw animate-recpulse"
              />
              Recording
            </MetaPill>
          ) : (
            metaParts(meeting).map((p) => (
              <MetaPill key={p.text} tone={p.tone}>
                {p.text}
              </MetaPill>
            ))
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
      {/* Same pill as the meeting headers (MeetingMetaPills), so a meeting
          reads "Local · medium.en" identically in the list and in the note.
          Nothing for pre-column rows rather than a guessed "Local". */}
      <EnginePill sttEngine={meeting.stt_engine} />
      <LlmPill llmModel={meeting.llm_model} />
      <MeetingCardActions id={meeting.id} starred={meeting.starred} labels={meeting.labels} />
    </Link>
  );
}
