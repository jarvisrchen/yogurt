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
 *
 * MTG-14 adds multi-select: this route owns the client-only
 * `SelectionState` (see `components/library/selection.ts`) and forwards
 * it down through `<DateGroup>` to each `<MeetingCard>`'s checkbox, and
 * mounts the floating `<SelectionActionBar>` once anything is selected.
 */

import { useState } from "react";
import { Navigate, useNavigate, useParams } from "react-router";
import {
  useActiveRecording,
  useCreateMeeting,
  useMeetings,
  useMeetingsSearch,
} from "../lib/api/meetings";
import { useLabels } from "../lib/api/labels";
import { DateGroup } from "../components/library/DateGroup";
import { Greeting } from "../components/library/Greeting";
import { SearchPill } from "../components/library/SearchPill";
import { SelectionActionBar } from "../components/library/SelectionActionBar";
import { EMPTY_SELECTION, clickSelect, clearSelection, selectAll } from "../components/library/selection";
import { Sidebar } from "../components/library/Sidebar";
import { ShimmerSkeleton } from "../components/ShimmerSkeleton";
import { EmptyLibrary } from "../components/states/EmptyLibrary";
import { MicPermissionDenied } from "../components/states/MicPermissionDenied";
import { PermissionDenied } from "../components/states/PermissionDenied";
import { useKeyboardShortcut } from "../hooks/useKeyboardShortcut";
import { useMicrophoneStatus } from "../hooks/useMicrophoneStatus";
import { useScreenRecordingStatus } from "../hooks/useScreenRecordingStatus";

interface LibraryProps {
  /** When true (the `/starred` route), only starred meetings render. */
  starredOnly?: boolean;
}

export function Library({ starredOnly = false }: LibraryProps) {
  const navigate = useNavigate();
  const { labelId } = useParams<{ labelId: string }>();
  const labels = useLabels();
  const createMeeting = useCreateMeeting();
  const [query, setQuery] = useState("");
  const trimmedQuery = query.trim();
  const isSearching = trimmedQuery.length > 0;

  // MTG-14 - multi-select. Client-only, cleared on route change (the
  // `key` isn't needed: navigating away unmounts Library entirely).
  const [selection, setSelection] = useState(EMPTY_SELECTION);

  // Library isn't a page where permission state changes mid-session (the
  // recovery flow requires restarting Yogurt anyway, which naturally
  // refetches everything) — 60s avoids polling `/api/audio/permission`
  // every 2s on every Library view. Onboarding's `<Welcome />` keeps the
  // fast default.
  const { granted, isLoading: permissionLoading } = useScreenRecordingStatus({
    refetchIntervalMs: 60_000,
  });
  const { granted: micGranted, isLoading: micLoading } = useMicrophoneStatus({
    refetchIntervalMs: 60_000,
  });

  const all = useMeetings();
  const found = useMeetingsSearch(query);
  // Single poll for the whole list (not one per card) — cards compare
  // their own id against this to route to the live view instead of /post.
  const activeId = useActiveRecording().data?.id ?? null;

  const fetched = isSearching ? (found.data ?? []) : (all.data ?? []);
  const filteredByStar = starredOnly ? fetched.filter((m) => m.starred) : fetched;
  const meetings = labelId
    ? filteredByStar.filter((m) => m.labels.some((l) => l.id === labelId))
    : filteredByStar;
  const isLoading = isSearching ? found.isLoading : all.isLoading;
  const error = isSearching ? found.error : all.error;

  // MTG-14 - the visible order shift-click ranges over and "Select all"
  // selects; the meeting objects (not just ids) selected labels need for
  // add/remove.
  const orderedIds = meetings.map((m) => m.id);
  const selectedMeetings = meetings.filter((m) => selection.selected.has(m.id));
  // Gate rendering on the meetings actually still in view, not the raw id
  // set: an id can linger in `selection.selected` after its meeting is
  // deleted elsewhere or drops out of the current search/label filter,
  // which would otherwise leave a phantom "0 selected" action bar floating
  // on screen with every checkbox pinned visible for nothing.
  const hasSelection = selectedMeetings.length > 0;

  const onToggleSelect = (id: string, shiftKey: boolean) =>
    setSelection((s) => clickSelect(s, id, orderedIds, shiftKey));
  const onSelectAll = () => setSelection(selectAll(orderedIds));
  const onClearSelection = () => setSelection(clearSelection());

  // A label filter for a deleted label id: bounce to the Library root once
  // the labels list has resolved and confirms the id is gone. Guarded on
  // `!labels.isLoading` so a fresh page load doesn't redirect before the
  // labels list has had a chance to populate.
  if (labelId && !labels.isLoading && labels.data && !labels.data.some((l) => l.id === labelId)) {
    return <Navigate to="/" replace />;
  }

  // ⌘N — same action as the sidebar "+ New meeting" button (EmptyLibrary
  // advertises the shortcut). Suppressed while a text input / editor has
  // focus. Note: some browsers reserve ⌘N for "new window" and never
  // deliver the event — the sidebar button stays the canonical path.
  useKeyboardShortcut(
    { key: "n", metaOrCtrl: true, ignoreWhenTyping: true },
    async () => {
      if (createMeeting.isPending) return;
      try {
        const m = await createMeeting.mutateAsync(undefined);
        // See Sidebar.tsx's `handleNew` — same auto-start contract.
        navigate(`/meeting/${m.id}`, { state: { autoStart: true } });
      } catch (e) {
        console.error("create meeting failed", e);
      }
    },
  );

  // Escape clears an in-progress multi-select (MTG-14). `ignoreWhenTyping`
  // matters here: without it, canceling an inline title/label rename (or
  // closing the label picker) with Escape also silently wiped the whole
  // selection, since the same keypress bubbles up from any input.
  useKeyboardShortcut({ key: "Escape", ignoreWhenTyping: true }, () => {
    if (selection.selected.size > 0) onClearSelection();
  });

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
          <Greeting
            count={
              isSearching || starredOnly || labelId
                ? meetings.length
                : (all.data ?? []).length
            }
          />
          <SearchPill value={query} onChange={setQuery} />
        </div>

        {isLoading && <LibrarySkeleton />}
        {error && (
          <div className="inline-block text-[13px] text-ink bg-strsoft border border-straw/40 rounded-button px-3 py-2">
            Couldn't load meetings: {error.message}
          </div>
        )}
        {!isLoading && !error && meetings.length === 0 && (
          isSearching ? (
            <NoMatches />
          ) : labelId ? (
            <NoLabelMeetings />
          ) : starredOnly ? (
            <NoStarred />
          ) : (
            <EmptyLibrary />
          )
        )}
        {!isLoading && !error && meetings.length > 0 && (
          <DateGroup
            meetings={meetings}
            activeId={activeId}
            selectionActive={hasSelection}
            selectedIds={selection.selected}
            onToggleSelect={onToggleSelect}
          />
        )}
      </main>
      {hasSelection && (
        <SelectionActionBar
          meetings={selectedMeetings}
          onSelectAll={onSelectAll}
          onClear={onClearSelection}
        />
      )}
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
    <div className="text-[13px] text-mut">No matches</div>
  );
}

/** Empty state for a `/label/:labelId` filter with zero matches. */
function NoLabelMeetings() {
  return (
    <div className="text-[13px] text-mut">
      No meetings with this label yet.
    </div>
  );
}

/** Empty state for the `/starred` filter — deliberately quieter than
 *  `<EmptyLibrary />`; the user already has meetings, just no stars. */
function NoStarred() {
  return (
    <div className="text-[13px] text-mut">
      No starred meetings yet — hover a meeting and click the star.
    </div>
  );
}

/**
 * Loading skeleton for the meeting list — four card-shaped shimmer rows
 * (42px avatar block + title bar) matching the MeetingCard layout so the
 * resolved list doesn't cause a layout jump.
 */
function LibrarySkeleton() {
  return (
    <div
      className="flex flex-col gap-1"
      aria-hidden
      data-testid="library-skeleton"
    >
      {[0, 1, 2, 3].map((i) => (
        <div key={i} className="flex items-center gap-3 py-2 px-2 -mx-2">
          <div className="w-[42px] h-[42px] rounded-button shimmer shrink-0" />
          <div className="flex-1">
            <ShimmerSkeleton
              staggerMs={0}
              widthClass={i % 2 === 0 ? "w-1/2" : "w-1/3"}
            />
          </div>
        </div>
      ))}
    </div>
  );
}
