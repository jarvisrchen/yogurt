/**
 * Phase 7 (Plan 07-01) — per-day grouping for the Library feed.
 *
 * Meetings render under uppercase mono day headers:
 *   Today / Yesterday / weekday name (2–6 days back) / "Aug 13"-style
 *   short date beyond that (with the year appended once it differs).
 * Day boundaries are local-time midnight (so "yesterday" doesn't include
 * 11:59 PM the night before from the user's perspective).
 *
 * `dayLabel` and `groupMeetingsByDay` are pure functions so the vitest
 * suite can assert behavior without rendering React.
 */

import type { Meeting } from "../../lib/api/meetings";
import { MeetingCard } from "./MeetingCard";

/** Return midnight (00:00:00.000) of the supplied date, in local time. */
function localMidnight(d: Date): Date {
  const m = new Date(d);
  m.setHours(0, 0, 0, 0);
  return m;
}

/**
 * Human day header for `d` relative to `now`:
 *   same day      → "Today"
 *   previous day  → "Yesterday"
 *   2–6 days back → weekday name ("Monday")
 *   older         → "Aug 13" (plus ", 2025" once the year differs)
 * Pure function — no implicit `new Date()` so tests are deterministic.
 */
export function dayLabel(d: Date, now: Date): string {
  // Round instead of truncate so DST-shifted 23h/25h days still count
  // as exactly one calendar day.
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
  /** Local-midnight epoch ms of the day — stable React key. */
  key: number;
  label: string;
  meetings: Meeting[];
}

/**
 * Group meetings by local calendar day, preserving the input order both
 * WITHIN each group and across groups. Callers pass a list already
 * sorted newest-first, which is what `GET /api/meetings` returns.
 */
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
  /** Optional clock override for testing — defaults to now. */
  now?: Date;
}

export function DateGroup({ meetings, now = new Date() }: Props) {
  const groups = groupMeetingsByDay(meetings, now);
  return (
    <div className="flex flex-col gap-8">
      {groups.map((g) => (
        <section key={g.key}>
          <h2 className="text-[11px] font-mono uppercase tracking-wider text-mut mb-2">
            {g.label}
          </h2>
          <ul className="flex flex-col gap-1">
            {g.meetings.map((m) => (
              <li key={m.id}>
                <MeetingCard meeting={m} />
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
