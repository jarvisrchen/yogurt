/**
 * Phase 7 (Plan 07-01) — Meeting card.
 *
 * Layout (PRD §5.9 + D-06, restyled to the Whimsical×Blueberry mock):
 *   Title (bold) + star            [labels] [engine pill] [llm pill] [actions]
 *   caption: "2:45 PM · 47 min"        (or a strawberry MetaPill when live)
 * The card itself is a bordered/shadowed surface (`.mcard` in the mock);
 * a live meeting gets a strawberry border instead of the neutral one.
 */

import { Link } from "react-router";
import { Star } from "lucide-react";
import type { Meeting } from "../../lib/api/meetings";
import { LabelChip } from "../labels/LabelChip";
import { EnginePill, LlmPill, MetaPill } from "../MeetingMetaPills";
import { InlineTitle } from "./InlineTitle";
import { MeetingCardActions } from "./MeetingCardActions";

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
  const parts = metaParts(meeting);
  const caption = parts
    .filter((p) => p.tone !== "warn")
    .map((p) => p.text)
    .join(" · ");
  const warnPart = parts.find((p) => p.tone === "warn");
  return (
    <Link
      // A meeting that's still recording routes to the LIVE capture surface
      // (`/meeting/:id`) — the post view is a frozen read of saved notes and
      // has no Start/Stop controls or live transcript. Everything else opens
      // the post-meeting READ view, which hydrates saved notes via
      // GET /api/meetings/:id.
      to={isLive ? `/meeting/${meeting.id}` : `/meeting/${meeting.id}/post`}
      className={`group flex items-start justify-between gap-4 px-[18px] py-4 bg-card border ${
        isLive ? "border-straw" : "border-line"
      } rounded-card shadow-card hover:border-grey/60 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40`}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 min-w-0">
          <InlineTitle
            id={meeting.id}
            title={meeting.title}
            className="block min-w-0 text-[15px] font-semibold text-ink truncate"
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
        {isLive ? (
          <div className="mt-1">
            <MetaPill tone="warn">
              <span
                aria-hidden="true"
                className="inline-block w-1.5 h-1.5 rounded-pill bg-straw animate-recpulse"
              />
              Recording
            </MetaPill>
          </div>
        ) : (
          <div className="mt-1 flex flex-wrap items-center gap-1.5">
            {caption && <span className="text-[12px] text-mut">{caption}</span>}
            {warnPart && <MetaPill tone="warn">{warnPart.text}</MetaPill>}
          </div>
        )}
      </div>
      <div className="flex items-center gap-1.5 shrink-0">
        {meeting.labels.map((l) => (
          <LabelChip key={l.id} label={l} size="sm" />
        ))}
        {/* Same pill as the meeting headers (MeetingMetaPills), so a meeting
            reads "Local · medium.en" identically in the list and in the note.
            Nothing for pre-column rows rather than a guessed "Local". */}
        <EnginePill sttEngine={meeting.stt_engine} />
        <LlmPill llmModel={meeting.llm_model} />
        <MeetingCardActions id={meeting.id} starred={meeting.starred} labels={meeting.labels} />
      </div>
    </Link>
  );
}
