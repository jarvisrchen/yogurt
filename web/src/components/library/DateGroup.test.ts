import { describe, expect, it } from "vitest";
import { dayLabel, groupMeetingsByDay } from "./DateGroup";
import type { Meeting } from "../../lib/api/meetings";

function fakeMeeting(id: string, startedAt: Date): Meeting {
  return {
    id,
    title: "T",
    started_at: startedAt.getTime(),
    ended_at: null,
    notes_md: "",
    enriched_md: null,
    transcript_json: "[]",
    starred: false,
    stt_engine: null,
    llm_model: null,
    labels: [],
    created_at: startedAt.toISOString(),
    updated_at: startedAt.toISOString(),
  };
}

describe("dayLabel", () => {
  // 2026-06-25 is a Thursday.
  const now = new Date("2026-06-25T15:00:00");

  it("labels the same day Today", () => {
    expect(dayLabel(new Date("2026-06-25T01:30:00"), now)).toBe("Today");
  });

  it("treats midnight as the start of Today (not end of Yesterday)", () => {
    expect(dayLabel(new Date("2026-06-25T00:00:00"), now)).toBe("Today");
  });

  it("labels the prior day Yesterday", () => {
    expect(dayLabel(new Date("2026-06-24T23:59:00"), now)).toBe("Yesterday");
  });

  it("labels 2–6 days back with the weekday name", () => {
    for (const iso of ["2026-06-23T12:00:00", "2026-06-19T08:00:00"]) {
      const d = new Date(iso);
      expect(dayLabel(d, now)).toBe(
        d.toLocaleDateString(undefined, { weekday: "long" }),
      );
    }
  });

  it("labels a week or more back with a short date", () => {
    const d = new Date("2026-06-18T12:00:00"); // exactly 7 days back
    expect(dayLabel(d, now)).toBe(
      d.toLocaleDateString(undefined, { month: "short", day: "numeric" }),
    );
  });

  it("appends the year once it differs from the current one", () => {
    const d = new Date("2025-08-13T12:00:00");
    expect(dayLabel(d, now)).toBe(
      d.toLocaleDateString(undefined, {
        month: "short",
        day: "numeric",
        year: "numeric",
      }),
    );
  });
});

describe("groupMeetingsByDay", () => {
  const now = new Date("2026-06-25T15:00:00");

  it("groups per calendar day, preserving input order", () => {
    const a = fakeMeeting("a", new Date("2026-06-25T10:00:00")); // Today
    const b = fakeMeeting("b", new Date("2026-06-25T09:00:00")); // Today
    const c = fakeMeeting("c", new Date("2026-06-24T18:00:00")); // Yesterday
    const d = fakeMeeting("d", new Date("2026-06-20T18:00:00")); // Saturday
    const e = fakeMeeting("e", new Date("2026-06-10T09:00:00")); // Jun 10
    const groups = groupMeetingsByDay([a, b, c, d, e], now);
    expect(groups.map((g) => g.meetings.map((m) => m.id))).toEqual([
      ["a", "b"],
      ["c"],
      ["d"],
      ["e"],
    ]);
    expect(groups[0]!.label).toBe("Today");
    expect(groups[1]!.label).toBe("Yesterday");
  });

  it("returns no groups for no meetings", () => {
    expect(groupMeetingsByDay([], now)).toEqual([]);
  });
});
