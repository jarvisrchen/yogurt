import { describe, expect, it } from "vitest";
import type { LibraryRow, MeetingRow } from "./api";
import { groupMeetingsByDay } from "@web/components/library/dayGroups";
import { defaultTab, duration, forDevice, historyLabel, summaryEntry, tabsFor, toEntry } from "./library";

const row = (id: string, device: string, at: string, over: Partial<LibraryRow> = {}): LibraryRow => ({
  device_id: device,
  push_id: 1,
  pushed_at: "2026-10-22T14:10:00Z",
  meeting_id: id,
  title: id,
  started_at: new Date(at).getTime(),
  ended_at: null,
  starred: false,
  labels: [],
  total: 3,
  ...over,
});

describe("historyLabel", () => {
  it("counts with the right plural", () => {
    expect(historyLabel(1, 0)).toBe("1 meeting updated");
    expect(historyLabel(2, 0)).toBe("2 meetings updated");
    expect(historyLabel(0, 12)).toBe("12 meetings removed");
    expect(historyLabel(0, 1)).toBe("1 meeting removed");
  });
  it("combines both", () => {
    expect(historyLabel(2, 1)).toBe("2 meetings updated, 1 removed");
    expect(historyLabel(1, 3)).toBe("1 meeting updated, 3 removed");
  });
});

describe("library entries", () => {
  const rows = [row("c", "d1", "2026-10-25T15:00:00"), row("b", "d2", "2026-10-25T09:00:00"), row("a", "d1", "2026-10-16T10:00:00")];
  it("keeps the server's newest first order and groups by day", () => {
    const groups = groupMeetingsByDay(rows.map(summaryEntry), new Date("2026-10-25T18:00:00"));
    expect(groups.map((g) => [g.label, g.meetings.map((m) => m.title)])).toEqual([
      ["Today", ["c", "b"]],
      ["Oct 16", ["a"]],
    ]);
  });
  it("has no content until the body loads", () => {
    expect(summaryEntry(rows[0])).toMatchObject({ key: "d1/c", notes: "", enhanced: null, segments: [], pushId: 1 });
  });
  it("filters by device and keeps everything for All devices", () => {
    const entries = rows.map(summaryEntry);
    expect(forDevice(entries, null)).toHaveLength(3);
    expect(forDevice(entries, "d1").map((e) => e.title)).toEqual(["c", "a"]);
    expect(forDevice(entries, "nope")).toEqual([]);
  });
  it("names an untitled meeting", () => {
    expect(summaryEntry(row("a", "d1", "2026-10-16T10:00:00", { title: "  " })).title).toBe("Untitled meeting");
  });
});

const body = (over: Partial<MeetingRow["body"]["meeting"]>): MeetingRow => ({
  push_id: 4,
  pushed_at: "2026-10-22T14:10:00Z",
  body: { v: 1, meeting: { id: "a", title: "A", started_at: 1000, ended_at: null, notes_md: "", enriched_md: null, transcript_json: "[]", ...over } },
});

describe("a loaded meeting", () => {
  it("takes its key from the device and meeting it was asked for", () => {
    expect(toEntry("d1", "m/1", body({ notes_md: "hi" }))).toMatchObject({ key: "d1/m/1", pushId: 4, notes: "hi", title: "A" });
  });
});

describe("tabs", () => {
  const entry = (over: Partial<MeetingRow["body"]["meeting"]>) => toEntry("d1", "a", body(over));
  it("hides a tab with no content", () => {
    expect(tabsFor(entry({ notes_md: "hi" }))).toEqual(["notes"]);
    const all = entry({ notes_md: "hi", enriched_md: "# x", transcript_json: '[{"ts_ms":1,"channel":"me","text":"yo"}]' });
    expect(tabsFor(all)).toEqual(["notes", "enhanced", "transcript"]);
    expect(defaultTab(all)).toBe("enhanced");
  });
  it("keeps Notes for an empty meeting and ignores a broken transcript", () => {
    expect(tabsFor(entry({ transcript_json: "not json" }))).toEqual(["notes"]);
  });
});

describe("duration", () => {
  it("is missing for a live or zero length meeting", () => {
    expect(duration({ started_at: 1000, endedAt: null })).toBeNull();
    expect(duration({ started_at: 1000, endedAt: 1000 })).toBeNull();
    expect(duration({ started_at: 0, endedAt: 11 * 60_000 })).toBe("11 min");
  });
});
