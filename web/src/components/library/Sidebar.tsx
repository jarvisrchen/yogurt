/**
 * Phase 7 (Plan 07-01) — Library sidebar (212px, paper bg, blueberry CTA).
 *
 * PRD §5.9 + D-02. Top-to-bottom layout:
 *   1. Yogurt swirl logo + "yogurt" wordmark (Hanken 700)
 *   2. Primary "+ New meeting" blueberry button (creates → /meeting/:id)
 *   3. Nav: "All meetings" (lilac active), "Starred"
 *   4. Footer:
 *      - matcha "Local-only · on" pill iff no active cloud provider
 *      - ⚙ Settings row → /settings
 *
 * The frontend route shape was kept at `/meeting/:id` (the Phase 3
 * convention) instead of the plan-suggested `/m/:id`, since the existing
 * Meeting + MeetingPost surfaces already mount under that prefix. Same
 * conceptual link, smaller blast radius. (Auto-fix Rule 3.)
 */

import { useState } from "react";
import { Link, NavLink, useNavigate } from "react-router";
import { Plus, Settings as SettingsIcon } from "lucide-react";
import { Logo } from "../Logo";
import { useCreateMeeting } from "../../lib/api/meetings";
import { useCreateLabel, useLabels } from "../../lib/api/labels";
import { useQuery } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";
import { SidebarLabelRow } from "./SidebarLabelRow";
import { Button } from "../Button";

export function Sidebar() {
  const navigate = useNavigate();
  const createMeeting = useCreateMeeting();
  const labels = useLabels();
  const createLabel = useCreateLabel();
  const [addingLabel, setAddingLabel] = useState(false);
  const [newLabelName, setNewLabelName] = useState("");

  function commitNewLabel() {
    const name = newLabelName.trim();
    if (name.length > 0) {
      createLabel.mutate({ name });
    }
    setNewLabelName("");
    setAddingLabel(false);
  }

  // Phase 5 settings drive the "Local-only · on" pill — only show it
  // when no `kind === "cloud"` provider is active. The shape is the
  // ProviderView from `lib/api/settings.ts`; we treat absence of
  // `kind` as cloud (most providers default that way).
  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: () => settingsApi.get(),
    staleTime: 30_000,
  });

  const isLocalOnly =
    !settings.data?.providers?.some(
      (p) => p.is_active && !isLocalProvider(p.base_url),
    );

  const handleNew = async () => {
    try {
      const m = await createMeeting.mutateAsync(undefined);
      // `autoStart` tells Meeting.tsx to POST /start immediately on
      // mount (task NOTES-01) — Granola-style one-click-and-recording,
      // not create-then-hunt-for-the-Start-button.
      navigate(`/meeting/${m.id}`, { state: { autoStart: true } });
    } catch (e) {
      // Surface inline; the consumer Library page will catch this via
      // the mutation's error state. Logging is enough for v1.
      console.error("create meeting failed", e);
    }
  };

  return (
    <aside
      // `sticky top-0 h-screen` pins the rail to the viewport instead of
      // letting it stretch to the height of a long meeting list. Before
      // this, the flex parent sized the aside to the tallest child, so
      // `mt-auto` parked Settings at the bottom of the *document* and you
      // had to scroll the library to reach it. Sticky keeps the document
      // scroll on <main> only, which is what the layout always meant.
      className="w-[212px] shrink-0 flex flex-col bg-paper border-r border-line sticky top-0 h-screen"
    >
      {/* Header — logo + wordmark */}
      <div className="px-5 pt-5 pb-4 flex items-center gap-2">
        <Logo size={28} />
        <span className="heading-wordmark">
          yogurt
        </span>
      </div>

      {/* Primary CTA */}
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

      {/* Nav */}
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
            `px-3 py-1.5 rounded-button text-[13px] ${
              isActive
                ? "bg-blsoft text-blue font-semibold"
                : "text-ink hover:bg-line/40"
            }`
          }
        >
          Starred
        </NavLink>
      </nav>

      {/* Labels */}
      <div className="px-5 pt-5 pb-1 flex items-center justify-between text-[11px] font-mono uppercase tracking-wider text-mut">
        <span>Labels</span>
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
        {(labels.data ?? []).length === 0 && !addingLabel ? (
          <p className="px-3 py-1 text-[12px] font-mono text-mut">No labels yet</p>
        ) : (
          (labels.data ?? []).map((l) => <SidebarLabelRow key={l.id} label={l} />)
        )}
      </div>

      {/* Footer */}
      <div className="mt-auto px-4 py-4 flex flex-col gap-2">
        {isLocalOnly && (
          <div
            className="self-start px-2.5 py-1 rounded-pill bg-mtsoft text-matcha text-[11px] font-mono"
          >
            Local-only · on
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

/**
 * Heuristic for "is this a local-network provider" — used to decide
 * whether the Local-only pill should appear. Returns true for
 * Ollama / LM Studio / vLLM / llama.cpp-server / 127.0.0.1 / localhost
 * URLs, false for cloud SaaS. The settings shape doesn't currently carry
 * a `kind: "local" | "cloud"` discriminator, so we infer from URL.
 */
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
