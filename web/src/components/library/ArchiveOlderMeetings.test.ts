import { describe, expect, it } from "vitest";
import { localMidnight } from "./ArchiveOlderMeetings";

describe("localMidnight", () => {
  it("reads a date input value as the start of that day in local time", () => {
    expect(localMidnight("2026-01-01")).toBe(new Date(2026, 0, 1).getTime());
  });

  it("is null for an empty field", () => {
    expect(localMidnight("")).toBeNull();
  });
});
