import { describe, expect, it } from "vitest";
import { snippetParts } from "./search";

describe("snippetParts", () => {
  it("splits text around marked matches", () => {
    expect(snippetParts("Who brings the kombucha to the offsite?")).toEqual([
      { text: "Who brings the ", hit: false },
      { text: "kombucha", hit: true },
      { text: " to the ", hit: false },
      { text: "offsite", hit: true },
      { text: "?", hit: false },
    ]);
  });

  it("leaves markup as plain text and collapses whitespace", () => {
    expect(snippetParts("a  <b>x</b>\n y")).toEqual([
      { text: "a <b>x</b> ", hit: false },
      { text: "y", hit: true },
    ]);
  });

  it("handles no match, an empty string, and a match at either end", () => {
    expect(snippetParts("plain")).toEqual([{ text: "plain", hit: false }]);
    expect(snippetParts("")).toEqual([]);
    expect(snippetParts("all")).toEqual([{ text: "all", hit: true }]);
    expect(snippetParts("a b")).toEqual([
      { text: "a", hit: true },
      { text: " b", hit: false },
    ]);
  });
});
