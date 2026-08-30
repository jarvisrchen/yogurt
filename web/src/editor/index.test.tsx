// YogurtEditor host contract: markdown the editor emits via `onChange` and
// gets handed straight back as `enrichedMarkdown` (MeetingPost keeps the
// tab's markdown in state) must NOT be re-applied with `setContent` -
// the markdown round-trip trims the trailing space you just typed, so
// spaces never landed in My notes.

import { act, render, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { YogurtEditor } from "./index";

function EchoHost() {
  const [md, setMd] = useState("hello");
  return (
    <YogurtEditor initialMarkdown="hello" enrichedMarkdown={md} onChange={setMd} />
  );
}

describe("YogurtEditor echo guard", () => {
  it("keeps a trailing space when the host echoes onChange back as enrichedMarkdown", async () => {
    const { container } = render(<EchoHost />);
    const pm = container.querySelector(".ProseMirror") as HTMLElement;
    const text = pm.querySelector("p")!.firstChild as Text;
    // Mutate the contenteditable the way a keystroke does; ProseMirror's
    // DOMObserver turns the mutation into a transaction -> onChange.
    await act(async () => {
      text.data = "hello ";
      await new Promise((r) => setTimeout(r, 20));
    });
    await waitFor(() => {
      expect(pm.textContent).toBe("hello ");
    });
  });
});
