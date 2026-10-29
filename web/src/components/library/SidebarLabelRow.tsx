import { useEffect, useRef, useState } from "react";
import { NavLink, useLocation, useNavigate } from "react-router";
import { MoreHorizontal } from "lucide-react";
import {
  useDeleteLabel,
  useUpdateLabel,
  type PaletteColor,
  type LabelWithCount,
} from "../../lib/api/labels";
import { useArchiveMatching, useArchivePreview } from "../../lib/api/meetings";
import { formatCount } from "./pagination";
import { DEFAULT_CUSTOM_COLOR, isHexColor, labelTone } from "../labels/LabelChip";

const SWATCH_COLORS: PaletteColor[] = ["blue", "matcha", "straw", "lilac", "honey", "slate"];


interface Props {
  label: LabelWithCount;
  /** The row opens and counts the label's archived meetings, in the Archive view and under Archived labels. */
  inArchive: boolean;
}

export function SidebarLabelRow({ label, inArchive }: Props) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [confirming, setConfirming] = useState<"delete" | "archive" | null>(null);
  const [draftName, setDraftName] = useState(label.name);
  const [customColor, setCustomColor] = useState(
    isHexColor(label.color) ? label.color : DEFAULT_CUSTOM_COLOR,
  );
  const wrapperRef = useRef<HTMLDivElement>(null);
  const update = useUpdateLabel();
  const del = useDeleteLabel();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const live = label.meeting_count - label.archived_count;
  const archive = useArchiveMatching();
  const preview = useArchivePreview(confirming === "archive" ? { labelId: label.id } : null);
  const archivable = preview.data?.count;

  useEffect(() => {
    if (!menuOpen) return;
    setCustomColor(isHexColor(label.color) ? label.color : DEFAULT_CUSTOM_COLOR);
    const onDown = (e: MouseEvent) => {
      if (wrapperRef.current && !wrapperRef.current.contains(e.target as Node)) {
        setMenuOpen(false);
        setConfirming(null);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [menuOpen, label.color]);

  function commitCustomColor(hex: string) {
    if (isHexColor(hex)) update.mutate({ id: label.id, color: hex });
  }

  // Only Delete times out: the archive confirm waits on a count and may show an error.
  useEffect(() => {
    if (confirming !== "delete") return;
    const t = setTimeout(() => setConfirming(null), 3000);
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

  const to = inArchive ? `/archive/label/${label.id}` : `/label/${label.id}`;

  return (
    <div ref={wrapperRef} className="relative group/label">
      <NavLink
        to={to}
        onClick={(e) => {
          if (menuOpen) e.preventDefault();
          // A modified click keeps its browser meaning, such as a new tab.
          else if (pathname === to && e.button === 0 && !(e.metaKey || e.ctrlKey || e.shiftKey || e.altKey)) {
            e.preventDefault();
            navigate(inArchive ? "/archive" : "/");
          }
        }}
        className={({ isActive }) =>
          `flex items-center gap-2 px-3 py-1.5 rounded-button text-[13px] ${
            isActive ? "bg-blsoft text-blue font-semibold" : "text-ink hover:bg-line/40"
          }`
        }
      >
        <span className="w-[9px] h-[9px] rounded-[3px] shrink-0" style={{ background: tone.fg }} aria-hidden />
        <span className="flex-1 truncate">{label.name}</span>
        <span className="text-[11px] text-mut">{inArchive ? label.archived_count : live}</span>
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
              // Falls back past a half-typed hex: browsers render an invalid color input value as black.
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
          {confirming === "archive" ? (
            <InlineConfirm
              action="Archive?"
              tone="bg-blsoft text-blue border-blue/30"
              disabled={!archivable || preview.isFetching || archive.isPending}
              onConfirm={() =>
                archive.mutateAsync({ labelId: label.id }).then(() => {
                  setMenuOpen(false);
                  setConfirming(null);
                  // The label's own page would now be empty, so follow its meetings into the Archive.
                  if (pathname === `/label/${label.id}`) navigate(`/archive/label/${label.id}`);
                }, () => {})
              }
              onCancel={() => setConfirming(null)}
              caption={
                archive.error
                  ? `Couldn't archive meetings: ${archive.error.message}`
                  : preview.error
                    ? `Couldn't count meetings: ${preview.error.message}`
                    : archivable == null || preview.isFetching
                      ? "Counting meetings…"
                      : archivable === 0
                        ? "No meetings left to archive"
                        : `Archives ${formatCount(archivable)} meeting${archivable === 1 ? "" : "s"}`
              }
            />
          ) : confirming === "delete" ? (
            <InlineConfirm
              action="Delete?"
              tone="bg-strsoft text-ink border-straw/40"
              disabled={del.isPending}
              onConfirm={async () => {
                setMenuOpen(false);
                setConfirming(null);
                await del.mutateAsync(label.id);
              }}
              onCancel={() => setConfirming(null)}
              caption={`Removes the label from ${label.meeting_count} meeting${label.meeting_count === 1 ? "" : "s"}`}
            />
          ) : (
            <>
              {live > 0 && (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    archive.reset();
                    setConfirming("archive");
                  }}
                  className="block w-full text-left py-1 text-ink hover:text-blue"
                >
                  Archive meetings
                </button>
              )}
              <button
                type="button"
                role="menuitem"
                onClick={() => setConfirming("delete")}
                className="block w-full text-left py-1 text-straw hover:opacity-80"
              >
                Delete
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}

interface InlineConfirmProps {
  action: string;
  tone: string;
  disabled: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  caption: string;
}

function InlineConfirm({ action, tone, disabled, onConfirm, onCancel, caption }: InlineConfirmProps) {
  const ref = useRef<HTMLButtonElement>(null);
  // A button that mounts disabled cannot take autoFocus, so focus it once it is enabled.
  useEffect(() => {
    if (!disabled) ref.current?.focus();
  }, [disabled]);
  return (
    <div className="space-y-1">
      <div className="flex items-center gap-2">
        <button
          ref={ref}
          type="button"
          role="menuitem"
          onClick={onConfirm}
          disabled={disabled}
          className={`px-2 py-1 rounded-button border font-semibold hover:opacity-90 disabled:opacity-50 ${tone}`}
        >
          {action}
        </button>
        <button
          type="button"
          role="menuitem"
          onClick={onCancel}
          className="px-2 py-1 rounded-button text-mut hover:text-ink"
        >
          Cancel
        </button>
      </div>
      <p className="text-[11px] text-mut">{caption}</p>
    </div>
  );
}
