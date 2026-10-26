import type { SearchHit } from "./api";
import { keyOf } from "./library";

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

/** The line under the search field. A failed search shows only its error, so this says nothing then. */
export function searchStatus(s: { active: boolean; pending: boolean; hits: SearchHit[] | null; more: boolean; error: string | null }, query: string): string {
  if (!s.active || s.error) return "";
  if (s.pending) return "Searching…";
  if (!s.hits?.length) {
    const echo = query.replace(/["“”]/g, "").trim();
    return echo ? `No meetings match ${echo}.` : "No meetings match.";
  }
  if (s.more) return "Showing the newest matches. Add words to narrow.";
  return `${s.hits.length} ${s.hits.length === 1 ? "match" : "matches"}`;
}

/**
 * The note an opened search hit needs: "missing" when the loaded library lacks the meeting, and "newer" when its loaded copy is older than the hit.
 * The as-of view shows the library at an earlier push, so it never shows either.
 */
export function hitNote(hit: SearchHit | null, open: string | null, loaded: { pushId: number } | null, asOf: boolean): "missing" | "newer" | null {
  if (asOf || !hit || keyOf(hit) !== open) return null;
  if (!loaded) return "missing";
  return loaded.pushId < hit.push_id ? "newer" : null;
}
