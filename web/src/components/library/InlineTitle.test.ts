import { describe, expect, it } from "vitest";
import { splitTitle } from "./InlineTitle";

const OCT6 = new Date("2026-10-06T12:00:00").getTime();

describe("splitTitle", () => {
  it("peels the meeting's own date off as the fixed suffix", () => {
    expect(splitTitle("Steven 1:1 10/6/26", OCT6)).toEqual({ name: "Steven 1:1", date: "10/6/26" });
  });

  it("keeps a title without its date whole, still offering the date to append", () => {
    expect(splitTitle("Standup 9/1/26", OCT6)).toEqual({ name: "Standup 9/1/26", date: "10/6/26" });
  });

  it("has no suffix without a start time", () => {
    expect(splitTitle("Standup 10/6/26")).toEqual({ name: "Standup 10/6/26", date: null });
  });
});
