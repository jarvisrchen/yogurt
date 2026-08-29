/**
 * Shared delete affordance — the inline "Delete?" / "Cancel" confirm plus
 * the "also delete .md" checkbox. Used by both the Library card kebab
 * (`MeetingCardActions`) and the post-meeting header (`MeetingPost`), so
 * the confirm semantics live in exactly one place.
 *
 * D-10 / PRD §5.7: the SQLite row and the on-disk markdown in
 * `~/.yogurt/notes/` are separate deletes. The checkbox is pre-checked —
 * a meeting you're purging almost never has notes worth keeping — but
 * unchecking preserves the grep-able file the decision was written for.
 *
 * The confirm auto-reverts after 3s of inaction; toggling the checkbox
 * re-arms that timer so it can't vanish mid-decision.
 */

import { useEffect, useState } from "react";
import { Trash2 } from "lucide-react";
import { useDeleteMeeting } from "../../lib/api/meetings";

interface Props {
  id: string;
  /** Called after the delete resolves — close a menu, navigate away, … */
  onDeleted?: () => void;
  /**
   * `menuitem` renders a full-width row for a `role="menu"` dropdown.
   * `icon` renders a compact trash button for a page header.
   */
  variant: "menuitem" | "icon";
}

export function DeleteMeetingConfirm({ id, onDeleted, variant }: Props) {
  const [confirming, setConfirming] = useState(false);
  const [deleteFile, setDeleteFile] = useState(true);
  const del = useDeleteMeeting();
  const isIcon = variant === "icon";

  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(false), 3000);
    return () => clearTimeout(t);
  }, [confirming, deleteFile]);

  const stop = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
  };

  const onOpen = (e: React.MouseEvent) => {
    stop(e);
    // Re-arm the checkbox on every open so an uncheck never leaks into an
    // unrelated delete later in the session.
    setDeleteFile(true);
    setConfirming(true);
  };

  const onConfirm = async (e: React.MouseEvent) => {
    stop(e);
    setConfirming(false);
    await del.mutateAsync({ id, deleteFile });
    onDeleted?.();
  };

  // Wrapper is `relative` in the icon variant so the absolute-positioned
  // confirm popover anchors under the trashcan instead of pushing the
  // surrounding topbar around (which is what shifted the Enhance button
  // before this fix). Menuitem variant preserves the existing
  // kebab-menu "morph" behavior - the row replaces, no overlay.
  return isIcon ? (
    <div className="relative inline-block">
      <button
        type="button"
        aria-label="Delete meeting"
        onClick={onOpen}
        className="shrink-0 p-1.5 rounded-button text-mut hover:text-straw hover:bg-line/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-straw/40"
      >
        <Trash2 size={16} aria-hidden />
      </button>
      {confirming && (
        <ConfirmPanel
          testId="delete-confirm-popover"
          containerClassName="absolute top-full right-0 mt-1 z-20 px-3 py-2 flex flex-col gap-1.5 bg-card border border-line rounded-card shadow-pop min-w-[200px]"
          deleteFile={deleteFile}
          setDeleteFile={setDeleteFile}
          onConfirm={onConfirm}
          onCancel={() => setConfirming(false)}
          stop={stop}
          isPending={del.isPending}
        />
      )}
    </div>
  ) : !confirming ? (
    <button
      type="button"
      role="menuitem"
      onClick={onOpen}
      className="block w-full text-left px-3 py-2 text-straw hover:bg-paper"
    >
      Delete
    </button>
  ) : (
    <ConfirmPanel
      containerClassName="px-3 py-2 flex flex-col gap-1.5"
      deleteFile={deleteFile}
      setDeleteFile={setDeleteFile}
      onConfirm={onConfirm}
      onCancel={() => setConfirming(false)}
      stop={stop}
      isPending={del.isPending}
    />
  );
}

interface ConfirmPanelProps {
  testId?: string;
  containerClassName: string;
  deleteFile: boolean;
  setDeleteFile: (v: boolean) => void;
  onConfirm: (e: React.MouseEvent) => void;
  onCancel: (e: React.MouseEvent) => void;
  stop: (e: React.MouseEvent) => void;
  isPending: boolean;
}

function ConfirmPanel({
  testId,
  containerClassName,
  deleteFile,
  setDeleteFile,
  onConfirm,
  onCancel,
  stop,
  isPending,
}: ConfirmPanelProps) {
  return (
    <div
      {...(testId ? { "data-testid": testId } : {})}
      className={containerClassName}
    >
      <div className="flex items-center gap-2">
        <button
          type="button"
          role="menuitem"
          autoFocus
          onClick={onConfirm}
          disabled={isPending}
          className="px-2 py-1 rounded-button bg-strsoft text-ink border border-straw/40 font-semibold hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-straw/50 disabled:opacity-50"
        >
          Delete?
        </button>
        <button
          type="button"
          role="menuitem"
          onClick={(e) => {
            stop(e);
            onCancel(e);
          }}
          className="px-2 py-1 rounded-button text-mut hover:text-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40"
        >
          Cancel
        </button>
      </div>
      <label className="flex items-start gap-1.5 text-[11px] font-mono text-mut cursor-pointer">
        <input
          type="checkbox"
          checked={deleteFile}
          onChange={(e) => setDeleteFile(e.target.checked)}
          onClick={(e) => e.stopPropagation()}
          className="mt-[1px] accent-straw"
        />
        <span>
          {deleteFile
            ? "also delete .md in ~/.yogurt/notes"
            : ".md file stays in ~/.yogurt/notes"}
        </span>
      </label>
    </div>
  );
}
