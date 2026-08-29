import { useEffect, useId, useRef, useState } from "react";

/**
 * ComboBox - a text input paired with a clickable dropdown of suggestions.
 *
 * Why not `<input list="…">` + `<datalist>`: native HTML autocomplete is a
 * *typing* aid, not a *click-to-open* affordance. On Safari/macOS the
 * dropdown arrow often doesn't render at all, and even when it does,
 * clicking it shows nothing. This combo-box replaces that pattern with a
 * guaranteed-to-work button + popup that lists every option, with a
 * text-filter input for narrowing down.
 *
 * Free text is always allowed - the popup is just for picking a known
 * suggestion, never for constraining what the user can type. The value
 * surfaces up through `onChange` on every keystroke (so the parent's
 * draft state keeps working); `onCommit` fires separately, once, when
 * the user actually commits to a value - picking an option, pressing
 * Enter with nothing highlighted, or blurring away from the component -
 * so a parent doing a PATCH-on-commit flow can tell a keystroke from a
 * decision.
 *
 * Filtering: typing narrows the popup to options whose id contains the
 * typed text (case-insensitive). Opening the popup any other way (focus,
 * trigger click, ArrowDown) shows the full list again - the user hasn't
 * typed a filter yet.
 *
 * Accessibility: the input keeps a normal `aria-label`, the trigger
 * button has an `aria-label` that names the provider, the popup uses
 * `role="listbox"` with `role="option"` children, and Escape / click
 * outside both close the popup.
 */

interface Props {
  value: string;
  onChange: (next: string) => void;
  /** Fires once when the user commits to a value: picking an option,
   *  pressing Enter with nothing highlighted, or blurring outside the
   *  component. Does not fire per keystroke. */
  onCommit?: (value: string) => void;
  options: string[];
  placeholder?: string;
  ariaLabel: string;
  /** Accessible name for the dropdown trigger - distinct from the input's
   *  own label so a screen reader user can tell them apart. */
  triggerLabel: string;
  disabled?: boolean;
  id?: string;
}

export function ComboBox({
  value,
  onChange,
  onCommit,
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
  // -1 means nothing highlighted - Enter in that state commits the typed
  // text instead of picking an option.
  const [highlight, setHighlight] = useState(-1);
  // null means "show everything" (popup just opened without typing); a
  // string is the in-progress filter from typing.
  const [query, setQuery] = useState<string | null>(null);

  const filtered = query
    ? options.filter((o) => o.toLowerCase().includes(query.toLowerCase()))
    : options;

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

  function openShowingAll() {
    setQuery(null);
    setHighlight(-1);
    setOpen(true);
  }

  function pick(option: string) {
    onChange(option);
    onCommit?.(option);
    setOpen(false);
    setQuery(null);
    setHighlight(-1);
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (!open) {
      if (e.key === "ArrowDown") {
        openShowingAll();
        return;
      }
      if (e.key === "Enter") {
        onCommit?.(value);
        return;
      }
      return;
    }
    if (e.key === "Escape") {
      setOpen(false);
      setQuery(null);
      setHighlight(-1);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlight((h) => Math.min(h + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlight((h) => Math.max(h - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const opt = highlight >= 0 ? filtered[highlight] : undefined;
      if (opt !== undefined) {
        pick(opt);
      } else {
        setOpen(false);
        setQuery(null);
        setHighlight(-1);
        onCommit?.(value);
      }
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
        onChange={(e) => {
          const next = e.target.value;
          onChange(next);
          setQuery(next);
          setHighlight(-1);
          setOpen(true);
        }}
        onFocus={() => {
          if (!open) openShowingAll();
        }}
        onBlur={(e) => {
          if (
            containerRef.current &&
            e.relatedTarget &&
            containerRef.current.contains(e.relatedTarget as Node)
          ) {
            return;
          }
          setOpen(false);
          setQuery(null);
          setHighlight(-1);
          onCommit?.(value);
        }}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        aria-label={ariaLabel}
        aria-autocomplete="list"
        aria-expanded={open}
        aria-controls={listboxId}
        aria-activedescendant={
          open && highlight >= 0 && filtered[highlight] !== undefined
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
        onClick={() => (open ? setOpen(false) : openShowingAll())}
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
          {options.length === 0
            ? "No saved suggestions - type a model id and press Enter."
            : "No matches - press Enter to use what you typed."}
        </div>
      )}
    </div>
  );
}
