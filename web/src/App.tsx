import { Link, useNavigate } from "react-router";
import { Logo } from "./components/Logo";

/**
 * Phase 3 root → Phase 4 library stub.
 *
 * Phase 3 used a `useState<View>` toggle to flip between library and
 * meeting views. Phase 4 (Plan 04-04) promotes the meeting flow to
 * URL-driven routes registered in `router.tsx`:
 *   - `/meeting/new`        → `<Meeting />` bootstrap
 *   - `/meeting/:id`        → `<Meeting />`
 *   - `/meeting/:id/post`   → `<MeetingPost />`  (hero post-meeting UX)
 *
 * `MeetingPost` is the hero post-meeting route; this comment exists so
 * cross-references from `App.tsx` to `MeetingPost` are discoverable via
 * grep without re-creating a duplicate `<Routes>` block here.
 *
 * The CTA below navigates to `/meeting/new` which bootstraps a meeting
 * row server-side and replace-redirects to `/meeting/:id` — preserving
 * the single-click "Open a new meeting →" UX from Phase 3.
 */
export function App() {
  const navigate = useNavigate();

  return (
    <main className="mx-auto max-w-2xl px-10 py-16 space-y-8">
      <header className="flex items-center gap-3">
        <Logo size={44} />
        <div>
          <h1 className="font-serif text-[44px] leading-none text-ink">
            yogurt
          </h1>
          <p className="mt-1 text-[13px] text-mut">
            Phase 4 · the meeting library lands in Phase 7.
          </p>
        </div>
      </header>

      <button
        type="button"
        onClick={() => navigate("/meeting/new")}
        className="px-5 py-3 rounded-button bg-blue text-white text-[14px] font-semibold shadow-[0_2px_8px_rgba(91,79,199,0.3)] hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-paper"
      >
        Open a new meeting →
      </button>

      <p className="text-[12px] text-mut">
        <Link
          to="/style-guide"
          className="text-blue underline-offset-2 hover:underline rounded-chip focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-paper"
        >
          /style-guide →
        </Link>
      </p>
    </main>
  );
}
