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
import { LibraryAskExperience } from "../components/AskExperience";
import { EmptyLibrary } from "../components/states/EmptyLibrary";
import { MicPermissionDenied } from "../components/states/MicPermissionDenied";
import { PermissionDenied } from "../components/states/PermissionDenied";
import { useKeyboardShortcut } from "../hooks/useKeyboardShortcut";
import { useMicrophoneStatus } from "../hooks/useMicrophoneStatus";
import { useScreenRecordingStatus } from "../hooks/useScreenRecordingStatus";

interface LibraryProps {
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

  const [selection, setSelection] = useState(EMPTY_SELECTION);

  // Permission only changes after a restart here, so poll slowly.
  const { granted, isLoading: permissionLoading } = useScreenRecordingStatus({
    refetchIntervalMs: 60_000,
  });
  const { granted: micGranted, isLoading: micLoading } = useMicrophoneStatus({
    refetchIntervalMs: 60_000,
  });

  const all = useMeetings();
  const found = useMeetingsSearch(query);
  // One poll for the whole list, not one per card.
  const activeId = useActiveRecording().data?.id ?? null;

  const fetched = isSearching ? (found.data ?? []) : (all.data ?? []);
  const filteredByStar = starredOnly ? fetched.filter((m) => m.starred) : fetched;
  const meetings = labelId
    ? filteredByStar.filter((m) => m.labels.some((l) => l.id === labelId))
    : filteredByStar;
  const isLoading = isSearching ? found.isLoading : all.isLoading;
  const error = isSearching ? found.error : all.error;

  const orderedIds = meetings.map((m) => m.id);
  const selectedMeetings = meetings.filter((m) => selection.selected.has(m.id));
  // An id can linger in `selection.selected` after its meeting is deleted or
  // filtered out, which would leave a phantom "0 selected" action bar.
  const hasSelection = selectedMeetings.length > 0;

  const onToggleSelect = (id: string, shiftKey: boolean) =>
    setSelection((s) => clickSelect(s, id, orderedIds, shiftKey));
  const onSelectAll = () => setSelection(selectAll(orderedIds));
  const onClearSelection = () => setSelection(clearSelection());

  // Wait for the labels list so a fresh load doesn't redirect early.
  if (labelId && !labels.isLoading && labels.data && !labels.data.some((l) => l.id === labelId)) {
    return <Navigate to="/" replace />;
  }

  // Some browsers reserve ⌘N for "new window" and never deliver the event.
  useKeyboardShortcut(
    { key: "n", metaOrCtrl: true, ignoreWhenTyping: true },
    async () => {
      if (createMeeting.isPending) return;
      try {
        const m = await createMeeting.mutateAsync(undefined);
        navigate(`/meeting/${m.id}`, { state: { autoStart: true } });
      } catch (e) {
        console.error("create meeting failed", e);
      }
    },
  );

  // ignoreWhenTyping: Escape from an inline rename or the label picker
  // must not also clear the selection.
  useKeyboardShortcut({ key: "Escape", ignoreWhenTyping: true }, () => {
    if (selection.selected.size > 0) onClearSelection();
  });

  // Wait for the probe to avoid a one-frame flash of PermissionDenied.
  // Screen recording denial wins over mic denial. Both keep the Sidebar so
  // the user can still reach `/settings`.
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
      <main className="flex-1 px-12 pt-10 pb-28 max-w-[860px]">
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
      {/* Both anchor bottom-center, so the selection bar takes the slot while it shows. */}
      {hasSelection ? (
        <SelectionActionBar
          meetings={selectedMeetings}
          onSelectAll={onSelectAll}
          onClear={onClearSelection}
        />
      ) : (
        <LibraryAskExperience />
      )}
    </div>
  );
}

function NoMatches() {
  return (
    <div className="text-[13px] text-mut">No matches</div>
  );
}

function NoLabelMeetings() {
  return (
    <div className="text-[13px] text-mut">
      No meetings with this label yet.
    </div>
  );
}

function NoStarred() {
  return (
    <div className="text-[13px] text-mut">
      No starred meetings yet — hover a meeting and click the star.
    </div>
  );
}

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
