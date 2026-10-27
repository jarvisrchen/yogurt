import { describe, expect, it } from "vitest";
import type { SearchHit } from "./api";
import { searchStatus, snippetParts } from "./search";

describe("snippetParts", () => {
  it("splits text around marked matches", () => {
    expect(snippetParts("Who brings the \uE000kombucha\uE001 to the \uE000offsite\uE001?")).toEqual([
      { text: "Who brings the ", hit: false },
      { text: "kombucha", hit: true },
      { text: " to the ", hit: false },
      { text: "offsite", hit: true },
      { text: "?", hit: false },
    ]);
  });

  it("leaves markup as plain text and collapses whitespace", () => {
    expect(snippetParts("a  <b>x</b>\n \uE000y\uE001")).toEqual([
      { text: "a <b>x</b> ", hit: false },
      { text: "y", hit: true },
    ]);
  });

  it("handles no match, an empty string, and a match at either end", () => {
    expect(snippetParts("plain")).toEqual([{ text: "plain", hit: false }]);
    expect(snippetParts("")).toEqual([]);
    expect(snippetParts("\uE000all\uE001")).toEqual([{ text: "all", hit: true }]);
    expect(snippetParts("\uE000a\uE001 b")).toEqual([
      { text: "a", hit: true },
      { text: " b", hit: false },
    ]);
  });
});

const hit = (over: Partial<SearchHit> = {}): SearchHit => ({ device_id: "d", meeting_id: "m", push_id: 5, started_at: 0, ended_at: null, title: "T", snippet: null, more: false, ...over });

describe("searchStatus", () => {
  const idle = { active: true, pending: false, hits: [] as SearchHit[], more: false, offset: 0, error: null };
  it("says nothing without a query or after a failure", () => {
    expect(searchStatus({ ...idle, active: false }, "")).toBe("");
    expect(searchStatus({ ...idle, hits: null, error: "boom" }, "kombucha")).toBe("");
  });
  it("says it is searching while pending", () => {
    expect(searchStatus({ ...idle, pending: true }, "kombucha")).toBe("Searching…");
  });
  it("echoes a miss without its quotes", () => {
    expect(searchStatus(idle, " kombucha ")).toBe("No meetings match kombucha.");
    expect(searchStatus(idle, '"vendor contract" or pears')).toBe("No meetings match vendor contract or pears.");
    expect(searchStatus(idle, "\u201cvendor\u201d")).toBe("No meetings match vendor.");
    expect(searchStatus(idle, "James'")).toBe("No meetings match James'.");
    expect(searchStatus(idle, '"')).toBe("No meetings match.");
  });
  it("counts the hits of a single page", () => {
    expect(searchStatus({ ...idle, hits: [hit()] }, "x")).toBe("1 match");
    expect(searchStatus({ ...idle, hits: [hit(), hit()] }, "x")).toBe("2 matches");
  });
  it("gives the range once there is more than one page", () => {
    const page = Array.from({ length: 25 }, () => hit());
    expect(searchStatus({ ...idle, hits: page, more: true }, "x")).toBe("Matches 1–25");
    expect(searchStatus({ ...idle, hits: page, more: true, offset: 25 }, "x")).toBe("Matches 26–50");
    expect(searchStatus({ ...idle, hits: [hit(), hit()], offset: 50 }, "x")).toBe("Matches 51–52");
  });
  it("says so when a later page has nothing left", () => {
    expect(searchStatus({ ...idle, offset: 25 }, "x")).toBe("No more matches.");
  });
});
