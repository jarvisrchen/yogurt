import clsx from "clsx";
import { ProgressRing } from "./ProgressRing";

interface Props {
  kind: "running" | "done" | "failed";
  /** Percent for the ring while running, or null for an indeterminate one. */
  pct: number | null;
  text: string;
  label: string;
  onClick: () => void;
  closeLabel: string;
  onClose: () => void;
  testId: string;
}

/** The import and push pills: a ring while running, then ✓ or ! with a close button. */
export function TaskPill({ kind, pct, text, label, onClick, closeLabel, onClose, testId }: Props) {
  const failed = kind === "failed";
  const finished = kind !== "running";
  return (
    <div
      className={clsx(
        "flex items-center bg-card border rounded-full shadow-pop text-[13px] text-ink whitespace-nowrap transition-colors",
        failed ? "border-straw" : "border-line hover:border-matcha",
      )}
    >
      <button
        type="button"
        onClick={onClick}
        aria-label={label}
        data-testid={testId}
        className={clsx("flex items-center gap-2 py-1.5 pl-3", finished ? "pr-1.5" : "pr-3")}
      >
        {finished ? (
          <span
            aria-hidden="true"
            className={clsx(
              "inline-flex w-3.5 justify-center font-semibold",
              failed ? "text-straw" : "text-matcha",
            )}
          >
            {failed ? "!" : "✓"}
          </span>
        ) : (
          <ProgressRing pct={pct} />
        )}
        <span className={clsx("font-medium", failed && "text-straw")}>{text}</span>
      </button>
      {finished && (
        <button
          type="button"
          onClick={onClose}
          aria-label={closeLabel}
          className="self-stretch flex items-center pl-1 pr-3 text-[15px] leading-none text-mut hover:text-ink"
        >
          ×
        </button>
      )}
    </div>
  );
}
