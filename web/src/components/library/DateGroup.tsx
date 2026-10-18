import type { Meeting } from "../../lib/api/meetings";
import { MeetingCard } from "./MeetingCard";
import { LABEL } from "../settings/labelClass";

function localMidnight(d: Date): Date {
  const m = new Date(d);
  m.setHours(0, 0, 0, 0);
  return m;
}

export function dayLabel(d: Date, now: Date): string {
  // Round so DST-shifted 23h/25h days still count as one calendar day.
  const daysAgo = Math.round(
    (localMidnight(now).getTime() - localMidnight(d).getTime()) / 86_400_000,
  );
  if (daysAgo === 0) return "Today";
  if (daysAgo === 1) return "Yesterday";
  if (daysAgo >= 2 && daysAgo <= 6) {
    return d.toLocaleDateString(undefined, { weekday: "long" });
  }
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    ...(d.getFullYear() !== now.getFullYear() ? { year: "numeric" } : {}),
  });
}

export interface DayGroup {
  key: number;
  label: string;
  meetings: Meeting[];
}

export function groupMeetingsByDay(
  meetings: ReadonlyArray<Meeting>,
  now: Date,
): DayGroup[] {
  const out: DayGroup[] = [];
  const byKey = new Map<number, DayGroup>();
  for (const m of meetings) {
    const started = new Date(m.started_at);
    const key = localMidnight(started).getTime();
    let group = byKey.get(key);
    if (!group) {
      group = { key, label: dayLabel(started, now), meetings: [] };
      byKey.set(key, group);
      out.push(group);
    }
    group.meetings.push(m);
  }
  return out;
}

interface Props {
  meetings: ReadonlyArray<Meeting>;
  now?: Date;
  activeId?: string | null;
  selectionActive?: boolean;
  selectedIds?: ReadonlySet<string>;
  onToggleSelect?: (id: string, shiftKey: boolean) => void;
}

export function DateGroup({
  meetings,
  now = new Date(),
  activeId,
  selectionActive,
  selectedIds,
  onToggleSelect,
}: Props) {
  const groups = groupMeetingsByDay(meetings, now);
  return (
    <div className="flex flex-col gap-8">
      {groups.map((g) => (
        <section key={g.key}>
          <h2 className={`${LABEL} mb-2`}>
            {g.label}
          </h2>
          <ul className="flex flex-col gap-2">
            {g.meetings.map((m) => (
              <li key={m.id}>
                <MeetingCard
                  meeting={m}
                  activeId={activeId}
                  selectionActive={selectionActive}
                  selected={selectedIds?.has(m.id) ?? false}
                  onToggleSelect={onToggleSelect}
                />
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
