/**
 * Phase 7 (Plan 07-01 + 07-02 + 07-04) — Library home route (`/`).
 *
 * Layout per PRD §5.9 + D-01..D-06:
 *   ┌────────────────┬──────────────────────────────────────────┐
 *   │  <Sidebar />   │  <Greeting />   <SearchPill />           │
 *   │   (212px)      │  <DateGroup meetings=... />              │
 *   └────────────────┴──────────────────────────────────────────┘
 *
 * Plan 07-02 wires the search pill to `useMeetingsSearch` (FTS5).
 * Plan 07-04 replaces the inline empty stub with the floating-logo
 * `<EmptyLibrary />` and gates the entire main pane on Screen Recording
 * via `useScreenRecordingStatus` → `<PermissionDenied />`. The Sidebar
 * remains visible in the denied state so the user can still reach
 * `/settings` to e.g. paste an API key while the macOS prompt is open.
 *
 * Quick task 260628-g71 adds a parallel mic-permission gate below the
 * screen-recording gate. Render order is precedence-aware: screen
 * recording denial wins (it's the more fundamental capture path —
 * without system audio there is no "other side of the call"), and the
 * mic denial card only surfaces when screen-recording is granted but
 * mic is not.
 *
 * Search-active empty (`isSearching && meetings.length === 0`) renders
 * the inline "No matches" line — we do NOT swap in `<EmptyLibrary />`
 * there, because the user has clearly typed something and a giant
 * "Start your first meeting" CTA would be confusing.
 */

import { useState } from "react";
import { useMeetings, useMeetingsSearch } from "../lib/api/meetings";
import { DateGroup } from "../components/library/DateGroup";
import { Greeting } from "../components/library/Greeting";
import { SearchPill } from "../components/library/SearchPill";
import { Sidebar } from "../components/library/Sidebar";
import { EmptyLibrary } from "../components/states/EmptyLibrary";
import { MicPermissionDenied } from "../components/states/MicPermissionDenied";
import { PermissionDenied } from "../components/states/PermissionDenied";
import { useMicrophoneStatus } from "../hooks/useMicrophoneStatus";
import { useScreenRecordingStatus } from "../hooks/useScreenRecordingStatus";

export function Library() {
  const [query, setQuery] = useState("");
  const trimmedQuery = query.trim();
  const isSearching = trimmedQuery.length > 0;

  const { granted, isLoading: permissionLoading } = useScreenRecordingStatus();
  const { granted: micGranted, isLoading: micLoading } = useMicrophoneStatus();

  const all = useMeetings();
  const found = useMeetingsSearch(query);

  const meetings = isSearching ? (found.data ?? []) : (all.data ?? []);
  const isLoading = isSearching ? found.isLoading : all.isLoading;
  const error = isSearching ? found.error : all.error;

  // Permission gate (STATE-02). While the permission probe is still
  // loading we render the chrome but no empty/list content — avoids a
  // one-frame flash of PermissionDenied before the granted poll returns.
  // Screen-recording denial takes precedence (more fundamental capture
  // path); mic denial is shown only when screen recording is already
  // granted. Both keep the Sidebar visible so the user can still reach
  // `/settings`.
  if (!permissionLoading && !granted) {
    return (
      <div className="flex">
        <Sidebar />
        <main className="flex-1">
          <PermissionDenied />
        </main>
      </div>
    );
  }
  if (!micLoading && !micGranted) {
    return (
      <div className="flex">
        <Sidebar />
        <main className="flex-1">
          <MicPermissionDenied />
        </main>
      </div>
    );
  }

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
          isSearching ? <NoMatches /> : <EmptyLibrary />
        )}
        {!isLoading && !error && meetings.length > 0 && (
          <DateGroup meetings={meetings} />
        )}
      </main>
    </div>
  );
}

/**
 * Inline "no matches" line for the search-active branch. We intentionally
 * keep this small and inline — a full-page empty-state would obscure the
 * search input that produced the empty result.
 */
function NoMatches() {
  return (
    <div className="text-[13px] font-mono text-mut">No matches</div>
  );
}
