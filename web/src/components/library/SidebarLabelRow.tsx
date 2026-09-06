/**
 * `SidebarLabelRow` — one label row in the Sidebar's Labels section.
 *
 * Extracted from `Sidebar.tsx` (per the plan's ~250-line guidance) because
 * the interactive states — rename, recolor swatches, delete confirm — are
 * substantial on their own. Mirrors `MeetingCardActions`' inline
 * "Delete?" / "Cancel" confirm pattern (auto-reverts after 3s, no modal).
 */

import { useEffect, useRef, useState } from "react";
import { NavLink } from "react-router";
import { MoreHorizontal } from "lucide-react";
import {
  useDeleteLabel,
  useUpdateLabel,
  type PaletteColor,
  type LabelWithCount,
} from "../../lib/api/labels";
import { isHexColor, labelTone } from "../labels/LabelChip";

const SWATCH_COLORS: PaletteColor[] = ["blue", "matcha", "straw", "lilac", "honey", "slate"];

/** Starting point shown in the custom-color inputs for a label that
 *  doesn't have a hex color yet. */
const DEFAULT_CUSTOM_COLOR = "#8a8fa3";

interface Props {
  label: LabelWithCount;
}

export function SidebarLabelRow({ label }: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [draftName, setDraftName] = useState(label.name);
  const [customColor, setCustomColor] = useState(
    isHexColor(label.color) ? label.color : DEFAULT_CUSTOM_COLOR,
  );
  const wrapperRef = useRef<HTMLDivElement>(null);
  const update = useUpdateLabel();
  const del = useDeleteLabel();

  useEffect(() => {
    if (!menuOpen) return;
    setCustomColor(isHexColor(label.color) ? label.color : DEFAULT_CUSTOM_COLOR);
    const onDown = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
        setConfirming(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [menuOpen, label.color]);

  function commitCustomColor(hex: string) {
    if (isHexColor(hex)) update.mutate({ id: label.id, color: hex });
  }

  useEffect(() => {
    if (!confirming) return;
    const t = setTimeout(() => setConfirming(false), 3000);
    return () => clearTimeout(t);
  }, [confirming]);

  const tone = labelTone(label.color);

  function commitRename() {
    const name = draftName.trim();
    if (name.length > 0 && name !== label.name) {
      update.mutate({ id: label.id, name });
    }
    setRenaming(false);
  }

  if (renaming) {
    return (
      <div className="px-3 py-1">
        <input
          autoFocus
          value={draftName}
          onChange={(e) => setDraftName(e.target.value)}
          onBlur={commitRename}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitRename();
            if (e.key === "Escape") {
              setDraftName(label.name);
              setRenaming(false);
            }
          }}
          className="w-full px-2 py-1 rounded-button border border-line bg-paper text-[13px] text-ink focus:outline-none focus:ring-2 focus:ring-blue/30"
        />
      </div>
    );
  }

  return (
    <div ref={wrapperRef} className="relative group/label">
      <NavLink
        to={`/label/${label.id}`}
        onClick={(e) => {
          if (menuOpen) e.preventDefault();
        }}
        className={({ isActive }) =>
          `flex items-center gap-2 px-3 py-1.5 rounded-button text-[13px] ${
            isActive ? "bg-blsoft text-blue font-semibold" : "text-ink hover:bg-line/40"
          }`
        }
      >
        <span className="w-[9px] h-[9px] rounded-[3px] shrink-0" style={{ background: tone.fg }} aria-hidden />
        <span className="flex-1 truncate">{label.name}</span>
        <span className="text-[11px] text-mut">{label.meeting_count}</span>
        <button
          type="button"
          aria-label={`${label.name} label options`}
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            setMenuOpen((o) => !o);
          }}
          className="shrink-0 text-mut hover:text-ink opacity-0 group-hover/label:opacity-100 focus-visible:opacity-100"
        >
          <MoreHorizontal size={14} aria-hidden />
        </button>
      </NavLink>
      {menuOpen && (
        <div
          role="menu"
          className="absolute right-0 top-full mt-1 bg-card border border-line rounded-card shadow-pop py-2 px-3 min-w-[200px] z-20 text-[13px]"
        >
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setMenuOpen(false);
              setDraftName(label.name);
              setRenaming(true);
            }}
            className="block w-full text-left py-1 text-ink hover:text-blue"
          >
            Rename
          </button>
          <div className="flex items-center gap-1.5 py-1.5">
            {SWATCH_COLORS.map((c) => {
              const swatch = labelTone(c);
              return (
                <button
                  key={c}
                  type="button"
                  aria-label={`Set color ${c}`}
                  onClick={() => update.mutate({ id: label.id, color: c })}
                  className={`w-4 h-4 rounded-[5px] ${
                    label.color === c ? "ring-2 ring-offset-1 ring-blue" : ""
                  }`}
                  style={{ background: swatch.fg }}
                />
              );
            })}
          </div>
          <div className="flex items-center gap-1.5 pb-1.5">
            <input
              type="color"
              aria-label="Custom label color"
              // Falls back past a not-yet-valid `customColor` draft (e.g.
              // mid-typed hex in the text field below) so this native input
              // never gets fed an invalid value, which browsers silently
              // render as black.
              value={
                isHexColor(label.color)
                  ? label.color
                  : isHexColor(customColor)
                    ? customColor
                    : DEFAULT_CUSTOM_COLOR
              }
              onChange={(e) => {
                setCustomColor(e.target.value);
                update.mutate({ id: label.id, color: e.target.value });
              }}
              className="w-4 h-4 rounded-[5px] border-none bg-transparent p-0 cursor-pointer"
            />
            <input
              type="text"
              aria-label="Custom label color hex"
              value={customColor}
              onChange={(e) => setCustomColor(e.target.value)}
              onBlur={() => commitCustomColor(customColor)}
              onKeyDown={(e) => {
                if (e.key === "Enter") commitCustomColor(customColor);
              }}
              placeholder="#a3c9ff"
              className="w-20 px-1.5 py-0.5 rounded-button border border-line bg-paper text-[11px] text-ink focus:outline-none focus:ring-2 focus:ring-blue/30"
            />
          </div>
          {confirming ? (
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  role="menuitem"
                  autoFocus
                  onClick={async () => {
                    setMenuOpen(false);
                    setConfirming(false);
                    await del.mutateAsync(label.id);
                  }}
                  disabled={del.isPending}
                  className="px-2 py-1 rounded-button bg-strsoft text-ink border border-straw/40 font-semibold hover:opacity-90 disabled:opacity-50"
                >
                  Delete?
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => setConfirming(false)}
                  className="px-2 py-1 rounded-button text-mut hover:text-ink"
                >
                  Cancel
                </button>
              </div>
              <p className="text-[11px] text-mut">
                Removes the label from {label.meeting_count} meeting
                {label.meeting_count === 1 ? "" : "s"}
              </p>
            </div>
          ) : (
            <button
              type="button"
              role="menuitem"
              onClick={() => setConfirming(true)}
              className="block w-full text-left py-1 text-straw hover:opacity-80"
            >
              Delete
            </button>
          )}
        </div>
      )}
    </div>
  );
}
