import { useEffect, useState } from "react";
import { Check, Share } from "lucide-react";
import { exportMeetingFile } from "../lib/api/meetings";

/** Saves the meeting as a `.yogurt` file to hand to someone who also runs yogurt. */
export function ExportMeetingButton({
  id,
  title,
  beforeExport,
}: {
  id: string;
  title: string;
  /** Saves edits still waiting on the autosave debounce, so the file has them. */
  beforeExport: () => Promise<void>;
}) {
  const [state, setState] = useState<"idle" | "busy" | "done" | { error: string }>("idle");

  useEffect(() => {
    if (state === "idle" || state === "busy") return;
    const t = setTimeout(() => setState("idle"), typeof state === "string" ? 1500 : 4000);
    return () => clearTimeout(t);
  }, [state]);

  const onClick = () => {
    setState("busy");
    beforeExport()
      .then(() => exportMeetingFile(id, title))
      .then(
      () => setState("done"),
        (e: Error) => setState({ error: e.message }),
      );
  };

  const label =
    typeof state === "object" ? `Couldn't export the meeting: ${state.error}` : "Export meeting file";
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={state === "busy"}
      className={`shrink-0 p-1.5 rounded-button focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 ${
        state === "done"
          ? "text-matcha"
          : typeof state === "object"
            ? "text-straw"
            : "text-mut hover:text-ink hover:bg-line/40"
      }`}
    >
      {state === "done" ? <Check size={16} aria-hidden /> : <Share size={16} aria-hidden />}
    </button>
  );
}
