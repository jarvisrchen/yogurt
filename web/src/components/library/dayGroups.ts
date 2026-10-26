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

export interface DayGroup<M> {
  key: number;
  label: string;
  meetings: M[];
}

export function groupMeetingsByDay<M extends { started_at: number }>(
  meetings: ReadonlyArray<M>,
  now: Date = new Date(),
): DayGroup<M>[] {
  const out: DayGroup<M>[] = [];
  const byKey = new Map<number, DayGroup<M>>();
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
