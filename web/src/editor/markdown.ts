// Not prosemirror-markdown's defaultMarkdownParser: its CommonMark schema does not match the TipTap StarterKit schema.
// Parsing goes markdown-it (`html: true`, so wire-format spans pass through) to HTML to `setContent`, and the
// parseHTML rules on `aiGrey` / `transcriptLink` restore the marks.
import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";
import type { Node as PMNode } from "@tiptap/pm/model";

/**
 * Defense in depth: the server already sanitizes `enriched_md`, but mock data, pasted markdown and a
 * server allowlist bug must not be able to run script in the browser.
 */
export function markdownToHtml(md: string): string {
  const renderer = new MarkdownIt({ html: true, linkify: false, breaks: false });
  const dirty = renderer.render(md);
  return sanitizeHtml(dirty);
}

/** Allowlist matching the server's: markdown structural tags plus `<span>` with data-ai-grey, data-transcript-link, data-ts. */
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

/** MM:SS, with hours folded into minutes (3725 becomes "62:05"). */
export function formatTs(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}
