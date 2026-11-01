import { useEffect, useId, useRef, useState } from "react";
import { ALargeSmall } from "lucide-react";
import { TEXT_SIZES } from "../lib/dockLayout";
import { LABEL } from "./settings/labelClass";

/** The "A" on each step grows with it, so the control previews what it does. */
const GLYPH_PX = [12, 14, 17, 20];

/** The dock header's Aa button and its popover of text sizes. A pick applies at once and leaves the popover open. */
export function DockTextSize({ value, onChange }: { value: number; onChange: (px: number) => void }) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const name = useId();

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    // On document, ahead of window-level Escape handlers such as the chat window's, which must not also close.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      setOpen(false);
      buttonRef.current?.focus();
    };
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const current = TEXT_SIZES.find((s) => s.px === value) ?? TEXT_SIZES[1];

  return (
    <div ref={rootRef} className="relative">
      <button
        ref={buttonRef}
        type="button"
        aria-label="Text size"
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen(!open)}
        className={`inline-flex items-center justify-center h-7 w-7 rounded-button transition-colors ${
          open ? "bg-blsoft text-blue" : "text-mut hover:text-ink hover:bg-blsoft/50"
        }`}
      >
        <ALargeSmall size={16} aria-hidden="true" />
      </button>
      {open && (
        <div
          role="dialog"
          aria-label="Text size"
          className="absolute right-0 top-[calc(100%+8px)] z-40 w-[232px] rounded-card bg-card border border-line shadow-pop p-3 animate-pop-up"
        >
          <fieldset>
            <legend className={LABEL}>Text size</legend>
            <div className="mt-2 flex gap-0.5 rounded-card bg-line/60 p-1">
              {TEXT_SIZES.map((s, i) => {
                const on = s.px === value;
                return (
                  <label
                    key={s.px}
                    className={`flex-1 h-9 flex items-center justify-center rounded-button font-bold leading-none cursor-pointer has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-blue ${
                      on ? "bg-card text-ink shadow-card" : "text-mut hover:text-ink"
                    }`}
                    style={{ fontSize: GLYPH_PX[i] }}
                  >
                    <input
                      type="radio"
                      name={name}
                      className="sr-only"
                      checked={on}
                      onChange={() => onChange(s.px)}
                      aria-label={s.label}
                    />
                    <span aria-hidden="true">A</span>
                  </label>
                );
              })}
            </div>
          </fieldset>
          <p className="mt-2 text-[12px] text-mut">
            {current.label} · {current.px} px
          </p>
        </div>
      )}
    </div>
  );
}
