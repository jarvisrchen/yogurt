import clsx from "clsx";

/** A 14px ring. `pct` null spins a quarter arc for a job whose size is not known yet. */
export function ProgressRing({ pct }: { pct: number | null }) {
  const r = 5.5;
  const c = 2 * Math.PI * r;
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 14 14"
      className={clsx("w-3.5 h-3.5 -rotate-90", pct == null && "animate-spin")}
    >
      <circle cx="7" cy="7" r={r} fill="none" strokeWidth="2" className="stroke-mtsoft" />
      <circle
        cx="7"
        cy="7"
        r={r}
        fill="none"
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - (pct ?? 25) / 100)}
        className="stroke-matcha transition-[stroke-dashoffset] duration-200"
      />
    </svg>
  );
}
