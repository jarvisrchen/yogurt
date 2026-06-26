import { describe, expect, it } from "vitest";
import { bucketFor, groupMeetings } from "./DateGroup";
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
    created_at: startedAt.toISOString(),
    updated_at: startedAt.toISOString(),
  };
}

describe("bucketFor", () => {
  const now = new Date("2026-06-25T15:00:00");

  it("classifies same-day as TODAY", () => {
    const d = new Date("2026-06-25T01:30:00");
    expect(bucketFor(d, now)).toBe("TODAY");
  });

  it("classifies the prior day as YESTERDAY", () => {
    const d = new Date("2026-06-24T23:59:00");
    expect(bucketFor(d, now)).toBe("YESTERDAY");
  });

  it("classifies 2+ days back as EARLIER", () => {
    const d = new Date("2026-06-23T12:00:00");
    expect(bucketFor(d, now)).toBe("EARLIER");
  });

  it("treats midnight as the start of TODAY (not end of YESTERDAY)", () => {
    const midnight = new Date("2026-06-25T00:00:00");
    expect(bucketFor(midnight, now)).toBe("TODAY");
  });
});

describe("groupMeetings", () => {
  const now = new Date("2026-06-25T15:00:00");

  it("buckets each meeting into the right bin, preserving input order", () => {
    const a = fakeMeeting("a", new Date("2026-06-25T10:00:00")); // TODAY
    const b = fakeMeeting("b", new Date("2026-06-25T09:00:00")); // TODAY
    const c = fakeMeeting("c", new Date("2026-06-24T18:00:00")); // YESTERDAY
    const d = fakeMeeting("d", new Date("2026-06-20T18:00:00")); // EARLIER
    const grouped = groupMeetings([a, b, c, d], now);
    expect(grouped.TODAY.map((m) => m.id)).toEqual(["a", "b"]);
    expect(grouped.YESTERDAY.map((m) => m.id)).toEqual(["c"]);
    expect(grouped.EARLIER.map((m) => m.id)).toEqual(["d"]);
  });
});
