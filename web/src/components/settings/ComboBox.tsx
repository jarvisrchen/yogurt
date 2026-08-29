import { useEffect, useId, useRef, useState } from "react";

/**
 * ComboBox — a text input paired with a clickable dropdown of suggestions.
 *
 * Why not `<input list="…">` + `<datalist>`: native HTML autocomplete is a
 * *typing* aid, not a *click-to-open* affordance. On Safari/macOS the
 * dropdown arrow often doesn't render at all, and even when it does,
 * clicking it shows nothing. This combo-box replaces that pattern with a
 * guaranteed-to-work button + popup that lists every option, with a
 * text-filter input for narrowing down.
 *
 * Free text is always allowed — the popup is just for picking a known
 * suggestion, never for constraining what the user can type. The value
 * surfaces up through `onChange` on every keystroke (so the parent's
 * draft / PATCH flow keeps working); clicking a suggestion just sets
 * the value in one shot.
 *
 * Accessibility: the input keeps a normal `aria-label`, the trigger
 * button has an `aria-label` that names the provider, the popup uses
 * `role="listbox"` with `role="option"` children, and Escape / click
 * outside both close the popup.
 */

interface Props {
  value: string;
  onChange: (next: string) => void;
  options: string[];
  placeholder?: string;
  ariaLabel: string;
  /** Accessible name for the dropdown trigger — distinct from the input's
   *  own label so a screen reader user can tell them apart. */
  triggerLabel: string;
  disabled?: boolean;
  id?: string;
}

export function ComboBox({
  value,
  onChange,
  options,
  placeholder,
  ariaLabel,
  triggerLabel,
  disabled,
  id,
}: Props) {
  const reactId = useId();
  const inputId = id ?? `combobox-input-${reactId}`;
  const listboxId = `${inputId}-listbox`;
  const triggerId = `${inputId}-trigger`;
  const containerRef = useRef<HTMLDivElement>(null);

  const [open, setOpen] = useState(false);
  const [highlight, setHighlight] = useState(0);

  // Reset highlight whenever the option set or filter changes.
  useEffect(() => {
    setHighlight(0);
  }, [options, open]);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    function onDocClick(e: MouseEvent) {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [open]);

  // `value` is the source of truth for the input; the dropdown is just a
  // filter+picker over the options.
  const filtered = options;

  function pick(option: string) {
    onChange(option);
    setOpen(false);
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (!open && (e.key === "ArrowDown" || e.key === "Enter")) {
      setOpen(true);
      return;
    }
    if (e.key === "Escape") {
      setOpen(false);
      return;
    }
    if (!open) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlight((h) => Math.min(h + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlight((h) => Math.max(h - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const opt = filtered[highlight];
      if (opt !== undefined) pick(opt);
    }
  }

  return (
    <div ref={containerRef} className="relative flex-1 min-w-0">
      <input
        id={inputId}
        type="text"
        autoComplete="off"
        spellCheck={false}
        disabled={disabled}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onFocus={() => setOpen(true)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        aria-label={ariaLabel}
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listboxId}
        aria-activedescendant={
          open && filtered[highlight] !== undefined
            ? `${listboxId}-opt-${highlight}`
            : undefined
        }
        role="combobox"
        className="w-full font-mono text-[12.5px] border-b border-line focus:border-[var(--color-blue)] outline-none py-1 disabled:opacity-50"
      />
      <button
        id={triggerId}
        type="button"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        aria-label={triggerLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        className="absolute right-0 top-0 bottom-0 px-2 flex items-center text-mut hover:text-ink disabled:opacity-40"
      >
        <svg
          width="10"
          height="10"
          viewBox="0 0 10 10"
          fill="none"
          aria-hidden="true"
        >
          <path
            d="M2 4 L5 7 L8 4"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {open && filtered.length > 0 && (
        <ul
          id={listboxId}
          role="listbox"
          aria-label={ariaLabel}
          className="absolute z-10 mt-1 left-0 right-0 max-h-60 overflow-y-auto bg-white border border-line rounded-md shadow-lg"
        >
          {filtered.map((option, idx) => (
            <li
              key={option}
              id={`${listboxId}-opt-${idx}`}
              role="option"
              aria-selected={value === option}
              onMouseEnter={() => setHighlight(idx)}
              onMouseDown={(e) => {
                // mousedown (not click) so the input doesn't lose focus
                // before the selection lands.
                e.preventDefault();
                pick(option);
              }}
              className={`px-3 py-1.5 font-mono text-[12.5px] cursor-pointer ${
                idx === highlight
                  ? "bg-[var(--color-blsoft)] text-ink"
                  : "text-ink"
              } ${value === option ? "font-semibold" : ""}`}
            >
              {option}
            </li>
          ))}
        </ul>
      )}
      {open && filtered.length === 0 && (
        <div
          id={listboxId}
          role="listbox"
          className="absolute z-10 mt-1 left-0 right-0 bg-white border border-line rounded-md shadow-lg px-3 py-2 text-[12px] text-mut"
        >
          No saved suggestions — type a model id and press Enter.
        </div>
      )}
    </div>
  );
}
