/**
 * Phase 7 (Plan 07-01) — Library home route (`/`).
 *
 * Layout per PRD §5.9 + D-01..D-06:
 *   ┌────────────────┬──────────────────────────────────────────┐
 *   │  <Sidebar />   │  <Greeting />                            │
 *   │   (212px)      │  <DateGroup meetings=... />              │
 *   └────────────────┴──────────────────────────────────────────┘
 *
 * The Phase 7 plan-02 will fill in the search slot + EmptyLibrary +
 * PermissionDenied surfaces; this plan ships a minimal placeholder so
 * the route is reachable end-to-end.
 */

import { useMeetings } from "../lib/api/meetings";
import { DateGroup } from "../components/library/DateGroup";
import { Greeting } from "../components/library/Greeting";
import { Sidebar } from "../components/library/Sidebar";

export function Library() {
  const { data, isLoading, error } = useMeetings();
  const meetings = data ?? [];

  return (
    <div className="flex">
      <Sidebar />
      <main className="flex-1 px-12 py-10 max-w-[860px]">
        <Greeting count={meetings.length} />

        {/* Plan 07-02 fills this with the real <SearchPill /> wired to FTS5. */}
        <div data-search-slot className="mb-8" />

        {isLoading && (
          <div className="text-[13px] font-mono text-mut">Loading…</div>
        )}
        {error && (
          <div className="text-[13px] font-mono text-straw">
            Couldn't load meetings: {error.message}
          </div>
        )}
        {!isLoading && !error && meetings.length === 0 && <EmptyStub />}
        {!isLoading && !error && meetings.length > 0 && (
          <DateGroup meetings={meetings} />
        )}
      </main>
    </div>
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
