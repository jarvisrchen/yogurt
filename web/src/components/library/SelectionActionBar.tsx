import { useEffect, useState } from "react";
import { ConfirmPanel } from "./DeleteMeetingConfirm";
import { BulkLabelPicker } from "../labels/BulkLabelPicker";
import { exportMeetingFile, useBulkDeleteMeetings, useSetArchived, type Meeting } from "../../lib/api/meetings";

interface Props {
  // Label bulk-ops need each meeting's current `labels` to compute its next set.
  meetings: Pick<Meeting, "id" | "title" | "labels">[];
  onSelectAll: () => void;
  onClear: () => void;
  /** The Archive view, where the bar offers Unarchive. */
  archived?: boolean;
  /** The meeting being recorded, which Archive and Export skip because the server refuses it. */
  activeId?: string | null;
}

export function SelectionActionBar({
  meetings,
  onSelectAll,
  onClear,
  archived = false,
  activeId = null,
}: Props) {
  const [addOpen, setAddOpen] = useState(false);
  const [removeOpen, setRemoveOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [deleteFile, setDeleteFile] = useState(true);
  const [exporting, setExporting] = useState(false);
  const [exportFailed, setExportFailed] = useState(0);
  const bulkDelete = useBulkDeleteMeetings();
  const setArchived = useSetArchived();

  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(false), 3000);
    return () => clearTimeout(t);
  }, [confirming, deleteFile]);

  useEffect(() => {
    if (!exportFailed) return;
    const t = setTimeout(() => setExportFailed(0), 4000);
    return () => clearTimeout(t);
  }, [exportFailed]);

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
      // A partial failure still invalidates the list, so clearing the selection
      // avoids a stuck "N selected" bar.
      console.error("bulk delete meetings failed", err);
    }
    onClear();
  };

  // No confirmation, since nothing is lost. The hook logs a partial failure.
  const onArchive = async () => {
    const toArchive = ids.filter((id) => id !== activeId);
    await setArchived.mutateAsync({ ids: toArchive, archived: !archived }).catch(() => {});
    onClear();
  };

  const onExport = async () => {
    setExporting(true);
    let failed = 0;
    for (const m of meetings) {
      if (m.id === activeId) continue;
      await exportMeetingFile(m.id, m.title).catch((err) => {
        failed++;
        console.error("export meeting failed", m.id, err);
      });
    }
    setExporting(false);
    if (failed) setExportFailed(failed);
    else onClear();
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
          // BulkLabelPicker's outside-click listener fires on mousedown, so stop it
          // here or the picker closes as it opens.
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
      <button
        type="button"
        className={`${itemButton} text-ink`}
        disabled={setArchived.isPending}
        onClick={onArchive}
      >
        {archived ? "Unarchive" : "Archive"}
      </button>
      <button
        type="button"
        className={`${itemButton} ${exportFailed ? "text-straw" : "text-ink"}`}
        disabled={exporting}
        onClick={onExport}
      >
        {exporting ? "Exporting…" : exportFailed ? `Couldn't export ${exportFailed}` : "Export"}
      </button>
      <div className="relative">
        <button
          type="button"
          className={`${itemButton} text-straw`}
          aria-expanded={confirming}
          onClick={(e) => {
            stop(e);
            setDeleteFile(true);
            setConfirming((c) => !c);
          }}
        >
          Delete
        </button>
        {confirming && (
          <ConfirmPanel
            testId="bulk-delete-confirm-popover"
            containerClassName="absolute bottom-full left-0 mb-2 px-3 py-2 flex flex-col gap-1.5 bg-card border border-line rounded-card shadow-pop min-w-[220px]"
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
