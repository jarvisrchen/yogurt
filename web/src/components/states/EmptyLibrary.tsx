/** The `.float-3500` class name encodes its 3.5s cycle. Do not parameterize it. */

import { useNavigate } from "react-router";
import { useCreateMeeting } from "../../lib/api/meetings";
import { Logo } from "../Logo";
import { Button } from "../Button";

export function EmptyLibrary() {
  const create = useCreateMeeting();
  const nav = useNavigate();

  const start = async () => {
    try {
      const m = await create.mutateAsync(undefined);
      nav(`/meeting/${m.id}`);
    } catch (e) {
      // Logged only; there is no richer error UX.
      console.error("create meeting failed", e);
    }
  };

  return (
    <div className="flex flex-col items-center text-center mt-24">
      <div className="float-3500 mb-8">
        <Logo size={64} ariaLabel="yogurt" />
      </div>
      <h2 className="heading-state mb-3">
        No meetings yet
      </h2>
      <p className="text-[15px] text-mut max-w-md mb-6">
        Start one and yogurt listens to both sides of the call without
        joining it as a bot. Your notes and audio stay on this Mac.
      </p>
      <Button onClick={start} disabled={create.isPending}>
        Start your first meeting
        <kbd className="bg-white/20 text-white/90 text-[11px] font-mono px-1.5 py-0.5 rounded">
          ⌘N
        </kbd>
      </Button>
      <p className="mt-6 text-[11px] text-mut">
        notes saved to <code className="font-mono">~/.yogurt/notes/*.md</code>
      </p>
    </div>
  );
}
