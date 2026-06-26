// `aiGrey` mark + `transcriptLink` node — jsdom render tests.
//
// These pin the wire-format ↔ TipTap parse/render surface that Plan 04-03's
// HTTP handler depends on. Promote-on-edit is exercised in Plan 04-04's
// manual smoke (jsdom doesn't reproduce ProseMirror input rules identically
// to a real browser, so unit-testing it here would over-fit on jsdom quirks).

import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { useEditor, EditorContent } from "@tiptap/react";
import { yogurtExtensions } from "../extensions";

function Mount({ html }: { html: string }) {
  const editor = useEditor({ extensions: yogurtExtensions(), content: html });
  return <EditorContent editor={editor} />;
}

describe("aiGrey mark", () => {
  it("renders [data-ai-grey] ancestor with .ai-grey class", () => {
    const { container } = render(
      <Mount html={`<p>before <span data-ai-grey data-ts="120">enriched</span> after</p>`} />,
    );
    const span = container.querySelector("[data-ai-grey]");
    expect(span).not.toBeNull();
    expect(span!.classList.contains("ai-grey")).toBe(true);
    expect(span!.textContent).toContain("enriched");
  });
});

describe("transcriptLink node", () => {
  it("renders ↳ HH:MM formatted from data-ts attribute", () => {
    const { container } = render(
      <Mount
        html={`<p>bullet <span data-transcript-link data-ts="662">↳ 11:02</span></p>`}
      />,
    );
    const link = container.querySelector("[data-transcript-link]");
    expect(link).not.toBeNull();
    expect(link!.getAttribute("role")).toBe("link");
    expect(link!.textContent).toContain("↳ 11:02");
  });
});
