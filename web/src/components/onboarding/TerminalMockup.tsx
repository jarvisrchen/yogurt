/**
 * Phase 7 Plan 07-04 — Welcome left-column terminal mockup (PRD §5.10).
 *
 * Visual: a small macOS-style window with three traffic-light dots and a
 * 4-line boot sequence. The copy is fixed — it's a *mockup*, not a live
 * tail — and intentionally encodes the CLI-first launch story:
 *
 *   $ yogurt start
 *   ✓ server live on :7878
 *   ✓ opening your browser…
 *   → waiting for screen-recording grant
 *
 * Colors come straight from the macOS traffic-light spec (#FF5F57, #FEBC2E,
 * #28C840). The panel background `#211D18` matches `--color-ink` so the
 * mock feels native to the Yogurt palette without going truly black.
 */

const TRAFFIC_LIGHTS = [
  { color: "#FF5F57", label: "close" },
  { color: "#FEBC2E", label: "minimize" },
  { color: "#28C840", label: "zoom" },
];

const BOOT_SEQUENCE = `$ yogurt start
✓ server live on :7878
✓ opening your browser…
→ waiting for screen-recording grant`;

export function TerminalMockup() {
  return (
    <div
      className="rounded-card overflow-hidden shadow-window"
      style={{ background: "#211D18" }}
    >
      <div
        className="flex items-center gap-2 px-3 py-2"
        style={{ background: "#1a1713" }}
      >
        {TRAFFIC_LIGHTS.map((dot) => (
          <span
            key={dot.label}
            className="w-3 h-3 rounded-full"
            style={{ background: dot.color }}
            aria-hidden
          />
        ))}
      </div>
      <pre
        className="font-mono text-[12px] leading-relaxed px-4 py-4 m-0"
        style={{ color: "#EDEAE2" }}
      >
        {BOOT_SEQUENCE}
      </pre>
    </div>
  );
}
