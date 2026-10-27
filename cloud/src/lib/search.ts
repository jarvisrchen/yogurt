import type { SearchHit } from "./api";

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

/** Hits per search page, the server's cap. */
export const SEARCH_PAGE = 25;

/** The line under the search field. A failed search shows only its error, so this says nothing then. */
export function searchStatus(s: { active: boolean; pending: boolean; hits: SearchHit[] | null; more: boolean; offset: number; error: string | null }, query: string): string {
  if (!s.active || s.error) return "";
  if (s.pending) return "Searching…";
  if (!s.hits?.length) {
    if (s.offset) return "No more matches.";
    const echo = query.replace(/["“”]/g, "").trim();
    return echo ? `No meetings match ${echo}.` : "No meetings match.";
  }
  if (s.more || s.offset) return `Matches ${s.offset + 1}–${s.offset + s.hits.length}`;
  return `${s.hits.length} ${s.hits.length === 1 ? "match" : "matches"}`;
}
