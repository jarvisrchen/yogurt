export interface SnippetPart {
  text: string;
  hit: boolean;
}

/** The server wraps each matched word in these private-use characters, so no text a meeting holds can collide with them. */
const START = "";
const STOP = "";

/** Splits a server headline into plain text and matched runs. Callers render both as text, so no markup in a meeting reaches the DOM. */
export function snippetParts(snippet: string): SnippetPart[] {
  const parts: SnippetPart[] = [];
  const text = snippet.replace(/\s+/g, " ").trim();
  let hit = false;
  let rest = text;
  while (rest) {
    const at = rest.search(hit ? STOP : START);
    const chunk = at < 0 ? rest : rest.slice(0, at);
    if (chunk) parts.push({ text: chunk, hit });
    if (at < 0) break;
    rest = rest.slice(at + 1);
    hit = !hit;
  }
  return parts;
}
