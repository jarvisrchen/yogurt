// Markdown ↔ ProseMirror bridge for the YogurtEditor.
//
// Phase 4 wire-format invariants (CONTEXT D-05 / D-06):
//   - AI runs in markdown appear as `<span data-ai-grey data-ts="N">…</span>`
//   - Transcript deep-links appear as `<span data-transcript-link data-ts="N">↳ HH:MM</span>`
//
// We do NOT use prosemirror-markdown's `defaultMarkdownParser` because it
// targets prosemirror's CommonMark schema, which is incompatible with the
// TipTap StarterKit schema our editor uses. Instead the parsing path is:
//
//     markdown string
//       → markdown-it (with html:true so wire-format spans pass through)
//       → HTML string
//       → TipTap editor.commands.setContent(html, { parseOptions })
//
// The HTML pass-through preserves our marker spans intact; TipTap's
// `parseHTML` rules on the `aiGrey` mark and `transcriptLink` node
// (defined in `marks/aiGrey.ts` and `marks/transcriptLink.ts`) then pick
// them up and re-instantiate the editor doc with marks attached.
//
// For serialization (doc → markdown) we do a small ProseMirror walker that
// emits markdown blocks, then wraps inline runs carrying the `aiGrey` mark
// back into the wire-format spans. This is the single round-trip surface
// the round-trip test pins.

import MarkdownIt from "markdown-it";
import type { Node as PMNode } from "@tiptap/pm/model";

/**
 * Render markdown to HTML for TipTap consumption.
 *
 * `html: true` is mandatory — our wire-format spans
 * (`<span data-ai-grey…>`, `<span data-transcript-link…>`) must survive
 * verbatim so the TipTap parseHTML rules pick them up.
 */
export function markdownToHtml(md: string): string {
  const renderer = new MarkdownIt({ html: true, linkify: false, breaks: false });
  return renderer.render(md);
}

/**
 * Convenience alias retained for API symmetry with the plan's
 * `markdownToDoc(schema, md)` shape. Returns HTML — TipTap's
 * `editor.commands.setContent(html, { parseOptions: { preserveWhitespace: 'full' } })`
 * builds the actual doc.
 */
export function markdownToDoc(_schema: unknown, md: string): string {
  return markdownToHtml(md);
}

/**
 * Serialize a TipTap ProseMirror doc back to markdown, preserving our
 * `aiGrey` mark and `transcriptLink` inline-atom node as wire-format spans.
 *
 * Round-trip contract (locked by `markdown.test.ts`): plain paragraph
 * markdown survives unchanged.
 */
export function docToMarkdown(doc: PMNode): string {
  const out: string[] = [];
  doc.forEach((block) => {
    out.push(serializeBlock(block));
  });
  // Trim trailing blank lines but preserve a single terminating newline.
  return out.join("\n\n").replace(/\n+$/, "");
}

function serializeBlock(node: PMNode): string {
  switch (node.type.name) {
    case "paragraph":
      return serializeInline(node);
    case "heading": {
      const level = (node.attrs.level as number | undefined) ?? 1;
      return `${"#".repeat(level)} ${serializeInline(node)}`;
    }
    case "bulletList":
      return serializeList(node, "- ");
    case "orderedList":
      return serializeList(node, "1. ");
    case "blockquote": {
      const inner: string[] = [];
      node.forEach((child) => inner.push(serializeBlock(child)));
      return inner
        .join("\n\n")
        .split("\n")
        .map((l) => (l ? `> ${l}` : ">"))
        .join("\n");
    }
    case "codeBlock": {
      const lang = (node.attrs.language as string | undefined) ?? "";
      return "```" + lang + "\n" + (node.textContent ?? "") + "\n```";
    }
    case "horizontalRule":
      return "---";
    default:
      return serializeInline(node);
  }
}

function serializeList(list: PMNode, marker: string): string {
  const lines: string[] = [];
  list.forEach((item) => {
    // listItem usually contains one or more block children.
    const itemLines: string[] = [];
    item.forEach((child) => itemLines.push(serializeBlock(child)));
    const joined = itemLines.join("\n");
    const firstLine = joined.split("\n")[0] ?? "";
    const rest = joined.split("\n").slice(1);
    lines.push(`${marker}${firstLine}`);
    for (const r of rest) {
      lines.push("  " + r);
    }
  });
  return lines.join("\n");
}

function serializeInline(node: PMNode): string {
  let out = "";
  node.forEach((child) => {
    if (child.type.name === "transcriptLink") {
      const ts = (child.attrs.ts as number | undefined) ?? 0;
      out += `<span data-transcript-link data-ts="${ts}">↳ ${formatTs(ts)}</span>`;
      return;
    }
    if (child.type.name === "hardBreak") {
      out += "  \n";
      return;
    }
    if (child.isText) {
      const text = child.text ?? "";
      const aiMark = child.marks.find((m) => m.type.name === "aiGrey");
      if (aiMark) {
        const ts = (aiMark.attrs.transcriptTs as number | undefined) ?? 0;
        out += `<span data-ai-grey data-ts="${ts}">${text}</span>`;
      } else {
        out += text;
      }
      return;
    }
    // Fallback: walk inline children.
    out += serializeInline(child);
  });
  return out;
}

/**
 * Format a seconds value as MM:SS (zero-padded). Hours collapse into the
 * minutes field (e.g. 3725 → "62:05") — Phase 4 meetings cap at ~2 hours
 * and the editor's link affordance is small so HH:MM:SS would be visually
 * noisy.
 */
export function formatTs(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}
