const SIZES = {
  md: "px-4 py-[6px] text-[13px]",
  sm: "px-3 py-[5px] text-[12.5px]",
};

interface SegmentedProps<T extends string> {
  label: string;
  options: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
  size?: keyof typeof SIZES;
}

/** A pill-grouped single choice, exposed as a radiogroup. */
export function Segmented<T extends string>({ label, options, value, onChange, size = "md" }: SegmentedProps<T>) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex gap-0.5 rounded-card bg-line/60 p-1">
      {options.map((opt) => {
        const selected = value === opt.value;
        return (
          <button
            key={opt.value}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(opt.value)}
            className={`rounded-button ${SIZES[size]} font-semibold transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue ${
              selected ? "bg-card text-ink shadow-card" : "text-mut hover:text-ink"
            }`}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}
