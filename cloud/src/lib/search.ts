/** The server wraps each matched run in these private-use characters, so no text a meeting holds can collide with them. */
const MARK = /[]/;

/** Splits a server headline into plain text and matched runs. Both render as text, so no markup in a meeting reaches the DOM. */
export const snippetParts = (snippet: string) =>
  snippet
    .replace(/\s+/g, " ")
    .trim()
    .split(MARK)
    .map((text, i) => ({ text, hit: i % 2 === 1 }))
    .filter((p) => p.text);
