import { Archive, ArchiveRestore } from "lucide-react";
import { useMeeting, useSetArchived } from "../../lib/api/meetings";

/** The meeting page's archive toggle. An archived meeting's tooltip says when, since a meeting opened from a link would not show why the Library leaves it out. */
export function ArchiveMeetingButton({ id }: { id: string }) {
  const archivedAt = useMeeting(id).data?.archived_at;
  const setArchived = useSetArchived();
  const archived = archivedAt != null;
  const onClick = () => setArchived.mutate({ ids: [id], archived: !archived });

  if (!archived) {
    return (
      <button
        type="button"
        aria-label="Archive meeting"
        title="Archive meeting"
        onClick={onClick}
        disabled={setArchived.isPending}
        className="shrink-0 p-1.5 rounded-button text-mut hover:text-ink hover:bg-line/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40"
      >
        <Archive size={16} aria-hidden />
      </button>
    );
  }
  const date = new Date(archivedAt).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
  return (
    <div className="relative inline-block group/archive">
      <button
        type="button"
        aria-label="Unarchive meeting"
        aria-pressed="true"
        aria-describedby="archived-at-tooltip"
        onClick={onClick}
        disabled={setArchived.isPending}
        className="shrink-0 p-1.5 rounded-button text-blue bg-blsoft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40"
      >
        <ArchiveRestore size={16} aria-hidden />
      </button>
      <span
        id="archived-at-tooltip"
        role="tooltip"
        className="pointer-events-none absolute right-0 top-full z-20 mt-1.5 flex flex-col gap-1 whitespace-nowrap rounded-chip border border-line bg-card px-2.5 py-2 text-left leading-none shadow-pop opacity-0 translate-y-0.5 transition duration-150 ease-pop group-hover/archive:translate-y-0 group-hover/archive:opacity-100 group-hover/archive:delay-150 group-focus-within/archive:translate-y-0 group-focus-within/archive:opacity-100"
      >
        <span className="text-[12px] font-medium text-ink">Archived {date}</span>
      </span>
    </div>
  );
}
