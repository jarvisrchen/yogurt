import { Link } from "react-router";
import { Star } from "lucide-react";
import type { MeetingSummary } from "../../lib/api/meetings";
import { LabelChip } from "../labels/LabelChip";
import { EnginePill, LlmPill, MetaPill } from "../MeetingMetaPills";
import { InlineTitle, shortDate } from "./InlineTitle";
import { MeetingCardActions } from "./MeetingCardActions";

// The "not enhanced" warn pill marks only an ended meeting with no enrichment
// pass. Flagging the normal enhanced case would be noise.
export function metaParts(m: MeetingSummary): { text: string; tone: "neutral" | "warn" }[] {
  const parts: { text: string; tone: "neutral" | "warn" }[] = [
    {
      text: shortDate(m.started_at),
      tone: "neutral",
    },
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
  if (m.ended_at != null && !m.has_enriched) {
    parts.push({ text: "not enhanced", tone: "warn" });
  }
  return parts;
}

interface Props {
  meeting: MeetingSummary;
  // Passed down from the Library route so the list shares one poll.
  activeId?: string | null;
  selectionActive?: boolean;
  selected?: boolean;
  onToggleSelect?: (id: string, shiftKey: boolean) => void;
}

export function MeetingCard({
  meeting,
  activeId,
  selectionActive = false,
  selected = false,
  onToggleSelect,
}: Props) {
  const isLive = activeId != null && activeId === meeting.id;
  const parts = metaParts(meeting);
  const caption = parts
    .filter((p) => p.tone !== "warn")
    .map((p) => p.text)
    .join(" · ");
  const warnPart = parts.find((p) => p.tone === "warn");
  return (
    <Link
      // A live meeting opens the capture surface. The post view has no controls or live transcript.
      to={isLive ? `/meeting/${meeting.id}` : `/meeting/${meeting.id}/post`}
      className={`group flex items-start justify-between gap-4 px-[18px] py-4 bg-card border ${
        isLive ? "border-straw" : "border-line"
      } rounded-card shadow-card hover:border-grey/60 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40`}
    >
      <label
        className={`shrink-0 flex items-center h-[22px] pr-0.5 ${
          selectionActive
            ? "opacity-100"
            : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        }`}
        // No preventDefault, so the label still toggles its checkbox natively.
        onClick={(e) => e.stopPropagation()}
      >
        <input
          type="checkbox"
          checked={selected}
          // onChange rather than onClick+preventDefault: suppressing the native
          // toggle from a click handler leaves the DOM `checked` out of sync with
          // `selected` (a shift-range-selected box stayed unchecked).
          onChange={(e) =>
            onToggleSelect?.(meeting.id, (e.nativeEvent as MouseEvent).shiftKey)
          }
          aria-label={selected ? `Deselect ${meeting.title}` : `Select ${meeting.title}`}
          className="w-4 h-4 rounded-[4px] border border-line accent-blue cursor-pointer"
        />
      </label>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 min-w-0">
          <InlineTitle
            id={meeting.id}
            title={meeting.title}
            startedAt={meeting.started_at}
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
        {/* Pre-column rows render nothing rather than a guessed "Local". */}
        <EnginePill sttEngine={meeting.stt_engine} />
        <LlmPill llmModel={meeting.llm_model} />
        <MeetingCardActions id={meeting.id} starred={meeting.starred} labels={meeting.labels} />
      </div>
    </Link>
  );
}
