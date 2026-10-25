import { useState } from "react";
import { Link, NavLink, useNavigate } from "react-router";
import { Plus, Settings as SettingsIcon, Star } from "lucide-react";
import { Logo } from "../Logo";
import { useCreateMeeting } from "../../lib/api/meetings";
import { useCreateLabel, useLabels, useReorderLabels } from "../../lib/api/labels";
import { useQuery } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";
import { useCloudStatus } from "../../lib/api/cloud";
import { SidebarLabelRow } from "./SidebarLabelRow";
import { Button } from "../Button";
import { LABEL } from "../settings/labelClass";
import { getLabelSortMode, setLabelSortMode, sortLabels } from "../../lib/labelSort";

export function Sidebar() {
  const navigate = useNavigate();
  const createMeeting = useCreateMeeting();
  const labels = useLabels();
  const createLabel = useCreateLabel();
  const reorderLabels = useReorderLabels();
  const [addingLabel, setAddingLabel] = useState(false);
  const [newLabelName, setNewLabelName] = useState("");
  const [sortMode, setSortMode] = useState(getLabelSortMode);
  const [dragIndex, setDragIndex] = useState<number | null>(null);
  const sortedLabels = sortLabels(labels.data ?? [], sortMode);

  function handleDrop(targetIndex: number) {
    if (dragIndex === null || dragIndex === targetIndex) {
      setDragIndex(null);
      return;
    }
    const ids = sortedLabels.map((l) => l.id);
    const [moved] = ids.splice(dragIndex, 1);
    ids.splice(targetIndex, 0, moved);
    reorderLabels.mutate(ids);
    setDragIndex(null);
  }

  function commitNewLabel() {
    const name = newLabelName.trim();
    if (name.length > 0) {
      createLabel.mutate({ name });
    }
    setNewLabelName("");
    setAddingLabel(false);
  }

  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: () => settingsApi.get(),
    staleTime: 30_000,
  });

  const cloudOn = useCloudStatus().data?.connected === true;
  const isLocalOnly =
    !settings.data?.providers?.some(
      (p) => p.is_active && !isLocalProvider(p.base_url),
    );

  const handleNew = async () => {
    try {
      const m = await createMeeting.mutateAsync(undefined);
      // `autoStart` makes Meeting.tsx POST /start on mount.
      navigate(`/meeting/${m.id}`, { state: { autoStart: true } });
    } catch (e) {
      console.error("create meeting failed", e);
    }
  };

  return (
    <aside
      // Sticky keeps the rail at viewport height so `mt-auto` parks Settings at
      // the bottom of the screen, not the bottom of a long meeting list.
      className="w-[212px] shrink-0 flex flex-col bg-paper border-r border-line sticky top-0 h-screen"
    >
      <div className="px-5 pt-5 pb-4 flex items-center gap-2">
        <Logo size={28} />
        <span className="heading-wordmark">
          yogurt
        </span>
      </div>

      <div className="px-4 pb-3">
        <Button
          onClick={handleNew}
          disabled={createMeeting.isPending}
          className="w-full"
        >
          <Plus size={16} aria-hidden />
          New meeting
        </Button>
      </div>

      <nav className="px-2 flex flex-col gap-0.5">
        <NavLink
          to="/"
          end
          className={({ isActive }) =>
            `px-3 py-1.5 rounded-button text-[13px] ${
              isActive
                ? "bg-blsoft text-blue font-semibold"
                : "text-ink hover:bg-line/40"
            }`
          }
        >
          All meetings
        </NavLink>
        <NavLink
          to="/starred"
          className={({ isActive }) =>
            `px-3 py-1.5 rounded-button text-[13px] flex items-center gap-1.5 ${
              isActive
                ? "bg-blsoft text-blue font-semibold"
                : "text-ink hover:bg-line/40"
            }`
          }
        >
          <Star size={14} className="shrink-0 fill-current" aria-hidden />
          Starred
        </NavLink>
      </nav>

      <div className={`px-5 pt-5 pb-1 flex items-center justify-between ${LABEL}`}>
        <span>Labels</span>
        <div className="flex items-center gap-1">
          <select
            aria-label="Sort labels"
            value={sortMode}
            onChange={(e) => {
              const mode = e.target.value as typeof sortMode;
              setSortMode(mode);
              setLabelSortMode(mode);
            }}
            className="text-[10px] text-mut bg-transparent border-none text-right focus:outline-none cursor-pointer"
          >
            <option value="alpha">A-Z</option>
            <option value="updated">Last updated</option>
            <option value="custom">Custom</option>
          </select>
          <button
            type="button"
            aria-label="New label"
            onClick={() => {
              setNewLabelName("");
              setAddingLabel(true);
            }}
            disabled={createLabel.isPending}
            className="text-mut hover:text-ink disabled:opacity-50"
          >
            <Plus size={12} aria-hidden />
          </button>
        </div>
      </div>
      <div className="px-2 flex flex-col gap-0.5">
        {addingLabel && (
          <div className="px-3 py-1">
            <input
              autoFocus
              placeholder="Label name"
              value={newLabelName}
              onChange={(e) => setNewLabelName(e.target.value)}
              onBlur={commitNewLabel}
              disabled={createLabel.isPending}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitNewLabel();
                if (e.key === "Escape") {
                  setNewLabelName("");
                  setAddingLabel(false);
                }
              }}
              className="w-full px-2 py-1 rounded-button border border-line bg-paper text-[13px] text-ink focus:outline-none focus:ring-2 focus:ring-blue/30"
            />
          </div>
        )}
        {sortedLabels.length === 0 && !addingLabel ? (
          <p className="px-3 py-1 text-[12px] text-mut">No labels yet</p>
        ) : (
          sortedLabels.map((l, i) => (
            <div
              key={l.id}
              draggable={sortMode === "custom"}
              onDragStart={() => setDragIndex(i)}
              onDragOver={(e) => {
                if (sortMode === "custom") e.preventDefault();
              }}
              onDrop={() => handleDrop(i)}
              onDragEnd={() => setDragIndex(null)}
              className={dragIndex === i ? "opacity-50" : undefined}
            >
              <SidebarLabelRow label={l} />
            </div>
          ))
        )}
      </div>

      <div className="mt-auto px-4 py-4 flex flex-col gap-2">
        {(cloudOn || isLocalOnly) && (
          <div
            className="self-start px-2.5 py-1 rounded-pill bg-mtsoft text-ink text-[11px] font-medium"
          >
            {cloudOn ? "Cloud sync · on" : "Local-only · on"}
          </div>
        )}
        <Link
          to="/settings"
          className="flex items-center gap-2 px-2 py-1.5 rounded-button text-[13px] text-ink hover:bg-line/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40"
        >
          <SettingsIcon size={16} aria-hidden />
          <span>Settings</span>
        </Link>
      </div>
    </aside>
  );
}

// The settings shape has no local/cloud discriminator, so infer it from the URL.
function isLocalProvider(baseUrl: string): boolean {
  const u = baseUrl.toLowerCase();
  return (
    u.includes("localhost") ||
    u.includes("127.0.0.1") ||
    u.includes("0.0.0.0") ||
    u.includes(":11434") ||
    u.includes(":1234")
  );
}
