import { useQueryClient } from "@tanstack/react-query";
import { RotateCw } from "lucide-react";

/** Refetches the mic and output device lists. cpal only enumerates devices
 *  present at query time, so a device hot-plugged after the page loaded
 *  needs this to appear without a reload. */
export function RefreshDevicesButton({ className = "" }: { className?: string }) {
  const qc = useQueryClient();
  return (
    <button
      type="button"
      aria-label="Refresh device list"
      title="Refresh device list"
      className={`text-mut hover:text-ink p-0.5 rounded-md ${className}`}
      onClick={() => {
        void qc.invalidateQueries({ queryKey: ["audio-devices"] });
        void qc.invalidateQueries({ queryKey: ["audio-output-devices"] });
      }}
    >
      <RotateCw size={13} />
    </button>
  );
}
