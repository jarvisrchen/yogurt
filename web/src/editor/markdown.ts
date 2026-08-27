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
import DOMPurify from "dompurify";
import type { Node as PMNode } from "@tiptap/pm/model";

/**
 * Render markdown to HTML for TipTap consumption.
 *
 * `html: true` is mandatory — our wire-format spans
 * (`<span data-ai-grey…>`, `<span data-transcript-link…>`) must survive
 * verbatim so the TipTap parseHTML rules pick them up.
 *
 * BL-2 (Phase 4 review): even though the server runs ammonia + html-escape
 * on the enriched_md before sending, we DOMPurify on the client as
 * defense-in-depth. Three reasons:
 *   1. Tests + dev tools that bypass the server (mock data, snapshot
 *      fixtures, future copy-paste of saved markdown) skip the server
 *      sanitizer.
 *   2. A bug in the server allowlist (broken ammonia config, missed
 *      handler path) should not pop alerts in the user's browser.
 *   3. If the user pastes attacker-supplied markdown directly into the
 *      editor, the round-trip through markdown-it still has to be safe.
 *
 * The allowlist matches the server's: <span> + data-ai-grey,
 * data-transcript-link, data-ts.
 */
export function markdownToHtml(md: string): string {
  const renderer = new MarkdownIt({ html: true, linkify: false, breaks: false });
  const dirty = renderer.render(md);
  return sanitizeHtml(dirty);
}

/**
 * Allowlist sanitizer for the editor input pipeline.
 *
 * - ALLOWED tags: all markdown structural tags (p, h1-h6, ul, ol, li,
 *   blockquote, code, pre, em, strong, br, hr) plus our wire-format
 *   <span>. NO <script>, <iframe>, <object>, <embed>, <style>, <link>,
 *   <form>, <input>, <img>.
 * - ALLOWED attributes on <span>: data-ai-grey, data-transcript-link,
 *   data-ts only.
 * - ALL event handlers (on*) stripped.
 * - javascript: / data: / vbscript: URLs stripped (DOMPurify default).
 */
export function sanitizeHtml(input: string): string {
  return DOMPurify.sanitize(input, {
    ALLOWED_TAGS: [
      "p",
      "br",
      "hr",
      "h1",
      "h2",
      "h3",
      "h4",
      "h5",
      "h6",
      "ul",
      "ol",
      "li",
      "blockquote",
      "code",
      "pre",
      "em",
      "strong",
      "span",
    ],
    ALLOWED_ATTR: ["data-ai-grey", "data-transcript-link", "data-ts"],
    KEEP_CONTENT: true,
  });
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
      // Bold/italic are the only two inline marks StarterKit contributes
      // beyond aiGrey/transcriptLink (Meeting.tsx task 4 — "that is all
      // StarterKit allows here"). Wrap innermost-first so aiGrey's span
      // (if present) wraps the markdown syntax chars too — round-trips
      // cleanly since markdown-it still parses ** / _ inside html_inline
      // span content.
      const isBold = child.marks.some((m) => m.type.name === "bold");
      const isItalic = child.marks.some((m) => m.type.name === "italic");
      let decorated = text;
      if (isBold && isItalic) decorated = `**_${decorated}_**`;
      else if (isBold) decorated = `**${decorated}**`;
      else if (isItalic) decorated = `_${decorated}_`;
      if (aiMark) {
        const ts = (aiMark.attrs.transcriptTs as number | undefined) ?? 0;
        out += `<span data-ai-grey data-ts="${ts}">${decorated}</span>`;
      } else {
        out += decorated;
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
