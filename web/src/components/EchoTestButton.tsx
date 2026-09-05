import { useMutation } from "@tanstack/react-query";
import { audioApi } from "../lib/api/settings";

/**
 * "Test" button for the mic echo output device: plays a 440 Hz tone on
 * `device` without needing a recording; the verdict sits inline so a label
 * row keeps its height.
 */
export function EchoTestButton({
  device,
  className = "",
}: {
  device: string;
  className?: string;
}) {
  const test = useMutation({
    mutationFn: () => audioApi.testEcho(device),
  });

  const verdict = test.isError
    ? { ok: false, text: "Could not reach the yogurt server" }
    : test.data
      ? test.data.ok
        ? { ok: true, text: "Tone played" }
        : { ok: false, text: test.data.error ?? "Test failed" }
      : null;

  return (
    <span className={`inline-flex items-center gap-2 min-w-0 ${className}`}>
      {verdict && (
        <span
          role="status"
          title={verdict.text}
          className={`text-[11px] font-mono truncate ${
            verdict.ok ? "text-matcha" : "text-[var(--color-straw)]"
          }`}
        >
          {verdict.ok ? "✓" : "✗"} {verdict.text}
        </span>
      )}
      <button
        type="button"
        disabled={test.isPending}
        aria-label="Play a test tone on the echo output device"
        className="text-[11px] font-mono text-mut hover:text-ink px-2 py-0.5 rounded-md border border-line disabled:opacity-40 shrink-0"
        onClick={() => test.mutate()}
      >
        {test.isPending ? "Testing…" : "Test"}
      </button>
    </span>
  );
}
