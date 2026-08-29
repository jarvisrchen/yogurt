import { describe, expect, it } from "vitest";
import { metaParts } from "./MeetingCard";

const texts = (m: Meeting) => metaParts(m).map((p) => p.text);
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
    llm_model: null,
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

describe("metaParts", () => {
  it("shows start time and duration when the meeting has ended", () => {
    const m = meeting({ ended_at: START + 47 * 60_000, enriched_md: "x" });
    expect(texts(m)).toEqual([startTime, "47 min"]);
  });

  it("omits duration while ended_at is null", () => {
    expect(texts(meeting())).toEqual([startTime]);
  });

  it("rounds sub-minute meetings up to 1 min", () => {
    const m = meeting({ ended_at: START + 10_000, enriched_md: "x" });
    expect(texts(m)).toEqual([startTime, "1 min"]);
  });

  it("does not tag the normal case (ended + enhanced)", () => {
    const m = meeting({ ended_at: START + 60_000, enriched_md: "x" });
    expect(texts(m)).toEqual([startTime, "1 min"]);
  });

  it("flags 'not enhanced' (warn tone) only when ended without enrichment", () => {
    const ended = meeting({ ended_at: START + 60_000, enriched_md: null });
    expect(metaParts(ended).at(-1)).toEqual({ text: "not enhanced", tone: "warn" });
    // Still live: no verdict yet, so no flag.
    expect(texts(meeting({ enriched_md: null }))).toEqual([startTime]);
  });
});
