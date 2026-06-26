/**
 * Phase 7 (Plan 07-01) — Library hero greeting + meeting-count caption.
 *
 * PRD §5.9 + D-03:
 *   - "Good morning, you" in Instrument Serif at 40px
 *   - "N meeting{s} · all on this Mac" in JetBrains Mono at 13px
 */

import { useGreeting } from "../../hooks/useGreeting";

interface Props {
  /** Total number of meetings — drives pluralization. */
  count: number;
}

export function Greeting({ count }: Props) {
  const { greeting } = useGreeting();
  const plural = count === 1 ? "meeting" : "meetings";
  return (
    <header className="mb-8">
      <h1 className="font-serif text-[40px] leading-tight text-ink">{greeting}</h1>
      <p className="mt-1 text-[13px] font-mono text-mut">
        {count} {plural} · all on this Mac
      </p>
    </header>
  );
}
