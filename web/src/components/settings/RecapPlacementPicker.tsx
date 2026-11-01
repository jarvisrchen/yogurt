import type { RecapPlacement } from "../../lib/api/settings";

const OPTIONS: { value: RecapPlacement; label: string }[] = [
  { value: "dock", label: "In the transcript panel" },
  { value: "main", label: "Above my notes" },
];

const WIDTHS = [86, 64, 76, 52];

function Lines({ n, className }: { n: number; className: string }) {
  return (
    <>
      {Array.from({ length: n }, (_, i) => (
        <span key={i} className={`block h-[3px] rounded-full ${className}`} style={{ width: `${WIDTHS[i % 4]}%` }} />
      ))}
    </>
  );
}

/** A sketch of the meeting page with the recap in blueberry where it lands. */
function Thumb({ where }: { where: RecapPlacement }) {
  const dock = where === "dock";
  return (
    <div aria-hidden="true" className="relative overflow-hidden rounded-[8px] border border-line bg-paper aspect-[16/10]">
      <div className="absolute top-[12%] w-[56%] space-y-[5px]" style={{ left: dock ? "14%" : "22%" }}>
        <span className="block h-[5px] w-[42%] rounded-full bg-ink/70" />
        <span className="block h-[3px] w-[28%] rounded-full bg-line" />
        <div className="pt-[3px] space-y-[5px]">
          {!dock && (
            <div className="rounded-[4px] bg-blsoft border border-blue/50 px-1.5 py-1 space-y-[3px]">
              <Lines n={3} className="bg-blue/70" />
            </div>
          )}
          <div
            className="rounded-[4px] bg-card border border-line px-1.5 py-1.5 space-y-[3px]"
            style={{ height: dock ? 56 : 34 }}
          >
            <Lines n={dock ? 5 : 3} className="bg-line" />
          </div>
        </div>
      </div>
      {dock && (
        <div className="absolute right-0 top-0 h-full w-[26%] bg-blsoft border-l border-blue/50 px-1.5 pt-[14%] space-y-[4px]">
          <Lines n={4} className="bg-blue/70" />
        </div>
      )}
    </div>
  );
}

interface RecapPlacementPickerProps {
  value: RecapPlacement;
  onChange: (value: RecapPlacement) => void;
  disabled?: boolean;
}

/** Native radios, visually hidden and labelled by their tiles, so arrow keys and screen readers work as for any radio group. */
export function RecapPlacementPicker({ value, onChange, disabled = false }: RecapPlacementPickerProps) {
  return (
    <div
      role="radiogroup"
      aria-label="Show the recap"
      aria-disabled={disabled || undefined}
      className={`flex gap-4 max-w-[420px] ${disabled ? "opacity-50" : ""}`}
    >
      {OPTIONS.map((opt) => {
        const selected = value === opt.value;
        return (
          <label key={opt.value} className={`group flex-1 min-w-0 ${disabled ? "cursor-not-allowed" : "cursor-pointer"}`}>
            <input
              type="radio"
              name="recap-placement"
              value={opt.value}
              checked={selected}
              disabled={disabled}
              onChange={() => onChange(opt.value)}
              className="peer sr-only"
            />
            <div
              className={`rounded-[10px] peer-focus-visible:outline-2 peer-focus-visible:outline-offset-4 peer-focus-visible:outline-blue ${
                selected ? "ring-2 ring-blue ring-offset-2 ring-offset-card" : disabled ? "" : "group-hover:ring-1 group-hover:ring-line"
              }`}
            >
              <Thumb where={opt.value} />
            </div>
            <span className={`mt-2 block text-center text-[13px] ${selected ? "font-semibold text-ink" : "text-mut"}`}>
              {opt.label}
            </span>
          </label>
        );
      })}
    </div>
  );
}
