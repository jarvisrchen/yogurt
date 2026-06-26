/**
 * Phase 7 (Plan 07-01 + 07-02) — Library home route (`/`).
 *
 * Layout per PRD §5.9 + D-01..D-06:
 *   ┌────────────────┬──────────────────────────────────────────┐
 *   │  <Sidebar />   │  <Greeting />   <SearchPill />           │
 *   │   (212px)      │  <DateGroup meetings=... />              │
 *   └────────────────┴──────────────────────────────────────────┘
 *
 * Plan 07-02 wires the search pill to `useMeetingsSearch` (FTS5).
 * When the user types, the pill debounces and lifts the query up here;
 * we then choose between the chronological `useMeetings()` cache and
 * the `useMeetingsSearch(query)` cache for the rendered list. The
 * `<DateGroup>` component handles empty buckets gracefully — when
 * search yields zero results we render a small "No matches" line so
 * the user gets an explicit confirmation that the query ran.
 */

import { useState } from "react";
import { useMeetings, useMeetingsSearch } from "../lib/api/meetings";
import { DateGroup } from "../components/library/DateGroup";
import { Greeting } from "../components/library/Greeting";
import { SearchPill } from "../components/library/SearchPill";
import { Sidebar } from "../components/library/Sidebar";

export function Library() {
  const [query, setQuery] = useState("");
  const trimmedQuery = query.trim();
  const isSearching = trimmedQuery.length > 0;

  const all = useMeetings();
  const found = useMeetingsSearch(query);

  const meetings = isSearching ? (found.data ?? []) : (all.data ?? []);
  const isLoading = isSearching ? found.isLoading : all.isLoading;
  const error = isSearching ? found.error : all.error;

  return (
    <div className="flex">
      <Sidebar />
      <main className="flex-1 px-12 py-10 max-w-[860px]">
        <div className="flex items-start justify-between mb-8">
          <Greeting count={isSearching ? meetings.length : (all.data ?? []).length} />
          <SearchPill value={query} onChange={setQuery} />
        </div>

        {isLoading && (
          <div className="text-[13px] font-mono text-mut">Loading…</div>
        )}
        {error && (
          <div className="text-[13px] font-mono text-straw">
            Couldn't load meetings: {error.message}
          </div>
        )}
        {!isLoading && !error && meetings.length === 0 && (
          isSearching ? <NoMatches /> : <EmptyStub />
        )}
        {!isLoading && !error && meetings.length > 0 && (
          <DateGroup meetings={meetings} />
        )}
      </main>
    </div>
  );
}

/**
 * Inline "no matches" line for the search-active branch. Plan 07-04
 * may upgrade this to a richer empty-state when the full Library
 * polish lands; the present styling matches the loading/error rows.
 */
function NoMatches() {
  return (
    <div className="text-[13px] font-mono text-mut">No matches</div>
  );
}

/**
 * Minimal empty-state placeholder. Plan 07-04 ships the full
 * `<EmptyLibrary />` with the 64px floating logo + `Start your first
 * meeting` CTA + `⌘N` kbd badge.
 */
function EmptyStub() {
  return (
    <div className="py-12 text-center">
      <p className="font-serif text-[28px] text-ink">No meetings yet</p>
      <p className="mt-2 text-[13px] font-mono text-mut">
        Start one from the "+ New meeting" button on the left.
      </p>
    </div>
  );
}
