import { useState } from "react";
import { Link } from "react-router";
import { Logo } from "./components/Logo";
import { Meeting } from "./routes/Meeting";

type View = "library" | "meeting";

/**
 * Phase 3 root. Library stub ↔ Meeting view switch.
 *
 * Phase 3 deliberately uses a `useState<View>` toggle rather than a full
 * router move — Phase 7 introduces /library, /meetings/:id, /settings/* and
 * decides the routing question properly (Phase 3 D-20). The /style-guide
 * route from Phase 1 is still wired through router.tsx and reachable via
 * the small link in the library stub footer.
 */
export function App() {
  const [view, setView] = useState<View>("library");

  if (view === "meeting") {
    return <Meeting />;
  }

  return (
    <main className="mx-auto max-w-2xl px-10 py-16 space-y-8">
      <header className="flex items-center gap-3">
        <Logo size={44} />
        <div>
          <h1 className="font-serif text-[44px] leading-none text-ink">
            yogurt
          </h1>
          <p className="mt-1 text-[13px] text-mut">
            Phase 3 · the meeting library lands in Phase 7.
          </p>
        </div>
      </header>

      <button
        type="button"
        onClick={() => setView("meeting")}
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
