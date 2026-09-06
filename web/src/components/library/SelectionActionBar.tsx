/**
 * Floating bottom action bar for the Library's multi-select mode (MTG-14).
 * Mounted only while at least one meeting is selected (`Library.tsx` gates
 * on selection size), same fixed/floating placement family as
 * `RecordingPill`.
 *
 * Delete reuses the exact "Delete? / Cancel" + "also delete .md" confirm
 * surface the per-card kebab menu uses (`ConfirmPanel`, exported from
 * `DeleteMeetingConfirm`) - only the mutation underneath is the bulk one.
 */

import { useEffect, useState } from "react";
import { ConfirmPanel } from "./DeleteMeetingConfirm";
import { BulkLabelPicker } from "../labels/BulkLabelPicker";
import { useBulkDeleteMeetings, type Meeting } from "../../lib/api/meetings";

interface Props {
  /** The selected meetings themselves (not just ids) - label bulk-ops
   *  need each one's current `labels` to compute its next set. */
  meetings: Pick<Meeting, "id" | "labels">[];
  onSelectAll: () => void;
  onClear: () => void;
}

export function SelectionActionBar({ meetings, onSelectAll, onClear }: Props) {
  const [addOpen, setAddOpen] = useState(false);
  const [removeOpen, setRemoveOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [deleteFile, setDeleteFile] = useState(true);
  const bulkDelete = useBulkDeleteMeetings();

  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(false), 3000);
    return () => clearTimeout(t);
  }, [confirming, deleteFile]);

  const stop = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  const ids = meetings.map((m) => m.id);

  const onConfirmDelete = async (e: React.MouseEvent) => {
    stop(e);
    setConfirming(false);
    try {
      await bulkDelete.mutateAsync({ ids, deleteFile });
    } catch (err) {
      // A partial failure still invalidates the list (see
      // useBulkDeleteMeetings), so anything that *did* delete is already
      // gone - clearing below just drops the rest instead of leaving a
      // stuck "N selected" bar with no visible way forward.
      console.error("bulk delete meetings failed", err);
    }
    onClear();
  };

  const itemButton =
    "px-2.5 py-1.5 rounded-button text-[13px] font-medium hover:bg-paper focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 whitespace-nowrap";

  return (
    <div
      data-testid="selection-action-bar"
      className="fixed bottom-6 left-1/2 -translate-x-1/2 flex items-center gap-1 px-2 py-1.5 bg-card border border-line rounded-card shadow-pop z-30"
    >
      <span className="px-2 text-[13px] font-semibold text-ink whitespace-nowrap">
        {ids.length} selected
      </span>
      <div className="w-px self-stretch bg-line mx-1" aria-hidden />
      <button type="button" className={`${itemButton} text-ink`} onClick={onSelectAll}>
        Select all
      </button>
      <div className="relative">
        <button
          type="button"
          className={`${itemButton} text-ink`}
          // Stop the mousedown before it reaches `document` so
          // BulkLabelPicker's own outside-click listener doesn't see this
          // button as "outside" and immediately re-close what we're
          // opening (same trick as MeetingCardActions' Tag button).
          onMouseDown={stop}
          onClick={(e) => {
            stop(e);
            setAddOpen((o) => !o);
            setRemoveOpen(false);
          }}
        >
          Add label
        </button>
        <BulkLabelPicker
          meetings={meetings}
          mode="add"
          open={addOpen}
          onClose={() => setAddOpen(false)}
        />
      </div>
      <div className="relative">
        <button
          type="button"
          className={`${itemButton} text-ink`}
          onMouseDown={stop}
          onClick={(e) => {
            stop(e);
            setRemoveOpen((o) => !o);
            setAddOpen(false);
          }}
        >
          Remove label
        </button>
        <BulkLabelPicker
          meetings={meetings}
          mode="remove"
          open={removeOpen}
          onClose={() => setRemoveOpen(false)}
        />
      </div>
      <div className="relative">
        {!confirming ? (
          <button
            type="button"
            className={`${itemButton} text-straw`}
            onClick={(e) => {
              stop(e);
              setDeleteFile(true);
              setConfirming(true);
            }}
          >
            Delete
          </button>
        ) : (
          <ConfirmPanel
            testId="bulk-delete-confirm-popover"
            containerClassName="absolute bottom-full left-0 mb-1 px-3 py-2 flex flex-col gap-1.5 bg-card border border-line rounded-card shadow-pop min-w-[220px]"
            deleteFile={deleteFile}
            setDeleteFile={setDeleteFile}
            onConfirm={onConfirmDelete}
            onCancel={() => setConfirming(false)}
            stop={stop}
            isPending={bulkDelete.isPending}
          />
        )}
      </div>
      <div className="w-px self-stretch bg-line mx-1" aria-hidden />
      <button type="button" className={`${itemButton} text-mut`} onClick={onClear}>
        Clear
      </button>
    </div>
  );
}
