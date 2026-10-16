// A node, not a mark: the link is a non-editable inline atom so the caret cannot land inside the token.
// Click and keyboard activation are handled by the host editor's event delegation.

import { Node, mergeAttributes } from "@tiptap/core";

function formatTs(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}

export const TranscriptLink = Node.create({
  name: "transcriptLink",
  group: "inline",
  inline: true,
  atom: true,
  selectable: false,

  addAttributes() {
    return {
      ts: {
        default: 0,
        parseHTML: (el) => {
          const raw = (el as HTMLElement).getAttribute("data-ts");
          const n = Number(raw ?? "0");
          return Number.isFinite(n) ? n : 0;
        },
        renderHTML: (attrs) => ({ "data-ts": String(attrs.ts ?? 0) }),
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-transcript-link]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    const ts = (node.attrs.ts as number | undefined) ?? 0;
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-transcript-link": "",
        class: "transcript-link",
        role: "link",
        tabindex: "0",
      }),
      `↳ ${formatTs(ts)}`,
    ];
  },
});
