// Not prosemirror-markdown's defaultMarkdownParser: its CommonMark schema does not match the TipTap StarterKit schema.
// Parsing goes markdown-it (`html: true`, so wire-format spans pass through) to HTML to `setContent`, and the
// parseHTML rules on `aiGrey` / `transcriptLink` restore the marks.
import type { Node as PMNode } from "@tiptap/pm/model";
import { formatTs, markdownToHtml } from "./html";

export { formatTs, markdownToHtml };

export function markdownToDoc(_schema: unknown, md: string): string {
  return markdownToHtml(md);
}

/** Serializes `aiGrey` marks and `transcriptLink` nodes back to wire-format spans. */
export function docToMarkdown(doc: PMNode): string {
  const out: string[] = [];
  doc.forEach((block) => {
    out.push(serializeBlock(block));
  });
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
      // Wrap innermost-first so aiGrey's span wraps the markdown syntax too. markdown-it still parses
      // ** and _ inside span content, so it round-trips.
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
    out += serializeInline(child);
  });
  return out;
}
