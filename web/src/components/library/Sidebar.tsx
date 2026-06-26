/**
 * Phase 7 (Plan 07-01) — Library sidebar (212px, paper bg, blueberry CTA).
 *
 * PRD §5.9 + D-02. Top-to-bottom layout:
 *   1. Yogurt swirl logo + "yogurt" wordmark (Instrument-Serif)
 *   2. Primary "+ New meeting" blueberry button (creates → /meeting/:id)
 *   3. Nav: "All meetings" (lilac active), "Starred"
 *   4. FOLDERS section: 3 hardcoded v1.1-stub folders, opacity-60
 *   5. Footer:
 *      - matcha "Local-only · on" pill iff no active cloud provider
 *      - ⚙ Settings row → /settings
 *
 * The frontend route shape was kept at `/meeting/:id` (the Phase 3
 * convention) instead of the plan-suggested `/m/:id`, since the existing
 * Meeting + MeetingPost surfaces already mount under that prefix. Same
 * conceptual link, smaller blast radius. (Auto-fix Rule 3.)
 */

import { Link, NavLink, useNavigate } from "react-router";
import { Logo } from "../Logo";
import { useCreateMeeting } from "../../lib/api/meetings";
import { useQuery } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";

const SAMPLE_FOLDERS: Array<{ name: string; color: string }> = [
  { name: "Work", color: "var(--color-blue)" },
  { name: "Hiring", color: "var(--color-straw)" },
  { name: "1:1s", color: "var(--color-matcha)" },
];

export function Sidebar() {
  const navigate = useNavigate();
  const createMeeting = useCreateMeeting();

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
      navigate(`/meeting/${m.id}`);
    } catch (e) {
      // Surface inline; the consumer Library page will catch this via
      // the mutation's error state. Logging is enough for v1.
      console.error("create meeting failed", e);
    }
  };

  return (
    <aside
      className="w-[212px] shrink-0 flex flex-col bg-paper border-r border-line"
      style={{ minHeight: "100vh" }}
    >
      {/* Header — logo + wordmark */}
      <div className="px-5 pt-5 pb-4 flex items-center gap-2">
        <Logo size={28} />
        <span className="font-serif text-[22px] text-ink leading-none">
          yogurt
        </span>
      </div>

      {/* Primary CTA */}
      <div className="px-4 pb-3">
        <button
          type="button"
          onClick={handleNew}
          disabled={createMeeting.isPending}
          className="w-full px-3 py-2 rounded-button bg-blue text-white text-[13px] font-semibold shadow-button-blue hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-paper disabled:opacity-50"
        >
          + New meeting
        </button>
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

      {/* Folders — v1.1 stub */}
      <div className="px-4 pt-6 pb-2 text-[11px] font-mono uppercase tracking-wider text-mut">
        Folders
      </div>
      <ul className="px-2 flex flex-col gap-0.5">
        {SAMPLE_FOLDERS.map((f) => (
          <li
            key={f.name}
            title="Coming in v1.1"
            className="px-3 py-1.5 rounded-button text-[13px] text-ink opacity-60 flex items-center gap-2 cursor-default"
          >
            <span
              aria-hidden
              className="w-2 h-2 rounded-full"
              style={{ background: f.color }}
            />
            <span>{f.name}</span>
          </li>
        ))}
      </ul>

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
          <span aria-hidden>⚙</span>
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
