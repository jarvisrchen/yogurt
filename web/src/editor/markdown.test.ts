// Round-trip: plain markdown → TipTap doc → markdown returns the original.
//
// Per CONTEXT D-05 / D-06, the round-trip surface that matters is
// `markdownToHtml` (consumed by editor.commands.setContent) + `docToMarkdown`
// (the writer). The plan-template's simple round-trip test ("Hello world"
// → doc → "Hello world") is satisfied below by spinning up a real TipTap
// editor with `yogurtExtensions()`, applying the HTML, then serializing
// back.

import { describe, expect, it } from "vitest";
import { Editor } from "@tiptap/core";
import { yogurtExtensions } from "./extensions";
import { docToMarkdown, markdownToHtml, formatTs } from "./markdown";

function roundTrip(md: string): string {
  const html = markdownToHtml(md);
  const editor = new Editor({
    extensions: yogurtExtensions(),
    content: html,
  });
  const back = docToMarkdown(editor.state.doc);
  editor.destroy();
  return back;
}

describe("markdown bridge", () => {
  it("round-trips a plain paragraph through markdownToHtml + docToMarkdown", () => {
    expect(roundTrip("Hello world").trim()).toBe("Hello world");
  });

  it("preserves an aiGrey-marked span through round-trip", () => {
    const md = `before <span data-ai-grey data-ts="120">enriched content</span> after`;
    const back = roundTrip(md);
    expect(back).toContain('data-ai-grey data-ts="120"');
    expect(back).toContain("enriched content");
  });

  it("preserves a transcriptLink inline atom through round-trip", () => {
    const md = `bullet <span data-transcript-link data-ts="662">↳ 11:02</span>`;
    const back = roundTrip(md);
    expect(back).toContain('data-transcript-link data-ts="662"');
    expect(back).toContain("↳ 11:02");
  });

  it("round-trips bold text", () => {
    expect(roundTrip("this is **bold** text").trim()).toBe(
      "this is **bold** text",
    );
  });

  it("round-trips italic text", () => {
    expect(roundTrip("this is _italic_ text").trim()).toBe(
      "this is _italic_ text",
    );
  });

  it("round-trips bold+italic text", () => {
    expect(roundTrip("this is **_both_** text").trim()).toBe(
      "this is **_both_** text",
    );
  });

  it("round-trips headings, bullet lists, and ordered lists together", () => {
    const md = [
      "# Heading",
      "",
      "- one",
      "- two with **bold**",
      "",
      "1. first",
      "2. second",
    ].join("\n");
    const back = roundTrip(md);
    expect(back).toContain("# Heading");
    expect(back).toContain("- one");
    expect(back).toContain("- two with **bold**");
    // serializeList emits a constant "1. " marker per item (not
    // incrementing) — valid markdown; browsers auto-renumber <ol> via CSS
    // counters regardless of the source numbers.
    expect(back).toContain("1. first");
    expect(back).toContain("1. second");
  });

  it("formats seconds as MM:SS zero-padded", () => {
    expect(formatTs(0)).toBe("00:00");
    expect(formatTs(5)).toBe("00:05");
    expect(formatTs(65)).toBe("01:05");
    expect(formatTs(662)).toBe("11:02");
    expect(formatTs(3725)).toBe("62:05");
  });
});
