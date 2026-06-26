/**
 * Phase 7 (Plan 07-01) — date-bucket grouping for the Library feed.
 *
 * PRD §5.9 + D-05: meetings render under uppercase mono headers
 * `TODAY` / `YESTERDAY` / `EARLIER`. Bucket boundaries are local-time
 * midnight (so "yesterday" doesn't include 11:59 PM the night before
 * from the user's perspective).
 *
 * `bucketFor` and `groupMeetings` are extracted as pure functions so the
 * vitest suite can assert behavior without rendering React.
 */

import type { Meeting } from "../../lib/api/meetings";
import { MeetingCard } from "./MeetingCard";

export type Bucket = "TODAY" | "YESTERDAY" | "EARLIER";

const BUCKETS: ReadonlyArray<Bucket> = ["TODAY", "YESTERDAY", "EARLIER"];

/** Return midnight (00:00:00.000) of the supplied date, in local time. */
function localMidnight(d: Date): Date {
  const m = new Date(d);
  m.setHours(0, 0, 0, 0);
  return m;
}

/**
 * Classify `d` into one of TODAY / YESTERDAY / EARLIER relative to `now`.
 * Pure function — no implicit `new Date()` so tests are deterministic.
 */
export function bucketFor(d: Date, now: Date): Bucket {
  const todayMid = localMidnight(now);
  const yesterdayMid = new Date(todayMid);
  yesterdayMid.setDate(yesterdayMid.getDate() - 1);
  if (d.getTime() >= todayMid.getTime()) return "TODAY";
  if (d.getTime() >= yesterdayMid.getTime()) return "YESTERDAY";
  return "EARLIER";
}

/**
 * Bucket and return meetings preserving the input order WITHIN each
 * bucket. Callers should pass a list already sorted newest-first, which
 * is exactly what `GET /api/meetings` returns.
 */
export function groupMeetings(
  meetings: ReadonlyArray<Meeting>,
  now: Date,
): Record<Bucket, Meeting[]> {
  const out: Record<Bucket, Meeting[]> = {
    TODAY: [],
    YESTERDAY: [],
    EARLIER: [],
  };
  for (const m of meetings) {
    const b = bucketFor(new Date(m.started_at), now);
    out[b].push(m);
  }
  return out;
}

interface Props {
  meetings: ReadonlyArray<Meeting>;
  /** Optional clock override for testing — defaults to now. */
  now?: Date;
}

export function DateGroup({ meetings, now = new Date() }: Props) {
  const grouped = groupMeetings(meetings, now);
  return (
    <div className="flex flex-col gap-8">
      {BUCKETS.filter((b) => grouped[b].length > 0).map((b) => (
        <section key={b}>
          <h2 className="text-[11px] font-mono uppercase tracking-wider text-mut mb-2">
            {b}
          </h2>
          <ul className="flex flex-col gap-1">
            {grouped[b].map((m) => (
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
