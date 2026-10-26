// Markdown to sanitized HTML. Kept apart from markdown.ts so a consumer without TipTap can import it.
import MarkdownIt from "markdown-it";
import DOMPurify from "dompurify";

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
function sanitizeHtml(input: string): string {
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

/** MM:SS, with hours folded into minutes (3725 becomes "62:05"). */
export function formatTs(seconds: number): string {
  const mins = Math.floor(seconds / 60);
  const secs = Math.floor(seconds % 60);
  return `${mins.toString().padStart(2, "0")}:${secs.toString().padStart(2, "0")}`;
}
