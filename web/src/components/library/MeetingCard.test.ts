import { describe, expect, it } from "vitest";
import { engineLabel, formatMeta } from "./MeetingCard";
import type { Meeting } from "../../lib/api/meetings";

const START = new Date("2026-06-25T14:45:00").getTime();

function meeting(over: Partial<Meeting> = {}): Meeting {
  return {
    id: "m1",
    title: "T",
    started_at: START,
    ended_at: null,
    notes_md: "",
    enriched_md: null,
    transcript_json: "[]",
    starred: false,
    stt_engine: null,
    labels: [],
    created_at: new Date(START).toISOString(),
    updated_at: new Date(START).toISOString(),
    ...over,
  };
}

const startTime = new Date(START).toLocaleTimeString(undefined, {
  hour: "numeric",
  minute: "2-digit",
});

describe("formatMeta", () => {
  it("shows start time and duration when the meeting has ended", () => {
    const m = meeting({ ended_at: START + 47 * 60_000 });
    expect(formatMeta(m)).toBe(`${startTime} · 47 min`);
  });

  it("omits duration entirely (no dash) while ended_at is null", () => {
    const m = meeting();
    expect(formatMeta(m)).toBe(startTime);
    expect(formatMeta(m)).not.toContain("—");
  });

  it("rounds sub-minute meetings up to 1 min", () => {
    const m = meeting({ ended_at: START + 10_000 });
    expect(formatMeta(m)).toBe(`${startTime} · 1 min`);
  });

  it("appends enhanced when enriched_md is present", () => {
    const m = meeting({ ended_at: START + 60_000, enriched_md: "x" });
    expect(formatMeta(m)).toBe(`${startTime} · 1 min · enhanced`);
  });
});

describe("engineLabel", () => {
  it("shows Local for a local · <model> stamp", () => {
    expect(engineLabel("local · small.en")).toBe("Local");
  });

  it("shows Cloud for a cloud · <model> stamp", () => {
    expect(engineLabel("cloud · nova-3")).toBe("Cloud");
  });

  it("falls back to Local for null (meetings recorded before this column)", () => {
    expect(engineLabel(null)).toBe("Local");
  });
});
